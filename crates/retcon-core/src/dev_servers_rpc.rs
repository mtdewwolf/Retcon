//! RPC orchestration for durable development servers.

use std::io::Read;
use std::path::Path;

use retcon_protocol::{DevServerConfigureParams, DevServerPortParams, DevServerStartParams};
use retcon_storage::{NewDevServerConfig, StorageError};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::{Request, Response};
use crate::state::CoreState;

const ACTOR: &str = "local_user";
const MAX_LOG_BYTES: usize = 512 * 1024;

fn invalid(id: u64, message: impl Into<String>) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The development server request is invalid.",
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
            "The requested development server information was not found.",
            format!("{kind} not found"),
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
fn uuid_param(params: &Value, name: &str) -> Result<Uuid, String> {
    params
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing '{name}' UUID"))
        .and_then(|raw| Uuid::parse_str(raw).map_err(|_| format!("invalid '{name}' UUID")))
}
fn optional_uuid(params: &Value, name: &str) -> Result<Option<Uuid>, String> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(raw)) => Uuid::parse_str(raw)
            .map(Some)
            .map_err(|_| format!("invalid '{name}' UUID")),
        _ => Err(format!("'{name}' must be a UUID string or null")),
    }
}

pub async fn handle(state: CoreState, request: Request) -> Response {
    let Request { id, method, params } = request;
    let repo = state.storage().database().dev_servers();
    match method.as_str() {
        "devServer.detect" => {
            let project_id = match uuid_param(&params, "projectId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            let root = params
                .get("rootPath")
                .and_then(Value::as_str)
                .unwrap_or(".");
            let candidates = detect(project_id, Path::new(root));
            Response::ok(id, json!({"candidates":candidates}))
        }
        "devServer.list" => {
            let project_id = match uuid_param(&params, "projectId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            match (
                repo.list_configs(project_id),
                repo.list_instances(project_id),
            ) {
                (Ok(configs), Ok(instances)) => {
                    Response::ok(id, json!({"configs":configs,"instances":instances}))
                }
                (Err(e), _) | (_, Err(e)) => storage_error(id, e),
            }
        }
        "devServer.configure" => {
            let input: DevServerConfigureParams = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return invalid(id, e.to_string()),
            };
            let config = NewDevServerConfig {
                id: input.id.unwrap_or_else(Uuid::new_v4),
                project_id: input.project_id,
                worktree_id: input.worktree_id,
                name: input.name,
                command: input.command,
                cwd: input.cwd,
                host: input.host,
                preferred_port: input.preferred_port,
                auto_start: input.auto_start,
                env_allowlist: input.env_allowlist,
                environment: input.environment,
            };
            match repo.save_config(&config) {
                Ok(config) => {
                    state.emit("dev_server.configured",json!({"configId":config.id,"projectId":config.project_id,"environmentKeys":config.environment_keys}));
                    Response::ok(id, json!({"config":config}))
                }
                Err(e) => storage_error(id, e),
            }
        }
        "devServer.start" => {
            let input: DevServerStartParams = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return invalid(id, e.to_string()),
            };
            start(&state, id, input.config_id, input.task_id)
        }
        "devServer.stop" => {
            let instance_id = match uuid_param(&params, "instanceId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            stop(&state, id, instance_id)
        }
        "devServer.restart" => {
            let instance_id = match uuid_param(&params, "instanceId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            let Some(instance) = (match repo.instance(instance_id) {
                Ok(v) => v,
                Err(e) => return storage_error(id, e),
            }) else {
                return missing(id, "server instance");
            };
            let stopped = stop(&state, id, instance_id);
            if stopped.error.is_some() {
                return stopped;
            }
            start(&state, id, instance.config_id, instance.task_id)
        }
        "devServer.logs" => {
            let instance_id = match uuid_param(&params, "instanceId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            let Some(instance) = (match repo.instance(instance_id) {
                Ok(v) => v,
                Err(e) => return storage_error(id, e),
            }) else {
                return missing(id, "server instance");
            };
            let bytes = if matches!(
                instance.status.as_str(),
                "starting" | "running" | "stopping"
            ) {
                match state.dev_server_runtime().logs(&instance) {
                    Ok(v) => v,
                    Err(e) => {
                        return Response::error(
                            id,
                            &runtime_error("read development server logs", e),
                        );
                    }
                }
            } else if let Some(hash) = &instance.log_artifact_hash {
                let mut output = Vec::new();
                let mut file = match state.storage().artifacts().get(hash) {
                    Ok(v) => v,
                    Err(e) => return storage_error(id, e),
                };
                if let Err(e) = file.read_to_end(&mut output) {
                    return Response::error(
                        id,
                        &CoreError::io("read development server log artifact", e),
                    );
                }
                output
            } else {
                Vec::new()
            };
            match persist_logs(&state, instance_id, &bytes) {
                Ok(log) => Response::ok(id, json!({"log":log})),
                Err(e) => storage_error(id, e),
            }
        }
        "devServer.status" => {
            if params.get("instanceId").is_some() {
                let iid = match uuid_param(&params, "instanceId") {
                    Ok(v) => v,
                    Err(e) => return invalid(id, e),
                };
                match repo.instance(iid) {
                    Ok(Some(v)) => Response::ok(id, json!({"instance":v})),
                    Ok(None) => missing(id, "server instance"),
                    Err(e) => storage_error(id, e),
                }
            } else {
                let pid = match uuid_param(&params, "projectId") {
                    Ok(v) => v,
                    Err(e) => return invalid(id, e),
                };
                match repo.list_instances(pid) {
                    Ok(v) => Response::ok(id, json!({"instances":v})),
                    Err(e) => storage_error(id, e),
                }
            }
        }
        "devServer.openPreview" => {
            let iid = match uuid_param(&params, "instanceId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            match repo.instance(iid) {
                Ok(Some(v)) => Response::ok(
                    id,
                    json!({"instanceId":v.id,"status":v.status,"url":v.url,"preview":v.preview,"port":v.port}),
                ),
                Ok(None) => missing(id, "server instance"),
                Err(e) => storage_error(id, e),
            }
        }
        "devServer.history" => {
            let iid = match uuid_param(&params, "instanceId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            match repo.history(iid) {
                Ok(v) => Response::ok(id, json!({"events":v})),
                Err(e) => storage_error(id, e),
            }
        }
        "devServer.port.assign" | "devServer.port.change" => {
            let input: DevServerPortParams = match serde_json::from_value(params) {
                Ok(v) => v,
                Err(e) => return invalid(id, e.to_string()),
            };
            match repo.assign_port(input.config_id, input.task_id, input.port, ACTOR) {
                Ok(v) => {
                    state.emit("dev_server.port_assigned",json!({"configId":input.config_id,"port":input.port,"taskId":input.task_id}));
                    Response::ok(id, json!({"lease":v}))
                }
                Err(e) => storage_error(id, e),
            }
        }
        "devServer.port.release" => {
            let cid = match uuid_param(&params, "configId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            let task = match optional_uuid(&params, "taskId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            match repo.release_port(cid, task) {
                Ok(v) => {
                    state.emit(
                        "dev_server.port_released",
                        json!({"configId":cid,"taskId":task}),
                    );
                    Response::ok(id, json!({"released":v}))
                }
                Err(e) => storage_error(id, e),
            }
        }
        "devServer.autoStart.set" => {
            let cid = match uuid_param(&params, "configId") {
                Ok(v) => v,
                Err(e) => return invalid(id, e),
            };
            let Some(enabled) = params.get("enabled").and_then(Value::as_bool) else {
                return invalid(id, "missing boolean 'enabled'");
            };
            match repo.set_auto_start(cid, enabled) {
                Ok(Some(v)) => {
                    state.emit(
                        "dev_server.auto_start_changed",
                        json!({"configId":cid,"enabled":enabled}),
                    );
                    Response::ok(id, json!({"config":v}))
                }
                Ok(None) => missing(id, "server config"),
                Err(e) => storage_error(id, e),
            }
        }
        _ => missing(id, "development server method"),
    }
}

