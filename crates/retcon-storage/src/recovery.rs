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
    pub interrupted_verifications: u64,
    pub orphaned_dev_servers: u64,
    pub stale_port_leases: u64,
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
            + self.interrupted_verifications
            + self.orphaned_dev_servers
            + self.stale_port_leases
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
            let recovered_at = now_ms();
            let interrupted_verifications = tx.query_row("SELECT count(*) FROM verification_runs WHERE status='running'", [], |row| row.get::<_,u64>(0))?;
            tx.execute("INSERT INTO verification_events(run_id,task_id,gate_id,kind,actor,payload_json,created_at) SELECT id,task_id,NULL,'interrupted','system','{\"reason\":\"process_restart\"}',?1 FROM verification_runs WHERE status='running'", [recovered_at])?;
            tx.execute("UPDATE verification_gates SET status='error',completed_at=?1,summary_json='{\"reason\":\"process_restart\"}' WHERE run_id IN (SELECT id FROM verification_runs WHERE status='running') AND status IN ('pending','running')", [recovered_at])?;
            tx.execute("UPDATE verification_runs SET status='error',completed_at=?1,summary_json='{\"reason\":\"process_restart\"}' WHERE status='running'", [recovered_at])?;
            let orphaned_dev_servers = tx.query_row("SELECT count(*) FROM dev_server_instances WHERE status IN ('starting','running','stopping')", [], |row| row.get::<_,u64>(0))?;
            tx.execute("INSERT INTO dev_server_events(instance_id,config_id,project_id,kind,actor,payload_json,created_at) SELECT id,config_id,project_id,'orphaned','system','{\"reason\":\"process_restart\"}',?1 FROM dev_server_instances WHERE status IN ('starting','running','stopping')", [recovered_at])?;
            tx.execute("UPDATE dev_server_instances SET status='orphaned',failure='core process restarted',stopped_at=?1 WHERE status IN ('starting','running','stopping')", [recovered_at])?;
            let stale_port_leases = tx.execute("UPDATE dev_server_port_leases SET status='stale',released_at=?1 WHERE status='active'", [recovered_at])? as u64;
            let pending_approvals = tx.query_row("SELECT count(*) FROM approvals WHERE status='pending'", [], |row| row.get::<_,u64>(0))?;
            let active_tasks = tx.query_row("SELECT count(*) FROM tasks WHERE status NOT IN ('completed','cancelled','failed')", [], |row| row.get::<_,u64>(0))?;
            Ok(RecoveryReport { recovered_at, interrupted_sessions, interrupted_turns, orphaned_jobs, interrupted_terminals, interrupted_browsers, interrupted_verifications, orphaned_dev_servers, stale_port_leases, pending_approvals, active_tasks })
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{NewProject, NewSession, NewTask, NewTurn};

    #[test]
    fn marks_in_flight_turns_failed_on_restart() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Retcon")).unwrap();
        let session = db
            .sessions()
            .create(&NewSession::new(project.id, "Turn recovery"))
            .unwrap();
        let mut new_turn = NewTurn::new(session.id, 1);
        new_turn.status = "running".into();
        let turn = db.turns().create(&new_turn).unwrap();

        let report = db.recover_interrupted().unwrap();
        assert_eq!(report.interrupted_turns, 1);
        let stored = db.turns().get(turn.id).unwrap().unwrap();
        assert_eq!(stored.status, "failed");
        assert!(stored.completed_at.is_some());
    }

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
}
