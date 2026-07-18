//! Testable boundary for invoking the external browser-verification runner.

#![allow(missing_docs)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use retcon_browser::BrowserService;
use retcon_storage::{
    BrowserVerificationOutcome, NewAccessibilityFinding, NewBrowserAssertionResult,
    NewBrowserVerificationArtifact, NewBrowserVerificationEvent, NewConsoleEvidence,
    NewNetworkEvidence, NewVisualComparison, Storage,
};
use serde_json::{Value, json};
use uuid::Uuid;

pub type BrowserVerificationFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, String>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct BrowserVerificationInvocation {
    pub wire: Value,
    pub required_assertions: HashMap<String, bool>,
}

/// The runner receives the exact `browser.verification.run` wire payload. Core
/// owns durable state and baseline mutation; implementations only execute and
/// return evidence.
pub trait BrowserVerificationRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        invocation: BrowserVerificationInvocation,
    ) -> BrowserVerificationFuture<'a, BrowserVerificationOutcome>;
    fn cancel(&self, run_id: Uuid) -> BrowserVerificationFuture<'_, ()>;
}

#[derive(Default)]
pub struct UnavailableBrowserVerificationRunner;

impl BrowserVerificationRunner for UnavailableBrowserVerificationRunner {
    fn run<'a>(
        &'a self,
        _invocation: BrowserVerificationInvocation,
    ) -> BrowserVerificationFuture<'a, BrowserVerificationOutcome> {
        Box::pin(async { Err("browser verification runner is unavailable".into()) })
    }

    fn cancel(&self, _run_id: Uuid) -> BrowserVerificationFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}

/// Production adapter over the authenticated Node browser-service boundary.
pub struct ServiceBrowserVerificationRunner {
    storage: Storage,
    service: Arc<dyn BrowserService>,
}

impl ServiceBrowserVerificationRunner {
    #[must_use]
    pub fn new(storage: Storage, service: Arc<dyn BrowserService>) -> Self {
        Self { storage, service }
    }
}

impl BrowserVerificationRunner for ServiceBrowserVerificationRunner {
    fn run<'a>(
        &'a self,
        invocation: BrowserVerificationInvocation,
    ) -> BrowserVerificationFuture<'a, BrowserVerificationOutcome> {
        Box::pin(async move {
            let run_id = invocation
                .wire
                .get("runId")
                .and_then(Value::as_str)
                .and_then(|raw| Uuid::parse_str(raw).ok())
                .ok_or("browser verification invocation omitted runId")?;
            let result = self
                .service
                .call(run_id, "browser.verification.run", invocation.wire)
                .await
                .map_err(|error| error.to_string())?;
            decode_outcome(&self.storage, result, &invocation.required_assertions)
        })
    }

    fn cancel(&self, run_id: Uuid) -> BrowserVerificationFuture<'_, ()> {
        Box::pin(async move {
            self.service
                .call(
                    run_id,
                    "browser.verification.cancel",
                    json!({"runId":run_id}),
                )
                .await
                .map(|_| ())
                .map_err(|error| error.to_string())
        })
    }
}

