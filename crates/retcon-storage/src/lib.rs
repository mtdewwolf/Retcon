//! Durable SQLite persistence and content-addressed artifact storage for Retcon.

mod artifacts;
mod database;
mod dev_servers;
mod error;
mod migrations;
mod recovery;
mod repositories;
mod storage;
mod task_planning;
mod verification;

pub use artifacts::{Artifact, ArtifactCleanup, ArtifactStore};
pub use database::{Database, IntegrityReport, MaintenanceReport};
pub use dev_servers::{
    DevServerConfig, DevServerEvent, DevServerInstance, DevServerLaunchConfig, DevServerPortLease,
    DevServerRepository, NewDevServerConfig,
};
pub use error::{Result, StorageError};
pub use recovery::RecoveryReport;
pub use repositories::{
    Approval, ApprovalRepository, BrowserSession, BrowserSessionRepository, CommandRecord,
    CommandRepository, FileChange, FileChangeRepository, GitCheckpoint, GitCheckpointRepository,
    GitWorktree, GitWorktreeRepository, Layout, LayoutRepository, Message, MessageRepository,
    NewApproval, NewBrowserSession, NewCommand, NewFileChange, NewGitCheckpoint, NewGitWorktree,
    NewLayout, NewMessage, NewPermissionRule, NewProject, NewSession, NewTask, NewTerminalSession,
    NewToolCall, NewTurn, PermissionRule, PermissionRuleRepository, Project, ProjectRepository,
    Session, SessionRepository, Setting, SettingsRepository, Task, TaskRepository, TerminalSession,
    TerminalSessionRepository, ToolCall, ToolCallRepository, Turn, TurnRepository,
};
pub use storage::{
    RecoverAction, RecoverOptions, Storage, StorageRecoverReport, StorageStatusReport,
};
pub use task_planning::{
    AcceptanceCriterion, AcceptanceCriterionEvent, CompletionBlockers, NewAcceptanceCriterion,
    PlanStepDraft, TaskDetails, TaskMutation, TaskPatch, TaskPlanningRepository, TaskStep,
};
pub use verification::{
    CompletionReport, NewVerificationArtifact, NewVerificationCommand, NewVerificationTestResult,
    VerificationArtifactRef, VerificationCommand, VerificationEvent, VerificationGate,
    VerificationMutation, VerificationRepository, VerificationRun, VerificationRunDetails,
    VerificationTestResult,
};
