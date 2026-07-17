//! Content-addressed artifact storage with atomic writes and integrity verification.

#![allow(missing_docs)]

use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{Database, Result, StorageError};

/// A stored artifact identified by the SHA-256 digest of its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub hash: String,
    pub size: u64,
}

/// Results from a retention cleanup pass.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArtifactCleanup {
    pub removed_files: u64,
    pub removed_bytes: u64,
    pub retained_files: u64,
}

/// A content-addressed store rooted at `{data_dir}/artifacts/sha256`.
#[derive(Clone, Debug)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    /// Create or open the artifact directory below a Retcon data directory.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self> {
        let root = data_dir.as_ref().join("artifacts").join("sha256");
        std::fs::create_dir_all(&root)
            .map_err(|e| StorageError::io("create artifact directory", &root, e))?;
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Store bytes atomically, deduplicating content with the same SHA-256 hash.
    pub fn store_bytes(&self, bytes: &[u8]) -> Result<Artifact> {
        self.store(std::io::Cursor::new(bytes))
    }

    /// Stream content into the store without loading the complete artifact into memory.
    pub fn store(&self, mut reader: impl Read) -> Result<Artifact> {
        let temp = self.root.join(format!(".{}.tmp", Uuid::new_v4()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|e| StorageError::io("create temporary artifact", &temp, e))?;
        let mut digest = Sha256::new();
        let mut size = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|e| StorageError::io("read artifact input", &temp, e))?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            file.write_all(&buffer[..count])
                .map_err(|e| StorageError::io("write artifact", &temp, e))?;
            size = size.saturating_add(count as u64);
        }
        file.sync_all()
            .map_err(|e| StorageError::io("sync artifact", &temp, e))?;
        drop(file);
        let hash = format!("{:x}", digest.finalize());
        let destination = self.path_for_valid_hash(&hash);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StorageError::io("create artifact shard", parent, e))?;
        }
        if destination.exists() {
            std::fs::remove_file(&temp)
                .map_err(|e| StorageError::io("remove duplicate artifact", &temp, e))?;
            let size = std::fs::metadata(&destination)
                .map_err(|e| StorageError::io("read duplicate artifact metadata", &destination, e))?
                .len();
            return Ok(Artifact { hash, size });
        }
        if let Err(error) = std::fs::rename(&temp, &destination) {
            if destination.exists() {
                let _ = std::fs::remove_file(&temp);
                let size = std::fs::metadata(&destination)
                    .map_err(|e| {
                        StorageError::io("read duplicate artifact metadata", &destination, e)
                    })?
                    .len();
                return Ok(Artifact { hash, size });
            }
            return Err(StorageError::io("publish artifact", &destination, error));
        }
        Ok(Artifact { hash, size })
    }

    /// Open an artifact after validating its hash format.
    pub fn get(&self, hash: &str) -> Result<File> {
        let path = self.path_for(hash)?;
        File::open(&path).map_err(|e| StorageError::io("open artifact", path, e))
    }

    /// Recompute an artifact digest and reject missing or modified content.
    pub fn verify(&self, hash: &str) -> Result<Artifact> {
        let mut file = self.get(hash)?;
        let mut digest = Sha256::new();
        let size = std::io::copy(&mut file, &mut digest)
            .map_err(|e| StorageError::io("verify artifact", self.path_for_valid_hash(hash), e))?;
        let actual = format!("{:x}", digest.finalize());
        if actual != hash {
            return Err(StorageError::ArtifactIntegrity {
                hash: hash.into(),
                details: format!("content hashes to {actual}"),
            });
        }
        Ok(Artifact {
            hash: hash.into(),
            size,
        })
    }

    /// Report bytes occupied by content files.
    pub fn disk_usage(&self) -> Result<u64> {
        let mut total = 0_u64;
        self.visit_files(|_, metadata| {
            total = total.saturating_add(metadata.len());
            Ok(())
        })?;
        Ok(total)
    }

    /// Report disk usage without blocking async worker threads.
    pub async fn disk_usage_async(&self) -> Result<u64> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.disk_usage())
            .await
            .map_err(|_| StorageError::ConnectionPoisoned)?
    }

    /// Remove old artifacts except hashes explicitly protected by active records.
    pub fn cleanup(
        &self,
        older_than: Duration,
        protected: &HashSet<String>,
    ) -> Result<ArtifactCleanup> {
        let cutoff = SystemTime::now()
            .checked_sub(older_than)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mut report = ArtifactCleanup::default();
        self.visit_files(|path, metadata| {
            let hash = hash_from_path(path);
            let old = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH) < cutoff;
            if old && hash.as_ref().is_some_and(|h| !protected.contains(h)) {
                std::fs::remove_file(path)
                    .map_err(|e| StorageError::io("remove expired artifact", path, e))?;
                report.removed_files += 1;
                report.removed_bytes = report.removed_bytes.saturating_add(metadata.len());
            } else {
                report.retained_files += 1;
            }
            Ok(())
        })?;
        Ok(report)
    }

    /// Apply retention while protecting every artifact currently referenced by SQLite.
    pub fn cleanup_referenced(
        &self,
        database: &Database,
        older_than: Duration,
    ) -> Result<ArtifactCleanup> {
        let protected = referenced_hashes(database)?;
        self.cleanup(older_than, &protected)
    }

    /// Async variant of [`Self::cleanup_referenced`].
    pub async fn cleanup_referenced_async(
        &self,
        database: &Database,
        older_than: Duration,
    ) -> Result<ArtifactCleanup> {
        let store = self.clone();
        let database = database.clone();
        tokio::task::spawn_blocking(move || store.cleanup_referenced(&database, older_than))
            .await
            .map_err(|_| StorageError::ConnectionPoisoned)?
    }

    fn path_for(&self, hash: &str) -> Result<PathBuf> {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(StorageError::ArtifactIntegrity {
                hash: hash.into(),
                details: "expected 64 lowercase hexadecimal characters".into(),
            });
        }
        Ok(self.path_for_valid_hash(hash))
    }
    fn path_for_valid_hash(&self, hash: &str) -> PathBuf {
        self.root.join(&hash[..2]).join(&hash[2..])
    }
    fn visit_files(
        &self,
        mut visitor: impl FnMut(&Path, &std::fs::Metadata) -> Result<()>,
    ) -> Result<()> {
        for shard in std::fs::read_dir(&self.root)
            .map_err(|e| StorageError::io("read artifact directory", &self.root, e))?
        {
            let shard =
                shard.map_err(|e| StorageError::io("read artifact shard", &self.root, e))?;
            if !shard
                .file_type()
                .map_err(|e| StorageError::io("inspect artifact shard", shard.path(), e))?
                .is_dir()
            {
                continue;
            }
            for entry in std::fs::read_dir(shard.path())
                .map_err(|e| StorageError::io("read artifact shard", shard.path(), e))?
            {
                let entry =
                    entry.map_err(|e| StorageError::io("read artifact entry", shard.path(), e))?;
                let metadata = entry
                    .metadata()
                    .map_err(|e| StorageError::io("inspect artifact", entry.path(), e))?;
                if metadata.is_file() {
                    visitor(&entry.path(), &metadata)?;
                }
            }
        }
        Ok(())
    }
}

