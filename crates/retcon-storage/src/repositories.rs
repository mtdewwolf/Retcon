//! Typed repositories for the durable state used by the session engine and desktop shell.

#![allow(missing_docs)]

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;
use uuid::Uuid;

use crate::{Database, Result, StorageError};

/// A Retcon project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Values used to create a project.
#[derive(Clone, Debug)]
pub struct NewProject {
    pub id: Uuid,
    pub name: String,
}

impl NewProject {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
        }
    }
}

/// A durable agent session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Values used to create a session.
#[derive(Clone, Debug)]
pub struct NewSession {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub status: String,
}

impl NewSession {
    #[must_use]
    pub fn new(project_id: Uuid, title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            title: title.into(),
            status: "created".into(),
        }
    }
}

/// A persisted agent turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub id: Uuid,
    pub session_id: Uuid,
    pub sequence: i64,
    pub status: String,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

/// Values used to create a turn.
#[derive(Clone, Debug)]
pub struct NewTurn {
    pub id: Uuid,
    pub session_id: Uuid,
    pub sequence: i64,
    pub status: String,
}

impl NewTurn {
    #[must_use]
    pub fn new(session_id: Uuid, sequence: i64) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            sequence,
            status: "queued".into(),
        }
    }
}

/// A persisted task and its current progress state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    pub worktree_id: Option<Uuid>,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub agent: Option<String>,
    pub provider: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: i64,
    pub estimated_cost_micros: Option<i64>,
    pub actual_cost_micros: Option<i64>,
    pub cost_currency: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

/// Values used to create a task.
#[derive(Clone, Debug)]
pub struct NewTask {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub parent_task_id: Option<Uuid>,
    pub worktree_id: Option<Uuid>,
    pub branch: Option<String>,
    pub worktree_path: Option<String>,
    pub agent: Option<String>,
    pub provider: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub priority: i64,
    pub estimated_cost_micros: Option<i64>,
    pub actual_cost_micros: Option<i64>,
    pub cost_currency: String,
}

impl NewTask {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: None,
            project_id: None,
            parent_task_id: None,
            worktree_id: None,
            branch: None,
            worktree_path: None,
            agent: None,
            provider: None,
            title: title.into(),
            description: None,
            status: "pending".into(),
            priority: 0,
            estimated_cost_micros: None,
            actual_cost_micros: None,
            cost_currency: "USD".into(),
        }
    }
}

/// A JSON setting scoped to the application, a project, or a workspace.
#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub scope: String,
    pub key: String,
    pub value: Value,
    pub updated_at: i64,
}

/// A conversation message within a turn.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: Uuid,
    pub turn_id: Uuid,
    pub sequence: i64,
    pub role: String,
    pub content: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewMessage {
    pub id: Uuid,
    pub turn_id: Uuid,
    pub sequence: i64,
    pub role: String,
    pub content: Value,
}

impl NewMessage {
    #[must_use]
    pub fn new(turn_id: Uuid, sequence: i64, role: impl Into<String>, content: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            turn_id,
            sequence,
            role: role.into(),
            content,
        }
    }
}

/// A provider tool invocation within a turn.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: Uuid,
    pub turn_id: Uuid,
    pub name: String,
    pub status: String,
    pub input: Value,
    pub output: Option<Value>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NewToolCall {
    pub id: Uuid,
    pub turn_id: Uuid,
    pub name: String,
    pub status: String,
    pub input: Value,
}

impl NewToolCall {
    #[must_use]
    pub fn new(turn_id: Uuid, name: impl Into<String>, input: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            turn_id,
            name: name.into(),
            status: "running".into(),
            input,
        }
    }
}

/// A supervised terminal session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSession {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub status: String,
    pub shell: String,
    pub cwd: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub log_artifact_hash: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewTerminalSession {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub status: String,
    pub shell: String,
    pub cwd: String,
    pub log_artifact_hash: Option<String>,
}

impl NewTerminalSession {
    #[must_use]
    pub fn new(shell: impl Into<String>, cwd: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: None,
            status: "starting".into(),
            shell: shell.into(),
            cwd: cwd.into(),
            log_artifact_hash: None,
        }
    }
}

/// A command executed inside a terminal session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandRecord {
    pub id: Uuid,
    pub terminal_session_id: Uuid,
    pub command: String,
    pub cwd: String,
    pub exit_code: Option<i64>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub output_artifact_hash: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewCommand {
    pub id: Uuid,
    pub terminal_session_id: Uuid,
    pub command: String,
    pub cwd: String,
    pub output_artifact_hash: Option<String>,
}

impl NewCommand {
    #[must_use]
    pub fn new(
        terminal_session_id: Uuid,
        command: impl Into<String>,
        cwd: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            terminal_session_id,
            command: command.into(),
            cwd: cwd.into(),
            output_artifact_hash: None,
        }
    }
}

/// A Git worktree tracked for an agent session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitWorktree {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub repository_location_id: Uuid,
    pub path: String,
    pub branch: Option<String>,
    pub head_oid: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NewGitWorktree {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub repository_location_id: Uuid,
    pub path: String,
    pub branch: Option<String>,
    pub head_oid: Option<String>,
    pub status: String,
}

impl NewGitWorktree {
    #[must_use]
    pub fn new(repository_location_id: Uuid, path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: None,
            repository_location_id,
            path: path.into(),
            branch: None,
            head_oid: None,
            status: "active".into(),
        }
    }
}

/// A Git checkpoint referencing a patch artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCheckpoint {
    pub id: Uuid,
    pub git_worktree_id: Uuid,
    pub turn_id: Option<Uuid>,
    pub kind: String,
    pub base_oid: Option<String>,
    pub patch_artifact_hash: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewGitCheckpoint {
    pub id: Uuid,
    pub git_worktree_id: Uuid,
    pub turn_id: Option<Uuid>,
    pub kind: String,
    pub base_oid: Option<String>,
    pub patch_artifact_hash: Option<String>,
}

impl NewGitCheckpoint {
    #[must_use]
    pub fn new(git_worktree_id: Uuid, kind: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            git_worktree_id,
            turn_id: None,
            kind: kind.into(),
            base_oid: None,
            patch_artifact_hash: None,
        }
    }
}

/// A file change recorded for a turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub id: Uuid,
    pub turn_id: Option<Uuid>,
    pub git_checkpoint_id: Option<Uuid>,
    pub path: String,
    pub change_kind: String,
    pub before_artifact_hash: Option<String>,
    pub after_artifact_hash: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewFileChange {
    pub id: Uuid,
    pub turn_id: Option<Uuid>,
    pub git_checkpoint_id: Option<Uuid>,
    pub path: String,
    pub change_kind: String,
    pub before_artifact_hash: Option<String>,
    pub after_artifact_hash: Option<String>,
}

impl NewFileChange {
    #[must_use]
    pub fn new(path: impl Into<String>, change_kind: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            turn_id: None,
            git_checkpoint_id: None,
            path: path.into(),
            change_kind: change_kind.into(),
            before_artifact_hash: None,
            after_artifact_hash: None,
        }
    }
}

