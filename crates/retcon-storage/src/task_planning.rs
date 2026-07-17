//! Durable task plans, dependencies, and acceptance gates.

#![allow(missing_docs)]

use std::collections::HashSet;

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::repositories::now_ms;
use crate::{Database, Result, StorageError, Task};

const TASK_STATUSES: &[&str] = &[
    "backlog",
    "planned",
    "pending",
    "ready",
    "in_progress",
    "blocked",
    "review",
    "completed",
    "done",
    "failed",
    "cancelled",
];
const STEP_STATUSES: &[&str] = &["pending", "in_progress", "completed", "skipped", "blocked"];

#[derive(Clone, Debug, Default)]
pub struct TaskPatch {
    pub session_id: Option<Option<Uuid>>,
    pub project_id: Option<Option<Uuid>>,
    pub parent_task_id: Option<Option<Uuid>>,
    pub worktree_id: Option<Option<Uuid>>,
    pub branch: Option<Option<String>>,
    pub worktree_path: Option<Option<String>>,
    pub agent: Option<Option<String>>,
    pub provider: Option<Option<String>>,
    pub title: Option<String>,
    pub description: Option<Option<String>>,
    pub priority: Option<i64>,
    pub estimated_cost_micros: Option<Option<i64>>,
    pub actual_cost_micros: Option<Option<i64>>,
    pub cost_currency: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStep {
    pub id: Uuid,
    pub task_id: Uuid,
    pub sequence: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub dependency_ids: Vec<Uuid>,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct PlanStepDraft {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub dependency_ids: Vec<Uuid>,
}

impl PlanStepDraft {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            description: None,
            status: "pending".into(),
            dependency_ids: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterion {
    pub id: Uuid,
    pub task_id: Uuid,
    pub description: String,
    pub status: String,
    pub evidence: Vec<Value>,
    pub sort_order: i64,
    pub is_required: bool,
    pub evaluated_at: Option<i64>,
    pub override_reason: Option<String>,
    pub overridden_by: Option<String>,
    pub overridden_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewAcceptanceCriterion {
    pub id: Uuid,
    pub task_id: Uuid,
    pub description: String,
    pub sort_order: i64,
    pub is_required: bool,
}

impl NewAcceptanceCriterion {
    #[must_use]
    pub fn new(task_id: Uuid, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            task_id,
            description: description.into(),
            sort_order: 0,
            is_required: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionBlockers {
    pub incomplete_step_ids: Vec<Uuid>,
    pub unmet_dependency_ids: Vec<Uuid>,
    pub unsatisfied_criterion_ids: Vec<Uuid>,
    pub verification_gate_ids: Vec<Uuid>,
}

impl CompletionBlockers {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.incomplete_step_ids.is_empty()
            && self.unmet_dependency_ids.is_empty()
            && self.unsatisfied_criterion_ids.is_empty()
            && self.verification_gate_ids.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetails {
    pub task: Task,
    pub dependency_ids: Vec<Uuid>,
    pub steps: Vec<TaskStep>,
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    pub completion_blockers: CompletionBlockers,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMutation<T> {
    pub task_id: Uuid,
    pub reopened: bool,
    pub value: T,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptanceCriterionEvent {
    pub id: i64,
    pub criterion_id: Uuid,
    pub task_id: Uuid,
    pub kind: String,
    pub actor: String,
    pub payload: Value,
    pub created_at: i64,
}

pub struct TaskPlanningRepository<'a>(&'a Database);

impl Database {
    #[must_use]
    pub fn task_planning(&self) -> TaskPlanningRepository<'_> {
        TaskPlanningRepository(self)
    }
}

impl TaskPlanningRepository<'_> {
    pub fn get_details(&self, task_id: Uuid) -> Result<Option<TaskDetails>> {
        let Some(task) = self.0.tasks().get(task_id)? else {
            return Ok(None);
        };
        Ok(Some(TaskDetails {
            task,
            dependency_ids: self.dependencies(task_id)?,
            steps: self.plan(task_id)?,
            acceptance_criteria: self.criteria(task_id)?,
            completion_blockers: self.completion_blockers(task_id)?,
        }))
    }

    pub fn list_tasks(
        &self,
        project_id: Option<Uuid>,
        session_id: Option<Uuid>,
        status: Option<&str>,
    ) -> Result<Vec<Task>> {
        let project = project_id.map(|id| id.as_bytes().to_vec());
        let session = session_id.map(|id| id.as_bytes().to_vec());
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,session_id,project_id,parent_task_id,worktree_id,branch,worktree_path,agent,provider,title,description,status,priority,estimated_cost_micros,actual_cost_micros,cost_currency,created_at,updated_at,started_at,completed_at \
                 FROM tasks WHERE (?1 IS NULL OR project_id=?1) AND (?2 IS NULL OR session_id=?2) \
                 AND (?3 IS NULL OR status=?3) ORDER BY priority DESC,updated_at DESC,id",
            )?;
            statement
                .query_map(params![project, session, status], row_task)?
                .collect()
        })
    }

    pub fn update_task(&self, id: Uuid, patch: &TaskPatch) -> Result<bool> {
        if patch
            .title
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(StorageError::Validation(
                "task title cannot be empty".into(),
            ));
        }
        if patch.parent_task_id == Some(Some(id)) {
            return Err(StorageError::Validation(
                "a task cannot be its own parent".into(),
            ));
        }
        if patch
            .estimated_cost_micros
            .flatten()
            .is_some_and(|value| value < 0)
            || patch
                .actual_cost_micros
                .flatten()
                .is_some_and(|value| value < 0)
        {
            return Err(StorageError::Validation(
                "task costs cannot be negative".into(),
            ));
        }
        if patch.cost_currency.as_deref().is_some_and(|value| {
            value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase())
        }) {
            return Err(StorageError::Validation(
                "cost currency must be a three-letter uppercase code".into(),
            ));
        }
        map_validation(self.0.transaction(|tx| {
            let Some(current) = tx
                .query_row(
                    "SELECT id,session_id,project_id,parent_task_id,worktree_id,branch,worktree_path,agent,provider,title,description,status,priority,estimated_cost_micros,actual_cost_micros,cost_currency,created_at,updated_at,started_at,completed_at FROM tasks WHERE id=?1",
                    [id.as_bytes()],
                    row_task,
                )
                .optional()?
            else {
                return Ok(false);
            };
            let session = patch.session_id.unwrap_or(current.session_id);
            let project = patch.project_id.unwrap_or(current.project_id);
            let parent = patch.parent_task_id.unwrap_or(current.parent_task_id);
            let worktree = patch.worktree_id.unwrap_or(current.worktree_id);
            validate_task_relationships(tx, id, project, session, parent, worktree)?;
            let session_id = optional_uuid_bytes(session);
            let project_id = optional_uuid_bytes(project);
            let parent_task_id = optional_uuid_bytes(parent);
            let worktree_id = optional_uuid_bytes(worktree);
            let branch = patch.branch.clone().unwrap_or(current.branch);
            let worktree_path = patch.worktree_path.clone().unwrap_or(current.worktree_path);
            let agent = patch.agent.clone().unwrap_or(current.agent);
            let provider = patch.provider.clone().unwrap_or(current.provider);
            let title = patch.title.as_deref().unwrap_or(&current.title);
            let description = patch.description.clone().unwrap_or(current.description);
            let priority = patch.priority.unwrap_or(current.priority);
            let estimated_cost = patch
                .estimated_cost_micros
                .unwrap_or(current.estimated_cost_micros);
            let actual_cost = patch
                .actual_cost_micros
                .unwrap_or(current.actual_cost_micros);
            let cost_currency = patch
                .cost_currency
                .as_deref()
                .unwrap_or(&current.cost_currency);
            let now = now_ms();
            Ok(tx.execute(
                "UPDATE tasks SET session_id=?2,project_id=?3,parent_task_id=?4,worktree_id=?5,branch=?6,worktree_path=?7,agent=?8,provider=?9,title=?10,description=?11,priority=?12,estimated_cost_micros=?13,actual_cost_micros=?14,cost_currency=?15,updated_at=?16 WHERE id=?1",
                params![id.as_bytes(), session_id, project_id, parent_task_id, worktree_id, branch, worktree_path, agent, provider, title, description, priority, estimated_cost, actual_cost, cost_currency, now],
            )? > 0)
        }))
    }

    pub fn delete_task(&self, id: Uuid) -> Result<bool> {
        Ok(self
            .0
            .execute("DELETE FROM tasks WHERE id=?1", &[&id.as_bytes()])?
            > 0)
    }

    pub fn set_task_status(&self, id: Uuid, status: &str) -> Result<bool> {
        validate_status(status, TASK_STATUSES, "task")?;
        let now = now_ms();
        let completed = matches!(status, "completed" | "done");
        let completed_at = completed.then_some(now);
        map_validation(self.0.transaction(|tx| {
            if completed && has_completion_blockers(tx, id)? {
                return Err(validation_error(
                    "task cannot complete while plan steps, dependencies, acceptance criteria, or verification gates are unsatisfied",
                ));
            }
            Ok(tx.execute(
                "UPDATE tasks SET status=?2,updated_at=?3,started_at=CASE WHEN ?2='in_progress' THEN COALESCE(started_at,?3) ELSE started_at END,completed_at=?4 WHERE id=?1",
                params![id.as_bytes(), status, now, completed_at],
            )? > 0)
        }))
    }

    pub fn dependencies(&self, task_id: Uuid) -> Result<Vec<Uuid>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT depends_on_task_id FROM task_dependencies WHERE task_id=?1 ORDER BY created_at,depends_on_task_id",
            )?;
            statement
                .query_map([task_id.as_bytes()], |row| uuid(row, 0))?
                .collect()
        })
    }

    pub fn replace_dependencies(
        &self,
        task_id: Uuid,
        dependency_ids: &[Uuid],
    ) -> Result<TaskMutation<()>> {
        if dependency_ids.contains(&task_id) {
            return Err(StorageError::Validation(
                "a task cannot depend on itself".into(),
            ));
        }
        let unique: HashSet<_> = dependency_ids.iter().copied().collect();
        if unique.len() != dependency_ids.len() {
            return Err(StorageError::Validation(
                "task dependencies must be unique".into(),
            ));
        }
        map_validation(self.0.transaction(|tx| {
            let task_project: Option<Vec<u8>> = tx
                .query_row(
                    "SELECT project_id FROM tasks WHERE id=?1",
                    [task_id.as_bytes()],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| validation_error("task does not exist"))?;
            tx.execute("DELETE FROM task_dependencies WHERE task_id=?1", [task_id.as_bytes()])?;
            for dependency_id in dependency_ids {
                let dependency_project: Option<Vec<u8>> = tx
                    .query_row(
                        "SELECT project_id FROM tasks WHERE id=?1",
                        [dependency_id.as_bytes()],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| validation_error(format!("dependency task '{dependency_id}' does not exist")))?;
                if dependency_project != task_project {
                    return Err(validation_error("task dependencies must belong to the same project"));
                }
                let creates_cycle: bool = tx.query_row(
                    "WITH RECURSIVE ancestors(id) AS (SELECT depends_on_task_id FROM task_dependencies WHERE task_id=?1 UNION SELECT d.depends_on_task_id FROM task_dependencies d JOIN ancestors a ON d.task_id=a.id) SELECT EXISTS(SELECT 1 FROM ancestors WHERE id=?2)",
                    params![dependency_id.as_bytes(), task_id.as_bytes()],
                    |row| row.get(0),
                )?;
                if creates_cycle {
                    return Err(validation_error("task dependency cycle detected"));
                }
                tx.execute(
                    "INSERT INTO task_dependencies(task_id,depends_on_task_id,created_at) VALUES (?1,?2,?3)",
                    params![task_id.as_bytes(), dependency_id.as_bytes(), now_ms()],
                )?;
            }
            let reopened = reopen_terminal(tx, task_id)?;
            Ok(TaskMutation { task_id, reopened, value: () })
        }))
    }

    pub fn plan(&self, task_id: Uuid) -> Result<Vec<TaskStep>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,task_id,sequence,title,description,status,updated_at FROM task_steps WHERE task_id=?1 ORDER BY sequence",
            )?;
            let mut steps: Vec<TaskStep> = statement
                .query_map([task_id.as_bytes()], row_step)?
                .collect::<rusqlite::Result<_>>()?;
            let mut dependency_statement = db.prepare(
                "SELECT depends_on_step_id FROM task_step_dependencies WHERE step_id=?1 ORDER BY created_at,depends_on_step_id",
            )?;
            for step in &mut steps {
                step.dependency_ids = dependency_statement
                    .query_map([step.id.as_bytes()], |row| uuid(row, 0))?
                    .collect::<rusqlite::Result<_>>()?;
            }
            Ok(steps)
        })
    }

    pub fn replace_plan(
        &self,
        task_id: Uuid,
        steps: &[PlanStepDraft],
    ) -> Result<TaskMutation<Vec<TaskStep>>> {
        validate_plan(steps)?;
        let reopened = map_validation(self.0.transaction(|tx| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
                [task_id.as_bytes()],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(validation_error("task does not exist"));
            }
            tx.execute("DELETE FROM task_steps WHERE task_id=?1", [task_id.as_bytes()])?;
            for (sequence, step) in steps.iter().enumerate() {
                tx.execute(
                    "INSERT INTO task_steps(id,task_id,sequence,title,description,status,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![step.id.as_bytes(), task_id.as_bytes(), sequence as i64 + 1, step.title.trim(), step.description, step.status, now_ms()],
                )?;
            }
            for step in steps {
                for dependency_id in &step.dependency_ids {
                    tx.execute(
                        "INSERT INTO task_step_dependencies(step_id,depends_on_step_id,created_at) VALUES (?1,?2,?3)",
                        params![step.id.as_bytes(), dependency_id.as_bytes(), now_ms()],
                    )?;
                }
            }
            let reopened = reopen_terminal(tx, task_id)?;
            tx.execute("UPDATE tasks SET updated_at=?2 WHERE id=?1", params![task_id.as_bytes(), now_ms()])?;
            Ok(reopened)
        }))?;
        Ok(TaskMutation {
            task_id,
            reopened,
            value: self.plan(task_id)?,
        })
    }

    pub fn criteria(&self, task_id: Uuid) -> Result<Vec<AcceptanceCriterion>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,task_id,description,status,evidence_json,sort_order,is_required,evaluated_at,override_reason,overridden_by,overridden_at,updated_at FROM acceptance_criteria WHERE task_id=?1 ORDER BY sort_order,updated_at,id",
            )?;
            statement
                .query_map([task_id.as_bytes()], row_criterion)?
                .collect()
        })
    }

    pub fn create_criterion(
        &self,
        criterion: &NewAcceptanceCriterion,
        actor: &str,
    ) -> Result<TaskMutation<AcceptanceCriterion>> {
        if criterion.description.trim().is_empty() {
            return Err(StorageError::Validation(
                "acceptance criterion description cannot be empty".into(),
            ));
        }
        validate_actor(actor)?;
        let now = now_ms();
        let value = AcceptanceCriterion {
            id: criterion.id,
            task_id: criterion.task_id,
            description: criterion.description.trim().into(),
            status: "pending".into(),
            evidence: Vec::new(),
            sort_order: criterion.sort_order,
            is_required: criterion.is_required,
            evaluated_at: None,
            override_reason: None,
            overridden_by: None,
            overridden_at: None,
            updated_at: now,
        };
        let reopened = self.0.transaction(|tx| {
            tx.execute(
                "INSERT INTO acceptance_criteria(id,task_id,description,status,evidence_json,sort_order,is_required,updated_at) VALUES (?1,?2,?3,'pending','[]',?4,?5,?6)",
                params![criterion.id.as_bytes(), criterion.task_id.as_bytes(), criterion.description.trim(), criterion.sort_order, criterion.is_required, now],
            )?;
            record_criterion_event(tx, &value, "created", actor, &json_snapshot(&value)?)?;
            reopen_terminal(tx, criterion.task_id)
        })?;
        Ok(TaskMutation {
            task_id: criterion.task_id,
            reopened,
            value,
        })
    }

    pub fn update_criterion(
        &self,
        id: Uuid,
        description: Option<&str>,
        sort_order: Option<i64>,
        is_required: Option<bool>,
        actor: &str,
    ) -> Result<Option<TaskMutation<AcceptanceCriterion>>> {
        if description.is_some_and(|value| value.trim().is_empty()) {
            return Err(StorageError::Validation(
                "acceptance criterion description cannot be empty".into(),
            ));
        }
        validate_actor(actor)?;
        let now = now_ms();
        self.0.transaction(|tx| {
            let Some(before) = get_criterion_tx(tx, id)? else {
                return Ok(None);
            };
            tx.execute(
                "UPDATE acceptance_criteria SET description=COALESCE(?2,description),sort_order=COALESCE(?3,sort_order),is_required=COALESCE(?4,is_required),updated_at=?5 WHERE id=?1",
                params![id.as_bytes(), description.map(str::trim), sort_order, is_required, now],
            )?;
            let after = get_criterion_tx(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            record_criterion_event(tx, &after, "updated", actor, &json_before_after(&before, &after)?)?;
            let reopened = reopen_terminal(tx, after.task_id)?;
            Ok(Some(TaskMutation { task_id: after.task_id, reopened, value: after }))
        })
    }

    pub fn delete_criterion(
        &self,
        id: Uuid,
        actor: &str,
    ) -> Result<Option<TaskMutation<AcceptanceCriterion>>> {
        validate_actor(actor)?;
        self.0.transaction(|tx| {
            let Some(before) = get_criterion_tx(tx, id)? else {
                return Ok(None);
            };
            record_criterion_event(tx, &before, "deleted", actor, &json_snapshot(&before)?)?;
            tx.execute(
                "DELETE FROM acceptance_criteria WHERE id=?1",
                [id.as_bytes()],
            )?;
            let reopened = reopen_terminal(tx, before.task_id)?;
            Ok(Some(TaskMutation {
                task_id: before.task_id,
                reopened,
                value: before,
            }))
        })
    }

    pub fn add_evidence(
        &self,
        id: Uuid,
        evidence: Value,
        actor: &str,
    ) -> Result<Option<TaskMutation<AcceptanceCriterion>>> {
        validate_actor(actor)?;
        let encoded = serde_json::to_string(&evidence).map_err(|error| {
            StorageError::database(
                "encode acceptance evidence",
                rusqlite::Error::ToSqlConversionFailure(Box::new(error)),
            )
        })?;
        self.0.transaction(|tx| {
            let Some(before) = get_criterion_tx(tx, id)? else {
                return Ok(None);
            };
            tx.execute(
                "UPDATE acceptance_criteria SET evidence_json=json_insert(COALESCE(evidence_json,'[]'),'$[#]',json(?2)),updated_at=?3 WHERE id=?1",
                params![id.as_bytes(), encoded, now_ms()],
            )?;
            let after = get_criterion_tx(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            record_criterion_event(tx, &after, "evidence_added", actor, &json_before_after(&before, &after)?)?;
            Ok(Some(TaskMutation { task_id: after.task_id, reopened: false, value: after }))
        })
    }

    pub fn evaluate_criterion(
        &self,
        id: Uuid,
        passed: bool,
        evidence: Option<Value>,
        actor: &str,
    ) -> Result<Option<TaskMutation<AcceptanceCriterion>>> {
        validate_actor(actor)?;
        let encoded_evidence = evidence
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| {
                StorageError::database(
                    "encode acceptance evidence",
                    rusqlite::Error::ToSqlConversionFailure(Box::new(error)),
                )
            })?;
        let status = if passed { "passed" } else { "failed" };
        let now = now_ms();
        self.0.transaction(|tx| {
            let Some(before) = get_criterion_tx(tx, id)? else {
                return Ok(None);
            };
            if let Some(encoded) = &encoded_evidence {
                tx.execute(
                    "UPDATE acceptance_criteria SET evidence_json=json_insert(COALESCE(evidence_json,'[]'),'$[#]',json(?2)) WHERE id=?1",
                    params![id.as_bytes(), encoded],
                )?;
            }
            tx.execute(
                "UPDATE acceptance_criteria SET status=?2,evaluated_at=?3,override_reason=NULL,overridden_by=NULL,overridden_at=NULL,updated_at=?3 WHERE id=?1",
                params![id.as_bytes(), status, now],
            )?;
            let after = get_criterion_tx(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            record_criterion_event(tx, &after, "evaluated", actor, &json_before_after(&before, &after)?)?;
            let reopened = reopen_terminal(tx, after.task_id)?;
            Ok(Some(TaskMutation { task_id: after.task_id, reopened, value: after }))
        })
    }

    pub fn override_criterion(
        &self,
        id: Uuid,
        reason: &str,
        actor: &str,
    ) -> Result<Option<TaskMutation<AcceptanceCriterion>>> {
        if reason.trim().is_empty() || actor.trim().is_empty() {
            return Err(StorageError::Validation(
                "acceptance override requires a reason and actor".into(),
            ));
        }
        let now = now_ms();
        self.0.transaction(|tx| {
            let Some(before) = get_criterion_tx(tx, id)? else {
                return Ok(None);
            };
            tx.execute(
                "UPDATE acceptance_criteria SET status='overridden',override_reason=?2,overridden_by=?3,overridden_at=?4,updated_at=?4 WHERE id=?1",
                params![id.as_bytes(), reason.trim(), actor.trim(), now],
            )?;
            let after = get_criterion_tx(tx, id)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            record_criterion_event(tx, &after, "overridden", actor, &json_before_after(&before, &after)?)?;
            let reopened = reopen_terminal(tx, after.task_id)?;
            Ok(Some(TaskMutation { task_id: after.task_id, reopened, value: after }))
        })
    }

    pub fn criterion_history(&self, id: Uuid) -> Result<Vec<AcceptanceCriterionEvent>> {
        self.0.read(|db| {
            let mut statement = db.prepare(
                "SELECT id,criterion_id,task_id,kind,actor,payload_json,created_at FROM acceptance_criterion_events WHERE criterion_id=?1 ORDER BY id",
            )?;
            statement.query_map([id.as_bytes()], row_criterion_event)?.collect()
        })
    }

    pub fn completion_blockers(&self, task_id: Uuid) -> Result<CompletionBlockers> {
        self.0.read(|db| {
            let collect = |sql: &str| -> rusqlite::Result<Vec<Uuid>> {
                let mut statement = db.prepare(sql)?;
                statement
                    .query_map([task_id.as_bytes()], |row| uuid(row, 0))?
                    .collect()
            };
            Ok(CompletionBlockers {
                incomplete_step_ids: collect("SELECT id FROM task_steps WHERE task_id=?1 AND status NOT IN ('completed','skipped') ORDER BY sequence")?,
                unmet_dependency_ids: collect("SELECT d.depends_on_task_id FROM task_dependencies d JOIN tasks t ON t.id=d.depends_on_task_id WHERE d.task_id=?1 AND t.status NOT IN ('completed','done') ORDER BY d.created_at")?,
                unsatisfied_criterion_ids: collect("SELECT id FROM acceptance_criteria WHERE task_id=?1 AND is_required=1 AND status NOT IN ('passed','overridden') ORDER BY sort_order,updated_at")?,
                verification_gate_ids: collect("SELECT g.id FROM verification_gates g WHERE g.run_id=(SELECT id FROM verification_runs WHERE task_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1) AND g.is_required=1 AND g.status<>'passed' ORDER BY g.gate_kind,g.command_key")?,
            })
        })
    }
}

const VALIDATION_PREFIX: &str = "retcon_validation:";

fn validation_error(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{VALIDATION_PREFIX}{}", message.into()))
}

pub(crate) fn map_validation<T>(result: Result<T>) -> Result<T> {
    match result {
        Err(StorageError::Database {
            source: rusqlite::Error::InvalidParameterName(message),
            ..
        }) if message.starts_with(VALIDATION_PREFIX) => Err(StorageError::Validation(
            message[VALIDATION_PREFIX.len()..].to_owned(),
        )),
        other => other,
    }
}

fn has_completion_blockers(tx: &Transaction<'_>, task_id: Uuid) -> rusqlite::Result<bool> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_steps WHERE task_id=?1 AND status NOT IN ('completed','skipped')) OR EXISTS(SELECT 1 FROM task_dependencies d JOIN tasks t ON t.id=d.depends_on_task_id WHERE d.task_id=?1 AND t.status NOT IN ('completed','done')) OR EXISTS(SELECT 1 FROM acceptance_criteria WHERE task_id=?1 AND is_required=1 AND status NOT IN ('passed','overridden')) OR EXISTS(SELECT 1 FROM verification_gates g WHERE g.run_id=(SELECT id FROM verification_runs WHERE task_id=?1 ORDER BY created_at DESC,id DESC LIMIT 1) AND g.is_required=1 AND g.status<>'passed')",
        [task_id.as_bytes()],
        |row| row.get(0),
    )
}

