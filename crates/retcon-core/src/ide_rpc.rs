//! IDE detection, preference, validation, and safe launch RPCs.

use std::path::{Path, PathBuf};

use retcon_platform::{IdeAction, IdeError, IdeLaunchRequest, SUPPORTED_IDE_IDS};
use retcon_protocol::Request;
use serde_json::{Value, json};

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::Response;
use crate::state::CoreState;

const SETTINGS_SCOPE: &str = "global";
const SETTINGS_KEY: &str = "ide.configuration";
const MAX_PATH_LENGTH: usize = 32_768;

pub async fn handle(state: CoreState, request: Request) -> Response {
    let result = match request.method.as_str() {
        "ide.detect" => Ok(detection_payload(&state)),
        "ide.configuration.get" => configuration_payload(&state),
        "ide.configuration.update" => update_configuration(&state, &request.params),
        "ide.openProject" => open_directory(&state, &request.params, false),
        "ide.openWorktree" => open_directory(&state, &request.params, true),
        "ide.openFile" => open_file(&state, &request.params),
        "ide.openDiff" => open_diff(&state, &request.params),
        "ide.openTerminalLocation" => open_terminal_location(&state, &request.params),
        _ => Err(CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "The requested IDE operation is not available.",
            format!("unsupported method {}", request.method),
        )),
    };
    match result {
        Ok(value) => Response::ok(request.id, value),
        Err(error) => Response::error(request.id, &error),
    }
}

fn detection_payload(state: &CoreState) -> Value {
    let detections = state.ide().detect();
    json!({
        "ides": detections,
        "supportedIdeIds": SUPPORTED_IDE_IDS,
    })
}

fn configuration_payload(state: &CoreState) -> Result<Value, CoreError> {
    let preferred = preferred_ide(state)?;
    let detected = state.ide().detect();
    let effective = preferred.clone().or_else(|| {
        detected
            .iter()
            .find(|candidate| candidate.available)
            .map(|candidate| candidate.id.clone())
    });
    Ok(json!({
        "preferredIdeId": preferred,
        "effectiveIdeId": effective,
        "ides": detected,
        "supportedIdeIds": SUPPORTED_IDE_IDS,
    }))
}

fn update_configuration(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let Some(value) = params.get("preferredIdeId") else {
        return Err(invalid_request("missing preferredIdeId"));
    };
    match value {
        Value::Null => {
            state
                .storage()
                .database()
                .settings()
                .delete(SETTINGS_SCOPE, SETTINGS_KEY)?;
        }
        Value::String(ide_id) if SUPPORTED_IDE_IDS.contains(&ide_id.as_str()) => {
            state.storage().database().settings().set(
                SETTINGS_SCOPE,
                SETTINGS_KEY,
                &json!({"preferredIdeId": ide_id}),
            )?;
        }
        Value::String(_) => return Err(unsupported_ide()),
        _ => return Err(invalid_request("preferredIdeId must be a string or null")),
    }
    state.emit(
        "ide.configuration.updated",
        json!({"preferredIdeId": value}),
    );
    configuration_payload(state)
}

fn open_directory(state: &CoreState, params: &Value, worktree: bool) -> Result<Value, CoreError> {
    let path = canonical_directory(required_path(params, "path")?)?;
    let action = if worktree {
        IdeAction::OpenWorktree(path)
    } else {
        IdeAction::OpenProject(path)
    };
    launch(state, params, action)
}

fn open_file(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let workspace = canonical_directory(required_path(params, "workspacePath")?)?;
    let path = canonical_target(&workspace, required_path(params, "path")?, TargetKind::File)?;
    let line = optional_position(params, "line")?;
    let column = optional_position(params, "column")?;
    if column.is_some() && line.is_none() {
        return Err(invalid_request("column requires line"));
    }
    launch(state, params, IdeAction::OpenFile { path, line, column })
}

fn open_diff(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let workspace = canonical_directory(required_path(params, "workspacePath")?)?;
    let left = canonical_target(
        &workspace,
        required_path(params, "leftPath")?,
        TargetKind::File,
    )?;
    let right = canonical_target(
        &workspace,
        required_path(params, "rightPath")?,
        TargetKind::File,
    )?;
    launch(state, params, IdeAction::OpenDiff { left, right })
}