fn referenced_hashes(database: &Database) -> Result<HashSet<String>> {
    database.read(|db| {
        let mut statement = db.prepare(
            "SELECT log_artifact_hash FROM terminal_sessions WHERE log_artifact_hash IS NOT NULL
             UNION SELECT output_artifact_hash FROM commands WHERE output_artifact_hash IS NOT NULL
             UNION SELECT patch_artifact_hash FROM git_checkpoints WHERE patch_artifact_hash IS NOT NULL
             UNION SELECT before_artifact_hash FROM file_changes WHERE before_artifact_hash IS NOT NULL
             UNION SELECT after_artifact_hash FROM file_changes WHERE after_artifact_hash IS NOT NULL
             UNION SELECT artifact_hash FROM screenshots
             UNION SELECT report_artifact_hash FROM test_runs WHERE report_artifact_hash IS NOT NULL
             UNION SELECT artifact_hash FROM diagnostics WHERE artifact_hash IS NOT NULL
             UNION SELECT artifact_hash FROM verification_artifacts
             UNION SELECT log_artifact_hash FROM dev_server_instances WHERE log_artifact_hash IS NOT NULL",
        )?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()
    })
}

fn hash_from_path(path: &Path) -> Option<String> {
    let shard = path.parent()?.file_name()?.to_str()?;
    let rest = path.file_name()?.to_str()?;
    Some(format!("{shard}{rest}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn stores_deduplicates_reads_and_detects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let first = store.store_bytes(b"retcon").unwrap();
        let second = store.store_bytes(b"retcon").unwrap();
        assert_eq!(first, second);
        assert_eq!(store.disk_usage().unwrap(), 6);
        let mut bytes = Vec::new();
        store
            .get(&first.hash)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes, b"retcon");
        std::fs::write(store.path_for(&first.hash).unwrap(), b"changed").unwrap();
        assert!(matches!(
            store.verify(&first.hash),
            Err(StorageError::ArtifactIntegrity { .. })
        ));
    }
    #[test]
    fn cleanup_preserves_protected_content() {
        let dir = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let artifact = store.store_bytes(b"active").unwrap();
        let report = store
            .cleanup(Duration::ZERO, &HashSet::from([artifact.hash.clone()]))
            .unwrap();
        assert_eq!(report.retained_files, 1);
        assert!(store.verify(&artifact.hash).is_ok());
    }

    #[test]
    fn database_references_are_always_protected_from_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let store = ArtifactStore::open(dir.path()).unwrap();
        let db = Database::open_in_memory().unwrap();
        let artifact = store.store_bytes(b"terminal log").unwrap();
        let terminal_id = uuid::Uuid::new_v4();
        db.execute("INSERT INTO terminal_sessions (id,status,shell,cwd,started_at,ended_at,log_artifact_hash) VALUES (?1,'ended','pwsh','.',1,2,?2)", &[&terminal_id.as_bytes(), &artifact.hash]).unwrap();
        let report = store.cleanup_referenced(&db, Duration::ZERO).unwrap();
        assert_eq!(report.retained_files, 1);
        assert!(store.verify(&artifact.hash).is_ok());
    }
}