/// A user approval request for a tool call.
#[derive(Clone, Debug, PartialEq)]
pub struct Approval {
    pub id: Uuid,
    pub session_id: Uuid,
    pub tool_call_id: Option<Uuid>,
    pub status: String,
    pub request: Value,
    pub decision: Option<Value>,
    pub requested_at: i64,
    pub decided_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NewApproval {
    pub id: Uuid,
    pub session_id: Uuid,
    pub tool_call_id: Option<Uuid>,
    pub status: String,
    pub request: Value,
}

impl NewApproval {
    #[must_use]
    pub fn new(session_id: Uuid, request: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            tool_call_id: None,
            status: "pending".into(),
            request,
        }
    }
}

/// A permission rule scoped to a project or globally.
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionRule {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub scope: String,
    pub effect: String,
    pub matcher: Value,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NewPermissionRule {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub scope: String,
    pub effect: String,
    pub matcher: Value,
    pub expires_at: Option<i64>,
}

impl NewPermissionRule {
    #[must_use]
    pub fn new(scope: impl Into<String>, effect: impl Into<String>, matcher: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id: None,
            scope: scope.into(),
            effect: effect.into(),
            matcher,
            expires_at: None,
        }
    }
}

/// A persisted workspace layout preset.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub id: String,
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub layout: Value,
    pub is_active: bool,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewLayout {
    pub id: String,
    pub workspace_id: Option<Uuid>,
    pub name: String,
    pub layout: Value,
    pub is_active: bool,
}

impl NewLayout {
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, layout: Value) -> Self {
        Self {
            id: id.into(),
            workspace_id: None,
            name: name.into(),
            layout,
            is_active: true,
        }
    }
}

/// A browser automation session.
#[derive(Clone, Debug, PartialEq)]
pub struct BrowserSession {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub status: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub metadata: Value,
}

#[derive(Clone, Debug)]
pub struct NewBrowserSession {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub status: String,
    pub metadata: Value,
}

impl Default for NewBrowserSession {
    fn default() -> Self {
        Self::new()
    }
}

impl NewBrowserSession {
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: None,
            status: "starting".into(),
            metadata: Value::Object(Default::default()),
        }
    }
}

pub struct ProjectRepository<'a>(&'a Database);
pub struct SessionRepository<'a>(&'a Database);
pub struct TurnRepository<'a>(&'a Database);
pub struct TaskRepository<'a>(&'a Database);
pub struct SettingsRepository<'a>(&'a Database);
pub struct MessageRepository<'a>(&'a Database);
pub struct ToolCallRepository<'a>(&'a Database);
pub struct TerminalSessionRepository<'a>(&'a Database);
pub struct CommandRepository<'a>(&'a Database);
pub struct GitWorktreeRepository<'a>(&'a Database);
pub struct GitCheckpointRepository<'a>(&'a Database);
pub struct FileChangeRepository<'a>(&'a Database);
pub struct ApprovalRepository<'a>(&'a Database);
pub struct PermissionRuleRepository<'a>(&'a Database);
pub struct LayoutRepository<'a>(&'a Database);
pub struct BrowserSessionRepository<'a>(&'a Database);

impl Database {
    #[must_use]
    pub fn projects(&self) -> ProjectRepository<'_> {
        ProjectRepository(self)
    }
    #[must_use]
    pub fn sessions(&self) -> SessionRepository<'_> {
        SessionRepository(self)
    }
    #[must_use]
    pub fn turns(&self) -> TurnRepository<'_> {
        TurnRepository(self)
    }
    #[must_use]
    pub fn tasks(&self) -> TaskRepository<'_> {
        TaskRepository(self)
    }
    #[must_use]
    pub fn settings(&self) -> SettingsRepository<'_> {
        SettingsRepository(self)
    }
    #[must_use]
    pub fn messages(&self) -> MessageRepository<'_> {
        MessageRepository(self)
    }
    #[must_use]
    pub fn tool_calls(&self) -> ToolCallRepository<'_> {
        ToolCallRepository(self)
    }
    #[must_use]
    pub fn terminal_sessions(&self) -> TerminalSessionRepository<'_> {
        TerminalSessionRepository(self)
    }
    #[must_use]
    pub fn commands(&self) -> CommandRepository<'_> {
        CommandRepository(self)
    }
    #[must_use]
    pub fn git_worktrees(&self) -> GitWorktreeRepository<'_> {
        GitWorktreeRepository(self)
    }
    #[must_use]
    pub fn git_checkpoints(&self) -> GitCheckpointRepository<'_> {
        GitCheckpointRepository(self)
    }
    #[must_use]
    pub fn file_changes(&self) -> FileChangeRepository<'_> {
        FileChangeRepository(self)
    }
    #[must_use]
    pub fn approvals(&self) -> ApprovalRepository<'_> {
        ApprovalRepository(self)
    }
    #[must_use]
    pub fn permission_rules(&self) -> PermissionRuleRepository<'_> {
        PermissionRuleRepository(self)
    }
    #[must_use]
    pub fn layouts(&self) -> LayoutRepository<'_> {
        LayoutRepository(self)
    }
    #[must_use]
    pub fn browser_sessions(&self) -> BrowserSessionRepository<'_> {
        BrowserSessionRepository(self)
    }
}

impl ProjectRepository<'_> {
    pub fn create(&self, project: &NewProject) -> Result<Project> {
        let now = now_ms();
        self.0.execute(
            "INSERT INTO projects (id,name,created_at,updated_at) VALUES (?1,?2,?3,?3)",
            &[&project.id.as_bytes(), &project.name, &now],
        )?;
        Ok(Project {
            id: project.id,
            name: project.name.clone(),
            created_at: now,
            updated_at: now,
        })
    }
    pub fn get(&self, id: Uuid) -> Result<Option<Project>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,name,created_at,updated_at FROM projects WHERE id=?1",
                [id.as_bytes()],
                row_project,
            )
            .optional()
        })
    }
    /// List active projects, most recently updated first.
    pub fn list(&self) -> Result<Vec<Project>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,name,created_at,updated_at FROM projects WHERE archived_at IS NULL ORDER BY updated_at DESC",
            )?;
            statement.query_map([], row_project)?.collect()
        })
    }

    /// Find the project owning an exact repository root path.
    pub fn find_by_path(&self, path: &str) -> Result<Option<Project>> {
        self.0.read(|db| db.query_row(
            "SELECT p.id,p.name,p.created_at,p.updated_at FROM projects p JOIN repository_locations r ON r.project_id=p.id WHERE r.path=?1 AND p.archived_at IS NULL",
            [path], row_project).optional())
    }

    /// Resolve a repository location id from its root path.
    pub fn location_id_by_path(&self, path: &str) -> Result<Option<Uuid>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id FROM repository_locations WHERE path=?1",
                [path],
                |row| uuid(row, 0),
            )
            .optional()
        })
    }

    /// Associate a repository location with a project. Reopening an existing path is idempotent.
    pub fn add_location(
        &self,
        project_id: Uuid,
        path: &str,
        remote_url: Option<&str>,
    ) -> Result<()> {
        let now = now_ms();
        let location_id = Uuid::new_v4();
        self.0.execute(
            "INSERT INTO repository_locations (id,project_id,path,remote_url,created_at,last_seen_at) VALUES (?1,?2,?3,?4,?5,?5) ON CONFLICT(path) DO UPDATE SET last_seen_at=excluded.last_seen_at,remote_url=COALESCE(excluded.remote_url,repository_locations.remote_url)",
            &[&location_id.as_bytes(), &project_id.as_bytes(), &path, &remote_url, &now],
        )?;
        self.0.execute(
            "UPDATE projects SET updated_at=?2 WHERE id=?1",
            &[&project_id.as_bytes(), &now],
        )?;
        Ok(())
    }

    /// Mark a project as no longer shown in the recent-project list.
    pub fn archive(&self, id: Uuid) -> Result<bool> {
        let now = now_ms();
        Ok(self.0.execute(
            "UPDATE projects SET archived_at=?2,updated_at=?2 WHERE id=?1",
            &[&id.as_bytes(), &now],
        )? > 0)
    }
}