fn open_terminal_location(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let workspace = canonical_directory(required_path(params, "workspacePath")?)?;
    let raw = params.get("path").and_then(Value::as_str).unwrap_or(".");
    let path = canonical_target(&workspace, raw, TargetKind::Directory)?;
    launch(state, params, IdeAction::OpenTerminalLocation(path))
}

fn launch(state: &CoreState, params: &Value, action: IdeAction) -> Result<Value, CoreError> {
    let ide_id = select_ide(state, params)?;
    let receipt = state
        .ide()
        .launch(&IdeLaunchRequest { ide_id, action })
        .map_err(map_ide_error)?;
    state.emit(
        "ide.launched",
        json!({"ideId": receipt.ide_id, "action": receipt.action}),
    );
    serde_json::to_value(receipt).map_err(|error| {
        CoreError::new(
            ErrorCode::Internal,
            ErrorSource::Rpc,
            "Retcon could not encode the IDE launch receipt.",
            error.to_string(),
        )
    })
}

fn select_ide(state: &CoreState, params: &Value) -> Result<String, CoreError> {
    let requested = params.get("ideId").and_then(Value::as_str);
    if params.get("ideId").is_some() && requested.is_none() {
        return Err(invalid_request("ideId must be a string"));
    }
    if requested.is_some_and(|ide_id| !SUPPORTED_IDE_IDS.contains(&ide_id)) {
        return Err(unsupported_ide());
    }
    let preferred = preferred_ide(state)?;
    let detections = state.ide().detect();
    let selected = requested
        .map(str::to_owned)
        .or(preferred)
        .or_else(|| {
            detections
                .iter()
                .find(|candidate| candidate.available)
                .map(|candidate| candidate.id.clone())
        })
        .ok_or_else(no_ide_found)?;
    if detections
        .iter()
        .any(|candidate| candidate.id == selected && candidate.available)
    {
        Ok(selected)
    } else {
        Err(CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::System,
            "The selected IDE is not installed or its command-line launcher is unavailable.",
            format!("IDE executable not found for {selected}"),
        )
        .suggested_fix("Install the selected IDE or choose another detected IDE in Settings."))
    }
}

fn preferred_ide(state: &CoreState) -> Result<Option<String>, CoreError> {
    let setting = state
        .storage()
        .database()
        .settings()
        .get(SETTINGS_SCOPE, SETTINGS_KEY)?;
    Ok(setting.and_then(|setting| {
        setting
            .value
            .get("preferredIdeId")
            .and_then(Value::as_str)
            .filter(|id| SUPPORTED_IDE_IDS.contains(id))
            .map(str::to_owned)
    }))
}

fn required_path<'a>(params: &'a Value, name: &str) -> Result<&'a str, CoreError> {
    params
        .get(name)
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty() && path.len() <= MAX_PATH_LENGTH)
        .ok_or_else(|| invalid_request(format!("missing or invalid {name}")))
}

fn canonical_directory(raw: &str) -> Result<PathBuf, CoreError> {
    let path = std::fs::canonicalize(raw).map_err(|_| path_not_found("directory"))?;
    if !path.is_dir() {
        return Err(path_not_found("directory"));
    }
    Ok(path)
}

#[derive(Clone, Copy)]
enum TargetKind {
    File,
    Directory,
}

fn canonical_target(workspace: &Path, raw: &str, kind: TargetKind) -> Result<PathBuf, CoreError> {
    if raw.is_empty() || raw.len() > MAX_PATH_LENGTH {
        return Err(invalid_request("invalid workspace target path"));
    }
    let supplied = Path::new(raw);
    let candidate = if supplied.is_absolute() {
        supplied.to_path_buf()
    } else {
        workspace.join(supplied)
    };
    let canonical = std::fs::canonicalize(candidate).map_err(|_| path_not_found("target"))?;
    if !canonical.starts_with(workspace) {
        return Err(CoreError::new(
            ErrorCode::PermissionDenied,
            ErrorSource::System,
            "Retcon blocked an IDE target outside the active workspace.",
            "canonical IDE target escaped the workspace boundary",
        ));
    }
    let expected_kind = match kind {
        TargetKind::File => canonical.is_file(),
        TargetKind::Directory => canonical.is_dir(),
    };
    if !expected_kind {
        return Err(path_not_found("target"));
    }
    Ok(canonical)
}

fn optional_position(params: &Value, name: &str) -> Result<Option<u32>, CoreError> {
    match params.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|value| (1..=1_000_000).contains(value))
            .and_then(|value| u32::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| invalid_request(format!("{name} must be a positive integer"))),
    }
}

