//! Startup reconciliation for work that could not survive a process restart.

#![allow(missing_docs)]

use serde::Serialize;

use crate::repositories::now_ms;
use crate::{Database, Result};

/// Counts and state captured while reconciling interrupted work at startup.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RecoveryReport {
    pub recovered_at: i64,
    pub interrupted_sessions: u64,
    pub interrupted_turns: u64,
    pub orphaned_jobs: u64,
    pub interrupted_terminals: u64,
    pub interrupted_browsers: u64,
    pub pending_approvals: u64,
    pub active_tasks: u64,
}

impl RecoveryReport {
    #[must_use]
    pub fn changed_state(&self) -> bool {
        self.interrupted_sessions
            + self.interrupted_turns
            + self.orphaned_jobs
            + self.interrupted_terminals
            + self.interrupted_browsers
            > 0
    }
}

impl Database {
    /// Mark process-owned state left running by a previous core as interrupted or orphaned.
    pub fn recover_interrupted(&self) -> Result<RecoveryReport> {
        self.transaction(|tx| {
            let interrupted_sessions = tx.execute("UPDATE sessions SET status='disconnected',recovery_state='process_restart',updated_at=?1 WHERE status IN ('starting','running','waiting_for_approval','waiting_for_user')", [now_ms()])? as u64;
            let interrupted_turns = tx.execute("UPDATE turns SET status='failed',completed_at=?1,error_json='{\"reason\":\"process_restart\"}' WHERE status IN ('queued','sending','running','tool_execution','waiting_for_approval','completing')", [now_ms()])? as u64;
            let orphaned_jobs = tx.execute("UPDATE background_jobs SET status='orphaned',finished_at=?1,failure_class='internal',failure='core process restarted before job completion' WHERE status IN ('queued','running','stuck')", [now_ms()])? as u64;
            let interrupted_terminals = tx.execute("UPDATE terminal_sessions SET status='interrupted',ended_at=?1 WHERE status IN ('starting','running')", [now_ms()])? as u64;
            let interrupted_browsers = tx.execute("UPDATE browser_sessions SET status='interrupted',ended_at=?1 WHERE status IN ('starting','running')", [now_ms()])? as u64;
            let pending_approvals = tx.query_row("SELECT count(*) FROM approvals WHERE status='pending'", [], |row| row.get::<_,u64>(0))?;
            let active_tasks = tx.query_row("SELECT count(*) FROM tasks WHERE status NOT IN ('completed','cancelled','failed')", [], |row| row.get::<_,u64>(0))?;
            Ok(RecoveryReport { recovered_at: now_ms(), interrupted_sessions, interrupted_turns, orphaned_jobs, interrupted_terminals, interrupted_browsers, pending_approvals, active_tasks })
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{NewProject, NewSession, NewTask};

    #[test]
    fn marks_process_owned_work_and_preserves_task_progress() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Retcon")).unwrap();
        let mut new_session = NewSession::new(project.id, "Interrupted");
        new_session.status = "running".into();
        let session = db.sessions().create(&new_session).unwrap();
        let mut task = NewTask::new("Keep me");
        task.session_id = Some(session.id);
        let task = db.tasks().create(&task).unwrap();
        let job_id = uuid::Uuid::new_v4();
        db.execute("INSERT INTO background_jobs (id,owner,name,status,created_at,attempts,max_attempts,timeout_ms) VALUES (?1,'test','stale','running',1,1,1,1000)", &[&job_id.as_bytes()]).unwrap();

        let report = db.recover_interrupted().unwrap();
        assert_eq!(report.interrupted_sessions, 1);
        assert_eq!(report.orphaned_jobs, 1);
        assert_eq!(report.active_tasks, 1);
        assert_eq!(
            db.sessions().get(session.id).unwrap().unwrap().status,
            "disconnected"
        );
        assert_eq!(db.tasks().get(task.id).unwrap().unwrap().status, "pending");
    }

    #[test]
    fn recovers_waiting_sessions_and_queued_turns() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Retcon")).unwrap();
        let mut new_session = NewSession::new(project.id, "Waiting");
        new_session.status = "waiting_for_approval".into();
        let session = db.sessions().create(&new_session).unwrap();
        let turn_id = uuid::Uuid::new_v4();
        db.execute(
            "INSERT INTO turns (id,session_id,sequence,status,started_at) VALUES (?1,?2,1,'queued',1)",
            &[&turn_id.as_bytes() as &dyn rusqlite::ToSql, &session.id.as_bytes()],
        )
        .unwrap();

        let report = db.recover_interrupted().unwrap();
        assert_eq!(report.interrupted_sessions, 1);
        assert_eq!(report.interrupted_turns, 1);
        assert_eq!(
            db.sessions().get(session.id).unwrap().unwrap().status,
            "disconnected"
        );
        let status = db
            .read(|conn| {
                conn.query_row(
                    "SELECT status FROM turns WHERE id = ?1",
                    [turn_id.as_bytes()],
                    |row| row.get::<_, String>(0),
                )
            })
            .unwrap();
        assert_eq!(status, "failed");
    }
}
