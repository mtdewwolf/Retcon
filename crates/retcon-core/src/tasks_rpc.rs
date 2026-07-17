//! RPC adapter for durable task planning and acceptance gates.

use retcon_protocol::{PlanStepInput, TaskCreateParams, TaskStatus};
use retcon_storage::{NewAcceptanceCriterion, NewTask, PlanStepDraft, StorageError, TaskPatch};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;

const LOCAL_ACTOR: &str = "local_user";

fn error(id: u64, error: CoreError) -> Response {
    Response::error(id, &error)
}

fn invalid(id: u64, message: impl Into<String>) -> Response {
    error(
        id,
        CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The task request contains invalid information.",
            message,
        ),
    )
}

fn missing(id: u64, kind: &str) -> Response {
    error(
        id,
        CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "The requested task information could not be found.",
            format!("{kind} was not found"),
        ),
    )
}

fn storage_error(id: u64, error: StorageError) -> Response {
    if let StorageError::Validation(message) = error {
        invalid(id, message)
    } else {
        Response::error(id, &CoreError::from(error))
    }
}

fn required_uuid(params: &Value, name: &str) -> Result<Uuid, String> {
    params
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing '{name}' parameter"))
        .and_then(|raw| Uuid::parse_str(raw).map_err(|_| format!("invalid '{name}' UUID")))
}

fn optional_uuid(params: &Value, name: &str) -> Result<Option<Uuid>, String> {
    let Some(value) = params.get(name) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    value
        .as_str()
        .ok_or_else(|| format!("'{name}' must be a UUID string or null"))
        .and_then(|raw| Uuid::parse_str(raw).map_err(|_| format!("invalid '{name}' UUID")))
        .map(Some)
}

fn uuid_list(params: &Value, name: &str) -> Result<Vec<Uuid>, String> {
    params
        .get(name)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("missing '{name}' array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| format!("'{name}' entries must be UUID strings"))
                .and_then(|raw| {
                    Uuid::parse_str(raw).map_err(|_| format!("invalid UUID in '{name}'"))
                })
        })
        .collect()
}

fn nullable_uuid(params: &Value, name: &str) -> Result<Option<Option<Uuid>>, String> {
    let Some(value) = params.get(name) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(None));
    }
    value
        .as_str()
        .ok_or_else(|| format!("'{name}' must be a UUID string or null"))
        .and_then(|raw| Uuid::parse_str(raw).map_err(|_| format!("invalid '{name}' UUID")))
        .map(|id| Some(Some(id)))
}

fn nullable_string(params: &Value, name: &str) -> Result<Option<Option<String>>, String> {
    let Some(value) = params.get(name) else {
        return Ok(None);
    };
    if value.is_null() {
        Ok(Some(None))
    } else {
        value
            .as_str()
            .map(|value| Some(Some(value.to_owned())))
            .ok_or_else(|| format!("'{name}' must be a string or null"))
    }
}

fn emit_changed(state: &CoreState, task_id: Uuid, operation: &str) {
    state.emit(
        "task.changed",
        json!({"taskId": task_id, "operation": operation}),
    );
}

fn emit_mutation(state: &CoreState, task_id: Uuid, operation: &str, reopened: bool) {
    state.emit(
        "task.changed",
        json!({"taskId": task_id, "operation": operation, "reopened": reopened}),
    );
    if reopened {
        state.emit(
            "task.reopened",
            json!({"taskId": task_id, "reason": operation, "actor": LOCAL_ACTOR}),
        );
    }
}

