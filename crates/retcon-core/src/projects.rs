//! Project opening, lightweight repository analysis, and health reporting.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use uuid::Uuid;

use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::state::CoreState;

const METADATA_KEY: &str = "project.metadata";

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        ErrorSource::System,
        "Retcon could not open that project.",
        message,
    )
}

fn scope(id: Uuid) -> String {
    format!("project:{id}")
}

fn canonical_directory(path: &str) -> Result<PathBuf, CoreError> {
    let path = Path::new(path).canonicalize().map_err(|e| {
        CoreError::new(
            ErrorCode::Io,
            ErrorSource::System,
            "The project folder is unavailable.",
            format!("canonicalize project path: {e}"),
        )
        .suggested_fix("Choose a folder that exists and that you can read.")
    })?;
    if !path.is_dir() {
        return Err(invalid("project path is not a directory"));
    }
    Ok(path)
}

/// Open a local folder, creating (or refreshing) its durable project record.
pub async fn open(state: &CoreState, path: &str) -> Result<Value, CoreError> {
    let root = canonical_directory(path)?;
    let root_text = root.to_string_lossy().into_owned();
    let remote = retcon_git::run_git(&root, &["remote", "get-url", "origin"])
        .await
        .ok()
        .filter(|s| !s.is_empty());
    let project = match state.storage().projects().find_by_path(&root_text)? {
        Some(project) => project,
        None => {
            let project = state
                .storage()
                .projects()
                .create(&retcon_storage::NewProject::new(
                    root.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Project"),
                ))?;
            state
                .storage()
                .projects()
                .add_location(project.id, &root_text, remote.as_deref())?;
            project
        }
    };
    // A reopened project may have gained an origin or moved between remotes.
    state
        .storage()
        .projects()
        .add_location(project.id, &root_text, remote.as_deref())?;
    let current = metadata(state, project.id)?;
    let defaults = json!({
        "name": project.name, "repositoryPath": root_text, "remoteUrl": remote,
        "pinned": false, "preferredProvider": null, "preferredTerminal": null,
        "preferredIde": null, "devServerCommand": null, "testCommand": null,
        "buildCommand": null, "environmentProfile": null, "permissionProfile": null,
    });
    let merged = merge(defaults, current);
    save_metadata(state, project.id, &merged)?;
    state.emit(
        "project.opened",
        json!({"projectId": project.id, "path": root}),
    );
    let analysis_root = root.clone();
    let analysis = tokio::task::spawn_blocking(move || analyze(&analysis_root))
        .await
        .map_err(|error| {
            CoreError::new(
                ErrorCode::Internal,
                ErrorSource::System,
                "Retcon could not analyze that project.",
                error.to_string(),
            )
        })?;
    Ok(json!({"id": project.id, "metadata": merged, "analysis": analysis, "health": health(&root).await}))
}

/// Clone a repository and then open the resulting folder.
pub async fn clone(
    state: &CoreState,
    remote_url: &str,
    destination: &str,
) -> Result<Value, CoreError> {
    if remote_url.trim().is_empty() || destination.trim().is_empty() {
        return Err(invalid("clone requires 'remoteUrl' and 'destination'"));
    }
    let destination = Path::new(destination);
    if destination.exists() {
        return Err(invalid("clone destination already exists"));
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let dest_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("clone destination must have a file name"))?;
    retcon_git::run_git(parent, &["clone", "--", remote_url, dest_name])
        .await
        .map_err(|error| {
            CoreError::new(
                ErrorCode::Io,
                ErrorSource::System,
                "Retcon could not clone that repository.",
                error.to_string(),
            )
        })?;
    open(state, &destination.to_string_lossy()).await
}