impl SessionRepository<'_> {
    pub fn create(&self, session: &NewSession) -> Result<Session> {
        let now = now_ms();
        self.0.execute(
            "INSERT INTO sessions (id,project_id,title,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?5)",
            &[&session.id.as_bytes(), &session.project_id.as_bytes(), &session.title, &session.status, &now],
        )?;
        Ok(Session {
            id: session.id,
            project_id: session.project_id,
            title: session.title.clone(),
            status: session.status.clone(),
            created_at: now,
            updated_at: now,
        })
    }
    pub fn get(&self, id: Uuid) -> Result<Option<Session>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,project_id,title,status,created_at,updated_at FROM sessions WHERE id=?1",
                [id.as_bytes()],
                row_session,
            )
            .optional()
        })
    }
    pub fn list_for_project(&self, project_id: Uuid) -> Result<Vec<Session>> {
        self.0.read(|db| {
            let mut statement = db.prepare("SELECT id,project_id,title,status,created_at,updated_at FROM sessions WHERE project_id=?1 ORDER BY updated_at DESC")?;
            statement.query_map([project_id.as_bytes()], row_session)?.collect()
        })
    }
    pub fn set_status(&self, id: Uuid, status: &str) -> Result<bool> {
        let now = now_ms();
        Ok(self.0.execute(
            "UPDATE sessions SET status=?2,updated_at=?3 WHERE id=?1",
            &[&id.as_bytes(), &status, &now],
        )? > 0)
    }
}

impl TurnRepository<'_> {
    pub fn create(&self, turn: &NewTurn) -> Result<Turn> {
        let now = now_ms();
        self.0.execute(
            "INSERT INTO turns (id,session_id,sequence,status,started_at) VALUES (?1,?2,?3,?4,?5)",
            &[
                &turn.id.as_bytes(),
                &turn.session_id.as_bytes(),
                &turn.sequence,
                &turn.status,
                &now,
            ],
        )?;
        Ok(Turn {
            id: turn.id,
            session_id: turn.session_id,
            sequence: turn.sequence,
            status: turn.status.clone(),
            started_at: now,
            completed_at: None,
        })
    }
    pub fn get(&self, id: Uuid) -> Result<Option<Turn>> {
        self.0.read(|db| db.query_row("SELECT id,session_id,sequence,status,started_at,completed_at FROM turns WHERE id=?1", [id.as_bytes()], row_turn).optional())
    }
    pub fn set_status(&self, id: Uuid, status: &str, completed: bool) -> Result<bool> {
        let completed_at = completed.then(now_ms);
        Ok(self.0.execute(
            "UPDATE turns SET status=?2,completed_at=?3 WHERE id=?1",
            &[&id.as_bytes(), &status, &completed_at],
        )? > 0)
    }
    pub fn list_for_session(&self, session_id: Uuid) -> Result<Vec<Turn>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,session_id,sequence,status,started_at,completed_at FROM turns WHERE session_id=?1 ORDER BY sequence ASC",
            )?;
            statement
                .query_map([session_id.as_bytes()], row_turn)?
                .collect()
        })
    }
    pub fn next_sequence(&self, session_id: Uuid) -> Result<i64> {
        self.0.read(|db| {
            db.query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM turns WHERE session_id=?1",
                [session_id.as_bytes()],
                |row| row.get(0),
            )
        })
    }
}

impl TaskRepository<'_> {
    pub fn create(&self, task: &NewTask) -> Result<Task> {
        crate::task_planning::validate_new_task_fields(task)?;
        let now = now_ms();
        let session_id = task.session_id.map(|id| id.as_bytes().to_vec());
        let parent_task_id = task.parent_task_id.map(|id| id.as_bytes().to_vec());
        let worktree_id = task.worktree_id.map(|id| id.as_bytes().to_vec());
        let started_at = (task.status == "in_progress").then_some(now);
        let completed_at = matches!(task.status.as_str(), "completed" | "done").then_some(now);
        let effective_project = crate::task_planning::map_validation(self.0.transaction(|tx| {
            let effective_project = match (task.project_id, task.session_id, task.worktree_id) {
                (Some(project_id), _, _) => Some(project_id),
                (None, Some(session_id), _) => Some(tx.query_row(
                    "SELECT project_id FROM sessions WHERE id=?1",
                    [session_id.as_bytes()],
                    |row| uuid(row, 0),
                )?),
                (None, None, Some(worktree_id)) => Some(tx.query_row(
                    "SELECT r.project_id FROM git_worktrees w JOIN repository_locations r ON r.id=w.repository_location_id WHERE w.id=?1",
                    [worktree_id.as_bytes()],
                    |row| uuid(row, 0),
                )?),
                (None, None, None) => None,
            };
            crate::task_planning::validate_task_relationships(
                tx,
                task.id,
                effective_project,
                task.session_id,
                task.parent_task_id,
                task.worktree_id,
            )?;
            let project_id = effective_project.map(|id| id.as_bytes().to_vec());
            tx.execute("INSERT INTO tasks (id,session_id,project_id,parent_task_id,worktree_id,branch,worktree_path,agent,provider,title,description,status,priority,estimated_cost_micros,actual_cost_micros,cost_currency,created_at,updated_at,started_at,completed_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?17,?18,?19)", params![task.id.as_bytes(), session_id, project_id, parent_task_id, worktree_id, task.branch, task.worktree_path, task.agent, task.provider, task.title, task.description, task.status, task.priority, task.estimated_cost_micros, task.actual_cost_micros, task.cost_currency, now, started_at, completed_at])?;
            Ok(effective_project)
        }))?;
        Ok(Task {
            id: task.id,
            session_id: task.session_id,
            project_id: effective_project,
            parent_task_id: task.parent_task_id,
            worktree_id: task.worktree_id,
            branch: task.branch.clone(),
            worktree_path: task.worktree_path.clone(),
            agent: task.agent.clone(),
            provider: task.provider.clone(),
            title: task.title.clone(),
            description: task.description.clone(),
            status: task.status.clone(),
            priority: task.priority,
            estimated_cost_micros: task.estimated_cost_micros,
            actual_cost_micros: task.actual_cost_micros,
            cost_currency: task.cost_currency.clone(),
            created_at: now,
            updated_at: now,
            started_at,
            completed_at,
        })
    }
    pub fn get(&self, id: Uuid) -> Result<Option<Task>> {
        self.0.read(|db| db.query_row("SELECT id,session_id,project_id,parent_task_id,worktree_id,branch,worktree_path,agent,provider,title,description,status,priority,estimated_cost_micros,actual_cost_micros,cost_currency,created_at,updated_at,started_at,completed_at FROM tasks WHERE id=?1", [id.as_bytes()], row_task).optional())
    }
    pub fn set_status(&self, id: Uuid, status: &str) -> Result<bool> {
        self.0.task_planning().set_task_status(id, status)
    }
}

