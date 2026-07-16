//! Typed repositories for the durable state used by the session engine and desktop shell.

#![allow(missing_docs)]

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{OptionalExtension, Row, params};
use serde_json::Value;
use uuid::Uuid;

use crate::{Database, Result};

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
            status: "running".into(),
        }
    }
}

/// A persisted task and its current progress state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Values used to create a task.
#[derive(Clone, Debug)]
pub struct NewTask {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
}

impl NewTask {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id: None,
            title: title.into(),
            description: None,
            status: "pending".into(),
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

pub struct ProjectRepository<'a>(&'a Database);
pub struct SessionRepository<'a>(&'a Database);
pub struct TurnRepository<'a>(&'a Database);
pub struct TaskRepository<'a>(&'a Database);
pub struct SettingsRepository<'a>(&'a Database);

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
}

impl TaskRepository<'_> {
    pub fn create(&self, task: &NewTask) -> Result<Task> {
        let now = now_ms();
        let session_id = task.session_id.map(|id| *id.as_bytes());
        self.0.execute(
            "INSERT INTO tasks (id,session_id,title,description,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?6)",
            &[
                &task.id.as_bytes() as &dyn rusqlite::ToSql,
                &session_id,
                &task.title,
                &task.description,
                &task.status,
                &now,
            ],
        )?;
        Ok(Task {
            id: task.id,
            session_id: task.session_id,
            title: task.title.clone(),
            description: task.description.clone(),
            status: task.status.clone(),
            created_at: now,
            updated_at: now,
        })
    }
    pub fn get(&self, id: Uuid) -> Result<Option<Task>> {
        self.0.read(|db| db.query_row("SELECT id,session_id,title,description,status,created_at,updated_at FROM tasks WHERE id=?1", [id.as_bytes()], row_task).optional())
    }
    pub fn set_status(&self, id: Uuid, status: &str) -> Result<bool> {
        let now = now_ms();
        Ok(self.0.execute("UPDATE tasks SET status=?2,updated_at=?3,completed_at=CASE WHEN ?2='completed' THEN ?3 ELSE completed_at END WHERE id=?1", &[&id.as_bytes(), &status, &now])? > 0)
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
    let session: Option<[u8; 16]> = row.get(1)?;
    Ok(Task {
        id: uuid(row, 0)?,
        session_id: session.map(Uuid::from_bytes),
        title: row.get(2)?,
        description: row.get(3)?,
        status: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}
fn uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    Ok(Uuid::from_bytes(row.get::<_, [u8; 16]>(index)?))
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
}
