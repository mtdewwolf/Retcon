#![allow(missing_docs)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::artifacts::ArtifactStore;
use crate::database::{Database, IntegrityReport, MaintenanceReport};
use crate::error::{Result, StorageError};
use crate::recovery::RecoveryReport;

/// How [`Storage::recover_offline`] should respond to database problems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoverAction {
    /// Run integrity checks and return diagnostics without modifying files.
    Report,
    /// Copy the database to a timestamped backup path.
    Backup,
    /// Attempt lightweight SQLite repair (`REINDEX`) when the database opens.
    Repair,
    /// Move the damaged database aside and create a fresh empty schema.
    Reset,
}

/// Options for an online recovery request against a running database.
#[derive(Clone, Debug)]
pub struct RecoverOptions {
    pub action: RecoverAction,
    pub backup_path: Option<PathBuf>,
}

/// Health and maintenance snapshot returned by [`Storage::status`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StorageStatusReport {
    pub healthy: bool,
    pub database_path: PathBuf,
    pub schema_version: Option<u32>,
    pub integrity: IntegrityReport,
    pub maintenance: Option<MaintenanceReport>,
    pub startup_recovery: RecoveryReport,
    pub artifact_bytes: Option<u64>,
}

/// Outcome of a recovery attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StorageRecoverReport {
    pub action: String,
    pub healthy: bool,
    pub backup_path: Option<PathBuf>,
    pub integrity_messages: Vec<String>,
    pub notes: Vec<String>,
}

/// SQLite persistence and CAS artifacts for a Retcon data directory.
#[derive(Clone)]
pub struct Storage {
    data_dir: PathBuf,
    database: Database,
    artifacts: ArtifactStore,
    startup_recovery: RecoveryReport,
}

impl Storage {
    /// Open or create storage under `data_dir`, migrate schema, and reconcile interrupted work.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir)
            .map_err(|error| StorageError::io("create data directory", &data_dir, error))?;
        let database = Database::open(data_dir.join("retcon.db"))?;
        let startup_recovery = database.recover_interrupted()?;
        let artifacts = ArtifactStore::open(&data_dir)?;
        Ok(Self {
            data_dir,
            database,
            artifacts,
            startup_recovery,
        })
    }

    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    #[must_use]
    pub fn database(&self) -> &Database {
        &self.database
    }

    #[must_use]
    pub fn artifacts(&self) -> &ArtifactStore {
        &self.artifacts
    }

    #[must_use]
    pub fn startup_recovery(&self) -> &RecoveryReport {
        &self.startup_recovery
    }

    /// Collect integrity, schema, and artifact usage for RPC diagnostics.
    pub fn status(&self) -> Result<StorageStatusReport> {
        let integrity = self.database.inspect_integrity()?;
        let schema_version = self.database.schema_version().ok();
        let maintenance = self.database.maintain().ok();
        let artifact_bytes = self.artifacts.disk_usage().ok();
        Ok(StorageStatusReport {
            healthy: integrity.healthy,
            database_path: self.database.path().to_path_buf(),
            schema_version,
            integrity,
            maintenance,
            startup_recovery: self.startup_recovery.clone(),
            artifact_bytes,
        })
    }

    /// Run an online recovery action against the open database.
    pub fn recover(&self, options: RecoverOptions) -> Result<StorageRecoverReport> {
        recover_at_path(&self.data_dir, options.action, options.backup_path)
    }

    /// Attempt recovery without requiring a healthy open database handle.
    pub fn recover_offline(
        data_dir: impl AsRef<Path>,
        action: RecoverAction,
        backup_path: Option<PathBuf>,
    ) -> Result<StorageRecoverReport> {
        recover_at_path(data_dir.as_ref(), action, backup_path)
    }
}