fn reopen_terminal(tx: &Transaction<'_>, task_id: Uuid) -> rusqlite::Result<bool> {
    Ok(tx.execute(
        "UPDATE tasks SET status='review',completed_at=NULL,updated_at=?2 WHERE id=?1 AND status IN ('completed','done')",
        params![task_id.as_bytes(), now_ms()],
    )? > 0)
}

pub(crate) fn validate_task_relationships(
    tx: &Transaction<'_>,
    task_id: Uuid,
    project_id: Option<Uuid>,
    session_id: Option<Uuid>,
    parent_task_id: Option<Uuid>,
    worktree_id: Option<Uuid>,
) -> rusqlite::Result<()> {
    if parent_task_id == Some(task_id) {
        return Err(validation_error("a task cannot be its own parent"));
    }
    if let Some(session_id) = session_id {
        let session_project = tx
            .query_row(
                "SELECT project_id FROM sessions WHERE id=?1",
                [session_id.as_bytes()],
                |row| uuid(row, 0),
            )
            .optional()?
            .ok_or_else(|| validation_error("task session does not exist"))?;
        if project_id != Some(session_project) {
            return Err(validation_error(
                "task project must match its session project",
            ));
        }
    }
    if let Some(worktree_id) = worktree_id {
        let worktree_project = tx
            .query_row(
                "SELECT r.project_id FROM git_worktrees w JOIN repository_locations r ON r.id=w.repository_location_id WHERE w.id=?1",
                [worktree_id.as_bytes()],
                |row| uuid(row, 0),
            )
            .optional()?
            .ok_or_else(|| validation_error("task worktree does not exist"))?;
        if project_id != Some(worktree_project) {
            return Err(validation_error(
                "task project must match its worktree project",
            ));
        }
    }
    if let Some(parent_task_id) = parent_task_id {
        let parent_project = tx
            .query_row(
                "SELECT project_id FROM tasks WHERE id=?1",
                [parent_task_id.as_bytes()],
                |row| optional_uuid(row, 0),
            )
            .optional()?
            .ok_or_else(|| validation_error("parent task does not exist"))?;
        if project_id != parent_project {
            return Err(validation_error(
                "parent and child tasks must belong to the same project",
            ));
        }
        let creates_cycle: bool = tx.query_row(
            "WITH RECURSIVE parents(id) AS (SELECT parent_task_id FROM tasks WHERE id=?1 AND parent_task_id IS NOT NULL UNION SELECT t.parent_task_id FROM tasks t JOIN parents p ON t.id=p.id WHERE t.parent_task_id IS NOT NULL) SELECT EXISTS(SELECT 1 FROM parents WHERE id=?2)",
            params![parent_task_id.as_bytes(), task_id.as_bytes()],
            |row| row.get(0),
        )?;
        if creates_cycle {
            return Err(validation_error("task parent cycle detected"));
        }
    }
    let project = optional_uuid_bytes(project_id);
    let inconsistent_related: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE parent_task_id=?1 AND project_id IS NOT ?2) OR EXISTS(SELECT 1 FROM task_dependencies d JOIN tasks other ON other.id=CASE WHEN d.task_id=?1 THEN d.depends_on_task_id ELSE d.task_id END WHERE (d.task_id=?1 OR d.depends_on_task_id=?1) AND other.project_id IS NOT ?2)",
        params![task_id.as_bytes(), project],
        |row| row.get(0),
    )?;
    if inconsistent_related {
        return Err(validation_error(
            "task project must match its parent, children, and dependencies",
        ));
    }
    Ok(())
}