fn map_ide_error(error: IdeError) -> CoreError {
    match error {
        IdeError::UnsupportedIde(_) => unsupported_ide(),
        IdeError::NotFound(ide_id) => CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::System,
            "The selected IDE is not installed or its command-line launcher is unavailable.",
            format!("IDE executable not found for {ide_id}"),
        )
        .suggested_fix("Install the selected IDE or choose another detected IDE in Settings."),
        IdeError::UnsupportedAction { ide_id, action } => CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::System,
            "The selected IDE cannot perform this action through its safe command-line interface.",
            format!("unsupported IDE action: {ide_id} {action}"),
        )
        .suggested_fix("Open the workspace in the IDE, then perform this action from the IDE."),
        IdeError::Launch(error) => CoreError::io("launch IDE process", error),
    }
}

fn unsupported_ide() -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        ErrorSource::Rpc,
        "The selected IDE is not supported.",
        "unsupported IDE identifier",
    )
    .suggested_fix("Choose Visual Studio Code, Cursor, or Windsurf.")
}

fn no_ide_found() -> CoreError {
    CoreError::new(
        ErrorCode::NotFound,
        ErrorSource::System,
        "Retcon could not find a supported IDE.",
        "no supported IDE executable detected",
    )
    .suggested_fix("Install Visual Studio Code, Cursor, or Windsurf and restart Retcon.")
}

fn path_not_found(kind: &str) -> CoreError {
    CoreError::new(
        ErrorCode::NotFound,
        ErrorSource::System,
        "The requested IDE target was not found.",
        format!("IDE {kind} does not exist or is inaccessible"),
    )
}

