//! Durable SQLite persistence and content-addressed artifact storage for Retcon.

mod artifacts;
mod database;
mod error;
mod migrations;
mod recovery;
mod repositories;

pub use artifacts::{Artifact, ArtifactCleanup, ArtifactStore};
pub use database::{Database, IntegrityReport, MaintenanceReport};
pub use error::{Result, StorageError};
pub use recovery::RecoveryReport;
pub use repositories::{
    NewProject, NewSession, NewTask, NewTurn, Project, ProjectRepository, Session,
    SessionRepository, Setting, SettingsRepository, Task, TaskRepository, Turn, TurnRepository,
};
