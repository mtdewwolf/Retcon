//! Durable verification configuration, execution state, evidence, and reports.

#![allow(missing_docs)]

use std::collections::HashSet;

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::repositories::now_ms;
use crate::task_planning::map_validation;
use crate::{Database, Result, StorageError};

const GATE_KINDS: &[&str] = &[
    "test",
    "build",
    "browser",
    "accessibility",
    "security",
    "git",
    "custom",
];
const GATE_RESULTS: &[&str] = &["passed", "failed", "skipped", "error"];
const VALIDATION_PREFIX: &str = "retcon_validation:";

#[derive(Clone, Debug)]
pub struct NewVerificationCommand {
    pub id: Uuid,
    pub key: String,
    pub kind: String,
    pub command: String,
    pub cwd: Option<String>,
    pub required: bool,
    pub enabled: bool,
    pub timeout_ms: Option<i64>,
}

impl NewVerificationCommand {
    #[must_use]
    pub fn new(
        key: impl Into<String>,
        kind: impl Into<String>,
        command: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            kind: kind.into(),
            command: command.into(),
            cwd: None,
            required: true,
            enabled: true,
            timeout_ms: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationCommand {
    pub id: Uuid,
    pub project_id: Uuid,
    pub key: String,
    pub kind: String,
    pub command: String,
    pub cwd: Option<String>,
    pub required: bool,
    pub enabled: bool,
    pub timeout_ms: Option<i64>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRun {
    pub id: Uuid,
    pub task_id: Uuid,
    pub project_id: Uuid,
    pub rerun_of_id: Option<Uuid>,
    pub status: String,
    pub trigger_kind: String,
    pub summary: Value,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationGate {
    pub id: Uuid,
    pub run_id: Uuid,
    pub command_id: Option<Uuid>,
    pub key: String,
    pub kind: String,
    pub command: String,
    pub cwd: Option<String>,
    pub required: bool,
    pub timeout_ms: Option<i64>,
    pub status: String,
    pub summary: Value,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct NewVerificationTestResult {
    pub id: Uuid,
    pub suite: Option<String>,
    pub name: String,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub file_path: Option<String>,
    pub line: Option<i64>,
    pub message: Option<String>,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationTestResult {
    pub id: Uuid,
    pub run_id: Uuid,
    pub gate_id: Uuid,
    pub suite: Option<String>,
    pub name: String,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub file_path: Option<String>,
    pub line: Option<i64>,
    pub message: Option<String>,
    pub metadata: Value,
}

#[derive(Clone, Debug)]
pub struct NewVerificationArtifact {
    pub id: Uuid,
    pub kind: String,
    pub hash: String,
    pub size_bytes: i64,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationArtifactRef {
    pub id: Uuid,
    pub run_id: Uuid,
    pub gate_id: Option<Uuid>,
    pub kind: String,
    pub hash: String,
    pub size_bytes: i64,
    pub metadata: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationEvent {
    pub id: i64,
    pub run_id: Uuid,
    pub task_id: Uuid,
    pub gate_id: Option<Uuid>,
    pub kind: String,
    pub actor: String,
    pub payload: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRunDetails {
    pub run: VerificationRun,
    pub gates: Vec<VerificationGate>,
    pub results: Vec<VerificationTestResult>,
    pub artifacts: Vec<VerificationArtifactRef>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationMutation<T> {
    pub task_id: Uuid,
    pub reopened: bool,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionReport {
    pub task_id: Uuid,
    pub run_id: Uuid,
    pub status: String,
    pub files_changed: Vec<String>,
    pub tests: Value,
    pub build: Value,
    pub browser: Value,
    pub accessibility: Value,
    pub security: Value,
    pub git: Value,
    pub cost: Value,
    pub runtime: Value,
    pub approvals: Value,
    pub artifacts: Vec<VerificationArtifactRef>,
    pub limitations: Vec<String>,
}

pub struct VerificationRepository<'a>(&'a Database);

impl Database {
    #[must_use]
    pub fn verification(&self) -> VerificationRepository<'_> {
        VerificationRepository(self)
    }
}

impl VerificationRepository<'_> {
    pub fn replace_commands(
        &self,
        project_id: Uuid,
        commands: &[NewVerificationCommand],
    ) -> Result<Vec<VerificationCommand>> {
        validate_commands(commands)?;
        map_validation(self.0.transaction(|tx| {
            require_exists(tx, "projects", project_id, "project does not exist")?;
            tx.execute(
                "DELETE FROM project_verification_commands WHERE project_id=?1",
                [project_id.as_bytes()],
            )?;
            let now = now_ms();
            for command in commands {
                tx.execute(
                    "INSERT INTO project_verification_commands(id,project_id,command_key,gate_kind,command,cwd,is_required,is_enabled,timeout_ms,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![command.id.as_bytes(), project_id.as_bytes(), command.key.trim(), command.kind, command.command.trim(), command.cwd, command.required, command.enabled, command.timeout_ms, now],
                )?;
            }
            Ok(())
        }))?;
        self.commands(project_id)
    }

    pub fn commands(&self, project_id: Uuid) -> Result<Vec<VerificationCommand>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,project_id,command_key,gate_kind,command,cwd,is_required,is_enabled,timeout_ms,updated_at FROM project_verification_commands WHERE project_id=?1 ORDER BY gate_kind,command_key",
            )?;
            statement
                .query_map([project_id.as_bytes()], row_command)?
                .collect()
        })
    }

    pub fn create_run(
        &self,
        task_id: Uuid,
        selected_kinds: &[String],
        trigger_kind: &str,
        rerun_of_id: Option<Uuid>,
        actor: &str,
    ) -> Result<VerificationMutation<VerificationRunDetails>> {
        validate_actor(actor)?;
        let run_id = Uuid::new_v4();
        let selected: HashSet<&str> = selected_kinds.iter().map(String::as_str).collect();
        for kind in &selected {
            validate_gate_kind(kind)?;
        }
        let reopened = map_validation(self.0.transaction(|tx| {
            let project_id = tx
                .query_row(
                    "SELECT project_id FROM tasks WHERE id=?1",
                    [task_id.as_bytes()],
                    |row| optional_uuid(row, 0),
                )
                .optional()?
                .ok_or_else(|| validation_error("task does not exist"))?
                .ok_or_else(|| validation_error("task must belong to a project before verification"))?;
            if let Some(previous) = rerun_of_id {
                let previous_task = tx
                    .query_row(
                        "SELECT task_id FROM verification_runs WHERE id=?1",
                        [previous.as_bytes()],
                        |row| uuid(row, 0),
                    )
                    .optional()?
                    .ok_or_else(|| validation_error("rerun source does not exist"))?;
                if previous_task != task_id {
                    return Err(validation_error("rerun source belongs to a different task"));
                }
            }
            let mut statement = tx.prepare(
                "SELECT id,project_id,command_key,gate_kind,command,cwd,is_required,is_enabled,timeout_ms,updated_at FROM project_verification_commands WHERE project_id=?1 AND is_enabled=1 ORDER BY gate_kind,command_key",
            )?;
            let commands: Vec<VerificationCommand> = statement
                .query_map([project_id.as_bytes()], row_command)?
                .collect::<rusqlite::Result<_>>()?;
            drop(statement);
            let commands: Vec<_> = commands
                .into_iter()
                .filter(|command| selected.is_empty() || selected.contains(command.kind.as_str()))
                .collect();
            if commands.is_empty() {
                return Err(validation_error("no enabled verification commands matched this run"));
            }
            let now = now_ms();
            tx.execute(
                "INSERT INTO verification_runs(id,task_id,project_id,rerun_of_id,status,trigger_kind,summary_json,created_at) VALUES (?1,?2,?3,?4,'queued',?5,'{}',?6)",
                params![run_id.as_bytes(), task_id.as_bytes(), project_id.as_bytes(), optional_uuid_bytes(rerun_of_id), trigger_kind, now],
            )?;
            let mut has_required = false;
            for command in commands {
                has_required |= command.required;
                tx.execute(
                    "INSERT INTO verification_gates(id,run_id,command_id,command_key,gate_kind,command,cwd,is_required,timeout_ms,status,summary_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending','{}')",
                    params![Uuid::new_v4().as_bytes(), run_id.as_bytes(), command.id.as_bytes(), command.key, command.kind, command.command, command.cwd, command.required, command.timeout_ms],
                )?;
            }
            record_event(tx, run_id, task_id, None, "created", actor, &json!({"selectedKinds": selected_kinds, "rerunOfId": rerun_of_id}))?;
            if has_required {
                reopen_terminal(tx, task_id)
            } else {
                Ok(false)
            }
        }))?;
        let value = self.get(run_id)?.ok_or_else(|| {
            StorageError::Validation("created verification run disappeared".into())
        })?;
        Ok(VerificationMutation {
            task_id,
            reopened,
            value,
        })
    }

    pub fn get(&self, run_id: Uuid) -> Result<Option<VerificationRunDetails>> {
        let Some(run) = self.0.read(|db| {
            db.query_row(
                "SELECT id,task_id,project_id,rerun_of_id,status,trigger_kind,summary_json,created_at,started_at,completed_at FROM verification_runs WHERE id=?1",
                [run_id.as_bytes()],
                row_run,
            )
            .optional()
        })? else {
            return Ok(None);
        };
        Ok(Some(VerificationRunDetails {
            run,
            gates: self.gates(run_id)?,
            results: self.results(run_id)?,
            artifacts: self.artifacts(run_id)?,
        }))
    }

    pub fn list(&self, task_id: Uuid) -> Result<Vec<VerificationRun>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,task_id,project_id,rerun_of_id,status,trigger_kind,summary_json,created_at,started_at,completed_at FROM verification_runs WHERE task_id=?1 ORDER BY created_at DESC,id DESC",
            )?;
            statement.query_map([task_id.as_bytes()], row_run)?.collect()
        })
    }

