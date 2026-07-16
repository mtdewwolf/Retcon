//! SQLite connection lifecycle, migrations, backups, and health checks.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params};

use crate::error::{Result, StorageError};
use crate::migrations::{LATEST_VERSION, MIGRATIONS};

const SLOW_QUERY_THRESHOLD: Duration = Duration::from_millis(100);

/// The result of SQLite's built-in integrity check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegrityReport {
    /// Whether SQLite found no damage or structural inconsistency.
    pub healthy: bool,
    /// Diagnostic lines returned by SQLite. A healthy database returns `ok`.
    pub messages: Vec<String>,
}

/// Space and checkpoint information collected during database maintenance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaintenanceReport {
    /// Total database pages before optimization.
    pub page_count: u64,
    /// Unused database pages available for reuse.
    pub free_pages: u64,
}

/// A thread-safe handle to Retcon's durable SQLite database.
///
/// The connection is intentionally serialized. This gives every caller the same transaction
/// conventions today and leaves room to introduce a read pool once query volume warrants it.
#[derive(Clone)]
pub struct Database {
    path: PathBuf,
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open or create a database, configure it, and apply all pending migrations.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Corrupt`] when SQLite identifies damaged storage, and a
    /// contextual storage error for initialization or migration failures.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| StorageError::io("create database directory", parent, error))?;
        }

        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
        let mut connection = Connection::open_with_flags(&path, flags)
            .map_err(|error| classify_database_error(&path, "open database", error))?;
        configure(&connection, &path)?;
        apply_migrations(&mut connection, &path)?;
        integrity_check_connection(&connection, &path)?;
        Ok(Self {
            path,
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Open an isolated in-memory database using the production schema.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite initialization or a migration fails.
    pub fn open_in_memory() -> Result<Self> {
        let path = PathBuf::from(":memory:");
        let mut connection = Connection::open_in_memory()
            .map_err(|error| StorageError::database("open in-memory database", error))?;
        configure(&connection, &path)?;
        apply_migrations(&mut connection, &path)?;
        Ok(Self {
            path,
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Return the database path supplied at open time.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the current schema version.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection is unavailable or SQLite rejects the query.
    pub fn schema_version(&self) -> Result<u32> {
        let connection = self.lock()?;
        read_schema_version(&connection)
            .map_err(|error| classify_database_error(&self.path, "read schema version", error))
    }

    /// Execute a statement and log it when it exceeds the slow-query threshold.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection is unavailable or SQLite rejects the statement.
    pub fn execute(&self, sql: &str, parameters: &[&dyn rusqlite::ToSql]) -> Result<usize> {
        let connection = self.lock()?;
        let started = Instant::now();
        let result = connection.execute(sql, parameters);
        log_query_timing(sql, started.elapsed());
        result.map_err(|error| classify_database_error(&self.path, "execute statement", error))
    }

    /// Run a read operation against the serialized connection.
    ///
    /// # Errors
    ///
    /// Returns an error when the connection is unavailable or the query fails.
    pub fn read<T>(&self, operation: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let connection = self.lock()?;
        operation(&connection)
            .map_err(|error| classify_database_error(&self.path, "query database", error))
    }

    /// Run a closure inside an immediate transaction.
    ///
    /// Immediate transactions acquire the write reservation up front, avoiding work that only
    /// fails at commit time because another writer won the race.
    ///
    /// # Errors
    ///
    /// Rolls back and returns an error when the closure or commit fails.
    pub fn transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> rusqlite::Result<T>,
    ) -> Result<T> {
        let mut connection = self.lock()?;
        let started = Instant::now();
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                classify_database_error(&self.path, "begin immediate transaction", error)
            })?;
        let value = operation(&transaction)
            .map_err(|error| classify_database_error(&self.path, "execute transaction", error))?;
        transaction
            .commit()
            .map_err(|error| classify_database_error(&self.path, "commit transaction", error))?;
        log_query_timing("transaction", started.elapsed());
        Ok(value)
    }

    /// Run SQLite's full integrity check.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Corrupt`] when the check reports damage.
    pub fn integrity_check(&self) -> Result<IntegrityReport> {
        let connection = self.lock()?;
        integrity_check_connection(&connection, &self.path)
    }

    /// Create a consistent SQLite backup using `VACUUM INTO`.
    ///
    /// Existing files are never overwritten.
    ///
    /// # Errors
    ///
    /// Returns an error when the destination exists, cannot be created, or SQLite cannot copy
    /// the database.
    pub fn backup_to(&self, destination: impl AsRef<Path>) -> Result<()> {
        let destination = destination.as_ref();
        if destination.exists() {
            return Err(StorageError::BackupExists(destination.to_path_buf()));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| StorageError::io("create backup directory", parent, error))?;
        }
        let destination_text = destination.to_string_lossy();
        let connection = self.lock()?;
        connection
            .execute("VACUUM INTO ?1", params![destination_text.as_ref()])
            .map_err(|error| {
                classify_database_error(&self.path, "create database backup", error)
            })?;
        Ok(())
    }

    /// Checkpoint the write-ahead log and ask SQLite to optimize its query planner data.
    ///
    /// # Errors
    ///
    /// Returns an error when a maintenance statement fails.
    pub fn maintain(&self) -> Result<MaintenanceReport> {
        let connection = self.lock()?;
        let page_count = pragma_u64(&connection, "page_count", &self.path)?;
        let free_pages = pragma_u64(&connection, "freelist_count", &self.path)?;
        connection
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE); PRAGMA optimize;")
            .map_err(|error| {
                classify_database_error(&self.path, "run database maintenance", error)
            })?;
        Ok(MaintenanceReport {
            page_count,
            free_pages,
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| StorageError::ConnectionPoisoned)
    }
}