impl SettingsRepository<'_> {
    pub fn set(&self, scope: &str, key: &str, value: &Value) -> Result<Setting> {
        let now = now_ms();
        let encoded = serde_json::to_string(value).map_err(|error| {
            crate::StorageError::database(
                "encode setting",
                rusqlite::Error::ToSqlConversionFailure(Box::new(error)),
            )
        })?;
        self.0.execute("INSERT INTO settings (scope,key,value_json,updated_at) VALUES (?1,?2,?3,?4) ON CONFLICT(scope,key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at", &[&scope, &key, &encoded, &now])?;
        Ok(Setting {
            scope: scope.into(),
            key: key.into(),
            value: value.clone(),
            updated_at: now,
        })
    }
    pub fn get(&self, scope: &str, key: &str) -> Result<Option<Setting>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT scope,key,value_json,updated_at FROM settings WHERE scope=?1 AND key=?2",
                params![scope, key],
                |row| {
                    let encoded: String = row.get(2)?;
                    let value = serde_json::from_str(&encoded).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            encoded.len(),
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(Setting {
                        scope: row.get(0)?,
                        key: row.get(1)?,
                        value,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()
        })
    }
    pub fn delete(&self, scope: &str, key: &str) -> Result<bool> {
        Ok(self.0.execute(
            "DELETE FROM settings WHERE scope=?1 AND key=?2",
            &[&scope, &key],
        )? > 0)
    }
}

impl MessageRepository<'_> {
    pub fn create(&self, message: &NewMessage) -> Result<Message> {
        let now = now_ms();
        let encoded = encode_json(&message.content)?;
        self.0.execute(
            "INSERT INTO messages (id,turn_id,sequence,role,content_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            &[
                &message.id.as_bytes(),
                &message.turn_id.as_bytes(),
                &message.sequence,
                &message.role,
                &encoded,
                &now,
            ],
        )?;
        Ok(Message {
            id: message.id,
            turn_id: message.turn_id,
            sequence: message.sequence,
            role: message.role.clone(),
            content: message.content.clone(),
            created_at: now,
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<Message>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,turn_id,sequence,role,content_json,created_at FROM messages WHERE id=?1",
                [id.as_bytes()],
                row_message,
            )
            .optional()
        })
    }

    pub fn list_for_turn(&self, turn_id: Uuid) -> Result<Vec<Message>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,turn_id,sequence,role,content_json,created_at FROM messages WHERE turn_id=?1 ORDER BY sequence",
            )?;
            statement
                .query_map([turn_id.as_bytes()], row_message)?
                .collect()
        })
    }
}

impl ToolCallRepository<'_> {
    pub fn create(&self, tool_call: &NewToolCall) -> Result<ToolCall> {
        let now = now_ms();
        let input = encode_json(&tool_call.input)?;
        self.0.execute(
            "INSERT INTO tool_calls (id,turn_id,name,status,input_json,started_at) VALUES (?1,?2,?3,?4,?5,?6)",
            &[
                &tool_call.id.as_bytes(),
                &tool_call.turn_id.as_bytes(),
                &tool_call.name,
                &tool_call.status,
                &input,
                &now,
            ],
        )?;
        Ok(ToolCall {
            id: tool_call.id,
            turn_id: tool_call.turn_id,
            name: tool_call.name.clone(),
            status: tool_call.status.clone(),
            input: tool_call.input.clone(),
            output: None,
            started_at: now,
            completed_at: None,
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<ToolCall>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,turn_id,name,status,input_json,output_json,started_at,completed_at FROM tool_calls WHERE id=?1",
                [id.as_bytes()],
                row_tool_call,
            )
            .optional()
        })
    }

    pub fn set_status(&self, id: Uuid, status: &str, output: Option<&Value>) -> Result<bool> {
        let completed_at = completed_status(status).then(now_ms);
        let output_json = output.map(encode_json).transpose()?;
        Ok(self.0.execute(
            "UPDATE tool_calls SET status=?2,output_json=?3,completed_at=?4 WHERE id=?1",
            &[&id.as_bytes(), &status, &output_json, &completed_at],
        )? > 0)
    }
}

impl TerminalSessionRepository<'_> {
    pub fn create(&self, terminal: &NewTerminalSession) -> Result<TerminalSession> {
        validate_artifact_hash(terminal.log_artifact_hash.as_deref())?;
        let now = now_ms();
        let session_id = optional_uuid_bytes(terminal.session_id);
        self.0.execute(
            "INSERT INTO terminal_sessions (id,session_id,status,shell,cwd,started_at,log_artifact_hash) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            &[
                &terminal.id.as_bytes(),
                &session_id,
                &terminal.status,
                &terminal.shell,
                &terminal.cwd,
                &now,
                &terminal.log_artifact_hash,
            ],
        )?;
        Ok(TerminalSession {
            id: terminal.id,
            session_id: terminal.session_id,
            status: terminal.status.clone(),
            shell: terminal.shell.clone(),
            cwd: terminal.cwd.clone(),
            started_at: now,
            ended_at: None,
            log_artifact_hash: terminal.log_artifact_hash.clone(),
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<TerminalSession>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,session_id,status,shell,cwd,started_at,ended_at,log_artifact_hash FROM terminal_sessions WHERE id=?1",
                [id.as_bytes()],
                row_terminal_session,
            )
            .optional()
        })
    }

    pub fn set_log_artifact(&self, id: Uuid, hash: &str) -> Result<bool> {
        validate_artifact_hash(Some(hash))?;
        Ok(self.0.execute(
            "UPDATE terminal_sessions SET log_artifact_hash=?2 WHERE id=?1",
            &[&id.as_bytes(), &hash],
        )? > 0)
    }

    pub fn finish(&self, id: Uuid, status: &str) -> Result<bool> {
        let now = now_ms();
        Ok(self.0.execute(
            "UPDATE terminal_sessions SET status=?2,ended_at=?3 WHERE id=?1",
            &[&id.as_bytes(), &status, &now],
        )? > 0)
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<TerminalSession>> {
        self.0.read(|db| {
            let mut stmt = db.prepare(
                "SELECT id,session_id,status,shell,cwd,started_at,ended_at,log_artifact_hash FROM terminal_sessions ORDER BY started_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit as i64], row_terminal_session)?;
            rows.collect()
        })
    }
}

impl CommandRepository<'_> {
    pub fn create(&self, command: &NewCommand) -> Result<CommandRecord> {
        validate_artifact_hash(command.output_artifact_hash.as_deref())?;
        let now = now_ms();
        self.0.execute(
            "INSERT INTO commands (id,terminal_session_id,command,cwd,started_at,output_artifact_hash) VALUES (?1,?2,?3,?4,?5,?6)",
            &[
                &command.id.as_bytes(),
                &command.terminal_session_id.as_bytes(),
                &command.command,
                &command.cwd,
                &now,
                &command.output_artifact_hash,
            ],
        )?;
        Ok(CommandRecord {
            id: command.id,
            terminal_session_id: command.terminal_session_id,
            command: command.command.clone(),
            cwd: command.cwd.clone(),
            exit_code: None,
            started_at: now,
            completed_at: None,
            output_artifact_hash: command.output_artifact_hash.clone(),
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<CommandRecord>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,terminal_session_id,command,cwd,exit_code,started_at,completed_at,output_artifact_hash FROM commands WHERE id=?1",
                [id.as_bytes()],
                row_command,
            )
            .optional()
        })
    }

    pub fn complete(
        &self,
        id: Uuid,
        exit_code: i64,
        output_artifact_hash: Option<&str>,
    ) -> Result<bool> {
        validate_artifact_hash(output_artifact_hash)?;
        let now = now_ms();
        Ok(self.0.execute(
            "UPDATE commands SET exit_code=?2,completed_at=?3,output_artifact_hash=?4 WHERE id=?1",
            &[&id.as_bytes(), &exit_code, &now, &output_artifact_hash],
        )? > 0)
    }
}