    pub fn start(&self, run_id: Uuid, actor: &str) -> Result<Option<VerificationRunDetails>> {
        validate_actor(actor)?;
        let changed = map_validation(self.0.transaction(|tx| {
            let Some((task_id, status)) = run_identity(tx, run_id)? else {
                return Ok(false);
            };
            if status != "queued" {
                return Err(validation_error("only queued verification runs can start"));
            }
            let now = now_ms();
            tx.execute(
                "UPDATE verification_runs SET status='running',started_at=?2 WHERE id=?1",
                params![run_id.as_bytes(), now],
            )?;
            record_event(tx, run_id, task_id, None, "started", actor, &json!({}))?;
            Ok(true)
        }))?;
        if changed { self.get(run_id) } else { Ok(None) }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_gate(
        &self,
        run_id: Uuid,
        gate_id: Uuid,
        status: &str,
        summary: Value,
        results: &[NewVerificationTestResult],
        artifacts: &[NewVerificationArtifact],
        actor: &str,
    ) -> Result<Option<VerificationMutation<VerificationRunDetails>>> {
        validate_actor(actor)?;
        if !GATE_RESULTS.contains(&status) {
            return Err(StorageError::Validation(format!(
                "invalid verification gate result '{status}'"
            )));
        }
        validate_results(results)?;
        let summary_json = encode_json(&summary, "encode verification gate summary")?;
        let encoded_results: Vec<_> = results
            .iter()
            .map(|result| encode_json(&result.metadata, "encode verification result metadata"))
            .collect::<Result<_>>()?;
        let encoded_artifacts: Vec<_> = artifacts
            .iter()
            .map(|artifact| {
                encode_json(&artifact.metadata, "encode verification artifact metadata")
            })
            .collect::<Result<_>>()?;
        let outcome = map_validation(self.0.transaction(|tx| {
            let Some((task_id, run_status)) = run_identity(tx, run_id)? else {
                return Ok(None);
            };
            if run_status != "running" {
                return Err(validation_error("gate results require a running verification run"));
            }
            let required = tx
                .query_row(
                    "SELECT is_required FROM verification_gates WHERE id=?1 AND run_id=?2",
                    params![gate_id.as_bytes(), run_id.as_bytes()],
                    |row| row.get::<_, bool>(0),
                )
                .optional()?
                .ok_or_else(|| validation_error("verification gate does not belong to this run"))?;
            let now = now_ms();
            tx.execute(
                "UPDATE verification_gates SET status=?3,summary_json=?4,started_at=COALESCE(started_at,?5),completed_at=?5 WHERE id=?1 AND run_id=?2",
                params![gate_id.as_bytes(), run_id.as_bytes(), status, summary_json, now],
            )?;
            tx.execute("DELETE FROM verification_test_results WHERE gate_id=?1", [gate_id.as_bytes()])?;
            for (result, metadata_json) in results.iter().zip(&encoded_results) {
                tx.execute(
                    "INSERT INTO verification_test_results(id,run_id,gate_id,suite,name,status,duration_ms,file_path,line,message,metadata_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    params![result.id.as_bytes(), run_id.as_bytes(), gate_id.as_bytes(), result.suite, result.name, result.status, result.duration_ms, result.file_path, result.line, result.message, metadata_json],
                )?;
            }
            for (artifact, metadata_json) in artifacts.iter().zip(&encoded_artifacts) {
                tx.execute(
                    "INSERT INTO verification_artifacts(id,run_id,gate_id,kind,artifact_hash,size_bytes,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![artifact.id.as_bytes(), run_id.as_bytes(), gate_id.as_bytes(), artifact.kind, artifact.hash, artifact.size_bytes, metadata_json, now],
                )?;
            }
            record_event(tx, run_id, task_id, Some(gate_id), "gate_recorded", actor, &json!({"status": status, "resultCount": results.len(), "artifactCount": artifacts.len()}))?;
            let reopened = if required && status != "passed" {
                reopen_terminal(tx, task_id)?
            } else {
                false
            };
            Ok(Some((task_id, reopened)))
        }))?;
        let Some((task_id, reopened)) = outcome else {
            return Ok(None);
        };
        Ok(self.get(run_id)?.map(|value| VerificationMutation {
            task_id,
            reopened,
            value,
        }))
    }

    pub fn finish(
        &self,
        run_id: Uuid,
        actor: &str,
    ) -> Result<Option<VerificationMutation<VerificationRunDetails>>> {
        validate_actor(actor)?;
        let outcome = map_validation(self.0.transaction(|tx| {
            let Some((task_id, status)) = run_identity(tx, run_id)? else {
                return Ok(None);
            };
            if status != "running" {
                return Err(validation_error("only running verification runs can finish"));
            }
            let pending_required: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM verification_gates WHERE run_id=?1 AND is_required=1 AND status IN ('pending','running'))",
                [run_id.as_bytes()],
                |row| row.get(0),
            )?;
            if pending_required {
                return Err(validation_error("required verification gates are still pending"));
            }
            let failed_required: i64 = tx.query_row(
                "SELECT COUNT(*) FROM verification_gates WHERE run_id=?1 AND is_required=1 AND status<>'passed'",
                [run_id.as_bytes()],
                |row| row.get(0),
            )?;
            let passed: i64 = tx.query_row(
                "SELECT COUNT(*) FROM verification_gates WHERE run_id=?1 AND status='passed'",
                [run_id.as_bytes()],
                |row| row.get(0),
            )?;
            let total: i64 = tx.query_row(
                "SELECT COUNT(*) FROM verification_gates WHERE run_id=?1",
                [run_id.as_bytes()],
                |row| row.get(0),
            )?;
            let final_status = if failed_required == 0 { "passed" } else { "failed" };
            let summary = json!({"totalGates": total, "passedGates": passed, "failedRequiredGates": failed_required});
            let summary_json = encode_json_sql(&summary)?;
            let now = now_ms();
            tx.execute(
                "UPDATE verification_runs SET status=?2,summary_json=?3,completed_at=?4 WHERE id=?1",
                params![run_id.as_bytes(), final_status, summary_json, now],
            )?;
            record_event(tx, run_id, task_id, None, "finished", actor, &json!({"status": final_status, "summary": summary}))?;
            let reopened = if failed_required > 0 { reopen_terminal(tx, task_id)? } else { false };
            Ok(Some((task_id, reopened)))
        }))?;
        let Some((task_id, reopened)) = outcome else {
            return Ok(None);
        };
        Ok(self.get(run_id)?.map(|value| VerificationMutation {
            task_id,
            reopened,
            value,
        }))
    }

    pub fn cancel(
        &self,
        run_id: Uuid,
        actor: &str,
    ) -> Result<Option<VerificationMutation<VerificationRunDetails>>> {
        validate_actor(actor)?;
        let outcome = map_validation(self.0.transaction(|tx| {
            let Some((task_id, status)) = run_identity(tx, run_id)? else {
                return Ok(None);
            };
            if !matches!(status.as_str(), "queued" | "running") {
                return Err(validation_error("only queued or running verification runs can be cancelled"));
            }
            let now = now_ms();
            tx.execute(
                "UPDATE verification_runs SET status='cancelled',completed_at=?2 WHERE id=?1",
                params![run_id.as_bytes(), now],
            )?;
            tx.execute(
                "UPDATE verification_gates SET status='cancelled',completed_at=?2 WHERE run_id=?1 AND status IN ('pending','running')",
                params![run_id.as_bytes(), now],
            )?;
            record_event(tx, run_id, task_id, None, "cancelled", actor, &json!({}))?;
            let has_required: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM verification_gates WHERE run_id=?1 AND is_required=1)",
                [run_id.as_bytes()],
                |row| row.get(0),
            )?;
            let reopened = if has_required { reopen_terminal(tx, task_id)? } else { false };
            Ok(Some((task_id, reopened)))
        }))?;
        let Some((task_id, reopened)) = outcome else {
            return Ok(None);
        };
        Ok(self.get(run_id)?.map(|value| VerificationMutation {
            task_id,
            reopened,
            value,
        }))
    }

    pub fn history(&self, run_id: Uuid) -> Result<Vec<VerificationEvent>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,run_id,task_id,gate_id,kind,actor,payload_json,created_at FROM verification_events WHERE run_id=?1 ORDER BY id",
            )?;
            statement.query_map([run_id.as_bytes()], row_event)?.collect()
        })
    }

    pub fn report(&self, run_id: Uuid) -> Result<Option<CompletionReport>> {
        let Some(details) = self.get(run_id)? else {
            return Ok(None);
        };
        let task_id = details.run.task_id;
        let (session_id, branch, worktree_path, estimated, actual, currency, task_started, task_completed) = self.0.read(|db| {
            db.query_row(
                "SELECT session_id,branch,worktree_path,estimated_cost_micros,actual_cost_micros,cost_currency,started_at,completed_at FROM tasks WHERE id=?1",
                [task_id.as_bytes()],
                |row| Ok((optional_uuid(row, 0)?, row.get::<_, Option<String>>(1)?, row.get::<_, Option<String>>(2)?, row.get::<_, Option<i64>>(3)?, row.get::<_, Option<i64>>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<i64>>(6)?, row.get::<_, Option<i64>>(7)?)),
            )
        })?;
        let files_changed = if let Some(session_id) = session_id {
            self.0.read(|db| {
                let mut statement = db.prepare("SELECT DISTINCT f.path FROM file_changes f JOIN turns t ON t.id=f.turn_id WHERE t.session_id=?1 ORDER BY f.path")?;
                statement.query_map([session_id.as_bytes()], |row| row.get(0))?.collect::<rusqlite::Result<Vec<String>>>()
            })?
        } else {
            Vec::new()
        };
        let approvals = if let Some(session_id) = session_id {
            self.0.read(|db| db.query_row(
                "SELECT COUNT(*),SUM(CASE WHEN status='approved' THEN 1 ELSE 0 END),SUM(CASE WHEN status='denied' THEN 1 ELSE 0 END) FROM approvals WHERE session_id=?1",
                [session_id.as_bytes()],
                |row| Ok(json!({"total": row.get::<_, i64>(0)?, "approved": row.get::<_, Option<i64>>(1)?.unwrap_or(0), "denied": row.get::<_, Option<i64>>(2)?.unwrap_or(0)})),
            ))?
        } else {
            json!({"total": 0, "approved": 0, "denied": 0})
        };
        let section = |kind: &str| {
            let gates: Vec<_> = details
                .gates
                .iter()
                .filter(|gate| gate.kind == kind)
                .collect();
            let result_count = details
                .results
                .iter()
                .filter(|result| gates.iter().any(|gate| gate.id == result.gate_id))
                .count();
            json!({"gates": gates, "resultCount": result_count})
        };
        let mut limitations = Vec::new();
        for (kind, label) in [
            ("test", "tests"),
            ("build", "build"),
            ("browser", "browser"),
            ("accessibility", "accessibility"),
            ("security", "security"),
            ("git", "git"),
        ] {
            if !details.gates.iter().any(|gate| gate.kind == kind) {
                limitations.push(format!(
                    "no {label} verification command was configured for this run"
                ));
            }
        }
        limitations.push("stdout and stderr are retained as bounded artifact references, not embedded in this report".into());
        Ok(Some(CompletionReport {
            task_id,
            run_id,
            status: details.run.status.clone(),
            files_changed,
            tests: section("test"),
            build: section("build"),
            browser: section("browser"),
            accessibility: section("accessibility"),
            security: section("security"),
            git: json!({"gates": details.gates.iter().filter(|gate| gate.kind == "git").collect::<Vec<_>>(), "branch": branch, "worktreePath": worktree_path}),
            cost: json!({"estimatedMicros": estimated, "actualMicros": actual, "currency": currency}),
            runtime: json!({"taskStartedAt": task_started, "taskCompletedAt": task_completed, "runStartedAt": details.run.started_at, "runCompletedAt": details.run.completed_at}),
            approvals,
            artifacts: details.artifacts,
            limitations,
        }))
    }

    fn gates(&self, run_id: Uuid) -> Result<Vec<VerificationGate>> {
        self.0.read(|db| {
            let mut statement = db.prepare("SELECT id,run_id,command_id,command_key,gate_kind,command,cwd,is_required,timeout_ms,status,summary_json,started_at,completed_at FROM verification_gates WHERE run_id=?1 ORDER BY gate_kind,command_key")?;
            statement.query_map([run_id.as_bytes()], row_gate)?.collect()
        })
    }

    fn results(&self, run_id: Uuid) -> Result<Vec<VerificationTestResult>> {
        self.0.read(|db| {
            let mut statement = db.prepare("SELECT id,run_id,gate_id,suite,name,status,duration_ms,file_path,line,message,metadata_json FROM verification_test_results WHERE run_id=?1 ORDER BY gate_id,suite,name")?;
            statement.query_map([run_id.as_bytes()], row_result)?.collect()
        })
    }

    fn artifacts(&self, run_id: Uuid) -> Result<Vec<VerificationArtifactRef>> {
        self.0.read(|db| {
            let mut statement = db.prepare("SELECT id,run_id,gate_id,kind,artifact_hash,size_bytes,metadata_json,created_at FROM verification_artifacts WHERE run_id=?1 ORDER BY created_at,id")?;
            statement.query_map([run_id.as_bytes()], row_artifact)?.collect()
        })
    }
}