fn start(state: &CoreState, id: u64, config_id: Uuid, task_id: Option<Uuid>) -> Response {
    let repo = state.storage().database().dev_servers();
    let instance = match repo.prepare_start(config_id, task_id, ACTOR) {
        Ok(v) => v,
        Err(e) => return storage_error(id, e),
    };
    let Some(config) = (match repo.launch_config(config_id) {
        Ok(v) => v,
        Err(e) => return storage_error(id, e),
    }) else {
        return missing(id, "server config");
    };
    match state.dev_server_runtime().start(&config, &instance) {
        Ok(started) => match repo.mark_running(
            instance.id,
            started.pid,
            &started.url,
            &started.preview,
            ACTOR,
        ) {
            Ok(Some(value)) => {
                state.emit("dev_server.started",json!({"instanceId":value.id,"configId":value.config_id,"projectId":value.project_id,"taskId":value.task_id,"port":value.port,"url":value.url}));
                Response::ok(id, json!({"instance":value}))
            }
            Ok(None) => missing(id, "server instance"),
            Err(e) => storage_error(id, e),
        },
        Err(error) => {
            let _ = repo.mark_failed(instance.id, &error, "runtime");
            state.emit(
                "dev_server.failed",
                json!({"instanceId":instance.id,"configId":config_id,"error":error}),
            );
            Response::error(id, &runtime_error("start development server", error))
        }
    }
}
fn stop(state: &CoreState, id: u64, instance_id: Uuid) -> Response {
    let repo = state.storage().database().dev_servers();
    let Some(instance) = (match repo.begin_stop(instance_id, ACTOR) {
        Ok(v) => v,
        Err(e) => return storage_error(id, e),
    }) else {
        return missing(id, "server instance");
    };
    match state.dev_server_runtime().stop(&instance) {
        Ok(logs) => {
            if let Err(e) = persist_logs(state, instance.id, &logs) {
                return storage_error(id, e);
            }
            match repo.mark_stopped(instance.id, ACTOR) {
                Ok(Some(value)) => {
                    state.emit("dev_server.stopped",json!({"instanceId":value.id,"configId":value.config_id,"projectId":value.project_id}));
                    Response::ok(id, json!({"instance":value}))
                }
                Ok(None) => missing(id, "server instance"),
                Err(e) => storage_error(id, e),
            }
        }
        Err(error) => {
            let _ = repo.mark_failed(instance.id, &error, "runtime");
            Response::error(id, &runtime_error("stop development server", error))
        }
    }
}
fn persist_logs(
    state: &CoreState,
    instance_id: Uuid,
    bytes: &[u8],
) -> retcon_storage::Result<Value> {
    let retained = &bytes[..bytes.len().min(MAX_LOG_BYTES)];
    let artifact = state.storage().artifacts().store_bytes(retained)?;
    state
        .storage()
        .database()
        .dev_servers()
        .set_log_artifact(instance_id, &artifact.hash)?;
    Ok(
        json!({"artifactHash":artifact.hash,"retainedBytes":artifact.size,"originalBytes":bytes.len(),"truncated":bytes.len()>retained.len(),"text":String::from_utf8_lossy(retained)}),
    )
}
fn runtime_error(operation: &str, error: String) -> CoreError {
    CoreError::new(
        ErrorCode::Internal,
        ErrorSource::System,
        "The development server runtime could not complete the request.",
        format!("{operation}: {error}"),
    )
}
fn detect(project_id: Uuid, root: &Path) -> Vec<Value> {
    let mut out = Vec::new();
    if root.join("package.json").exists() {
        out.push(json!({"projectId":project_id,"name":"web","command":"npm run dev","cwd":root,"source":"package.json"}));
    }
    if root.join("Cargo.toml").exists() {
        out.push(json!({"projectId":project_id,"name":"rust","command":"cargo run","cwd":root,"source":"Cargo.toml"}));
    }
    if root.join("manage.py").exists() {
        out.push(json!({"projectId":project_id,"name":"django","command":"python manage.py runserver","cwd":root,"source":"manage.py"}));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::dev_servers::{DevServerRuntime, DevServerStarted};
    use retcon_storage::{DevServerInstance, DevServerLaunchConfig, NewProject};
    use std::sync::Arc;
    struct Fake;
    impl DevServerRuntime for Fake {
        fn start(
            &self,
            _: &DevServerLaunchConfig,
            i: &DevServerInstance,
        ) -> Result<DevServerStarted, String> {
            Ok(DevServerStarted {
                pid: Some(7),
                url: format!("http://127.0.0.1:{}", i.port),
                preview: json!({"title":"preview"}),
            })
        }
        fn stop(&self, _: &DevServerInstance) -> Result<Vec<u8>, String> {
            Ok(b"stopped".to_vec())
        }
        fn logs(&self, _: &DevServerInstance) -> Result<Vec<u8>, String> {
            Ok(vec![b'x'; MAX_LOG_BYTES + 10])
        }
    }
    #[tokio::test]
    async fn rpc_lifecycle_and_bounded_logs() {
        let dir = tempfile::tempdir().unwrap();
        let state = CoreState::new_with_dev_server_runtime(dir.path(), Arc::new(Fake)).unwrap();
        let p = state
            .storage()
            .database()
            .projects()
            .create(&NewProject::new("Web"))
            .unwrap();
        let configured=handle(state.clone(),Request{id:1,method:"devServer.configure".into(),params:json!({"projectId":p.id,"name":"web","command":"serve","cwd":".","envAllowlist":["PUBLIC_URL"],"environment":{"PUBLIC_URL":"private"}})}).await;
        assert!(
            !configured
                .result
                .as_ref()
                .unwrap()
                .to_string()
                .contains("private")
        );
        let cid = configured.result.unwrap()["config"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let started = handle(
            state.clone(),
            Request {
                id: 2,
                method: "devServer.start".into(),
                params: json!({"configId":cid}),
            },
        )
        .await;
        let iid = started.result.unwrap()["instance"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let logs = handle(
            state.clone(),
            Request {
                id: 3,
                method: "devServer.logs".into(),
                params: json!({"instanceId":iid}),
            },
        )
        .await;
        assert_eq!(
            logs.result.as_ref().unwrap()["log"]["retainedBytes"],
            MAX_LOG_BYTES as u64
        );
        assert_eq!(logs.result.as_ref().unwrap()["log"]["truncated"], true);
        let stopped = handle(
            state,
            Request {
                id: 4,
                method: "devServer.stop".into(),
                params: json!({"instanceId":iid}),
            },
        )
        .await;
        assert_eq!(stopped.result.unwrap()["instance"]["status"], "stopped");
    }
}