impl GitWorktreeRepository<'_> {
    pub fn create(&self, worktree: &NewGitWorktree) -> Result<GitWorktree> {
        let now = now_ms();
        let session_id = optional_uuid_bytes(worktree.session_id);
        self.0.execute(
            "INSERT INTO git_worktrees (id,session_id,repository_location_id,path,branch,head_oid,status,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            &[
                &worktree.id.as_bytes(),
                &session_id,
                &worktree.repository_location_id.as_bytes(),
                &worktree.path,
                &worktree.branch,
                &worktree.head_oid,
                &worktree.status,
                &now,
            ],
        )?;
        Ok(GitWorktree {
            id: worktree.id,
            session_id: worktree.session_id,
            repository_location_id: worktree.repository_location_id,
            path: worktree.path.clone(),
            branch: worktree.branch.clone(),
            head_oid: worktree.head_oid.clone(),
            status: worktree.status.clone(),
            created_at: now,
            removed_at: None,
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<GitWorktree>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,session_id,repository_location_id,path,branch,head_oid,status,created_at,removed_at FROM git_worktrees WHERE id=?1",
                [id.as_bytes()],
                row_git_worktree,
            )
            .optional()
        })
    }

    pub fn find_by_path(&self, path: &str) -> Result<Option<GitWorktree>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,session_id,repository_location_id,path,branch,head_oid,status,created_at,removed_at FROM git_worktrees WHERE path=?1 AND removed_at IS NULL",
                [path],
                row_git_worktree,
            )
            .optional()
        })
    }

    pub fn list_by_repository(&self, repository_location_id: Uuid) -> Result<Vec<GitWorktree>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,session_id,repository_location_id,path,branch,head_oid,status,created_at,removed_at FROM git_worktrees WHERE repository_location_id=?1 AND removed_at IS NULL ORDER BY created_at DESC",
            )?;
            statement
                .query_map([repository_location_id.as_bytes()], row_git_worktree)?
                .collect()
        })
    }

    pub fn list_by_session(&self, session_id: Uuid) -> Result<Vec<GitWorktree>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,session_id,repository_location_id,path,branch,head_oid,status,created_at,removed_at FROM git_worktrees WHERE session_id=?1 AND removed_at IS NULL ORDER BY created_at DESC",
            )?;
            statement
                .query_map([session_id.as_bytes()], row_git_worktree)?
                .collect()
        })
    }

    pub fn assign_session(&self, id: Uuid, session_id: Uuid) -> Result<bool> {
        Ok(self.0.execute(
            "UPDATE git_worktrees SET session_id=?2 WHERE id=?1 AND removed_at IS NULL",
            &[&id.as_bytes(), &session_id.as_bytes()],
        )? > 0)
    }

    pub fn mark_removed(&self, id: Uuid) -> Result<bool> {
        let now = now_ms();
        Ok(self.0.execute(
            "UPDATE git_worktrees SET status='removed', removed_at=?2 WHERE id=?1",
            &[&id.as_bytes(), &now],
        )? > 0)
    }
}

impl GitCheckpointRepository<'_> {
    pub fn create(&self, checkpoint: &NewGitCheckpoint) -> Result<GitCheckpoint> {
        validate_artifact_hash(checkpoint.patch_artifact_hash.as_deref())?;
        let now = now_ms();
        let turn_id = optional_uuid_bytes(checkpoint.turn_id);
        self.0.execute(
            "INSERT INTO git_checkpoints (id,git_worktree_id,turn_id,kind,base_oid,patch_artifact_hash,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            &[
                &checkpoint.id.as_bytes(),
                &checkpoint.git_worktree_id.as_bytes(),
                &turn_id,
                &checkpoint.kind,
                &checkpoint.base_oid,
                &checkpoint.patch_artifact_hash,
                &now,
            ],
        )?;
        Ok(GitCheckpoint {
            id: checkpoint.id,
            git_worktree_id: checkpoint.git_worktree_id,
            turn_id: checkpoint.turn_id,
            kind: checkpoint.kind.clone(),
            base_oid: checkpoint.base_oid.clone(),
            patch_artifact_hash: checkpoint.patch_artifact_hash.clone(),
            created_at: now,
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<GitCheckpoint>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,git_worktree_id,turn_id,kind,base_oid,patch_artifact_hash,created_at FROM git_checkpoints WHERE id=?1",
                [id.as_bytes()],
                row_git_checkpoint,
            )
            .optional()
        })
    }

    pub fn list_by_worktree(
        &self,
        git_worktree_id: Uuid,
        limit: usize,
    ) -> Result<Vec<GitCheckpoint>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,git_worktree_id,turn_id,kind,base_oid,patch_artifact_hash,created_at FROM git_checkpoints WHERE git_worktree_id=?1 ORDER BY created_at DESC LIMIT ?2",
            )?;
            statement
                .query_map(
                    rusqlite::params![git_worktree_id.as_bytes(), limit as i64],
                    row_git_checkpoint,
                )?
                .collect()
        })
    }

    pub fn list_for_turn(&self, turn_id: Uuid) -> Result<Vec<GitCheckpoint>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,git_worktree_id,turn_id,kind,base_oid,patch_artifact_hash,created_at FROM git_checkpoints WHERE turn_id=?1 ORDER BY created_at",
            )?;
            statement
                .query_map([turn_id.as_bytes()], row_git_checkpoint)?
                .collect()
        })
    }
}