fn validate_commands(commands: &[NewVerificationCommand]) -> Result<()> {
    let mut keys = HashSet::new();
    for command in commands {
        if command.key.trim().is_empty() || command.command.trim().is_empty() {
            return Err(StorageError::Validation(
                "verification command key and command cannot be empty".into(),
            ));
        }
        if !keys.insert(command.key.trim()) {
            return Err(StorageError::Validation(
                "verification command keys must be unique per project".into(),
            ));
        }
        validate_gate_kind(&command.kind)?;
        if command.timeout_ms.is_some_and(|value| value <= 0) {
            return Err(StorageError::Validation(
                "verification command timeout must be positive".into(),
            ));
        }
    }
    Ok(())
}

fn validate_gate_kind(kind: &str) -> Result<()> {
    if GATE_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(StorageError::Validation(format!(
            "invalid verification gate kind '{kind}'"
        )))
    }
}

fn validate_results(results: &[NewVerificationTestResult]) -> Result<()> {
    for result in results {
        if result.name.trim().is_empty() {
            return Err(StorageError::Validation(
                "verification result name cannot be empty".into(),
            ));
        }
        if !matches!(
            result.status.as_str(),
            "passed" | "failed" | "skipped" | "error"
        ) {
            return Err(StorageError::Validation(format!(
                "invalid verification result status '{}'",
                result.status
            )));
        }
        if result.duration_ms.is_some_and(|value| value < 0)
            || result.line.is_some_and(|value| value <= 0)
        {
            return Err(StorageError::Validation(
                "verification result duration and line must be non-negative/positive".into(),
            ));
        }
    }
    Ok(())
}

