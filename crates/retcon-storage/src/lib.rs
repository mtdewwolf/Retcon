//! Durable SQLite persistence and content-addressed artifact storage for Retcon.

mod artifacts;
mod browser;
mod browser_verification;
mod database;
mod dev_servers;
mod diagnostics;
mod error;
mod migrations;
mod recovery;
mod repositories;
mod storage;
mod task_planning;
mod verification;

pub use artifacts::{Artifact, ArtifactCleanup, ArtifactStore};
pub use browser::{
    BrowserHistoryEvent, BrowserObservation, BrowserProfile, BrowserRepository, BrowserTab,
    BrowserTakeover, DurableBrowserSession, NewDurableBrowserSession,
};
pub use browser_verification::{
    BrowserAccessibilityFinding, BrowserConsoleEvidence, BrowserNetworkEvidence,
    BrowserVerificationArtifact, BrowserVerificationBaseline, BrowserVerificationDefinition,
    BrowserVerificationDetails, BrowserVerificationEvent, BrowserVerificationMutation,
    BrowserVerificationOutcome, BrowserVerificationRepository, BrowserVerificationRun,
    BrowserVerificationVariant, BrowserVisualComparison, NewAccessibilityFinding,
    NewBrowserAssertionResult, NewBrowserVerificationArtifact, NewBrowserVerificationDefinition,
    NewBrowserVerificationEvent, NewBrowserVerificationRun, NewBrowserVerificationVariant,
    NewConsoleEvidence, NewNetworkEvidence, NewVisualComparison,
};
pub use database::{Database, IntegrityReport, MaintenanceReport};
pub use dev_servers::{
    DevServerConfig, DevServerEvent, DevServerInstance, DevServerLaunchConfig, DevServerPortLease,
    DevServerRepository, NewDevServerConfig,
};
pub use diagnostics::{
    DiagnosticLog, DiagnosticLogQuery, DiagnosticsDeletion, DiagnosticsPort, DiagnosticsRepository,
    DiagnosticsResources, MAX_LOCAL_METRICS, NewDiagnosticLog, NewPerformanceMetric,
    PerformanceMetric, PerformanceMetricQuery,
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