/// List recent projects, optionally filtering by name or repository path.
pub fn list(state: &CoreState, query: Option<&str>) -> Result<Value, CoreError> {
    let needle = query.unwrap_or_default().to_ascii_lowercase();
    let records = state
        .storage()
        .projects()
        .list()?
        .into_iter()
        .filter_map(|project| {
            let data = metadata(state, project.id).ok()?;
            let name = data
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(&project.name);
            let path = data
                .get("repositoryPath")
                .and_then(Value::as_str)
                .unwrap_or_default();
            (needle.is_empty()
                || name.to_ascii_lowercase().contains(&needle)
                || path.to_ascii_lowercase().contains(&needle))
            .then(|| json!({"id": project.id, "metadata": data, "updatedAt": project.updated_at}))
        })
        .collect::<Vec<_>>();
    Ok(json!({"projects": records}))
}

/// Persist a partial metadata update for a project.
pub fn update_metadata(state: &CoreState, id: Uuid, patch: &Value) -> Result<Value, CoreError> {
    if state.storage().projects().get(id)?.is_none() {
        return Err(CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::System,
            "That project is no longer available.",
            format!("unknown project id {id}"),
        ));
    }
    let value = merge(metadata(state, id)?, patch.clone());
    save_metadata(state, id, &value)?;
    state.emit("project.metadataUpdated", json!({"projectId": id}));
    Ok(value)
}

/// Remove a project from the recent-project list without touching its files.
pub fn remove(state: &CoreState, id: Uuid) -> Result<(), CoreError> {
    if !state.storage().projects().archive(id)? {
        return Err(CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::System,
            "That project is no longer available.",
            format!("unknown project id {id}"),
        ));
    }
    state.emit("project.removed", json!({"projectId": id}));
    Ok(())
}

/// Analyze a folder without adding it to the recent-project list.
pub async fn inspect(path: &str) -> Result<Value, CoreError> {
    let root = canonical_directory(path)?;
    let analysis_root = root.clone();
    let analysis = tokio::task::spawn_blocking(move || analyze(&analysis_root))
        .await
        .map_err(|error| {
            CoreError::new(
                ErrorCode::Internal,
                ErrorSource::System,
                "Retcon could not analyze that project.",
                error.to_string(),
            )
        })?;
    Ok(json!({"analysis": analysis, "health": health(&root).await}))
}

fn metadata(state: &CoreState, id: Uuid) -> Result<Value, CoreError> {
    Ok(state
        .storage()
        .settings()
        .get(&scope(id), METADATA_KEY)?
        .map(|s| s.value)
        .unwrap_or_else(|| json!({})))
}
fn save_metadata(state: &CoreState, id: Uuid, value: &Value) -> Result<(), CoreError> {
    state
        .storage()
        .settings()
        .set(&scope(id), METADATA_KEY, value)?;
    Ok(())
}
fn merge(mut base: Value, patch: Value) -> Value {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            if !value.is_null() {
                base.insert(key.clone(), value.clone());
            }
        }
    }
    base
}