fn validate_actor(actor: &str) -> Result<()> {
    if actor.trim().is_empty() {
        Err(StorageError::Validation(
            "verification actor cannot be empty".into(),
        ))
    } else {
        Ok(())
    }
}

fn validation_error(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{VALIDATION_PREFIX}{}", message.into()))
}

fn require_exists(
    tx: &Transaction<'_>,
    table: &str,
    id: Uuid,
    message: &str,
) -> rusqlite::Result<()> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)");
    let exists: bool = tx.query_row(&sql, [id.as_bytes()], |row| row.get(0))?;
    if exists {
        Ok(())
    } else {
        Err(validation_error(message))
    }
}

fn run_identity(tx: &Transaction<'_>, run_id: Uuid) -> rusqlite::Result<Option<(Uuid, String)>> {
    tx.query_row(
        "SELECT task_id,status FROM verification_runs WHERE id=?1",
        [run_id.as_bytes()],
        |row| Ok((uuid(row, 0)?, row.get(1)?)),
    )
    .optional()
}

fn record_event(
    tx: &Transaction<'_>,
    run_id: Uuid,
    task_id: Uuid,
    gate_id: Option<Uuid>,
    kind: &str,
    actor: &str,
    payload: &Value,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO verification_events(run_id,task_id,gate_id,kind,actor,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![run_id.as_bytes(), task_id.as_bytes(), optional_uuid_bytes(gate_id), kind, actor, encode_json_sql(payload)?, now_ms()],
    )?;
    Ok(())
}