fn decode_outcome(
    storage: &Storage,
    result: retcon_browser::BrowserCallResult,
    required_assertions: &HashMap<String, bool>,
) -> Result<BrowserVerificationOutcome, String> {
    let mut hashes = HashMap::new();
    let service_entries = result
        .value
        .get("artifacts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if service_entries.len() != result.artifacts.len() {
        return Err("browser verification artifact metadata did not match extracted files".into());
    }
    let mut artifacts = Vec::with_capacity(result.artifacts.len());
    for (entry, extracted) in service_entries.iter().zip(result.artifacts) {
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .ok_or("browser verification artifact omitted path")?;
        let stored = storage
            .artifacts()
            .store_bytes(&extracted.bytes)
            .map_err(|error| error.to_string())?;
        hashes.insert(path.to_owned(), stored.hash.clone());
        artifacts.push(NewBrowserVerificationArtifact {
            id: parsed_uuid(entry, "id").unwrap_or_else(Uuid::new_v4),
            kind: extracted.kind,
            hash: stored.hash,
            mime_type: extracted.mime_type,
            size_bytes: i64::try_from(stored.size)
                .map_err(|_| "browser verification artifact is too large")?,
            metadata: extracted.metadata,
        });
    }
    let timeline = array(&result.value, "timeline")
        .iter()
        .map(|value| {
            let severity = match value.get("status").and_then(Value::as_str) {
                Some("failed" | "error" | "cancelled") => "error",
                Some("warning") => "warning",
                _ => "info",
            };
            NewBrowserVerificationEvent {
                kind: string(value, "type", "event"),
                severity: severity.into(),
                payload: value
                    .get("details")
                    .cloned()
                    .unwrap_or_else(|| value.clone()),
            }
        })
        .collect();
    let mut assertions: Vec<_> = array(&result.value, "assertions")
        .iter()
        .map(|value| NewBrowserAssertionResult {
            name: format!(
                "{}:{}",
                string(value, "stepId", "step"),
                string(value, "kind", "assertion")
            ),
            required: value
                .get("stepId")
                .and_then(Value::as_str)
                .and_then(|id| required_assertions.get(id))
                .copied()
                .unwrap_or(true),
            status: string(value, "status", "failed"),
            message: value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    Some(format!(
                        "expected {:?}, actual {:?}",
                        value.get("expected"),
                        value.get("actual")
                    ))
                }),
        })
        .collect();
    let runner_status = result
        .value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("failed");
    let variant_failed = array(&result.value, "variants").iter().any(|value| {
        matches!(
            value.get("status").and_then(Value::as_str),
            Some("failed" | "error" | "cancelled")
        )
    });
    if matches!(runner_status, "failed" | "error" | "cancelled") || variant_failed {
        assertions.push(NewBrowserAssertionResult {
            name: "runner:status".into(),
            required: true,
            status: "failed".into(),
            message: Some(format!(
                "browser verification runner finished with status {runner_status}"
            )),
        });
    }
    let visual_comparisons = array(&result.value, "visualComparisons")
        .iter()
        .map(|value| {
            Ok(NewVisualComparison {
                id: parsed_uuid(value, "id").unwrap_or_else(Uuid::new_v4),
                baseline_id: parsed_uuid(value, "baselineId"),
                variant_key: value
                    .get("variant")
                    .or_else(|| value.get("variantKey"))
                    .and_then(Value::as_str)
                    .unwrap_or("desktop")
                    .to_owned(),
                current_hash: artifact_hash(
                    value,
                    &hashes,
                    &["currentPath", "currentArtifactPath", "artifactPath"],
                )
                .ok_or("visual comparison current artifact was not extracted")?,
                difference_hash: artifact_hash(
                    value,
                    &hashes,
                    &["diffPath", "differencePath", "differenceArtifactPath"],
                ),
                pixel_difference_ratio: number(
                    value,
                    "diffPixelRatio",
                    number(value, "pixelDifferenceRatio", 0.0),
                ),
                perceptual_difference_ratio: value
                    .get("perceptualDifference")
                    .or_else(|| value.get("perceptualDifferenceRatio"))
                    .and_then(Value::as_f64),
                threshold_ratio: value
                    .get("thresholds")
                    .and_then(|thresholds| thresholds.get("maxDiffPixelRatio"))
                    .and_then(Value::as_f64)
                    .unwrap_or_else(|| number(value, "thresholdRatio", 0.01)),
                status: match value.get("status").and_then(Value::as_str) {
                    Some("passed") => "matched",
                    Some("failed") => "different",
                    Some("created") => "missing_baseline",
                    Some("updated") => "approved",
                    Some(other) => other,
                    None => "matched",
                }
                .into(),
                ignore_regions: value
                    .get("ignoreRegions")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let console: Vec<NewConsoleEvidence> = result
        .value
        .get("console")
        .map(|value| array(value, "entries"))
        .unwrap_or(&[])
        .iter()
        .map(|value| NewConsoleEvidence {
            level: match value
                .get("level")
                .or_else(|| value.get("type"))
                .and_then(Value::as_str)
            {
                Some("warn") => "warning",
                Some(other) => other,
                None => "info",
            }
            .into(),
            message: value
                .get("message")
                .or_else(|| value.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            source: value
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_owned),
        })
        .collect();
    let network = result
        .value
        .get("network")
        .map(|value| array(value, "entries"))
        .unwrap_or(&[])
        .iter()
        .map(|value| NewNetworkEvidence {
            method: string(value, "method", "GET"),
            url: string(value, "url", ""),
            status_code: value
                .get("statusCode")
                .or_else(|| value.get("status"))
                .and_then(Value::as_i64),
            failure: value
                .get("failure")
                .or_else(|| value.get("failureText"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            duration_ms: value.get("durationMs").and_then(Value::as_i64),
        })
        .collect::<Vec<_>>();
    let mut console = console;
    console.extend(array(&result.value, "pageErrors").iter().map(|value| {
        NewConsoleEvidence {
            level: "error".into(),
            message: value
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_else(|| value.as_str().unwrap_or("page error"))
                .to_owned(),
            source: Some("page".into()),
        }
    }));
    let accessibility = array(&result.value, "accessibility")
        .iter()
        .flat_map(|variant| {
            let variant_name = string(variant, "variant", "default");
            let hashes = &hashes;
            array(variant, "findings")
                .iter()
                .map(move |value| NewAccessibilityFinding {
                    id: parsed_uuid(value, "id").unwrap_or_else(Uuid::new_v4),
                    rule_id: string(value, "rule", "unknown"),
                    severity: if value.get("severity").and_then(Value::as_str) == Some("error") {
                        "critical".into()
                    } else {
                        "warning".into()
                    },
                    message: string(value, "message", ""),
                    selector: value
                        .get("selector")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    help_url: value
                        .get("helpUrl")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    artifact_hash: artifact_hash(value, hashes, &["artifactPath"]),
                    metadata: json!({"variant":variant_name,"details":value.get("metadata")}),
                })
        })
        .collect();
    Ok(BrowserVerificationOutcome {
        runner_version: result.value.get("runnerVersion").and_then(Value::as_str).unwrap_or("browser-service").to_owned(),
        summary: result.value.get("summary").cloned().unwrap_or_else(||json!({"status":result.value.get("status"),"durationMs":result.value.get("durationMs")})),
        timeline, assertions, artifacts, visual_comparisons, console, network, accessibility,
    })
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}
fn string(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
}
fn number(value: &Value, key: &str, fallback: f64) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(fallback)
}
fn parsed_uuid(value: &Value, key: &str) -> Option<Uuid> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(|raw| Uuid::parse_str(raw).ok())
}
fn artifact_hash(value: &Value, hashes: &HashMap<String, String>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .and_then(|path| hashes.get(path).cloned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use super::*;
    use retcon_browser::{
        BROWSER_SERVICE_PROTOCOL, BrowserService, BrowserServiceArtifact, NodeBrowserService,
        NodeBrowserServiceConfig,
    };
    use retcon_storage::{
        NewBrowserVerificationDefinition, NewBrowserVerificationRun, NewProject, NewTask,
    };
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    async fn fixture() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(async move {
                    let (reader, mut writer) = stream.into_split();
                    let mut reader = BufReader::new(reader);
                    let mut line = String::new();
                    if reader.read_line(&mut line).await.is_err() {
                        return;
                    }
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).await.is_err()
                            || matches!(header.as_str(), "\r\n" | "\n" | "")
                        {
                            break;
                        }
                    }
                    let body = "<!doctype html><html lang=\"en\"><title>Verification</title><main><h1>Ready</h1></main></html>";
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = writer.write_all(response.as_bytes()).await;
                });
            }
        });
        (format!("http://{address}"), task)
    }

    #[test]
    fn settled_runner_contract_decodes_and_drives_completion_counters() {
        let directory = tempfile::tempdir().unwrap();
        let storage = Storage::open(directory.path()).unwrap();
        let current = "C:/managed/current.png";
        let difference = "C:/managed/diff.png";
        let value = json!({
            "status":"passed","durationMs":42,
            "variants":[{"name":"desktop","status":"passed"}],
            "timeline":[{"type":"navigate","status":"passed","details":{"url":"http://127.0.0.1:3000"}}],
            "assertions":[{"stepId":"text-1","kind":"assert.text","status":"passed","message":"found","expected":"Ready","actual":"Ready"}],
            "visualComparisons":[{"variant":"desktop","status":"failed","currentPath":current,"diffPath":difference,"diffPixelRatio":0.2,"perceptualDifference":0.1,"thresholds":{"maxDiffPixelRatio":0.01}}],
            "console":{"entries":[{"type":"error","text":"boom","source":"app.js"}]},
            "network":{"entries":[{"method":"GET","url":"http://127.0.0.1:3000/api","status":500,"durationMs":3}]},
            "pageErrors":[],
            "accessibility":[{"variant":"desktop","findings":[{"rule":"color-contrast","severity":"error","selector":"#cta","message":"contrast failed"}]}],
            "artifacts":[{"kind":"screenshot","mimeType":"image/png","path":current},{"kind":"difference","mimeType":"image/png","path":difference}]
        });
        let call = retcon_browser::BrowserCallResult {
            value,
            artifacts: vec![
                BrowserServiceArtifact {
                    kind: "screenshot".into(),
                    mime_type: "image/png".into(),
                    bytes: b"current".to_vec(),
                    metadata: json!({"servicePath":current}),
                },
                BrowserServiceArtifact {
                    kind: "difference".into(),
                    mime_type: "image/png".into(),
                    bytes: b"diff".to_vec(),
                    metadata: json!({"servicePath":difference}),
                },
            ],
        };
        let outcome = decode_outcome(&storage, call, &HashMap::new()).unwrap();
        assert_eq!(outcome.timeline[0].kind, "navigate");
        assert_eq!(outcome.console[0].message, "boom");
        assert_eq!(outcome.network[0].status_code, Some(500));
        assert_eq!(outcome.accessibility[0].severity, "critical");
        assert_eq!(outcome.visual_comparisons[0].status, "different");
        assert_eq!(outcome.visual_comparisons[0].pixel_difference_ratio, 0.2);

        let db = storage.database();
        let project = db.projects().create(&NewProject::new("Fixture")).unwrap();
        let mut task = NewTask::new("Verify");
        task.project_id = Some(project.id);
        let task = db.tasks().create(&task).unwrap();
        let mut definition =
            NewBrowserVerificationDefinition::new(project.id, "UI", "http://127.0.0.1:3000");
        definition.task_id = Some(task.id);
        let definition = db
            .browser_verification()
            .save_definition(&definition, "test")
            .unwrap();
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
        let completed = db
            .browser_verification()
            .complete(queued.value.run.id, &outcome, "runner")
            .unwrap()
            .unwrap();
        assert_eq!(completed.value.run.status, "failed");
        assert_eq!(completed.value.run.critical_accessibility, 1);
        assert_eq!(completed.value.run.visual_differences, 1);
        assert_eq!(completed.value.run.console_errors, 1);
    }

    #[test]
    fn failed_and_cancelled_runner_statuses_synthesize_blocking_failures() {
        let directory = tempfile::tempdir().unwrap();
        let storage = Storage::open(directory.path()).unwrap();
        for status in ["failed", "cancelled"] {
            let outcome = decode_outcome(
                &storage,
                retcon_browser::BrowserCallResult::value(
                    json!({"status":status,"variants":[{"status":status}],"artifacts":[]}),
                ),
                &HashMap::new(),
            )
            .unwrap();
            assert!(
                outcome
                    .assertions
                    .iter()
                    .any(|value| value.required && value.status == "failed")
            );
        }
    }

    #[tokio::test]
    async fn service_runner_reaches_node_stdio_chromium_and_decodes_first_visual_capture() {
        if std::env::var("RETCON_BROWSER_E2E").as_deref() != Ok("1") {
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let storage = Storage::open(directory.path()).unwrap();
        let service_dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/browser-service");
        let service = Arc::new(NodeBrowserService::new(NodeBrowserServiceConfig {
            node_path: PathBuf::from("node"),
            entrypoint: service_dir.join("src/main.ts"),
            service_dir,
            artifact_root: directory.path().join("browser-artifacts"),
            profile_root: directory.path().join("browser-profiles"),
            input_roots: vec![directory.path().to_path_buf()],
            request_timeout: Duration::from_secs(60),
            protocol_version: BROWSER_SERVICE_PROTOCOL,
        }));
        let runner = Arc::new(ServiceBrowserVerificationRunner::new(
            storage.clone(),
            service.clone(),
        ));
        let (url, fixture) = fixture().await;
        let run_id = Uuid::new_v4();
        let artifact_directory = directory
            .path()
            .join("browser-artifacts/verifications")
            .join(run_id.to_string());
        let outcome = runner
            .run(BrowserVerificationInvocation {
                wire: json!({
                    "runId":run_id,
                    "artifactDirectory":artifact_directory,
                    "timeoutMs":30_000,
                    "definition":{
                        "id":"rust-service-e2e",
                        "targetUrl":url,
                        "serverRef":"fixture",
                        "steps":[
                            {"id":"status","type":"assert.status","expected":200},
                            {"id":"visual","type":"screenshot","name":"visual","compare":true}
                        ],
                        "variants":[{"name":"desktop","width":800,"height":600}],
                        "timeoutMs":15_000,
                        "retries":0,
                        "failOnAccessibility":true,
                        "visual":{"updateBaseline":"never"}
                    }
                }),
                required_assertions: HashMap::from([("status".into(), true)]),
            })
            .await
            .unwrap();
        assert!(
            outcome
                .assertions
                .iter()
                .all(|assertion| assertion.status == "passed")
        );
        assert!(
            outcome
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == "visual-current")
        );
        assert_eq!(outcome.visual_comparisons.len(), 1);
        assert_eq!(outcome.visual_comparisons[0].status, "missing_baseline");
        storage
            .artifacts()
            .verify(&outcome.visual_comparisons[0].current_hash)
            .unwrap();

        let cancelled_run_id = Uuid::new_v4();
        let cancellation = BrowserVerificationInvocation {
            wire: json!({
                "runId":cancelled_run_id,
                "artifactDirectory":directory.path().join("browser-artifacts/verifications").join(cancelled_run_id.to_string()),
                "timeoutMs":30_000,
                "definition":{
                    "id":"rust-cancel-e2e",
                    "targetUrl":url,
                    "serverRef":"fixture",
                    "steps":[{"id":"wait","type":"wait","selector":"#never","timeoutMs":30_000}],
                    "variants":[{"name":"desktop","width":800,"height":600}],
                    "timeoutMs":30_000,
                    "retries":0,
                    "failOnAccessibility":true,
                    "visual":{"updateBaseline":"never"}
                }
            }),
            required_assertions: HashMap::new(),
        };
        let task_runner = runner.clone();
        let task = tokio::spawn(async move { task_runner.run(cancellation).await });
        tokio::time::sleep(Duration::from_millis(500)).await;
        runner.cancel(cancelled_run_id).await.unwrap();
        let cancelled = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(cancelled.contains("cancelled"));
        service.shutdown().await.unwrap();
        fixture.abort();
    }
}