fn emit_acceptance_audit(
    state: &CoreState,
    operation: &str,
    mutation: &retcon_storage::TaskMutation<retcon_storage::AcceptanceCriterion>,
) {
    let audit_kind = format!("task.acceptance.{operation}");
    state.emit(
        &audit_kind,
        json!({
            "taskId": mutation.task_id,
            "criterionId": mutation.value.id,
            "operation": operation,
            "actor": LOCAL_ACTOR,
            "reopened": mutation.reopened,
        }),
    );
    emit_mutation(
        state,
        mutation.task_id,
        &format!("acceptance.{operation}"),
        mutation.reopened,
    );
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let repository = state.storage().database().task_planning();
    match method.as_str() {
        "task.create" => {
            let input: TaskCreateParams = match serde_json::from_value(params) {
                Ok(input) => input,
                Err(error) => return invalid(id, error.to_string()),
            };
            if input.title.trim().is_empty() {
                return invalid(id, "task title cannot be empty");
            }
            if input.estimated_cost_micros.is_some_and(|value| value < 0)
                || input.actual_cost_micros.is_some_and(|value| value < 0)
            {
                return invalid(id, "task costs cannot be negative");
            }
            if input.cost_currency.len() != 3
                || !input
                    .cost_currency
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase())
            {
                return invalid(id, "costCurrency must be a three-letter uppercase code");
            }
            let mut task = NewTask::new(input.title.trim());
            task.id = input.id.unwrap_or_else(Uuid::new_v4);
            if input.parent_task_id == Some(task.id) {
                return invalid(id, "a task cannot be its own parent");
            }
            task.project_id = input.project_id;
            task.session_id = input.session_id;
            task.parent_task_id = input.parent_task_id;
            task.worktree_id = input.worktree_id;
            task.branch = input.branch;
            task.worktree_path = input.worktree_path;
            task.agent = input.agent;
            task.provider = input.provider;
            task.description = input.description;
            let requested_status = input.status;
            task.status = "backlog".into();
            task.priority = input.priority;
            task.estimated_cost_micros = input.estimated_cost_micros;
            task.actual_cost_micros = input.actual_cost_micros;
            task.cost_currency = input.cost_currency;
            let created = match state.storage().database().tasks().create(&task) {
                Ok(task) => task,
                Err(error) => return storage_error(id, error),
            };
            if let Err(error) = repository.replace_dependencies(created.id, &input.dependency_ids) {
                let _ = repository.delete_task(created.id);
                return storage_error(id, error);
            }
            if requested_status != TaskStatus::Backlog
                && let Err(error) =
                    repository.set_task_status(created.id, requested_status.as_str())
            {
                let _ = repository.delete_task(created.id);
                return storage_error(id, error);
            }
            emit_changed(&state, created.id, "created");
            match repository.get_details(created.id) {
                Ok(Some(details)) => Response::ok(id, json!({"task": details})),
                Ok(None) => missing(id, "task"),
                Err(error) => storage_error(id, error),
            }
        }
        "task.get" => match required_uuid(&params, "taskId") {
            Ok(task_id) => match repository.get_details(task_id) {
                Ok(Some(details)) => Response::ok(id, json!({"task": details})),
                Ok(None) => missing(id, "task"),
                Err(error) => storage_error(id, error),
            },
            Err(message) => invalid(id, message),
        },
        "task.list" => {
            let project_id = match optional_uuid(&params, "projectId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let session_id = match optional_uuid(&params, "sessionId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let status = match params.get("status") {
                None | Some(Value::Null) => None,
                Some(value) => match serde_json::from_value::<TaskStatus>(value.clone()) {
                    Ok(status) => Some(status),
                    Err(error) => return invalid(id, format!("invalid 'status': {error}")),
                },
            };
            match repository.list_tasks(project_id, session_id, status.map(TaskStatus::as_str)) {
                Ok(tasks) => Response::ok(id, json!({"tasks": tasks})),
                Err(error) => storage_error(id, error),
            }
        }
        "task.update" => {
            let task_id = match required_uuid(&params, "taskId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let patch = match task_patch(&params) {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.update_task(task_id, &patch) {
                Ok(true) => {
                    emit_changed(&state, task_id, "updated");
                    match repository.get_details(task_id) {
                        Ok(Some(details)) => Response::ok(id, json!({"task": details})),
                        Ok(None) => missing(id, "task"),
                        Err(error) => storage_error(id, error),
                    }
                }
                Ok(false) => missing(id, "task"),
                Err(error) => storage_error(id, error),
            }
        }
        "task.delete" => mutate_bool(&state, id, &params, "deleted", |task_id| {
            repository.delete_task(task_id)
        }),
        "task.status.set" => {
            let task_id = match required_uuid(&params, "taskId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let status: TaskStatus = match params.get("status").cloned().map(serde_json::from_value)
            {
                Some(Ok(value)) => value,
                Some(Err(error)) => return invalid(id, error.to_string()),
                None => return invalid(id, "missing 'status' parameter"),
            };
            match repository.set_task_status(task_id, status.as_str()) {
                Ok(true) => {
                    emit_changed(&state, task_id, "status");
                    match repository.get_details(task_id) {
                        Ok(Some(details)) => Response::ok(id, json!({"task": details})),
                        Ok(None) => missing(id, "task"),
                        Err(error) => storage_error(id, error),
                    }
                }
                Ok(false) => missing(id, "task"),
                Err(error) => storage_error(id, error),
            }
        }
        "task.dependencies.replace" => {
            let task_id = match required_uuid(&params, "taskId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let dependencies = match uuid_list(&params, "dependencyIds") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.replace_dependencies(task_id, &dependencies) {
                Ok(mutation) => {
                    emit_mutation(&state, task_id, "dependencies", mutation.reopened);
                    Response::ok(id, json!({"dependencyIds": dependencies}))
                }
                Err(error) => storage_error(id, error),
            }
        }
        "task.plan.get" => match required_uuid(&params, "taskId") {
            Ok(task_id) => match repository.plan(task_id) {
                Ok(steps) => Response::ok(id, json!({"steps": steps})),
                Err(error) => storage_error(id, error),
            },
            Err(message) => invalid(id, message),
        },
        "task.plan.replace" => replace_plan(&state, id, &params),
        "task.acceptance.list" => match required_uuid(&params, "taskId") {
            Ok(task_id) => match repository.criteria(task_id) {
                Ok(criteria) => Response::ok(id, json!({"acceptanceCriteria": criteria})),
                Err(error) => storage_error(id, error),
            },
            Err(message) => invalid(id, message),
        },
        "task.acceptance.create" => create_criterion(&state, id, &params),
        "task.acceptance.update" => update_criterion(&state, id, &params),
        "task.acceptance.delete" => {
            let criterion_id = match required_uuid(&params, "criterionId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.delete_criterion(criterion_id, LOCAL_ACTOR) {
                Ok(Some(mutation)) => {
                    emit_acceptance_audit(&state, "deleted", &mutation);
                    Response::ok(id, json!({"deleted": true}))
                }
                Ok(None) => missing(id, "acceptance criterion"),
                Err(error) => storage_error(id, error),
            }
        }
        "task.acceptance.evidence" => {
            let criterion_id = match required_uuid(&params, "criterionId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let Some(evidence) = params.get("evidence").cloned() else {
                return invalid(id, "missing 'evidence' parameter");
            };
            criterion_result(
                &state,
                id,
                "evidence_added",
                repository.add_evidence(criterion_id, evidence, LOCAL_ACTOR),
            )
        }
        "task.acceptance.evaluate" => {
            let criterion_id = match required_uuid(&params, "criterionId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let Some(passed) = params.get("passed").and_then(Value::as_bool) else {
                return invalid(id, "missing boolean 'passed' parameter");
            };
            criterion_result(
                &state,
                id,
                "evaluated",
                repository.evaluate_criterion(
                    criterion_id,
                    passed,
                    params.get("evidence").cloned(),
                    LOCAL_ACTOR,
                ),
            )
        }
        "task.acceptance.override" => {
            let criterion_id = match required_uuid(&params, "criterionId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let Some(reason) = params.get("reason").and_then(Value::as_str) else {
                return invalid(id, "missing 'reason' parameter");
            };
            criterion_result(
                &state,
                id,
                "overridden",
                repository.override_criterion(criterion_id, reason, LOCAL_ACTOR),
            )
        }
        _ => error(
            id,
            CoreError::new(
                ErrorCode::NotFound,
                ErrorSource::Rpc,
                "The requested task operation is not available.",
                format!("unknown RPC method: {method}"),
            ),
        ),
    }
}

fn task_patch(params: &Value) -> Result<TaskPatch, String> {
    let title = match params.get("title") {
        None => None,
        Some(value) => Some(
            value
                .as_str()
                .ok_or_else(|| "'title' must be a string".to_owned())?
                .to_owned(),
        ),
    };
    Ok(TaskPatch {
        session_id: nullable_uuid(params, "sessionId")?,
        project_id: nullable_uuid(params, "projectId")?,
        parent_task_id: nullable_uuid(params, "parentTaskId")?,
        worktree_id: nullable_uuid(params, "worktreeId")?,
        branch: nullable_string(params, "branch")?,
        worktree_path: nullable_string(params, "worktreePath")?,
        agent: nullable_string(params, "agent")?,
        provider: nullable_string(params, "provider")?,
        title,
        description: nullable_string(params, "description")?,
        priority: params
            .get("priority")
            .map(|value| {
                value
                    .as_i64()
                    .ok_or_else(|| "'priority' must be an integer".to_owned())
            })
            .transpose()?,
        estimated_cost_micros: nullable_i64(params, "estimatedCostMicros")?,
        actual_cost_micros: nullable_i64(params, "actualCostMicros")?,
        cost_currency: params
            .get("costCurrency")
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "'costCurrency' must be a string".to_owned())
            })
            .transpose()?,
    })
}