fn reopen_terminal(tx: &Transaction<'_>, task_id: Uuid) -> rusqlite::Result<bool> {
    Ok(tx.execute(
        "UPDATE tasks SET status='review',completed_at=NULL,updated_at=?2 WHERE id=?1 AND status IN ('completed','done')",
        params![task_id.as_bytes(), now_ms()],
    )? > 0)
}

fn encode_json(value: &Value, operation: &'static str) -> Result<String> {
    serde_json::to_string(value).map_err(|error| {
        StorageError::database(
            operation,
            rusqlite::Error::ToSqlConversionFailure(Box::new(error)),
        )
    })
}

fn encode_json_sql(value: &Value) -> rusqlite::Result<String> {
    serde_json::to_string(value)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn decode_json(row: &Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    let bytes: Vec<u8> = row.get(index)?;
    Uuid::from_slice(&bytes).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Blob,
            Box::new(error),
        )
    })
}

fn optional_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<Uuid>> {
    row.get::<_, Option<Vec<u8>>>(index)?
        .map(|bytes| {
            Uuid::from_slice(&bytes).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                )
            })
        })
        .transpose()
}

fn optional_uuid_bytes(value: Option<Uuid>) -> Option<Vec<u8>> {
    value.map(|id| id.as_bytes().to_vec())
}