impl FileChangeRepository<'_> {
    pub fn create(&self, change: &NewFileChange) -> Result<FileChange> {
        validate_artifact_hash(change.before_artifact_hash.as_deref())?;
        validate_artifact_hash(change.after_artifact_hash.as_deref())?;
        let now = now_ms();
        let turn_id = optional_uuid_bytes(change.turn_id);
        let git_checkpoint_id = optional_uuid_bytes(change.git_checkpoint_id);
        self.0.execute(
            "INSERT INTO file_changes (id,turn_id,git_checkpoint_id,path,change_kind,before_artifact_hash,after_artifact_hash,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            &[
                &change.id.as_bytes(),
                &turn_id,
                &git_checkpoint_id,
                &change.path,
                &change.change_kind,
                &change.before_artifact_hash,
                &change.after_artifact_hash,
                &now,
            ],
        )?;
        Ok(FileChange {
            id: change.id,
            turn_id: change.turn_id,
            git_checkpoint_id: change.git_checkpoint_id,
            path: change.path.clone(),
            change_kind: change.change_kind.clone(),
            before_artifact_hash: change.before_artifact_hash.clone(),
            after_artifact_hash: change.after_artifact_hash.clone(),
            created_at: now,
        })
    }

    pub fn list_for_turn(&self, turn_id: Uuid) -> Result<Vec<FileChange>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,turn_id,git_checkpoint_id,path,change_kind,before_artifact_hash,after_artifact_hash,created_at FROM file_changes WHERE turn_id=?1 ORDER BY created_at",
            )?;
            statement
                .query_map([turn_id.as_bytes()], row_file_change)?
                .collect()
        })
    }

    pub fn list_for_checkpoint(&self, git_checkpoint_id: Uuid) -> Result<Vec<FileChange>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,turn_id,git_checkpoint_id,path,change_kind,before_artifact_hash,after_artifact_hash,created_at FROM file_changes WHERE git_checkpoint_id=?1 ORDER BY created_at",
            )?;
            statement
                .query_map([git_checkpoint_id.as_bytes()], row_file_change)?
                .collect()
        })
    }
}

impl ApprovalRepository<'_> {
    pub fn create(&self, approval: &NewApproval) -> Result<Approval> {
        let now = now_ms();
        let tool_call_id = optional_uuid_bytes(approval.tool_call_id);
        let request = encode_json(&approval.request)?;
        self.0.execute(
            "INSERT INTO approvals (id,session_id,tool_call_id,status,request_json,requested_at) VALUES (?1,?2,?3,?4,?5,?6)",
            &[
                &approval.id.as_bytes(),
                &approval.session_id.as_bytes(),
                &tool_call_id,
                &approval.status,
                &request,
                &now,
            ],
        )?;
        Ok(Approval {
            id: approval.id,
            session_id: approval.session_id,
            tool_call_id: approval.tool_call_id,
            status: approval.status.clone(),
            request: approval.request.clone(),
            decision: None,
            requested_at: now,
            decided_at: None,
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<Approval>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,session_id,tool_call_id,status,request_json,decision_json,requested_at,decided_at FROM approvals WHERE id=?1",
                [id.as_bytes()],
                row_approval,
            )
            .optional()
        })
    }

    pub fn list(
        &self,
        status: Option<&str>,
        session_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Vec<Approval>> {
        self.0.read(|db| {
            let limit = i64::try_from(limit).unwrap_or(i64::MAX);
            match (status, session_id) {
                (Some(status), Some(session_id)) => {
                    let mut statement = db.prepare(
                        "SELECT id,session_id,tool_call_id,status,request_json,decision_json,requested_at,decided_at FROM approvals WHERE status=?1 AND session_id=?2 ORDER BY requested_at DESC LIMIT ?3",
                    )?;
                    statement
                        .query_map(rusqlite::params![status, session_id.as_bytes(), limit], row_approval)?
                        .collect()
                }
                (Some(status), None) => {
                    let mut statement = db.prepare(
                        "SELECT id,session_id,tool_call_id,status,request_json,decision_json,requested_at,decided_at FROM approvals WHERE status=?1 ORDER BY requested_at DESC LIMIT ?2",
                    )?;
                    statement
                        .query_map(rusqlite::params![status, limit], row_approval)?
                        .collect()
                }
                (None, Some(session_id)) => {
                    let mut statement = db.prepare(
                        "SELECT id,session_id,tool_call_id,status,request_json,decision_json,requested_at,decided_at FROM approvals WHERE session_id=?1 ORDER BY requested_at DESC LIMIT ?2",
                    )?;
                    statement
                        .query_map(rusqlite::params![session_id.as_bytes(), limit], row_approval)?
                        .collect()
                }
                (None, None) => {
                    let mut statement = db.prepare(
                        "SELECT id,session_id,tool_call_id,status,request_json,decision_json,requested_at,decided_at FROM approvals ORDER BY requested_at DESC LIMIT ?1",
                    )?;
                    statement.query_map([limit], row_approval)?.collect()
                }
            }
        })
    }

    pub fn list_pending_for_session(&self, session_id: Uuid) -> Result<Vec<Approval>> {
        self.list(Some("pending"), Some(session_id), 256)
    }

    pub fn find_pending_by_fingerprint(&self, fingerprint: &str) -> Result<Option<Approval>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,session_id,tool_call_id,status,request_json,decision_json,requested_at,decided_at FROM approvals WHERE status='pending' AND json_extract(request_json,'$.fingerprint')=?1 ORDER BY requested_at LIMIT 1",
            )?;
            statement
                .query_row([fingerprint], row_approval)
                .optional()
        })
    }

    pub fn decide(&self, id: Uuid, status: &str, decision: &serde_json::Value) -> Result<Approval> {
        let now = now_ms();
        let encoded = encode_json(decision)?;
        let updated = self.0.execute(
            "UPDATE approvals SET status=?2,decision_json=?3,decided_at=?4 WHERE id=?1 AND status='pending'",
            &[&id.as_bytes(), &status, &encoded, &now],
        )?;
        if updated == 0 {
            return self.get(id)?.ok_or_else(|| {
                StorageError::database("load approval", rusqlite::Error::QueryReturnedNoRows)
            });
        }
        self.get(id)?.ok_or_else(|| {
            StorageError::database("load approval", rusqlite::Error::QueryReturnedNoRows)
        })
    }

    pub fn count_pending(&self) -> Result<u64> {
        self.0.read(|db| {
            db.query_row(
                "SELECT count(*) FROM approvals WHERE status='pending'",
                [],
                |row| row.get::<_, u64>(0),
            )
        })
    }
}

impl PermissionRuleRepository<'_> {
    pub fn create(&self, rule: &NewPermissionRule) -> Result<PermissionRule> {
        let now = now_ms();
        let project_id = optional_uuid_bytes(rule.project_id);
        let matcher = encode_json(&rule.matcher)?;
        self.0.execute(
            "INSERT INTO permission_rules (id,project_id,scope,effect,matcher_json,created_at,expires_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            &[
                &rule.id.as_bytes(),
                &project_id,
                &rule.scope,
                &rule.effect,
                &matcher,
                &now,
                &rule.expires_at,
            ],
        )?;
        Ok(PermissionRule {
            id: rule.id,
            project_id: rule.project_id,
            scope: rule.scope.clone(),
            effect: rule.effect.clone(),
            matcher: rule.matcher.clone(),
            created_at: now,
            expires_at: rule.expires_at,
        })
    }

    pub fn list_for_project(&self, project_id: Uuid) -> Result<Vec<PermissionRule>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,project_id,scope,effect,matcher_json,created_at,expires_at FROM permission_rules WHERE project_id=?1 OR project_id IS NULL ORDER BY created_at",
            )?;
            statement
                .query_map([project_id.as_bytes()], row_permission_rule)?
                .collect()
        })
    }

    pub fn list_global(&self) -> Result<Vec<PermissionRule>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,project_id,scope,effect,matcher_json,created_at,expires_at FROM permission_rules WHERE project_id IS NULL ORDER BY created_at",
            )?;
            statement.query_map([], row_permission_rule)?.collect()
        })
    }

    pub fn delete(&self, id: Uuid) -> Result<bool> {
        Ok(self.0.execute(
            "DELETE FROM permission_rules WHERE id=?1",
            &[&id.as_bytes()],
        )? > 0)
    }
}