fn nullable_i64(params: &Value, name: &str) -> Result<Option<Option<i64>>, String> {
    let Some(value) = params.get(name) else {
        return Ok(None);
    };
    if value.is_null() {
        Ok(Some(None))
    } else {
        value
            .as_i64()
            .map(|value| Some(Some(value)))
            .ok_or_else(|| format!("'{name}' must be an integer or null"))
    }
}

fn mutate_bool(
    state: &CoreState,
    id: u64,
    params: &Value,
    operation: &str,
    mutation: impl FnOnce(Uuid) -> Result<bool, StorageError>,
) -> Response {
    let task_id = match required_uuid(params, "taskId") {
        Ok(value) => value,
        Err(message) => return invalid(id, message),
    };
    match mutation(task_id) {
        Ok(true) => {
            emit_changed(state, task_id, operation);
            Response::ok(id, json!({"accepted": true}))
        }
        Ok(false) => missing(id, "task"),
        Err(error) => storage_error(id, error),
    }
}

fn replace_plan(state: &CoreState, id: u64, params: &Value) -> Response {
    let task_id = match required_uuid(params, "taskId") {
        Ok(value) => value,
        Err(message) => return invalid(id, message),
    };
    let Some(raw_steps) = params.get("steps").cloned() else {
        return invalid(id, "missing 'steps' parameter");
    };
    let input: Vec<PlanStepInput> = match serde_json::from_value(raw_steps) {
        Ok(value) => value,
        Err(error) => return invalid(id, error.to_string()),
    };
    let steps: Vec<_> = input
        .into_iter()
        .map(|step| PlanStepDraft {
            id: step.id.unwrap_or_else(Uuid::new_v4),
            title: step.title,
            description: step.description,
            status: step.status,
            dependency_ids: step.dependency_ids,
        })
        .collect();
    match state
        .storage()
        .database()
        .task_planning()
        .replace_plan(task_id, &steps)
    {
        Ok(mutation) => {
            emit_mutation(state, task_id, "plan", mutation.reopened);
            Response::ok(id, json!({"steps": mutation.value}))
        }
        Err(error) => storage_error(id, error),
    }
}

