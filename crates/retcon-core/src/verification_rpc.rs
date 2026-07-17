//! RPC adapter for durable verification configuration, evidence, and reports.

use retcon_protocol::{
    VerificationCommandsConfigureParams, VerificationCreateParams, VerificationRecordParams,
};
use retcon_storage::{
    NewVerificationArtifact, NewVerificationCommand, NewVerificationTestResult, StorageError,
    VerificationMutation, VerificationRunDetails,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;
use crate::verification::ensure_default_commands;

const LOCAL_ACTOR: &str = "local_user";
const MAX_STREAM_ARTIFACT_BYTES: usize = 256 * 1024;

fn invalid(id: u64, message: impl Into<String>) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The verification request contains invalid information.",
            message,
        ),
    )
}

fn missing(id: u64, kind: &str) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "The requested verification information could not be found.",
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
        .ok_or_else(|| format!("missing '{name}' UUID"))
        .and_then(|raw| Uuid::parse_str(raw).map_err(|_| format!("invalid '{name}' UUID")))
}

fn emit_mutation(
    state: &CoreState,
    operation: &str,
    mutation: &VerificationMutation<VerificationRunDetails>,
) {
    state.emit(
        &format!("verification.run.{operation}"),
        json!({
            "runId": mutation.value.run.id,
            "taskId": mutation.task_id,
            "status": mutation.value.run.status,
            "reopened": mutation.reopened,
        }),
    );
    state.emit(
        "task.changed",
        json!({
            "taskId": mutation.task_id,
            "operation": format!("verification.{operation}"),
            "verificationRunId": mutation.value.run.id,
            "reopened": mutation.reopened,
        }),
    );
    if mutation.reopened {
        state.emit(
            "task.reopened",
            json!({
                "taskId": mutation.task_id,
                "reason": format!("verification.{operation}"),
                "actor": LOCAL_ACTOR,
            }),
        );
    }
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let repository = state.storage().database().verification();
    match method.as_str() {
        "verification.commands.list" => {
            let project_id = match required_uuid(&params, "projectId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match ensure_default_commands(state.storage(), project_id) {
                Ok(commands) => Response::ok(id, json!({"commands": commands})),
                Err(error) => invalid(id, error),
            }
        }
        "verification.commands.configure" => {
            let input: VerificationCommandsConfigureParams = match serde_json::from_value(params) {
                Ok(value) => value,
                Err(error) => return invalid(id, error.to_string()),
            };
            let commands: Vec<_> = input
                .commands
                .into_iter()
                .map(|command| NewVerificationCommand {
                    id: command.id.unwrap_or_else(Uuid::new_v4),
                    key: command.key,
                    kind: command.kind,
                    command: command.command,
                    cwd: command.cwd,
                    required: command.required,
                    enabled: command.enabled,
                    timeout_ms: command.timeout_ms,
                })
                .collect();
            match repository.replace_commands(input.project_id, &commands) {
                Ok(commands) => {
                    state.emit(
                        "verification.commands.configured",
                        json!({"projectId": input.project_id, "commandCount": commands.len()}),
                    );
                    Response::ok(id, json!({"commands": commands}))
                }
                Err(error) => storage_error(id, error),
            }
        }
        "verification.create" => {
            let input: VerificationCreateParams = match serde_json::from_value(params) {
                Ok(value) => value,
                Err(error) => return invalid(id, error.to_string()),
            };
            let task = match state.storage().database().tasks().get(input.task_id) {
                Ok(Some(task)) => task,
                Ok(None) => return missing(id, "task"),
                Err(error) => return storage_error(id, error),
            };
            let Some(project_id) = task.project_id else {
                return invalid(id, "task must belong to a project before verification");
            };
            if let Err(error) = ensure_default_commands(state.storage(), project_id) {
                return invalid(id, error);
            }
            match repository.create_run(input.task_id, &input.kinds, "manual", None, LOCAL_ACTOR) {
                Ok(mutation) => {
                    emit_mutation(&state, "created", &mutation);
                    Response::ok(id, json!({"verification": mutation.value}))
                }
                Err(error) => storage_error(id, error),
            }
        }
        "verification.start" => {
            let run_id = match required_uuid(&params, "runId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.start(run_id, LOCAL_ACTOR) {
                Ok(Some(details)) => {
                    if let Err(error) = state.verification_runner().start(&details) {
                        let _ = repository.cancel(run_id, "runner");
                        return Response::error(
                            id,
                            &CoreError::new(
                                ErrorCode::Internal,
                                ErrorSource::System,
                                "The verification runner could not start this run.",
                                error,
                            ),
                        );
                    }
                    state.emit(
                        "verification.run.started",
                        json!({"runId": run_id, "taskId": details.run.task_id}),
                    );
                    Response::ok(id, json!({"verification": details}))
                }
                Ok(None) => missing(id, "verification run"),
                Err(error) => storage_error(id, error),
            }
        }
        "verification.recordResult" => {
            let input: VerificationRecordParams = match serde_json::from_value(params) {
                Ok(value) => value,
                Err(error) => return invalid(id, error.to_string()),
            };
            let results: Vec<_> = input
                .results
                .into_iter()
                .map(|result| NewVerificationTestResult {
                    id: result.id.unwrap_or_else(Uuid::new_v4),
                    suite: result.suite,
                    name: result.name,
                    status: result.status,
                    duration_ms: result.duration_ms,
                    file_path: result.file_path,
                    line: result.line,
                    message: result.message,
                    metadata: result.metadata,
                })
                .collect();
            let mut artifacts = Vec::new();
            for (kind, content) in [("stdout", input.stdout), ("stderr", input.stderr)] {
                if let Some(content) = content {
                    let original_size = content.len();
                    let bytes = &content.as_bytes()[..original_size.min(MAX_STREAM_ARTIFACT_BYTES)];
                    let artifact = match state.storage().artifacts().store_bytes(bytes) {
                        Ok(value) => value,
                        Err(error) => return storage_error(id, error),
                    };
                    artifacts.push(NewVerificationArtifact {
                        id: Uuid::new_v4(),
                        kind: kind.into(),
                        hash: artifact.hash,
                        size_bytes: artifact.size as i64,
                        metadata: json!({
                            "truncated": original_size > MAX_STREAM_ARTIFACT_BYTES,
                            "originalBytes": original_size,
                            "retainedBytes": bytes.len(),
                        }),
                    });
                }
            }
            match repository.record_gate(
                input.run_id,
                input.gate_id,
                &input.status,
                input.summary,
                &results,
                &artifacts,
                LOCAL_ACTOR,
            ) {
                Ok(Some(mutation)) => {
                    state.emit(
                        "verification.gate.recorded",
                        json!({
                            "runId": input.run_id,
                            "gateId": input.gate_id,
                            "taskId": mutation.task_id,
                            "status": input.status,
                            "artifactCount": artifacts.len(),
                            "reopened": mutation.reopened,
                        }),
                    );
                    state.emit(
                        "task.changed",
                        json!({
                            "taskId": mutation.task_id,
                            "operation": "verification.evidence",
                            "verificationRunId": input.run_id,
                            "reopened": mutation.reopened,
                        }),
                    );
                    if mutation.reopened {
                        state.emit(
                            "task.reopened",
                            json!({"taskId": mutation.task_id, "reason": "verification.gate_failed", "actor": LOCAL_ACTOR}),
                        );
                    }
                    Response::ok(id, json!({"verification": mutation.value}))
                }
                Ok(None) => missing(id, "verification run"),
                Err(error) => storage_error(id, error),
            }
        }
        "verification.finish" => mutate_run(&state, id, &params, "finished", |run_id| {
            repository.finish(run_id, LOCAL_ACTOR)
        }),
        "verification.cancel" => {
            let run_id = match required_uuid(&params, "runId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.cancel(run_id, LOCAL_ACTOR) {
                Ok(Some(mutation)) => {
                    let _ = state.verification_runner().cancel(&mutation.value);
                    emit_mutation(&state, "cancelled", &mutation);
                    Response::ok(id, json!({"verification": mutation.value}))
                }
                Ok(None) => missing(id, "verification run"),
                Err(error) => storage_error(id, error),
            }
        }
        "verification.rerun" => {
            let run_id = match required_uuid(&params, "runId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            let Some(previous) = (match repository.get(run_id) {
                Ok(value) => value,
                Err(error) => return storage_error(id, error),
            }) else {
                return missing(id, "verification run");
            };
            match repository.create_run(
                previous.run.task_id,
                &[],
                "rerun",
                Some(run_id),
                LOCAL_ACTOR,
            ) {
                Ok(mutation) => {
                    emit_mutation(&state, "rerun_created", &mutation);
                    Response::ok(id, json!({"verification": mutation.value}))
                }
                Err(error) => storage_error(id, error),
            }
        }
        "verification.get" => {
            let run_id = match required_uuid(&params, "runId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.get(run_id) {
                Ok(Some(details)) => Response::ok(id, json!({"verification": details})),
                Ok(None) => missing(id, "verification run"),
                Err(error) => storage_error(id, error),
            }
        }
        "verification.list" => {
            let task_id = match required_uuid(&params, "taskId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.list(task_id) {
                Ok(runs) => Response::ok(id, json!({"verifications": runs})),
                Err(error) => storage_error(id, error),
            }
        }
        "verification.history" => {
            let run_id = match required_uuid(&params, "runId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.history(run_id) {
                Ok(events) => Response::ok(id, json!({"events": events})),
                Err(error) => storage_error(id, error),
            }
        }
        "verification.report" => {
            let run_id = match required_uuid(&params, "runId") {
                Ok(value) => value,
                Err(message) => return invalid(id, message),
            };
            match repository.report(run_id) {
                Ok(Some(report)) => Response::ok(id, json!({"report": report})),
                Ok(None) => missing(id, "verification run"),
                Err(error) => storage_error(id, error),
            }
        }
        _ => missing(id, "verification method"),
    }
}