impl LayoutRepository<'_> {
    pub fn upsert(&self, layout: &NewLayout) -> Result<Layout> {
        let now = now_ms();
        let workspace_id = optional_uuid_bytes(layout.workspace_id);
        let encoded = encode_json(&layout.layout)?;
        let is_active = i64::from(layout.is_active);
        if layout.is_active {
            if let Some(workspace_id) = layout.workspace_id {
                self.0.execute(
                    "UPDATE layouts SET is_active=0 WHERE workspace_id=?1",
                    &[&workspace_id.as_bytes()],
                )?;
            } else {
                self.0.execute(
                    "UPDATE layouts SET is_active=0 WHERE workspace_id IS NULL",
                    &[],
                )?;
            }
        }
        self.0.execute(
            "INSERT INTO layouts (id,workspace_id,name,layout_json,is_active,updated_at) VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET workspace_id=excluded.workspace_id,name=excluded.name,layout_json=excluded.layout_json,is_active=excluded.is_active,updated_at=excluded.updated_at",
            &[&layout.id, &workspace_id, &layout.name, &encoded, &is_active, &now],
        )?;
        Ok(Layout {
            id: layout.id.clone(),
            workspace_id: layout.workspace_id,
            name: layout.name.clone(),
            layout: layout.layout.clone(),
            is_active: layout.is_active,
            updated_at: now,
        })
    }

    pub fn get(&self, id: &str) -> Result<Option<Layout>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,workspace_id,name,layout_json,is_active,updated_at FROM layouts WHERE id=?1",
                [id],
                row_layout,
            )
            .optional()
        })
    }

    pub fn get_active(&self, workspace_id: Option<Uuid>) -> Result<Option<Layout>> {
        self.0.read(|db| match workspace_id {
            Some(id) => db
                .query_row(
                    "SELECT id,workspace_id,name,layout_json,is_active,updated_at FROM layouts WHERE workspace_id=?1 AND is_active=1 ORDER BY updated_at DESC LIMIT 1",
                    [id.as_bytes()],
                    row_layout,
                )
                .optional(),
            None => db
                .query_row(
                    "SELECT id,workspace_id,name,layout_json,is_active,updated_at FROM layouts WHERE workspace_id IS NULL AND is_active=1 ORDER BY updated_at DESC LIMIT 1",
                    [],
                    row_layout,
                )
                .optional(),
        })
    }
}

impl BrowserSessionRepository<'_> {
    pub fn create(&self, browser: &NewBrowserSession) -> Result<BrowserSession> {
        let now = now_ms();
        let session_id = optional_uuid_bytes(browser.session_id);
        let metadata = encode_json(&browser.metadata)?;
        self.0.execute(
            "INSERT INTO browser_sessions (id,session_id,status,started_at,metadata_json) VALUES (?1,?2,?3,?4,?5)",
            &[
                &browser.id.as_bytes(),
                &session_id,
                &browser.status,
                &now,
                &metadata,
            ],
        )?;
        Ok(BrowserSession {
            id: browser.id,
            session_id: browser.session_id,
            status: browser.status.clone(),
            started_at: now,
            ended_at: None,
            metadata: browser.metadata.clone(),
        })
    }

    pub fn get(&self, id: Uuid) -> Result<Option<BrowserSession>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT id,session_id,status,started_at,ended_at,metadata_json FROM browser_sessions WHERE id=?1",
                [id.as_bytes()],
                row_browser_session,
            )
            .optional()
        })
    }
}

fn row_project(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: uuid(row, 0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}
fn row_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: uuid(row, 0)?,
        project_id: uuid(row, 1)?,
        title: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}
