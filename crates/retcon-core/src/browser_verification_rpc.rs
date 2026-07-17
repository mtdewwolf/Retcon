//! Durable browser-verification RPC orchestration.

use std::collections::HashMap;

use retcon_storage::{
    NewBrowserVerificationDefinition, NewBrowserVerificationRun, NewBrowserVerificationVariant,
    StorageError,
};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::browser_verification::BrowserVerificationInvocation;
use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::Response;
use crate::state::CoreState;

const ACTOR: &str = "local_user";

pub async fn handle(state: &CoreState, id: u64, method: &str, params: &Value) -> Response {
    match method {
        "browser.verification.definition.create" => save_definition(state, id, params, false),
        "browser.verification.definition.update" => save_definition(state, id, params, true),
        "browser.verification.definition.list" => list_definitions(state, id, params),
        "browser.verification.definition.get" => get_definition(state, id, params),
        "browser.verification.run" => run(state, id, params).await,
        "browser.verification.cancel" => cancel(state, id, params).await,
        "browser.verification.get" => get_run(state, id, params),
        "browser.verification.list" => list_runs(state, id, params),
        "browser.verification.review" => review(state, id, params),
        "browser.verification.baseline.approve" => approve_baseline(state, id, params),
        _ => missing(id, "browser verification method"),
    }
}