fn invalid_request(message: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        ErrorSource::Rpc,
        "Retcon received an invalid IDE request.",
        message,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::{Arc, Mutex};

    use retcon_platform::{IdeDetection, IdeIntegration, IdeLaunchReceipt};
    use tempfile::TempDir;

    use super::*;

    #[derive(Default)]
    struct FakeIde {
        available: Vec<&'static str>,
        launches: Mutex<Vec<IdeLaunchRequest>>,
    }

    impl FakeIde {
        fn with_available(ids: Vec<&'static str>) -> Self {
            Self {
                available: ids,
                launches: Mutex::new(Vec::new()),
            }
        }
    }

    impl IdeIntegration for FakeIde {
        fn detect(&self) -> Vec<IdeDetection> {
            SUPPORTED_IDE_IDS
                .into_iter()
                .map(|id| IdeDetection {
                    id: id.to_owned(),
                    name: id.to_owned(),
                    available: self.available.contains(&id),
                    evidence: self.available.contains(&id).then(|| "test".to_owned()),
                    executable: self.available.contains(&id).then(|| id.to_owned()),
                    capabilities: ["openProject", "openWorktree", "openFile", "openDiff"]
                        .map(str::to_owned)
                        .to_vec(),
                })
                .collect()
        }

        fn launch(&self, request: &IdeLaunchRequest) -> Result<IdeLaunchReceipt, IdeError> {
            self.launches.lock().unwrap().push(request.clone());
            Ok(IdeLaunchReceipt {
                ide_id: request.ide_id.clone(),
                action: request.action.as_str().to_owned(),
                launched: true,
            })
        }
    }

    fn state(data: &TempDir, ide: Arc<FakeIde>) -> CoreState {
        CoreState::new_with_ide_integration(data.path(), ide).unwrap()
    }

    fn request(id: u64, method: &str, params: Value) -> Request {
        Request {
            id,
            method: method.to_owned(),
            params,
        }
    }

    #[tokio::test]
    async fn detection_reports_supported_and_available_ides() {
        let data = tempfile::tempdir().unwrap();
        let ide = Arc::new(FakeIde::with_available(vec!["cursor"]));
        let response = handle(state(&data, ide), request(1, "ide.detect", json!({}))).await;
        let result = response.result.unwrap();
        assert_eq!(result["supportedIdeIds"], json!(SUPPORTED_IDE_IDS));
        assert_eq!(result["ides"][1]["id"], "cursor");
        assert_eq!(result["ides"][1]["available"], true);
    }

    #[tokio::test]
    async fn preference_is_persisted_and_reloaded() {
        let data = tempfile::tempdir().unwrap();
        let ide = Arc::new(FakeIde::with_available(vec!["vscode", "cursor"]));
        let first = state(&data, ide.clone());
        let updated = handle(
            first,
            request(
                1,
                "ide.configuration.update",
                json!({"preferredIdeId": "cursor"}),
            ),
        )
        .await;
        assert_eq!(updated.result.unwrap()["preferredIdeId"], "cursor");

        let reopened = state(&data, ide);
        let loaded = handle(reopened, request(2, "ide.configuration.get", json!({}))).await;
        assert_eq!(loaded.result.unwrap()["effectiveIdeId"], "cursor");

        let cleared = handle(
            state(&data, Arc::new(FakeIde::with_available(vec!["vscode"]))),
            request(
                3,
                "ide.configuration.update",
                json!({"preferredIdeId": null}),
            ),
        )
        .await;
        assert_eq!(cleared.result.unwrap()["preferredIdeId"], Value::Null);
    }

    #[tokio::test]
    async fn explicit_ide_overrides_preference_and_file_is_canonical() {
        let data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::create_dir(workspace.path().join("src")).unwrap();
        std::fs::write(workspace.path().join("src/main.rs"), "fn main() {}").unwrap();
        let ide = Arc::new(FakeIde::with_available(vec!["vscode", "cursor"]));
        let state = state(&data, ide.clone());
        update_configuration(&state, &json!({"preferredIdeId": "cursor"})).unwrap();

        let response = handle(
            state,
            request(
                1,
                "ide.openFile",
                json!({
                    "workspacePath": workspace.path(),
                    "path": "src/main.rs",
                    "ideId": "vscode",
                    "line": 4,
                    "column": 2,
                }),
            ),
        )
        .await;
        assert_eq!(response.result.unwrap()["ideId"], "vscode");
        let launches = ide.launches.lock().unwrap();
        assert_eq!(launches.len(), 1);
        assert!(matches!(
            &launches[0].action,
            IdeAction::OpenFile { path, line: Some(4), column: Some(2) }
                if path == &std::fs::canonicalize(workspace.path().join("src/main.rs")).unwrap()
        ));
    }

    #[tokio::test]
    async fn diff_routes_two_canonical_workspace_files() {
        let data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("left.txt"), "left").unwrap();
        std::fs::write(workspace.path().join("right.txt"), "right").unwrap();
        let ide = Arc::new(FakeIde::with_available(vec!["windsurf"]));
        let response = handle(
            state(&data, ide.clone()),
            request(
                1,
                "ide.openDiff",
                json!({
                    "workspacePath": workspace.path(),
                    "leftPath": "left.txt",
                    "rightPath": "right.txt",
                }),
            ),
        )
        .await;
        assert_eq!(response.result.unwrap()["action"], "openDiff");
        assert!(matches!(
            ide.launches.lock().unwrap()[0].action,
            IdeAction::OpenDiff { .. }
        ));
    }

    #[tokio::test]
    async fn workspace_escape_is_rejected_without_launch() {
        let data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        let ide = Arc::new(FakeIde::with_available(vec!["vscode"]));
        let response = handle(
            state(&data, ide.clone()),
            request(
                1,
                "ide.openFile",
                json!({"workspacePath": workspace.path(), "path": outside.path()}),
            ),
        )
        .await;
        assert!(response.error.is_some());
        assert!(ide.launches.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unavailable_and_unknown_ides_return_useful_errors() {
        let data = tempfile::tempdir().unwrap();
        let ide = Arc::new(FakeIde::default());
        let no_ide = handle(
            state(&data, ide.clone()),
            request(1, "ide.openProject", json!({"path": data.path()})),
        )
        .await;
        assert!(no_ide.error.is_some());

        let unknown = handle(
            state(&data, ide),
            request(
                2,
                "ide.openProject",
                json!({"path": data.path(), "ideId": "other"}),
            ),
        )
        .await;
        assert!(unknown.error.is_some());
    }

    #[tokio::test]
    async fn invalid_positions_do_not_launch() {
        let data = tempfile::tempdir().unwrap();
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("file.txt"), "text").unwrap();
        let ide = Arc::new(FakeIde::with_available(vec!["vscode"]));
        let response = handle(
            state(&data, ide.clone()),
            request(
                1,
                "ide.openFile",
                json!({
                    "workspacePath": workspace.path(),
                    "path": "file.txt",
                    "column": 1,
                }),
            ),
        )
        .await;
        assert!(response.error.is_some());
        assert!(ide.launches.lock().unwrap().is_empty());
    }
}