fn row_turn(row: &Row<'_>) -> rusqlite::Result<Turn> {
    Ok(Turn {
        id: uuid(row, 0)?,
        session_id: uuid(row, 1)?,
        sequence: row.get(2)?,
        status: row.get(3)?,
        started_at: row.get(4)?,
        completed_at: row.get(5)?,
    })
}
fn row_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    let session: Option<Vec<u8>> = row.get(1)?;
    let project: Option<Vec<u8>> = row.get(2)?;
    Ok(Task {
        id: uuid(row, 0)?,
        session_id: session
            .map(|bytes| Uuid::from_slice(&bytes))
            .transpose()
            .map_err(from_uuid)?,
        project_id: optional_uuid_from_bytes(project)?,
        parent_task_id: optional_uuid_from_bytes(row.get(3)?)?,
        worktree_id: optional_uuid_from_bytes(row.get(4)?)?,
        branch: row.get(5)?,
        worktree_path: row.get(6)?,
        agent: row.get(7)?,
        provider: row.get(8)?,
        title: row.get(9)?,
        description: row.get(10)?,
        status: row.get(11)?,
        priority: row.get(12)?,
        estimated_cost_micros: row.get(13)?,
        actual_cost_micros: row.get(14)?,
        cost_currency: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        started_at: row.get(18)?,
        completed_at: row.get(19)?,
    })
}
fn row_message(row: &Row<'_>) -> rusqlite::Result<Message> {
    let encoded: String = row.get(4)?;
    Ok(Message {
        id: uuid(row, 0)?,
        turn_id: uuid(row, 1)?,
        sequence: row.get(2)?,
        role: row.get(3)?,
        content: decode_json(&encoded)?,
        created_at: row.get(5)?,
    })
}
fn row_tool_call(row: &Row<'_>) -> rusqlite::Result<ToolCall> {
    let input: String = row.get(4)?;
    let output: Option<String> = row.get(5)?;
    Ok(ToolCall {
        id: uuid(row, 0)?,
        turn_id: uuid(row, 1)?,
        name: row.get(2)?,
        status: row.get(3)?,
        input: decode_json(&input)?,
        output: output.as_deref().map(decode_json).transpose()?,
        started_at: row.get(6)?,
        completed_at: row.get(7)?,
    })
}
fn row_terminal_session(row: &Row<'_>) -> rusqlite::Result<TerminalSession> {
    let session: Option<Vec<u8>> = row.get(1)?;
    Ok(TerminalSession {
        id: uuid(row, 0)?,
        session_id: optional_uuid_from_bytes(session)?,
        status: row.get(2)?,
        shell: row.get(3)?,
        cwd: row.get(4)?,
        started_at: row.get(5)?,
        ended_at: row.get(6)?,
        log_artifact_hash: row.get(7)?,
    })
}
fn row_command(row: &Row<'_>) -> rusqlite::Result<CommandRecord> {
    Ok(CommandRecord {
        id: uuid(row, 0)?,
        terminal_session_id: uuid(row, 1)?,
        command: row.get(2)?,
        cwd: row.get(3)?,
        exit_code: row.get(4)?,
        started_at: row.get(5)?,
        completed_at: row.get(6)?,
        output_artifact_hash: row.get(7)?,
    })
}
fn row_git_worktree(row: &Row<'_>) -> rusqlite::Result<GitWorktree> {
    let session: Option<Vec<u8>> = row.get(1)?;
    Ok(GitWorktree {
        id: uuid(row, 0)?,
        session_id: optional_uuid_from_bytes(session)?,
        repository_location_id: uuid(row, 2)?,
        path: row.get(3)?,
        branch: row.get(4)?,
        head_oid: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        removed_at: row.get(8)?,
    })
}
fn row_git_checkpoint(row: &Row<'_>) -> rusqlite::Result<GitCheckpoint> {
    let turn: Option<Vec<u8>> = row.get(2)?;
    Ok(GitCheckpoint {
        id: uuid(row, 0)?,
        git_worktree_id: uuid(row, 1)?,
        turn_id: optional_uuid_from_bytes(turn)?,
        kind: row.get(3)?,
        base_oid: row.get(4)?,
        patch_artifact_hash: row.get(5)?,
        created_at: row.get(6)?,
    })
}
fn row_file_change(row: &Row<'_>) -> rusqlite::Result<FileChange> {
    let turn: Option<Vec<u8>> = row.get(1)?;
    let checkpoint: Option<Vec<u8>> = row.get(2)?;
    Ok(FileChange {
        id: uuid(row, 0)?,
        turn_id: optional_uuid_from_bytes(turn)?,
        git_checkpoint_id: optional_uuid_from_bytes(checkpoint)?,
        path: row.get(3)?,
        change_kind: row.get(4)?,
        before_artifact_hash: row.get(5)?,
        after_artifact_hash: row.get(6)?,
        created_at: row.get(7)?,
    })
}
fn row_approval(row: &Row<'_>) -> rusqlite::Result<Approval> {
    let tool_call: Option<Vec<u8>> = row.get(2)?;
    let request: String = row.get(4)?;
    let decision: Option<String> = row.get(5)?;
    Ok(Approval {
        id: uuid(row, 0)?,
        session_id: uuid(row, 1)?,
        tool_call_id: optional_uuid_from_bytes(tool_call)?,
        status: row.get(3)?,
        request: decode_json(&request)?,
        decision: decision.as_deref().map(decode_json).transpose()?,
        requested_at: row.get(6)?,
        decided_at: row.get(7)?,
    })
}
fn row_permission_rule(row: &Row<'_>) -> rusqlite::Result<PermissionRule> {
    let project: Option<Vec<u8>> = row.get(1)?;
    let matcher: String = row.get(4)?;
    Ok(PermissionRule {
        id: uuid(row, 0)?,
        project_id: optional_uuid_from_bytes(project)?,
        scope: row.get(2)?,
        effect: row.get(3)?,
        matcher: decode_json(&matcher)?,
        created_at: row.get(5)?,
        expires_at: row.get(6)?,
    })
}
fn row_layout(row: &Row<'_>) -> rusqlite::Result<Layout> {
    let workspace: Option<Vec<u8>> = row.get(1)?;
    let encoded: String = row.get(3)?;
    Ok(Layout {
        id: row.get(0)?,
        workspace_id: optional_uuid_from_bytes(workspace)?,
        name: row.get(2)?,
        layout: decode_json(&encoded)?,
        is_active: row.get::<_, i64>(4)? != 0,
        updated_at: row.get(5)?,
    })
}
fn row_browser_session(row: &Row<'_>) -> rusqlite::Result<BrowserSession> {
    let session: Option<Vec<u8>> = row.get(1)?;
    let metadata: String = row.get(5)?;
    Ok(BrowserSession {
        id: uuid(row, 0)?,
        session_id: optional_uuid_from_bytes(session)?,
        status: row.get(2)?,
        started_at: row.get(3)?,
        ended_at: row.get(4)?,
        metadata: decode_json(&metadata)?,
    })
}
fn uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::from_slice(&row.get::<_, Vec<u8>>(index)?).map_err(from_uuid)
}
fn from_uuid(error: uuid::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(16, rusqlite::types::Type::Blob, Box::new(error))
}
fn optional_uuid_bytes(id: Option<Uuid>) -> Option<Vec<u8>> {
    id.map(|value| value.as_bytes().to_vec())
}
fn optional_uuid_from_bytes(bytes: Option<Vec<u8>>) -> rusqlite::Result<Option<Uuid>> {
    bytes
        .map(|value| Uuid::from_slice(&value))
        .transpose()
        .map_err(from_uuid)
}
fn encode_json(value: &Value) -> Result<String> {
    serde_json::to_string(value).map_err(|error| {
        crate::StorageError::database(
            "encode json",
            rusqlite::Error::ToSqlConversionFailure(Box::new(error)),
        )
    })
}
fn decode_json(encoded: &str) -> rusqlite::Result<Value> {
    serde_json::from_str(encoded).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            encoded.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn validate_artifact_hash(hash: Option<&str>) -> Result<()> {
    if let Some(hash) = hash
        && (hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()))
    {
        return Err(crate::StorageError::ArtifactIntegrity {
            hash: hash.into(),
            details: "expected 64 lowercase hexadecimal characters".into(),
        });
    }
    Ok(())
}
fn completed_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "cancelled" | "interrupted")
}
pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::ArtifactStore;

    #[test]
    fn typed_state_round_trips() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Retcon")).unwrap();
        let session = db
            .sessions()
            .create(&NewSession::new(project.id, "Phase 5"))
            .unwrap();
        let turn = db.turns().create(&NewTurn::new(session.id, 1)).unwrap();
        let mut task = NewTask::new("Persist state");
        task.session_id = Some(session.id);
        let task = db.tasks().create(&task).unwrap();
        db.settings()
            .set("global", "theme", &serde_json::json!({"name":"dark"}))
            .unwrap();
        assert_eq!(db.sessions().get(session.id).unwrap(), Some(session));
        assert_eq!(db.turns().get(turn.id).unwrap(), Some(turn));
        assert_eq!(db.tasks().get(task.id).unwrap(), Some(task));
        assert_eq!(
            db.settings().get("global", "theme").unwrap().unwrap().value["name"],
            "dark"
        );
    }

    #[test]
    fn messages_layouts_and_artifact_hashes_round_trip() {
        let db = Database::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let artifacts = ArtifactStore::open(dir.path()).unwrap();
        let log = artifacts.store_bytes(b"terminal transcript").unwrap();

        let project = db.projects().create(&NewProject::new("Retcon")).unwrap();
        let session = db
            .sessions()
            .create(&NewSession::new(project.id, "Phase 5"))
            .unwrap();
        let turn = db.turns().create(&NewTurn::new(session.id, 1)).unwrap();
        let message = db
            .messages()
            .create(&NewMessage::new(
                turn.id,
                1,
                "user",
                serde_json::json!({"text":"hello"}),
            ))
            .unwrap();
        let layout = db
            .layouts()
            .upsert(&NewLayout::new(
                "default",
                "Default",
                serde_json::json!({"version":1,"root":{"type":"tabs"}}),
            ))
            .unwrap();
        let mut terminal = NewTerminalSession::new("pwsh", ".");
        terminal.log_artifact_hash = Some(log.hash.clone());
        let terminal = db.terminal_sessions().create(&terminal).unwrap();

        assert_eq!(db.messages().get(message.id).unwrap(), Some(message));
        assert_eq!(db.layouts().get("default").unwrap(), Some(layout));
        assert_eq!(
            db.terminal_sessions()
                .get(terminal.id)
                .unwrap()
                .unwrap()
                .log_artifact_hash,
            Some(log.hash)
        );
    }
}