fn save_definition(state: &CoreState, id: u64, params: &Value, update: bool) -> Response {
    let project_id = match uuid_param(params, "projectId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let definition_id = if update {
        match uuid_param(params, "definitionId") {
            Ok(value) => value,
            Err(error) => return invalid(id, error),
        }
    } else {
        match optional_uuid(params, "definitionId") {
            Ok(value) => value.unwrap_or_else(Uuid::new_v4),
            Err(error) => return invalid(id, error),
        }
    };
    if update {
        match state
            .storage()
            .database()
            .browser_verification()
            .definition(definition_id)
        {
            Ok(Some(existing)) if existing.project_id == project_id => {}
            Ok(Some(_)) => return missing(id, "browser verification definition"),
            Ok(None) => return missing(id, "browser verification definition"),
            Err(error) => return storage_error(id, error),
        }
    }
    let variants = match parse_variants(params.get("variants")) {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let mut input = NewBrowserVerificationDefinition::new(
        project_id,
        params
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        params
            .get("targetUrl")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    input.id = definition_id;
    input.task_id = match optional_uuid(params, "taskId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    input.dev_server_config_id = match optional_uuid(params, "devServerConfigId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    input.steps = params.get("steps").cloned().unwrap_or_else(|| json!([]));
    input.assertions = params
        .get("assertions")
        .cloned()
        .unwrap_or_else(|| json!([]));
    input.visual_policy = params
        .get("visualPolicy")
        .cloned()
        .unwrap_or_else(|| input.visual_policy.clone());
    input.fail_on_accessibility = params
        .get("failOnAccessibility")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    input.variants = variants.unwrap_or(input.variants);
    input.status = params
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("active")
        .into();
    input.required = params
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    input.timeout_ms = params
        .get("timeoutMs")
        .and_then(Value::as_i64)
        .unwrap_or(60_000);
    input.max_retries = params
        .get("maxRetries")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if let Err(error) = normalized_steps(&input.steps) {
        return invalid(id, error);
    }
    match state
        .storage()
        .database()
        .browser_verification()
        .save_definition(&input, ACTOR)
    {
        Ok(definition) => Response::ok(id, json!({"definition":definition})),
        Err(error) => storage_error(id, error),
    }
}

fn list_definitions(state: &CoreState, id: u64, params: &Value) -> Response {
    let project_id = match uuid_param(params, "projectId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .browser_verification()
        .definitions(project_id)
    {
        Ok(definitions) => Response::ok(id, json!({"definitions":definitions})),
        Err(error) => storage_error(id, error),
    }
}

fn get_definition(state: &CoreState, id: u64, params: &Value) -> Response {
    let definition_id = match uuid_param(params, "definitionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .browser_verification()
        .definition(definition_id)
    {
        Ok(Some(definition)) => Response::ok(id, json!({"definition":definition})),
        Ok(None) => missing(id, "browser verification definition"),
        Err(error) => storage_error(id, error),
    }
}

async fn run(state: &CoreState, id: u64, params: &Value) -> Response {
    let definition_id = match uuid_param(params, "definitionId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let task_id = match uuid_param(params, "taskId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let run_id = match optional_uuid(params, "runId") {
        Ok(value) => value.unwrap_or_else(Uuid::new_v4),
        Err(error) => return invalid(id, error),
    };
    let dev_server_instance_id = match optional_uuid(params, "devServerInstanceId") {
        Ok(Some(value)) => Some(value),
        Ok(None) => {
            return invalid(
                id,
                "devServerInstanceId is required for browser verification",
            );
        }
        Err(error) => return invalid(id, error),
    };
    let input = NewBrowserVerificationRun {
        id: run_id,
        definition_id,
        task_id,
        dev_server_instance_id,
        browser_session_id: match optional_uuid(params, "browserSessionId") {
            Ok(value) => value,
            Err(error) => return invalid(id, error),
        },
        idempotency_key: params
            .get("idempotencyKey")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };
    let repository = state.storage().database().browser_verification();
    let queued = match repository.queue_run(&input, ACTOR) {
        Ok(value) => value,
        Err(error) => return storage_error(id, error),
    };
    if queued.value.run.status != "queued" {
        return Response::ok(id, json!({"verification":queued.value,"idempotent":true}));
    }
    let running = match repository.start(queued.value.run.id, ACTOR) {
        Ok(Some(value)) => value,
        Ok(None) => return missing(id, "browser verification run"),
        Err(error) => return storage_error(id, error),
    };
    if let Err(error) = prepare_visual_baselines(state, &running) {
        let failure = scrub(&error);
        let _ = repository.fail(running.run.id, &failure, ACTOR);
        return service_error(id, failure);
    }
    let artifact_directory = state
        .storage()
        .data_dir()
        .join("browser-artifacts")
        .join("verifications")
        .join(running.run.id.to_string());
    if let Err(error) = std::fs::create_dir_all(&artifact_directory) {
        let failure = scrub(&error.to_string());
        let _ = repository.fail(running.run.id, &failure, ACTOR);
        return service_error(id, failure);
    }
    let invocation = match invocation(&running, &artifact_directory) {
        Ok(value) => value,
        Err(error) => {
            let _ = repository.fail(running.run.id, &error, ACTOR);
            return invalid(id, error);
        }
    };
    match state.browser_verification_runner().run(invocation).await {
        Ok(outcome) => {
            if let Err(error) = verify_outcome_artifacts(state, &outcome) {
                let failure = scrub(&error);
                let _ = repository.fail(running.run.id, &failure, ACTOR);
                return service_error(id, failure);
            }
            match repository.complete(running.run.id, &outcome, "browser_verification_runner") {
                Ok(Some(value)) => Response::ok(
                    id,
                    json!({"verification":value.value,"reopened":value.reopened}),
                ),
                Ok(None) => missing(id, "browser verification run"),
                Err(error) => storage_error(id, error),
            }
        }
        Err(error) => {
            let failure = scrub(&error);
            let _ = repository.fail(running.run.id, &failure, ACTOR);
            service_error(id, failure)
        }
    }
}

fn prepare_visual_baselines(
    state: &CoreState,
    details: &retcon_storage::BrowserVerificationDetails,
) -> Result<(), String> {
    let step_id = details
        .definition
        .steps
        .as_array()
        .and_then(|steps| {
            steps.iter().find(|step| {
                step.get("enabled").and_then(Value::as_bool) != Some(false)
                    && step.get("kind").and_then(Value::as_str) == Some("screenshot")
                    && step.get("compare").and_then(Value::as_bool) != Some(false)
            })
        })
        .and_then(|step| step.get("id"))
        .and_then(Value::as_str);
    let Some(step_id) = step_id else {
        return Ok(());
    };
    let root = state
        .storage()
        .data_dir()
        .join("browser-artifacts")
        .join("visual-baselines");
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("create visual baseline root: {error}"))?;
    let root = std::fs::canonicalize(&root)
        .map_err(|error| format!("resolve visual baseline root: {error}"))?;
    let baselines = state
        .storage()
        .database()
        .browser_verification()
        .active_baselines(details.definition.id)
        .map_err(|error| error.to_string())?;
    for variant in &details.definition.variants {
        let parent = root
            .join(details.definition.id.to_string())
            .join(&variant.key);
        std::fs::create_dir_all(&parent)
            .map_err(|error| format!("create visual baseline directory: {error}"))?;
        let parent = std::fs::canonicalize(&parent)
            .map_err(|error| format!("resolve visual baseline directory: {error}"))?;
        if !parent.starts_with(&root) {
            return Err("visual baseline directory escaped its managed root".into());
        }
        let target = parent.join(format!("{step_id}.png"));
        if let Some(baseline) = baselines
            .iter()
            .find(|baseline| baseline.variant_key == variant.key)
        {
            state
                .storage()
                .artifacts()
                .verify(&baseline.artifact_hash)
                .map_err(|error| error.to_string())?;
            let temporary = parent.join(format!(".{step_id}.{}.tmp", details.run.id));
            let mut source = state
                .storage()
                .artifacts()
                .get(&baseline.artifact_hash)
                .map_err(|error| error.to_string())?;
            let mut staged = std::fs::File::create(&temporary)
                .map_err(|error| format!("create staged visual baseline: {error}"))?;
            std::io::copy(&mut source, &mut staged)
                .map_err(|error| format!("stage visual baseline: {error}"))?;
            staged
                .sync_all()
                .map_err(|error| format!("sync visual baseline: {error}"))?;
            if target.exists() {
                std::fs::remove_file(&target)
                    .map_err(|error| format!("replace visual baseline: {error}"))?;
            }
            std::fs::rename(&temporary, &target)
                .map_err(|error| format!("install visual baseline: {error}"))?;
        } else if target.exists() {
            std::fs::remove_file(&target)
                .map_err(|error| format!("remove unapproved visual baseline: {error}"))?;
        }
    }
    Ok(())
}

async fn cancel(state: &CoreState, id: u64, params: &Value) -> Response {
    let run_id = match uuid_param(params, "runId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let runner_error = state
        .browser_verification_runner()
        .cancel(run_id)
        .await
        .err();
    match state
        .storage()
        .database()
        .browser_verification()
        .cancel(run_id, ACTOR)
    {
        Ok(Some(value)) => Response::ok(
            id,
            json!({"verification":value.value,"reopened":value.reopened,"runnerWarning":runner_error.map(|value|scrub(&value))}),
        ),
        Ok(None) => missing(id, "browser verification run"),
        Err(error) => storage_error(id, error),
    }
}

fn get_run(state: &CoreState, id: u64, params: &Value) -> Response {
    let run_id = match uuid_param(params, "runId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .browser_verification()
        .get(run_id)
    {
        Ok(Some(value)) => Response::ok(id, json!({"verification":value})),
        Ok(None) => missing(id, "browser verification run"),
        Err(error) => storage_error(id, error),
    }
}

fn list_runs(state: &CoreState, id: u64, params: &Value) -> Response {
    let task_id = match uuid_param(params, "taskId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    match state
        .storage()
        .database()
        .browser_verification()
        .runs(task_id)
    {
        Ok(runs) => Response::ok(id, json!({"runs":runs})),
        Err(error) => storage_error(id, error),
    }
}

fn review(state: &CoreState, id: u64, params: &Value) -> Response {
    let run_id = match uuid_param(params, "runId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let decision = params
        .get("decision")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let reason = scrub(
        params
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    match state
        .storage()
        .database()
        .browser_verification()
        .review(run_id, decision, &reason, ACTOR)
    {
        Ok(Some(value)) => Response::ok(
            id,
            json!({"verification":value.value,"reopened":value.reopened}),
        ),
        Ok(None) => missing(id, "browser verification run"),
        Err(error) => storage_error(id, error),
    }
}

fn approve_baseline(state: &CoreState, id: u64, params: &Value) -> Response {
    let comparison_id = match uuid_param(params, "comparisonId") {
        Ok(value) => value,
        Err(error) => return invalid(id, error),
    };
    let repository = state.storage().database().browser_verification();
    match repository.comparison_artifact_hash(comparison_id) {
        Ok(Some(hash)) => {
            if let Err(error) = state.storage().artifacts().verify(&hash) {
                return storage_error(id, error);
            }
        }
        Ok(None) => return missing(id, "browser visual comparison"),
        Err(error) => return storage_error(id, error),
    }
    match repository.approve_baseline(comparison_id, ACTOR) {
        Ok(Some(baseline)) => {
            if let Some(run_id) = baseline.source_run_id {
                match repository.get(run_id) {
                    Ok(Some(details)) => {
                        if let Err(error) = prepare_visual_baselines(state, &details) {
                            return service_error(id, error);
                        }
                    }
                    Ok(None) => return missing(id, "browser verification run"),
                    Err(error) => return storage_error(id, error),
                }
            }
            Response::ok(id, json!({"baseline":baseline}))
        }
        Ok(None) => missing(id, "browser visual comparison"),
        Err(error) => storage_error(id, error),
    }
}

fn invocation(
    details: &retcon_storage::BrowserVerificationDetails,
    artifact_directory: &std::path::Path,
) -> Result<BrowserVerificationInvocation, String> {
    let definition = &details.definition;
    let policy = &definition.visual_policy;
    let pixel_threshold = policy
        .get("pixelThreshold")
        .and_then(Value::as_f64)
        .unwrap_or(0.01);
    let max_diff = policy
        .get("maxDiffPixelRatio")
        .and_then(Value::as_f64)
        .unwrap_or(0.01);
    let perceptual = policy
        .get("perceptualThreshold")
        .and_then(Value::as_f64)
        .unwrap_or(0.01);
    let dynamic = policy
        .get("dynamicRegions")
        .or_else(|| policy.get("maskSelectors"))
        .cloned()
        .unwrap_or_else(|| json!([]));
    let ignored = policy
        .get("ignoreSelectors")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let (steps, required_assertions) = normalized_steps(&definition.steps)?;
    let wire = json!({
        "runId": details.run.id,
        "artifactDirectory": artifact_directory.to_string_lossy(),
        "timeoutMs": details.run.timeout_ms,
        "definition": {
            "id": definition.id,
            "targetUrl": definition.target_url,
            "serverRef": details.run.dev_server_instance_id.map(|value|value.to_string()),
            "steps": steps,
            "variants": definition.variants.iter().map(|variant|json!({
                "name":variant.key,"width":variant.width,"height":variant.height,
                "deviceScaleFactor":variant.device_scale,"isMobile":false
            })).collect::<Vec<_>>(),
            "timeoutMs": definition.timeout_ms,
            "retries": definition.max_retries,
            "failOnAccessibility": definition.fail_on_accessibility,
            "visual": {
                "updateBaseline":"never",
                "pixelThreshold":pixel_threshold,
                "maxDiffPixelRatio":max_diff,
                "perceptualThreshold":perceptual,
                "maskSelectors":dynamic,
                "ignoreSelectors":ignored
            }
        }
    });
    Ok(BrowserVerificationInvocation {
        wire,
        required_assertions,
    })
}

fn normalized_steps(steps: &Value) -> Result<(Value, HashMap<String, bool>), String> {
    let steps = steps
        .as_array()
        .ok_or("browser verification steps must be an array")?;
    let mut translated = Vec::new();
    let mut required_assertions = HashMap::new();
    for step in steps {
        let object = step
            .as_object()
            .ok_or("browser verification step must be an object")?;
        if object.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let kind = object
            .get("kind")
            .or_else(|| object.get("type"))
            .and_then(Value::as_str)
            .ok_or("browser verification step omitted kind")?;
        if kind == "takeover" {
            return Err(
                "takeover is timeline evidence and cannot be a browser verification step".into(),
            );
        }
        translated.push(translate_step(kind, object)?);
        if kind.starts_with("assert.") {
            record_requirement(object, &mut required_assertions);
        }
        if let Some(assertions) = object.get("assertions") {
            let assertions = assertions
                .as_array()
                .ok_or("browser verification nested assertions must be an array")?;
            for assertion in assertions {
                let assertion = assertion
                    .as_object()
                    .ok_or("browser verification assertion must be an object")?;
                let kind = assertion
                    .get("kind")
                    .and_then(Value::as_str)
                    .ok_or("browser verification assertion omitted kind")?;
                record_requirement(assertion, &mut required_assertions);
                translated.push(translate_assertion(kind, assertion)?);
            }
        }
        if translated.len() > 512 {
            return Err("browser verification expands to more than 512 runner steps".into());
        }
    }
    Ok((Value::Array(translated), required_assertions))
}

fn record_requirement(
    object: &serde_json::Map<String, Value>,
    requirements: &mut HashMap<String, bool>,
) {
    if let Some(id) = object.get("id").and_then(Value::as_str) {
        requirements.insert(
            id.to_owned(),
            object
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(true),
        );
    }
}

fn translate_step(kind: &str, object: &serde_json::Map<String, Value>) -> Result<Value, String> {
    let target = object.get("target").cloned();
    let value = object.get("value").cloned();
    let mut output = serde_json::Map::new();
    output.insert(
        "type".into(),
        Value::String(if kind == "waitFor" { "wait" } else { kind }.into()),
    );
    if let Some(id) = object.get("id") {
        output.insert("id".into(), id.clone());
    }
    match kind {
        "navigate" => insert_required(&mut output, "url", target, "navigate target")?,
        "click" => insert_required(&mut output, "selector", target, "click target")?,
        "fill" => {
            insert_required(&mut output, "selector", target, "fill target")?;
            insert_required(&mut output, "value", value, "fill value")?;
        }
        "press" => {
            if let Some(target) = target {
                output.insert("selector".into(), target);
            }
            insert_required(&mut output, "key", value, "press value")?;
        }
        "wait" | "waitFor" => {
            if let Some(target) = target {
                output.insert("selector".into(), target);
            }
            if let Some(value) = value {
                output.insert("state".into(), value);
            }
        }
        "screenshot" => {
            let compare = object
                .get("compare")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let name = if compare {
                object.get("id").cloned()
            } else {
                value.or(target)
            };
            if let Some(name) = name {
                output.insert("name".into(), name);
            }
            output.insert("compare".into(), Value::Bool(compare));
        }
        "accessibility" => {}
        "assert.text" | "assert.element" | "assert.status" | "assert.console" => {
            return translate_assertion(kind, object);
        }
        _ => {
            return Err(format!(
                "unsupported browser verification step kind '{kind}'"
            ));
        }
    }
    if let Some(timeout) = object.get("timeoutMs") {
        output.insert("timeoutMs".into(), timeout.clone());
    }
    Ok(Value::Object(output))
}

fn translate_assertion(
    kind: &str,
    object: &serde_json::Map<String, Value>,
) -> Result<Value, String> {
    let target = object.get("target").cloned();
    let value = object
        .get("expected")
        .or_else(|| object.get("value"))
        .cloned();
    let runner = match kind {
        "text" | "assert.text" => "assert.text",
        "element" | "assert.element" => "assert.element",
        "statusCode" | "assert.status" => "assert.status",
        "console" | "assert.console" => "assert.console",
        "screenshot" => "screenshot",
        "accessibility" => "accessibility",
        _ => {
            return Err(format!(
                "unsupported browser verification assertion kind '{kind}'"
            ));
        }
    };
    let mut output = serde_json::Map::new();
    output.insert("type".into(), Value::String(runner.into()));
    if let Some(id) = object.get("id") {
        output.insert("id".into(), id.clone());
    }
    match runner {
        "assert.text" => {
            if let Some(target) = target {
                output.insert("selector".into(), target);
            }
            insert_required(
                &mut output,
                "expected",
                value,
                "text assertion expected value",
            )?;
        }
        "assert.element" => {
            insert_required(&mut output, "selector", target, "element assertion target")?;
            if let Some(value) = value {
                output.insert("state".into(), value);
            }
        }
        "assert.status" => insert_required(
            &mut output,
            "expected",
            value,
            "status assertion expected value",
        )?,
        "assert.console" => {
            if let Some(value) = value {
                output.insert("text".into(), value);
            }
            output.insert(
                "absent".into(),
                object.get("absent").cloned().unwrap_or(Value::Bool(false)),
            );
        }
        "screenshot" => {
            if let Some(name) = value.or(target) {
                output.insert("name".into(), name);
            }
        }
        _ => {}
    }
    if let Some(timeout) = object.get("timeoutMs") {
        output.insert("timeoutMs".into(), timeout.clone());
    }
    Ok(Value::Object(output))
}

fn insert_required(
    output: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Option<Value>,
    label: &str,
) -> Result<(), String> {
    let value = value.ok_or_else(|| format!("browser verification {label} is required"))?;
    output.insert(key.into(), value);
    Ok(())
}

fn verify_outcome_artifacts(
    state: &CoreState,
    outcome: &retcon_storage::BrowserVerificationOutcome,
) -> Result<(), String> {
    let hashes = outcome
        .artifacts
        .iter()
        .map(|value| value.hash.as_str())
        .chain(outcome.visual_comparisons.iter().flat_map(|value| {
            std::iter::once(value.current_hash.as_str()).chain(value.difference_hash.as_deref())
        }))
        .chain(
            outcome
                .accessibility
                .iter()
                .filter_map(|value| value.artifact_hash.as_deref()),
        );
    for hash in hashes {
        state
            .storage()
            .artifacts()
            .verify(hash)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn parse_variants(
    value: Option<&Value>,
) -> Result<Option<Vec<NewBrowserVerificationVariant>>, String> {
    let Some(value) = value else { return Ok(None) };
    let values = value.as_array().ok_or("'variants' must be an array")?;
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            Ok(NewBrowserVerificationVariant {
                id: value
                    .get("id")
                    .and_then(Value::as_str)
                    .map(Uuid::parse_str)
                    .transpose()
                    .map_err(|_| "variant id must be a UUID")?
                    .unwrap_or_else(Uuid::new_v4),
                key: value
                    .get("name")
                    .or_else(|| value.get("key"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                width: value
                    .get("width")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                height: value
                    .get("height")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                device_scale: value
                    .get("deviceScaleFactor")
                    .or_else(|| value.get("deviceScale"))
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0),
                device_name: value
                    .get("deviceName")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                sort_order: i64::try_from(index).map_err(|_| "too many variants")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
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

fn invalid(id: u64, message: impl Into<String>) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::InvalidRequest,
            ErrorSource::Rpc,
            "The browser verification request is invalid.",
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
            "The requested browser verification information was not found.",
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

fn service_error(id: u64, error: impl Into<String>) -> Response {
    Response::error(
        id,
        &CoreError::new(
            ErrorCode::Internal,
            ErrorSource::System,
            "The browser verification runner could not complete the request.",
            error,
        ),
    )
}

fn scrub(message: &str) -> String {
    retcon_secrets::redact_text(message)
        .chars()
        .take(4_096)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use retcon_storage::{
        BrowserVerificationOutcome, NewBrowserVerificationArtifact,
        NewBrowserVerificationDefinition, NewBrowserVerificationRun, NewProject, NewTask,
        NewVisualComparison,
    };

    #[test]
    fn runner_boundary_translates_desktop_steps_and_flattens_assertions() {
        let (steps,requirements)=
            normalized_steps(&json!([
                {"id":"nav","kind":"navigate","target":"http://127.0.0.1:3000","timeoutMs":5000,"assertions":[{"id":"status","kind":"statusCode","expected":200,"required":false}]},
                {"id":"fill","kind":"fill","target":"#name","value":"Retcon"},
                {"id":"press","kind":"press","target":"#name","value":"Enter"},
                {"id":"ready","kind":"wait","target":"#ready","value":"visible","assertions":[{"id":"text","kind":"text","target":"#ready","expected":"Ready"},{"id":"element","kind":"element","target":"#ready","expected":"visible"}]},
                {"id":"shot-home","kind":"screenshot","compare":true,"value":"Home"},{"id":"a11y","kind":"accessibility"}
            ])).unwrap();
        assert_eq!(
            steps,
            json!([
                {"id":"nav","type":"navigate","url":"http://127.0.0.1:3000","timeoutMs":5000},
                {"id":"status","type":"assert.status","expected":200},
                {"id":"fill","type":"fill","selector":"#name","value":"Retcon"},
                {"id":"press","type":"press","selector":"#name","key":"Enter"},
                {"id":"ready","type":"wait","selector":"#ready","state":"visible"},
                {"id":"text","type":"assert.text","selector":"#ready","expected":"Ready"},
                {"id":"element","type":"assert.element","selector":"#ready","state":"visible"},
                {"id":"shot-home","type":"screenshot","name":"shot-home","compare":true},{"id":"a11y","type":"accessibility"}
            ])
        );
        assert_eq!(requirements.get("status"), Some(&false));
        assert_eq!(requirements.get("text"), Some(&true));
        assert!(normalized_steps(&json!([{"kind":"takeover"}])).is_err());
    }

    #[tokio::test]
    async fn run_without_development_server_is_rejected_before_service_call() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let response = handle(
            &state,
            1,
            "browser.verification.run",
            &json!({
                "definitionId":Uuid::new_v4(),"taskId":Uuid::new_v4()
            }),
        )
        .await;
        let encoded = response.error.unwrap().to_string();
        assert!(encoded.contains("devServerInstanceId is required"));
    }

    #[test]
    fn approved_baseline_is_materialized_and_replaced_from_cas() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let db = state.storage().database();
        let project = db.projects().create(&NewProject::new("Baseline")).unwrap();
        let mut task = NewTask::new("Visual");
        task.project_id = Some(project.id);
        let task = db.tasks().create(&task).unwrap();
        let mut definition =
            NewBrowserVerificationDefinition::new(project.id, "Visual", "http://127.0.0.1:3000");
        definition.task_id = Some(task.id);
        let definition = db
            .browser_verification()
            .save_definition(&definition, "test")
            .unwrap();
        let execute = |bytes: &[u8], status: &str| {
            let stored = state.storage().artifacts().store_bytes(bytes).unwrap();
            let comparison_id = Uuid::new_v4();
            let queued = db
                .browser_verification()
                .queue_run(
                    &NewBrowserVerificationRun {
                        id: Uuid::new_v4(),
                        definition_id: definition.id,
                        task_id: task.id,
                        dev_server_instance_id: None,
                        browser_session_id: None,
                        idempotency_key: None,
                    },
                    "test",
                )
                .unwrap();
            db.browser_verification()
                .start(queued.value.run.id, "test")
                .unwrap();
            let outcome = BrowserVerificationOutcome {
                runner_version: "fixture".into(),
                summary: json!({}),
                timeline: vec![],
                assertions: vec![],
                artifacts: vec![NewBrowserVerificationArtifact {
                    id: Uuid::new_v4(),
                    kind: "screenshot".into(),
                    hash: stored.hash.clone(),
                    mime_type: "image/png".into(),
                    size_bytes: i64::try_from(stored.size).unwrap(),
                    metadata: json!({}),
                }],
                visual_comparisons: vec![NewVisualComparison {
                    id: comparison_id,
                    baseline_id: None,
                    variant_key: "desktop".into(),
                    current_hash: stored.hash,
                    difference_hash: None,
                    pixel_difference_ratio: if status == "matched" { 0.0 } else { 0.2 },
                    perceptual_difference_ratio: Some(0.0),
                    threshold_ratio: 0.01,
                    status: status.into(),
                    ignore_regions: json!([]),
                }],
                console: vec![],
                network: vec![],
                accessibility: vec![],
            };
            let completed = db
                .browser_verification()
                .complete(queued.value.run.id, &outcome, "runner")
                .unwrap()
                .unwrap();
            (completed.value, comparison_id)
        };
        let (first, comparison) = execute(b"first", "missing_baseline");
        db.browser_verification()
            .approve_baseline(comparison, "reviewer")
            .unwrap();
        prepare_visual_baselines(&state, &first).unwrap();
        let target = directory
            .path()
            .join("browser-artifacts/visual-baselines")
            .join(definition.id.to_string())
            .join("desktop/visual.png");
        assert_eq!(std::fs::read(&target).unwrap(), b"first");
        let (matched, _) = execute(b"first", "matched");
        prepare_visual_baselines(&state, &matched).unwrap();
        assert_eq!(matched.run.status, "passed");
        let (changed, comparison) = execute(b"second", "different");
        db.browser_verification()
            .approve_baseline(comparison, "reviewer")
            .unwrap();
        prepare_visual_baselines(&state, &changed).unwrap();
        assert_eq!(std::fs::read(target).unwrap(), b"second");
    }
}