fn validate_actor(actor: &str) -> Result<()> {
    if actor.trim().is_empty() {
        Err(StorageError::Validation(
            "audit actor cannot be empty".into(),
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn validate_new_task_fields(task: &crate::NewTask) -> Result<()> {
    validate_status(&task.status, TASK_STATUSES, "task")?;
    if task.title.trim().is_empty() {
        return Err(StorageError::Validation(
            "task title cannot be empty".into(),
        ));
    }
    if task.estimated_cost_micros.is_some_and(|value| value < 0)
        || task.actual_cost_micros.is_some_and(|value| value < 0)
    {
        return Err(StorageError::Validation(
            "task costs cannot be negative".into(),
        ));
    }
    if task.cost_currency.len() != 3
        || !task
            .cost_currency
            .bytes()
            .all(|byte| byte.is_ascii_uppercase())
    {
        return Err(StorageError::Validation(
            "cost currency must be a three-letter uppercase code".into(),
        ));
    }
    Ok(())
}

fn optional_uuid_bytes(id: Option<Uuid>) -> Option<Vec<u8>> {
    id.map(|value| value.as_bytes().to_vec())
}

fn get_criterion_tx(
    tx: &Transaction<'_>,
    id: Uuid,
) -> rusqlite::Result<Option<AcceptanceCriterion>> {
    tx.query_row(
        "SELECT id,task_id,description,status,evidence_json,sort_order,is_required,evaluated_at,override_reason,overridden_by,overridden_at,updated_at FROM acceptance_criteria WHERE id=?1",
        [id.as_bytes()],
        row_criterion,
    )
    .optional()
}

fn json_snapshot(criterion: &AcceptanceCriterion) -> rusqlite::Result<Value> {
    serde_json::to_value(criterion)
        .map(|value| serde_json::json!({"criterion": value}))
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn json_before_after(
    before: &AcceptanceCriterion,
    after: &AcceptanceCriterion,
) -> rusqlite::Result<Value> {
    let before = serde_json::to_value(before)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let after = serde_json::to_value(after)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    Ok(serde_json::json!({"before": before, "after": after}))
}

fn record_criterion_event(
    tx: &Transaction<'_>,
    criterion: &AcceptanceCriterion,
    kind: &str,
    actor: &str,
    payload: &Value,
) -> rusqlite::Result<()> {
    let payload = serde_json::to_string(payload)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    tx.execute(
        "INSERT INTO acceptance_criterion_events(criterion_id,task_id,kind,actor,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![criterion.id.as_bytes(), criterion.task_id.as_bytes(), kind, actor.trim(), payload, now_ms()],
    )?;
    Ok(())
}

fn validate_plan(steps: &[PlanStepDraft]) -> Result<()> {
    let ids: HashSet<_> = steps.iter().map(|step| step.id).collect();
    if ids.len() != steps.len() {
        return Err(StorageError::Validation(
            "plan step IDs must be unique".into(),
        ));
    }
    for (index, step) in steps.iter().enumerate() {
        if step.title.trim().is_empty() {
            return Err(StorageError::Validation(
                "plan step title cannot be empty".into(),
            ));
        }
        validate_status(&step.status, STEP_STATUSES, "plan step")?;
        let dependencies: HashSet<_> = step.dependency_ids.iter().copied().collect();
        if dependencies.len() != step.dependency_ids.len()
            || step.dependency_ids.contains(&step.id)
            || step.dependency_ids.iter().any(|id| !ids.contains(id))
        {
            return Err(StorageError::Validation(
                "plan step dependencies must be unique IDs from the same plan".into(),
            ));
        }
        if step.dependency_ids.iter().any(|dependency| {
            steps
                .iter()
                .position(|candidate| candidate.id == *dependency)
                .is_some_and(|position| position >= index)
        }) {
            return Err(StorageError::Validation(
                "plan steps may only depend on earlier steps".into(),
            ));
        }
    }
    Ok(())
}

fn validate_status(status: &str, allowed: &[&str], kind: &str) -> Result<()> {
    if allowed.contains(&status) {
        Ok(())
    } else {
        Err(StorageError::Validation(format!(
            "unsupported {kind} status '{status}'"
        )))
    }
}

fn row_task(row: &Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: uuid(row, 0)?,
        session_id: optional_uuid(row, 1)?,
        project_id: optional_uuid(row, 2)?,
        parent_task_id: optional_uuid(row, 3)?,
        worktree_id: optional_uuid(row, 4)?,
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

fn row_step(row: &Row<'_>) -> rusqlite::Result<TaskStep> {
    Ok(TaskStep {
        id: uuid(row, 0)?,
        task_id: uuid(row, 1)?,
        sequence: row.get(2)?,
        title: row.get(3)?,
        description: row.get(4)?,
        status: row.get(5)?,
        dependency_ids: Vec::new(),
        updated_at: row.get(6)?,
    })
}

fn row_criterion(row: &Row<'_>) -> rusqlite::Result<AcceptanceCriterion> {
    let evidence: Option<String> = row.get(4)?;
    let evidence = evidence
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?
        .unwrap_or_default();
    Ok(AcceptanceCriterion {
        id: uuid(row, 0)?,
        task_id: uuid(row, 1)?,
        description: row.get(2)?,
        status: row.get(3)?,
        evidence,
        sort_order: row.get(5)?,
        is_required: row.get(6)?,
        evaluated_at: row.get(7)?,
        override_reason: row.get(8)?,
        overridden_by: row.get(9)?,
        overridden_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn row_criterion_event(row: &Row<'_>) -> rusqlite::Result<AcceptanceCriterionEvent> {
    let payload: String = row.get(5)?;
    let payload = serde_json::from_str(&payload).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            payload.len(),
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })?;
    Ok(AcceptanceCriterionEvent {
        id: row.get(0)?,
        criterion_id: uuid(row, 1)?,
        task_id: uuid(row, 2)?,
        kind: row.get(3)?,
        actor: row.get(4)?,
        payload,
        created_at: row.get(6)?,
    })
}

fn uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    Uuid::from_slice(&row.get::<_, Vec<u8>>(index)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(16, rusqlite::types::Type::Blob, Box::new(error))
    })
}

fn optional_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<Uuid>> {
    row.get::<_, Option<Vec<u8>>>(index)?
        .map(|bytes| Uuid::from_slice(&bytes))
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                16,
                rusqlite::types::Type::Blob,
                Box::new(error),
            )
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{NewProject, NewTask};
    use std::sync::{Arc, Barrier};

    fn task_in_project(db: &Database, project_id: Uuid, title: &str) -> Task {
        let mut task = NewTask::new(title);
        task.project_id = Some(project_id);
        db.tasks().create(&task).unwrap()
    }

    fn task(db: &Database, title: &str) -> Task {
        let project = db.projects().create(&NewProject::new(title)).unwrap();
        task_in_project(db, project.id, title)
    }

    #[test]
    fn plan_round_trips_dependencies_and_reordering() {
        let db = Database::open_in_memory().unwrap();
        let task = task(&db, "Plan");
        let first = PlanStepDraft::new("Build");
        let mut second = PlanStepDraft::new("Test");
        second.dependency_ids.push(first.id);
        let stored = db
            .task_planning()
            .replace_plan(task.id, &[first.clone(), second.clone()])
            .unwrap();
        assert_eq!(stored.value[1].dependency_ids, vec![first.id]);

        let reordered = db.task_planning().replace_plan(task.id, &[first]).unwrap();
        assert_eq!(reordered.value.len(), 1);
        assert_eq!(reordered.value[0].sequence, 1);
    }

    #[test]
    fn completion_is_gated_by_steps_dependencies_and_criteria() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Gate")).unwrap();
        let prerequisite = task_in_project(&db, project.id, "Prerequisite");
        let target = task_in_project(&db, project.id, "Target");
        db.task_planning()
            .replace_dependencies(target.id, &[prerequisite.id])
            .unwrap();
        let step = PlanStepDraft::new("Do it");
        db.task_planning()
            .replace_plan(target.id, std::slice::from_ref(&step))
            .unwrap();
        let criterion = db
            .task_planning()
            .create_criterion(
                &NewAcceptanceCriterion::new(target.id, "It works"),
                "local_user",
            )
            .unwrap();

        assert!(
            db.task_planning()
                .set_task_status(target.id, "completed")
                .is_err()
        );
        db.task_planning()
            .set_task_status(prerequisite.id, "completed")
            .unwrap();
        let mut complete_step = step;
        complete_step.status = "completed".into();
        db.task_planning()
            .replace_plan(target.id, &[complete_step])
            .unwrap();
        db.task_planning()
            .evaluate_criterion(
                criterion.value.id,
                true,
                Some(serde_json::json!({"test":"ok"})),
                "local_user",
            )
            .unwrap();
        assert!(
            db.task_planning()
                .set_task_status(target.id, "completed")
                .unwrap()
        );
    }

    #[test]
    fn failed_criterion_requires_an_auditable_override() {
        let db = Database::open_in_memory().unwrap();
        let task = task(&db, "Override");
        let criterion = db
            .task_planning()
            .create_criterion(
                &NewAcceptanceCriterion::new(task.id, "Manual check"),
                "local_user",
            )
            .unwrap();
        let failed = db
            .task_planning()
            .evaluate_criterion(criterion.value.id, false, None, "local_user")
            .unwrap()
            .unwrap();
        assert_eq!(failed.value.status, "failed");
        assert!(
            db.task_planning()
                .override_criterion(criterion.value.id, "", "local_user")
                .is_err()
        );
        let overridden = db
            .task_planning()
            .override_criterion(criterion.value.id, "Accepted risk", "local_user")
            .unwrap()
            .unwrap();
        assert_eq!(overridden.value.status, "overridden");
        assert_eq!(
            overridden.value.override_reason.as_deref(),
            Some("Accepted risk")
        );
    }

    #[test]
    fn dependency_cycles_are_rejected() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Cycles")).unwrap();
        let first = task_in_project(&db, project.id, "First");
        let second = task_in_project(&db, project.id, "Second");
        db.task_planning()
            .replace_dependencies(first.id, &[second.id])
            .unwrap();
        assert!(
            db.task_planning()
                .replace_dependencies(second.id, &[first.id])
                .is_err()
        );
    }

    #[test]
    fn full_roadmap_task_fields_round_trip_and_are_editable() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Roadmap")).unwrap();
        let parent = task_in_project(&db, project.id, "Parent");
        let mut input = NewTask::new("Phase 21");
        input.project_id = Some(project.id);
        input.parent_task_id = Some(parent.id);
        input.branch = Some("codex/phase21".into());
        input.worktree_path = Some("C:/worktrees/phase21".into());
        input.agent = Some("core-specialist".into());
        input.provider = Some("codex".into());
        input.estimated_cost_micros = Some(2_000_000);
        let created = db.tasks().create(&input).unwrap();

        db.task_planning()
            .update_task(
                created.id,
                &TaskPatch {
                    actual_cost_micros: Some(Some(1_750_000)),
                    ..TaskPatch::default()
                },
            )
            .unwrap();
        db.task_planning()
            .set_task_status(created.id, "in_progress")
            .unwrap();
        let stored = db.tasks().get(created.id).unwrap().unwrap();
        assert_eq!(stored.project_id, Some(project.id));
        assert_eq!(stored.parent_task_id, Some(parent.id));
        assert_eq!(stored.branch.as_deref(), Some("codex/phase21"));
        assert_eq!(stored.provider.as_deref(), Some("codex"));
        assert_eq!(stored.actual_cost_micros, Some(1_750_000));
        assert!(stored.started_at.is_some());
    }

    #[test]
    fn blocker_mutation_reopens_completed_task_and_preserves_override_history() {
        let db = Database::open_in_memory().unwrap();
        let task = task(&db, "Audited");
        let criterion = db
            .task_planning()
            .create_criterion(
                &NewAcceptanceCriterion::new(task.id, "Manual"),
                "local_user",
            )
            .unwrap();
        db.task_planning()
            .override_criterion(criterion.value.id, "Accepted risk", "local_user")
            .unwrap();
        db.task_planning()
            .set_task_status(task.id, "completed")
            .unwrap();

        let failed = db
            .task_planning()
            .evaluate_criterion(criterion.value.id, false, None, "local_user")
            .unwrap()
            .unwrap();
        assert!(failed.reopened);
        let stored = db.tasks().get(task.id).unwrap().unwrap();
        assert_eq!(stored.status, "review");
        assert!(stored.completed_at.is_none());
        db.task_planning()
            .delete_criterion(criterion.value.id, "local_user")
            .unwrap()
            .unwrap();
        let history = db
            .task_planning()
            .criterion_history(criterion.value.id)
            .unwrap();
        assert!(history.iter().any(|event| event.kind == "overridden"));
        assert!(history.iter().any(|event| event.kind == "deleted"));
        assert_eq!(history.last().unwrap().actor, "local_user");
    }

    #[test]
    fn concurrent_dependency_updates_cannot_create_a_cycle() {
        let db = Database::open_in_memory().unwrap();
        let project = db
            .projects()
            .create(&NewProject::new("Concurrent"))
            .unwrap();
        let first = task_in_project(&db, project.id, "First");
        let second = task_in_project(&db, project.id, "Second");
        let barrier = Arc::new(Barrier::new(3));
        let run = |task_id, dependency_id, db: Database, barrier: Arc<Barrier>| {
            std::thread::spawn(move || {
                barrier.wait();
                db.task_planning()
                    .replace_dependencies(task_id, &[dependency_id])
            })
        };
        let left = run(first.id, second.id, db.clone(), barrier.clone());
        let right = run(second.id, first.id, db.clone(), barrier.clone());
        barrier.wait();
        let results = [left.join().unwrap(), right.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
    }

    #[test]
    fn completion_and_blocker_mutation_are_atomic_under_concurrency() {
        let db = Database::open_in_memory().unwrap();
        let task = task(&db, "Atomic completion");
        let task_id = task.id;
        let barrier = Arc::new(Barrier::new(3));
        let completion_db = db.clone();
        let completion_barrier = barrier.clone();
        let completion = std::thread::spawn(move || {
            completion_barrier.wait();
            completion_db
                .task_planning()
                .set_task_status(task_id, "completed")
        });
        let plan_db = db.clone();
        let plan_barrier = barrier.clone();
        let plan = std::thread::spawn(move || {
            plan_barrier.wait();
            plan_db
                .task_planning()
                .replace_plan(task_id, &[PlanStepDraft::new("New blocker")])
        });
        barrier.wait();
        let _ = completion.join().unwrap();
        plan.join().unwrap().unwrap();

        let details = db.task_planning().get_details(task_id).unwrap().unwrap();
        assert!(!details.completion_blockers.is_empty());
        assert!(!matches!(
            details.task.status.as_str(),
            "completed" | "done"
        ));
        assert!(details.task.completed_at.is_none());
    }

    #[test]
    fn parent_cycles_and_cross_project_dependencies_are_rejected() {
        let db = Database::open_in_memory().unwrap();
        let first = task(&db, "First project");
        let second = task(&db, "Second project");
        assert!(
            db.task_planning()
                .replace_dependencies(first.id, &[second.id])
                .is_err()
        );

        let project = first.project_id.unwrap();
        let peer = task_in_project(&db, project, "Peer");
        db.task_planning()
            .update_task(
                first.id,
                &TaskPatch {
                    parent_task_id: Some(Some(peer.id)),
                    ..TaskPatch::default()
                },
            )
            .unwrap();
        assert!(
            db.task_planning()
                .update_task(
                    peer.id,
                    &TaskPatch {
                        parent_task_id: Some(Some(first.id)),
                        ..TaskPatch::default()
                    },
                )
                .is_err()
        );
    }

    #[test]
    fn low_level_create_rejects_unknown_status() {
        let db = Database::open_in_memory().unwrap();
        let mut task = NewTask::new("Invalid");
        task.status = "mystery".into();
        assert!(db.tasks().create(&task).is_err());
    }
}