fn create_criterion(state: &CoreState, id: u64, params: &Value) -> Response {
    let task_id = match required_uuid(params, "taskId") {
        Ok(value) => value,
        Err(message) => return invalid(id, message),
    };
    let Some(description) = params.get("description").and_then(Value::as_str) else {
        return invalid(id, "missing 'description' parameter");
    };
    let mut criterion = NewAcceptanceCriterion::new(task_id, description);
    criterion.id = match optional_uuid(params, "criterionId") {
        Ok(value) => value.unwrap_or_else(Uuid::new_v4),
        Err(message) => return invalid(id, message),
    };
    criterion.sort_order = match params.get("sortOrder") {
        None => 0,
        Some(value) => match value.as_i64() {
            Some(value) => value,
            None => return invalid(id, "'sortOrder' must be an integer"),
        },
    };
    criterion.is_required = match params.get("required") {
        None => true,
        Some(value) => match value.as_bool() {
            Some(value) => value,
            None => return invalid(id, "'required' must be a boolean"),
        },
    };
    match state
        .storage()
        .database()
        .task_planning()
        .create_criterion(&criterion, LOCAL_ACTOR)
    {
        Ok(mutation) => {
            emit_acceptance_audit(state, "created", &mutation);
            Response::ok(id, json!({"acceptanceCriterion": mutation.value}))
        }
        Err(error) => storage_error(id, error),
    }
}

fn update_criterion(state: &CoreState, id: u64, params: &Value) -> Response {
    let criterion_id = match required_uuid(params, "criterionId") {
        Ok(value) => value,
        Err(message) => return invalid(id, message),
    };
    let sort_order = match params.get("sortOrder") {
        None => None,
        Some(value) => match value.as_i64() {
            Some(value) => Some(value),
            None => return invalid(id, "'sortOrder' must be an integer"),
        },
    };
    let required = match params.get("required") {
        None => None,
        Some(value) => match value.as_bool() {
            Some(value) => Some(value),
            None => return invalid(id, "'required' must be a boolean"),
        },
    };
    let description = match params.get("description") {
        None => None,
        Some(value) => match value.as_str() {
            Some(value) => Some(value),
            None => return invalid(id, "'description' must be a string"),
        },
    };
    match state.storage().database().task_planning().update_criterion(
        criterion_id,
        description,
        sort_order,
        required,
        LOCAL_ACTOR,
    ) {
        Ok(Some(mutation)) => {
            emit_acceptance_audit(state, "updated", &mutation);
            Response::ok(id, json!({"acceptanceCriterion": mutation.value}))
        }
        Ok(None) => missing(id, "acceptance criterion"),
        Err(error) => storage_error(id, error),
    }
}