fn mutate_run(
    state: &CoreState,
    id: u64,
    params: &Value,
    operation: &str,
    mutate: impl FnOnce(
        Uuid,
    )
        -> retcon_storage::Result<Option<VerificationMutation<VerificationRunDetails>>>,
) -> Response {
    let run_id = match required_uuid(params, "runId") {
        Ok(value) => value,
        Err(message) => return invalid(id, message),
    };
    match mutate(run_id) {
        Ok(Some(mutation)) => {
            emit_mutation(state, operation, &mutation);
            Response::ok(id, json!({"verification": mutation.value}))
        }
        Ok(None) => missing(id, "verification run"),
        Err(error) => storage_error(id, error),
    }
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::verification::NoopVerificationRunner;
    use retcon_storage::{NewProject, NewTask};

    fn test_state(path: &std::path::Path) -> CoreState {
        CoreState::new_with_verification_runner(path, Arc::new(NoopVerificationRunner)).unwrap()
    }

    #[tokio::test]
    async fn rpc_flow_persists_streams_as_bounded_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let state = test_state(directory.path());
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Verify RPC"))
            .unwrap();
        let mut new_task = NewTask::new("Evidence");
        new_task.project_id = Some(project.id);
        let task = state
            .storage()
            .database()
            .tasks()
            .create(&new_task)
            .unwrap();
        let configure = handle(
            state.clone(),
            Request {
                id: 1,
                method: "verification.commands.configure".into(),
                params: json!({"projectId": project.id, "commands": [{"key": "unit", "kind": "test", "command": "cargo test"}]}),
            },
        )
        .await;
        assert!(configure.error.is_none());
        let created = handle(
            state.clone(),
            Request {
                id: 2,
                method: "verification.create".into(),
                params: json!({"taskId": task.id}),
            },
        )
        .await;
        let verification = &created.result.as_ref().unwrap()["verification"];
        let run_id = verification["run"]["id"].as_str().unwrap();
        let gate_id = verification["gates"][0]["id"].as_str().unwrap();
        assert!(
            handle(
                state.clone(),
                Request {
                    id: 3,
                    method: "verification.start".into(),
                    params: json!({"runId": run_id})
                }
            )
            .await
            .error
            .is_none()
        );
        let output = "x".repeat(MAX_STREAM_ARTIFACT_BYTES + 100);
        let recorded = handle(
            state.clone(),
            Request {
                id: 4,
                method: "verification.recordResult".into(),
                params: json!({"runId": run_id, "gateId": gate_id, "status": "passed", "stdout": output, "results": [{"name": "unit", "status": "passed"}]}),
            },
        )
        .await;
        let artifact = &recorded.result.as_ref().unwrap()["verification"]["artifacts"][0];
        assert_eq!(artifact["sizeBytes"], MAX_STREAM_ARTIFACT_BYTES as i64);
        assert_eq!(artifact["metadata"]["truncated"], true);
        assert!(
            state
                .storage()
                .artifacts()
                .verify(artifact["hash"].as_str().unwrap())
                .is_ok()
        );
        assert!(
            handle(
                state.clone(),
                Request {
                    id: 5,
                    method: "verification.finish".into(),
                    params: json!({"runId": run_id})
                }
            )
            .await
            .error
            .is_none()
        );
    }

    #[tokio::test]
    async fn malformed_configuration_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let state = test_state(directory.path());
        let response = handle(
            state,
            Request {
                id: 1,
                method: "verification.commands.configure".into(),
                params: json!({"projectId": 4, "commands": []}),
            },
        )
        .await;
        assert!(response.error.is_some());
    }

    #[tokio::test]
    async fn detected_defaults_do_not_replace_project_overrides() {
        let directory = tempfile::tempdir().unwrap();
        let state = test_state(directory.path());
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Configured verification"))
            .unwrap();
        let configured = handle(
            state.clone(),
            Request {
                id: 20,
                method: "verification.commands.configure".into(),
                params: json!({
                    "projectId": project.id,
                    "commands": [{
                        "key": "project-test-override",
                        "kind": "test",
                        "command": "cargo test --lib",
                        "required": true,
                        "enabled": true
                    }]
                }),
            },
        )
        .await;
        assert!(configured.error.is_none());

        let listed = handle(
            state,
            Request {
                id: 21,
                method: "verification.commands.list".into(),
                params: json!({"projectId": project.id}),
            },
        )
        .await;
        let commands = listed.result.unwrap()["commands"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0]["key"], "project-test-override");
        assert_eq!(commands[0]["command"], "cargo test --lib");
    }

    #[tokio::test]
    async fn detected_run_persists_evidence_and_passing_rerun_clears_completion_blocker() {
        let directory = tempfile::tempdir().unwrap();
        let project_root = directory.path().join("project");
        std::fs::create_dir_all(project_root.join("src")).unwrap();
        std::fs::write(
            project_root.join("Cargo.toml"),
            "[package]\nname = \"verification-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .unwrap();
        std::fs::write(
            project_root.join("src/lib.rs"),
            r#"#[cfg(test)]
mod tests {
    #[test]
    fn completion_gate() {
        assert!(std::path::Path::new("pass.marker").exists());
    }
}
"#,
        )
        .unwrap();
        let state = CoreState::new(&directory.path().join("data")).unwrap();
        let project = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Detected verification"))
            .unwrap();
        let canonical = project_root
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        state
            .storage()
            .database()
            .projects()
            .add_location(project.id, &canonical, None)
            .unwrap();
        let mut new_task = NewTask::new("Gate completion");
        new_task.project_id = Some(project.id);
        let task = state
            .storage()
            .database()
            .tasks()
            .create(&new_task)
            .unwrap();

        let detected = handle(
            state.clone(),
            Request {
                id: 10,
                method: "verification.commands.list".into(),
                params: json!({"projectId": project.id}),
            },
        )
        .await;
        assert!(detected.error.is_none());
        assert!(
            detected.result.unwrap()["commands"]
                .as_array()
                .unwrap()
                .len()
                >= 5
        );

        let created = handle(
            state.clone(),
            Request {
                id: 11,
                method: "verification.create".into(),
                params: json!({"taskId": task.id}),
            },
        )
        .await;
        let first_run_id = created.result.unwrap()["verification"]["run"]["id"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap();
        assert!(
            handle(
                state.clone(),
                Request {
                    id: 12,
                    method: "verification.start".into(),
                    params: json!({"runId": first_run_id}),
                },
            )
            .await
            .error
            .is_none()
        );
        let failed = wait_for_terminal_run(&state, first_run_id).await;
        assert_eq!(failed.run.status, "failed");
        assert!(
            failed
                .results
                .iter()
                .any(|result| result.status == "failed")
        );
        assert!(!failed.artifacts.is_empty());
        assert!(
            state
                .storage()
                .database()
                .task_planning()
                .set_task_status(task.id, "completed")
                .is_err()
        );
        let report = state
            .storage()
            .database()
            .verification()
            .report(first_run_id)
            .unwrap()
            .unwrap();
        assert_eq!(report.status, "failed");
        assert!(report.tests["resultCount"].as_u64().unwrap() > 0);

        std::fs::write(project_root.join("pass.marker"), "pass").unwrap();
        let rerun = handle(
            state.clone(),
            Request {
                id: 13,
                method: "verification.rerun".into(),
                params: json!({"runId": first_run_id}),
            },
        )
        .await;
        let second_run_id = rerun.result.unwrap()["verification"]["run"]["id"]
            .as_str()
            .unwrap()
            .parse::<Uuid>()
            .unwrap();
        assert!(
            handle(
                state.clone(),
                Request {
                    id: 14,
                    method: "verification.start".into(),
                    params: json!({"runId": second_run_id}),
                },
            )
            .await
            .error
            .is_none()
        );
        let passed = wait_for_terminal_run(&state, second_run_id).await;
        assert_eq!(passed.run.status, "passed");
        assert!(passed.gates.iter().all(|gate| gate.status == "passed"));
        assert!(
            state
                .storage()
                .database()
                .task_planning()
                .set_task_status(task.id, "completed")
                .unwrap()
        );
    }

    async fn wait_for_terminal_run(
        state: &CoreState,
        run_id: Uuid,
    ) -> retcon_storage::VerificationRunDetails {
        for _ in 0..600 {
            let details = state
                .storage()
                .database()
                .verification()
                .get(run_id)
                .unwrap()
                .unwrap();
            if !matches!(details.run.status.as_str(), "queued" | "running") {
                return details;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        panic!("verification run did not finish within 60 seconds")
    }
}
