//! Durable SQLite persistence and content-addressed artifact storage for Retcon.

mod artifacts;
mod database;
mod error;
mod migrations;
mod recovery;
mod repositories;
mod storage;

pub use artifacts::{Artifact, ArtifactCleanup, ArtifactStore};
pub use database::{Database, IntegrityReport, MaintenanceReport};
pub use error::{Result, StorageError};
pub use recovery::RecoveryReport;
pub use repositories::{
    Approval, ApprovalRepository, BrowserSession, BrowserSessionRepository, CommandRecord,
    CommandRepository, FileChange, FileChangeRepository, GitCheckpoint, GitCheckpointRepository,
    GitWorktree, GitWorktreeRepository, Layout, LayoutRepository, Message, MessageRepository,
    NewApproval, NewBrowserSession, NewCommand, NewFileChange, NewGitCheckpoint, NewGitWorktree,
    NewLayout, NewMessage, NewPermissionRule, NewProject, NewSession, NewTask, NewTerminalSession,
    NewToolCall, NewTurn, PermissionRule, PermissionRuleRepository, Project, ProjectRepository,
    Session, SessionRepository, Setting, SettingsRepository, Task, TaskRepository,
    TerminalSession, TerminalSessionRepository, ToolCall, ToolCallRepository, Turn, TurnRepository,
};
pub use storage::{
    RecoverAction, RecoverOptions, Storage, StorageRecoverReport, StorageStatusReport,
};