fn analyze(root: &Path) -> Value {
    let mut languages = BTreeSet::new();
    let mut frameworks = BTreeSet::new();
    let mut managers = BTreeSet::new();
    let mut tests = BTreeSet::new();
    let mut instructions = Vec::new();
    let mut environment_templates = Vec::new();
    let has = |name: &str| root.join(name).is_file();
    if has("Cargo.toml") {
        languages.insert("Rust");
        managers.insert("Cargo");
    }
    if has("package.json") {
        languages.insert("JavaScript/TypeScript");
        managers.insert(if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "Yarn"
        } else {
            "npm"
        });
    }
    if has("pyproject.toml") || has("requirements.txt") {
        languages.insert("Python");
        managers.insert("pip");
    }
    if has("go.mod") {
        languages.insert("Go");
        managers.insert("Go modules");
    }
    if has("Gemfile") {
        languages.insert("Ruby");
        managers.insert("Bundler");
    }
    if has("package.json") {
        let package = std::fs::read_to_string(root.join("package.json")).unwrap_or_default();
        for (needle, name) in [
            ("next", "Next.js"),
            ("react", "React"),
            ("vite", "Vite"),
            ("flutter", "Flutter"),
            ("vitest", "Vitest"),
            ("jest", "Jest"),
        ] {
            if package.contains(&format!("\"{needle}\"")) {
                if matches!(name, "Vitest" | "Jest") {
                    tests.insert(name);
                } else {
                    frameworks.insert(name);
                }
            }
        }
    }
    for (file, label) in [
        ("Dockerfile", "Docker"),
        ("docker-compose.yml", "Docker Compose"),
        ("compose.yaml", "Docker Compose"),
    ] {
        if has(file) {
            frameworks.insert(label);
        }
    }
    for name in [
        "AGENTS.md",
        "CLAUDE.md",
        "CONTRIBUTING.md",
        ".github/copilot-instructions.md",
    ] {
        if has(name) {
            instructions.push(name);
        }
    }
    for name in [".env.example", ".env.sample", ".env.template"] {
        if has(name) {
            environment_templates.push(name);
        }
    }
    let ci = root.join(".github/workflows").is_dir()
        || has(".gitlab-ci.yml")
        || has("azure-pipelines.yml");
    json!({"languages": languages, "frameworks": frameworks, "packageManagers": managers, "testFrameworks": tests, "monorepo": has("pnpm-workspace.yaml") || has("Cargo.toml") && root.join("crates").is_dir(), "docker": root.join("Dockerfile").is_file() || root.join("compose.yaml").is_file(), "ci": ci, "repositoryInstructions": instructions, "agentInstructions": instructions.iter().filter(|x| matches!(**x, "AGENTS.md" | "CLAUDE.md")).collect::<Vec<_>>(), "environmentTemplates": environment_templates})
}

async fn health(root: &Path) -> Value {
    let git_version = retcon_git::run_git(root, &["--version"]).await.is_ok();
    let git_status = retcon_git::run_git(root, &["status", "--porcelain=v1", "--branch"])
        .await
        .ok();
    let branch = git_status
        .as_deref()
        .and_then(|s| s.lines().next())
        .and_then(|s| s.strip_prefix("## "))
        .map(|s| s.split("...").next().unwrap_or(s));
    let dirty = git_status
        .as_deref()
        .map(|s| s.lines().skip(1).next().is_some())
        .unwrap_or(false);
    let disk = std::fs::metadata(root)
        .ok()
        .map(|_| "available")
        .unwrap_or("unavailable");
    json!({"checks": [
      {"name":"Git installed", "status": if git_version {"pass"} else {"warning"}},
      {"name":"Repository readable", "status": if git_status.is_some() {"pass"} else {"warning"}},
      {"name":"Branch detected", "status": if branch.is_some() {"pass"} else {"warning"}, "detail": branch},
      {"name":"Working tree", "status": if dirty {"warning"} else {"pass"}, "detail": if dirty {"uncommitted changes"} else {"clean"}},
      {"name":"Project folder", "status": if disk == "available" {"pass"} else {"error"}},
      {"name":"Environment template", "status": if root.join(".env.example").is_file() || !root.join(".env").exists() {"pass"} else {"warning"}, "detail":"Review local environment files before running commands."}
    ]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analysis_finds_rust_and_agent_instructions() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "rules").unwrap();
        let report = analyze(dir.path());
        assert!(
            report["languages"]
                .as_array()
                .unwrap()
                .contains(&json!("Rust"))
        );
        assert_eq!(report["agentInstructions"][0], "AGENTS.md");
    }

    #[tokio::test]
    async fn opening_a_project_persists_its_metadata() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("Cargo.toml"), "[package]").unwrap();
        let state = CoreState::new(data.path()).unwrap();
        let opened = open(&state, &project.path().to_string_lossy())
            .await
            .unwrap();
        let id = Uuid::parse_str(opened["id"].as_str().unwrap()).unwrap();
        update_metadata(
            &state,
            id,
            &json!({"pinned": true, "testCommand": "cargo test"}),
        )
        .unwrap();
        let projects = list(&state, None).unwrap();
        assert_eq!(projects["projects"][0]["metadata"]["pinned"], true);
        assert_eq!(
            projects["projects"][0]["metadata"]["testCommand"],
            "cargo test"
        );
    }
}