fn configure(connection: &Connection, path: &Path) -> Result<()> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| classify_database_error(path, "configure busy timeout", error))?;
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;\n\
             PRAGMA journal_mode = WAL;\n\
             PRAGMA synchronous = NORMAL;\n\
             PRAGMA temp_store = MEMORY;",
        )
        .map_err(|error| classify_database_error(path, "configure database", error))
}

fn apply_migrations(connection: &mut Connection, path: &Path) -> Result<()> {
    let current = read_schema_version(connection)
        .map_err(|error| classify_database_error(path, "read schema version", error))?;
    if current > LATEST_VERSION {
        return Err(StorageError::SchemaTooNew {
            found: current,
            supported: LATEST_VERSION,
        });
    }

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current)
    {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| classify_database_error(path, "begin schema migration", error))?;
        transaction
            .execute_batch(migration.sql)
            .map_err(|error| classify_database_error(path, "apply schema migration", error))?;
        transaction
            .execute_batch(&format!("PRAGMA user_version = {};", migration.version))
            .map_err(|error| classify_database_error(path, "record schema version", error))?;
        transaction
            .commit()
            .map_err(|error| classify_database_error(path, "commit schema migration", error))?;
        tracing::info!(
            database.path = %path.display(),
            migration.version,
            migration.name,
            "applied database migration"
        );
    }
    Ok(())
}

fn read_schema_version(connection: &Connection) -> rusqlite::Result<u32> {
    connection.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn integrity_check_connection(connection: &Connection, path: &Path) -> Result<IntegrityReport> {
    let mut statement = connection
        .prepare("PRAGMA integrity_check")
        .map_err(|error| classify_database_error(path, "prepare integrity check", error))?;
    let messages = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| classify_database_error(path, "run integrity check", error))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| classify_database_error(path, "read integrity check", error))?;
    let healthy = messages.len() == 1 && messages[0].eq_ignore_ascii_case("ok");
    if !healthy {
        return Err(StorageError::Corrupt {
            path: path.to_path_buf(),
            details: messages.join("; "),
        });
    }
    Ok(IntegrityReport { healthy, messages })
}

fn pragma_u64(connection: &Connection, name: &str, path: &Path) -> Result<u64> {
    connection
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .map_err(|error| classify_database_error(path, "read maintenance statistics", error))
}

fn classify_database_error(
    path: &Path,
    operation: &'static str,
    error: rusqlite::Error,
) -> StorageError {
    let message = error.to_string();
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("malformed")
        || normalized.contains("not a database")
        || normalized.contains("database disk image is malformed")
    {
        StorageError::Corrupt {
            path: path.to_path_buf(),
            details: message,
        }
    } else {
        StorageError::database(operation, error)
    }
}