fn criterion_result(
    state: &CoreState,
    id: u64,
    operation: &str,
    result: Result<
        Option<retcon_storage::TaskMutation<retcon_storage::AcceptanceCriterion>>,
        StorageError,
    >,
) -> Response {
    match result {
        Ok(Some(mutation)) => {
            emit_acceptance_audit(state, operation, &mutation);
            Response::ok(id, json!({"acceptanceCriterion": mutation.value}))
        }
        Ok(None) => missing(id, "acceptance criterion"),
        Err(error) => storage_error(id, error),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::rpc::Request;

    #[tokio::test]
    async fn rpc_completion_requires_passing_required_criteria() {
        let dir = tempfile::tempdir().unwrap();
        let state = CoreState::new(dir.path()).unwrap();
        let created = handle(
            state.clone(),
            Request {
                id: 1,
                method: "task.create".into(),
                params: json!({"title":"Ship phase 21"}),
            },
        )
        .await;
        let task_id = created.result.unwrap()["task"]["task"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let criterion = handle(
            state.clone(),
            Request {
                id: 2,
                method: "task.acceptance.create".into(),
                params: json!({"taskId":task_id,"description":"Tests pass"}),
            },
        )
        .await;
        let criterion_id = criterion.result.unwrap()["acceptanceCriterion"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let blocked = handle(
            state.clone(),
            Request {
                id: 3,
                method: "task.status.set".into(),
                params: json!({"taskId":task_id,"status":"completed"}),
            },
        )
        .await;
        assert!(blocked.error.is_some());
        handle(
            state.clone(),
            Request {
                id: 4,
                method: "task.acceptance.evaluate".into(),
                params: json!({"criterionId":criterion_id,"passed":true,"evidence":{"suite":"ok"}}),
            },
        )
        .await;
        let completed = handle(
            state,
            Request {
                id: 5,
                method: "task.status.set".into(),
                params: json!({"taskId":task_id,"status":"completed"}),
            },
        )
        .await;
        assert!(completed.error.is_none());
    }

    #[tokio::test]
    async fn rpc_rejects_wrong_typed_filters_instead_of_broadening_scope() {
        let dir = tempfile::tempdir().unwrap();
        let state = CoreState::new(dir.path()).unwrap();
        let response = handle(
            state,
            Request {
                id: 1,
                method: "task.list".into(),
                params: json!({"projectId": 42, "status": true}),
            },
        )
        .await;
        assert!(response.result.is_none());
        assert_eq!(response.error.unwrap()["code"], "invalid_request");
    }

    #[tokio::test]
    async fn rpc_uses_server_actor_and_emits_audit_for_override() {
        let dir = tempfile::tempdir().unwrap();
        let state = CoreState::new(dir.path()).unwrap();
        let created = handle(
            state.clone(),
            Request {
                id: 1,
                method: "task.create".into(),
                params: json!({"title":"Audited task"}),
            },
        )
        .await;
        let task_id = created.result.unwrap()["task"]["task"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let criterion = handle(
            state.clone(),
            Request {
                id: 2,
                method: "task.acceptance.create".into(),
                params: json!({"taskId":task_id,"description":"Manual review"}),
            },
        )
        .await;
        let criterion_id = criterion.result.unwrap()["acceptanceCriterion"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let override_response = handle(
            state.clone(),
            Request {
                id: 3,
                method: "task.acceptance.override".into(),
                params: json!({
                    "criterionId": criterion_id,
                    "reason": "Accepted locally",
                    "actor": "spoofed_remote_actor"
                }),
            },
        )
        .await;
        assert_eq!(
            override_response.result.unwrap()["acceptanceCriterion"]["overriddenBy"],
            LOCAL_ACTOR
        );
        let history = state
            .storage()
            .database()
            .task_planning()
            .criterion_history(Uuid::parse_str(&criterion_id).unwrap())
            .unwrap();
        assert_eq!(history.last().unwrap().actor, LOCAL_ACTOR);
        assert!(state.events().replay(0, 100).iter().any(|event| {
            event.kind == "task.acceptance.overridden" && event.payload["actor"] == LOCAL_ACTOR
        }));
    }
}