fn recover_at_path(
    data_dir: &Path,
    action: RecoverAction,
    backup_path: Option<PathBuf>,
) -> Result<StorageRecoverReport> {
    let db_path = data_dir.join("retcon.db");
    let mut notes = Vec::new();
    let action_name = match action {
        RecoverAction::Report => "report",
        RecoverAction::Backup => "backup",
        RecoverAction::Repair => "repair",
        RecoverAction::Reset => "reset",
    }
    .to_owned();

    match action {
        RecoverAction::Report => {
            let integrity = inspect_path(&db_path)?;
            Ok(StorageRecoverReport {
                action: action_name,
                healthy: integrity.healthy,
                backup_path: None,
                integrity_messages: integrity.messages,
                notes,
            })
        }
        RecoverAction::Backup => {
            let destination = backup_path.unwrap_or_else(|| default_backup_path(data_dir));
            if db_path.exists() {
                if Database::open(&db_path).is_ok() {
                    let database = Database::open(&db_path)?;
                    database.backup_to(&destination)?;
                    notes.push("created consistent sqlite backup".into());
                } else {
                    std::fs::copy(&db_path, &destination).map_err(|error| {
                        StorageError::io("copy damaged database backup", &destination, error)
                    })?;
                    notes.push(
                        "copied raw database bytes because sqlite could not open the file".into(),
                    );
                }
            } else {
                notes.push("database file does not exist; nothing to back up".into());
            }
            let integrity = inspect_path(&db_path)?;
            Ok(StorageRecoverReport {
                action: action_name,
                healthy: integrity.healthy,
                backup_path: Some(destination),
                integrity_messages: integrity.messages,
                notes,
            })
        }
        RecoverAction::Repair => {
            let database = Database::open(&db_path)?;
            database.execute("REINDEX", &[])?;
            notes.push("rebuilt sqlite indexes".into());
            let integrity = database.inspect_integrity()?;
            Ok(StorageRecoverReport {
                action: action_name,
                healthy: integrity.healthy,
                backup_path: None,
                integrity_messages: integrity.messages,
                notes,
            })
        }
        RecoverAction::Reset => {
            if db_path.exists() {
                let quarantine = data_dir.join(format!("retcon.db.corrupt-{}", timestamp_suffix()));
                std::fs::rename(&db_path, &quarantine).map_err(|error| {
                    StorageError::io("quarantine damaged database", &quarantine, error)
                })?;
                notes.push(format!(
                    "moved damaged database to {}",
                    quarantine.display()
                ));
            }
            let _fresh = Database::open(&db_path)?;
            notes.push("created fresh database schema".into());
            let integrity = inspect_path(&db_path)?;
            Ok(StorageRecoverReport {
                action: action_name,
                healthy: integrity.healthy,
                backup_path: None,
                integrity_messages: integrity.messages,
                notes,
            })
        }
    }
}

fn inspect_path(db_path: &Path) -> Result<IntegrityReport> {
    if !db_path.exists() {
        return Ok(IntegrityReport {
            healthy: false,
            messages: vec!["database file does not exist".into()],
        });
    }
    match Database::open(db_path) {
        Ok(database) => database.inspect_integrity(),
        Err(StorageError::Corrupt { details, .. }) => Ok(IntegrityReport {
            healthy: false,
            messages: vec![details],
        }),
        Err(error) => Err(error),
    }
}

fn default_backup_path(data_dir: &Path) -> PathBuf {
    data_dir.join(format!("backups/retcon-{}.db", timestamp_suffix()))
}

fn timestamp_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn reset_quarantines_corrupt_database_and_recreates_schema() {
        let directory = tempfile::tempdir().unwrap();
        let db_path = directory.path().join("retcon.db");
        std::fs::write(&db_path, b"not sqlite").unwrap();
        let report = Storage::recover_offline(directory.path(), RecoverAction::Reset, None).unwrap();
        assert_eq!(report.action, "reset");
        assert!(report.healthy);
        assert!(db_path.exists());
        assert!(Database::open(&db_path).is_ok());
    }
}