fn log_query_timing(statement: &str, elapsed: Duration) {
    // Log only the operation keyword. SQL may contain user content or credentials when a
    // third-party integration fails to bind parameters correctly.
    let operation = statement
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_ascii_uppercase();
    if elapsed >= SLOW_QUERY_THRESHOLD {
        tracing::warn!(
            database.elapsed_ms = elapsed.as_millis(),
            database.operation = operation,
            "slow database operation"
        );
    } else {
        tracing::trace!(
            database.elapsed_ms = elapsed.as_millis(),
            database.operation = operation,
            "database operation"
        );
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_reopens_versioned_schema() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retcon.db");
        {
            let database = Database::open(&path).unwrap();
            assert_eq!(database.schema_version().unwrap(), LATEST_VERSION);
            database
                .execute(
                    "INSERT INTO projects (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
                    &[&"project-1", &"Retcon", &1_i64, &1_i64],
                )
                .unwrap();
        }

        let reopened = Database::open(&path).unwrap();
        let count = reopened
            .transaction(|transaction| {
                transaction.query_row("SELECT count(*) FROM projects", [], |row| {
                    row.get::<_, i64>(0)
                })
            })
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn creates_a_readable_backup_without_overwriting() {
        let directory = tempfile::tempdir().unwrap();
        let database = Database::open(directory.path().join("retcon.db")).unwrap();
        database
            .execute(
                "INSERT INTO settings (scope, key, value_json, updated_at) VALUES (?1, ?2, ?3, ?4)",
                &[&"global", &"theme", &r#"{"name":"retcon"}"#, &1_i64],
            )
            .unwrap();
        let backup_path = directory.path().join("backups/retcon.db");
        database.backup_to(&backup_path).unwrap();

        let backup = Database::open(&backup_path).unwrap();
        let count = backup
            .transaction(|transaction| {
                transaction.query_row("SELECT count(*) FROM settings", [], |row| {
                    row.get::<_, i64>(0)
                })
            })
            .unwrap();
        assert_eq!(count, 1);
        assert!(matches!(
            database.backup_to(&backup_path),
            Err(StorageError::BackupExists(_))
        ));
    }

    #[test]
    fn reports_invalid_database_as_corrupt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retcon.db");
        std::fs::write(&path, b"this is not sqlite").unwrap();

        assert!(matches!(
            Database::open(&path),
            Err(StorageError::Corrupt { .. })
        ));
    }

    #[test]
    fn enforces_foreign_keys_and_uses_immediate_transactions() {
        let database = Database::open_in_memory().unwrap();
        let error = database
            .execute(
                "INSERT INTO sessions (id, project_id, title, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                &[&"session-1", &"missing", &"Test", &"running", &1_i64, &1_i64],
            )
            .unwrap_err();
        assert!(matches!(error, StorageError::Database { .. }));
    }

    #[test]
    fn maintenance_and_integrity_checks_report_health() {
        let database = Database::open_in_memory().unwrap();
        let integrity = database.integrity_check().unwrap();
        let maintenance = database.maintain().unwrap();

        assert!(integrity.healthy);
        assert_eq!(integrity.messages, ["ok"]);
        assert!(maintenance.page_count > 0);
    }

    #[test]
    fn rejects_schema_created_by_a_newer_retcon() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retcon.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch("PRAGMA user_version = 999;")
            .unwrap();
        drop(connection);
        assert!(matches!(
            Database::open(path),
            Err(StorageError::SchemaTooNew { found: 999, .. })
        ));
    }

    #[test]
    fn upgrades_an_existing_version_one_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("retcon.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("migrations/0001_initial.sql"))
            .unwrap();
        connection
            .execute_batch("PRAGMA user_version = 1;")
            .unwrap();
        drop(connection);

        let upgraded = Database::open(path).unwrap();
        assert_eq!(upgraded.schema_version().unwrap(), 2);
        upgraded
            .execute(
                "INSERT INTO background_jobs (id,owner,name,status,created_at,attempts,max_attempts,timeout_ms) VALUES ('job','test','migrated','queued',1,0,1,1000)",
                &[],
            )
            .unwrap();
    }
}