fn row_command(row: &Row<'_>) -> rusqlite::Result<VerificationCommand> {
    Ok(VerificationCommand {
        id: uuid(row, 0)?,
        project_id: uuid(row, 1)?,
        key: row.get(2)?,
        kind: row.get(3)?,
        command: row.get(4)?,
        cwd: row.get(5)?,
        required: row.get(6)?,
        enabled: row.get(7)?,
        timeout_ms: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn row_run(row: &Row<'_>) -> rusqlite::Result<VerificationRun> {
    Ok(VerificationRun {
        id: uuid(row, 0)?,
        task_id: uuid(row, 1)?,
        project_id: uuid(row, 2)?,
        rerun_of_id: optional_uuid(row, 3)?,
        status: row.get(4)?,
        trigger_kind: row.get(5)?,
        summary: decode_json(row, 6)?,
        created_at: row.get(7)?,
        started_at: row.get(8)?,
        completed_at: row.get(9)?,
    })
}

fn row_gate(row: &Row<'_>) -> rusqlite::Result<VerificationGate> {
    Ok(VerificationGate {
        id: uuid(row, 0)?,
        run_id: uuid(row, 1)?,
        command_id: optional_uuid(row, 2)?,
        key: row.get(3)?,
        kind: row.get(4)?,
        command: row.get(5)?,
        cwd: row.get(6)?,
        required: row.get(7)?,
        timeout_ms: row.get(8)?,
        status: row.get(9)?,
        summary: decode_json(row, 10)?,
        started_at: row.get(11)?,
        completed_at: row.get(12)?,
    })
}

fn row_result(row: &Row<'_>) -> rusqlite::Result<VerificationTestResult> {
    Ok(VerificationTestResult {
        id: uuid(row, 0)?,
        run_id: uuid(row, 1)?,
        gate_id: uuid(row, 2)?,
        suite: row.get(3)?,
        name: row.get(4)?,
        status: row.get(5)?,
        duration_ms: row.get(6)?,
        file_path: row.get(7)?,
        line: row.get(8)?,
        message: row.get(9)?,
        metadata: decode_json(row, 10)?,
    })
}

fn row_artifact(row: &Row<'_>) -> rusqlite::Result<VerificationArtifactRef> {
    Ok(VerificationArtifactRef {
        id: uuid(row, 0)?,
        run_id: uuid(row, 1)?,
        gate_id: optional_uuid(row, 2)?,
        kind: row.get(3)?,
        hash: row.get(4)?,
        size_bytes: row.get(5)?,
        metadata: decode_json(row, 6)?,
        created_at: row.get(7)?,
    })
}

fn row_event(row: &Row<'_>) -> rusqlite::Result<VerificationEvent> {
    Ok(VerificationEvent {
        id: row.get(0)?,
        run_id: uuid(row, 1)?,
        task_id: uuid(row, 2)?,
        gate_id: optional_uuid(row, 3)?,
        kind: row.get(4)?,
        actor: row.get(5)?,
        payload: decode_json(row, 6)?,
        created_at: row.get(7)?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{NewProject, NewTask};

    fn project_and_task(db: &Database) -> (Uuid, Uuid) {
        let project = db
            .projects()
            .create(&NewProject::new("Verification"))
            .unwrap();
        let mut task = NewTask::new("Ship safely");
        task.project_id = Some(project.id);
        let task = db.tasks().create(&task).unwrap();
        (project.id, task.id)
    }

    #[test]
    fn snapshots_commands_records_results_and_builds_report() {
        let db = Database::open_in_memory().unwrap();
        let (project_id, task_id) = project_and_task(&db);
        let repo = db.verification();
        repo.replace_commands(
            project_id,
            &[NewVerificationCommand::new("unit", "test", "cargo test")],
        )
        .unwrap();
        let created = repo
            .create_run(task_id, &[], "manual", None, "test")
            .unwrap();
        let run_id = created.value.run.id;
        let gate_id = created.value.gates[0].id;
        repo.start(run_id, "test").unwrap();
        let result = NewVerificationTestResult {
            id: Uuid::new_v4(),
            suite: Some("storage".into()),
            name: "persists".into(),
            status: "passed".into(),
            duration_ms: Some(4),
            file_path: None,
            line: None,
            message: None,
            metadata: json!({}),
        };
        repo.record_gate(
            run_id,
            gate_id,
            "passed",
            json!({"ok": true}),
            &[result],
            &[],
            "test",
        )
        .unwrap();
        let finished = repo.finish(run_id, "test").unwrap().unwrap();
        assert_eq!(finished.value.run.status, "passed");
        assert_eq!(finished.value.results.len(), 1);
        assert_eq!(repo.history(run_id).unwrap().len(), 4);
        assert!(repo.report(run_id).unwrap().unwrap().tests["gates"].is_array());
    }

    #[test]
    fn required_failed_gate_reopens_and_blocks_completion_until_rerun_passes() {
        let db = Database::open_in_memory().unwrap();
        let (project_id, task_id) = project_and_task(&db);
        let repo = db.verification();
        repo.replace_commands(
            project_id,
            &[NewVerificationCommand::new("build", "build", "cargo build")],
        )
        .unwrap();
        let first = repo
            .create_run(task_id, &[], "manual", None, "test")
            .unwrap();
        repo.start(first.value.run.id, "test").unwrap();
        repo.record_gate(
            first.value.run.id,
            first.value.gates[0].id,
            "failed",
            json!({}),
            &[],
            &[],
            "test",
        )
        .unwrap();
        repo.finish(first.value.run.id, "test").unwrap();
        assert!(
            db.task_planning()
                .set_task_status(task_id, "completed")
                .is_err()
        );
        let second = repo
            .create_run(task_id, &[], "rerun", Some(first.value.run.id), "test")
            .unwrap();
        repo.start(second.value.run.id, "test").unwrap();
        repo.record_gate(
            second.value.run.id,
            second.value.gates[0].id,
            "passed",
            json!({}),
            &[],
            &[],
            "test",
        )
        .unwrap();
        repo.finish(second.value.run.id, "test").unwrap();
        assert!(
            db.task_planning()
                .set_task_status(task_id, "completed")
                .unwrap()
        );
    }

    #[test]
    fn startup_recovery_marks_running_verification_interrupted() {
        let db = Database::open_in_memory().unwrap();
        let (project_id, task_id) = project_and_task(&db);
        let repo = db.verification();
        repo.replace_commands(
            project_id,
            &[NewVerificationCommand::new("unit", "test", "cargo test")],
        )
        .unwrap();
        let created = repo
            .create_run(task_id, &[], "manual", None, "test")
            .unwrap();
        repo.start(created.value.run.id, "test").unwrap();

        let recovery = db.recover_interrupted().unwrap();
        let recovered = repo.get(created.value.run.id).unwrap().unwrap();
        assert_eq!(recovery.interrupted_verifications, 1);
        assert_eq!(recovered.run.status, "error");
        assert_eq!(recovered.gates[0].status, "error");
        assert_eq!(repo.history(created.value.run.id).unwrap().len(), 3);
    }
}
