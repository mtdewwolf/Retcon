//! Authenticated, bounded, privacy-safe diagnostics RPCs.

#![allow(missing_docs)]

use std::cmp::Ordering;

use retcon_diagnostics::{MetricUnit, NewLogRecord, NewMetricSample, Severity};
use retcon_protocol::Request;
use retcon_storage::{DiagnosticLogQuery, NewDiagnosticLog, PerformanceMetricQuery};
use serde_json::{Map, Value, json};
use uuid::Uuid;

use crate::diagnostics::now_ms;
use crate::error::{CoreError, ErrorCode, ErrorSource};
use crate::rpc::Response;
use crate::state::CoreState;

const MAX_INGEST: usize = 64;
const MAX_SUPPORT_BYTES: usize = 4 * 1024 * 1024;

pub async fn handle(state: CoreState, request: Request) -> Response {
    let id = request.id;
    let result = match request.method.as_str() {
        "diagnostics.snapshot" => {
            let limit = request
                .params
                .get("recentErrorLimit")
                .and_then(Value::as_u64)
                .unwrap_or(20);
            if limit > 100 {
                Err(invalid("recentErrorLimit must be at most 100"))
            } else {
                Ok(snapshot(&state, limit as i64).await)
            }
        }
        "diagnostics.logs.list" => logs(&state, &request.params),
        "diagnostics.metrics.query" => metrics(&state, &request.params),
        "diagnostics.logs.ingest" => ingest_logs(&state, &request.params),
        "diagnostics.metrics.ingest" => ingest_metrics(&state, &request.params),
        "diagnostics.privacy.get" => Ok(json!({"privacy": state.diagnostics_service().privacy()})),
        "diagnostics.privacy.update" => update_privacy(&state, &request.params),
        "diagnostics.fields" => Ok(json!({"fields": collected_fields()})),
        "diagnostics.supportBundle.create" | "diagnostics.data.export" => {
            create_bundle(&state, &request.params).await
        }
        "diagnostics.data.delete" => delete_data(&state, &request.params),
        _ => Err(CoreError::new(
            ErrorCode::NotFound,
            ErrorSource::Rpc,
            "Unknown diagnostics method.",
            format!("unsupported method {}", request.method),
        )),
    };
    match result {
        Ok(value) => Response::ok(id, value),
        Err(error) => Response::error(id, &error),
    }
}

pub async fn snapshot(state: &CoreState, recent_error_limit: i64) -> Value {
    let database = state.storage().database();
    let integrity = database.integrity_check().ok();
    let resources = database.diagnostics().resources().unwrap_or_default();
    let recent_errors = database
        .diagnostics()
        .logs(&DiagnosticLogQuery {
            severity: Some("error".into()),
            limit: recent_error_limit.clamp(1, 100),
            ..DiagnosticLogQuery::default()
        })
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "timestamp": entry.timestamp,
                "component": entry.component,
                "code": entry.code,
                "severity": entry.severity,
                "message": entry.message,
            })
        })
        .collect::<Vec<_>>();
    let browser = state.browser_service().diagnostics().ok();
    let artifact_bytes = state.storage().artifacts().disk_usage_async().await.ok();
    let database_bytes = std::fs::metadata(database.path())
        .ok()
        .map(|value| value.len());
    let storage_bytes = database_bytes
        .unwrap_or(0)
        .saturating_add(artifact_bytes.unwrap_or(0));
    let resource_view = json!({
        "processes": [
            {"kind":"core","status":"running","count":1},
            {"kind":"dev_server","status":"active","count":resources.dev_server_processes}
        ],
        "sessions": [
            {"kind":"browser","status":"active","count":resources.browser_sessions},
            {"kind":"terminal","status":"active","count":resources.terminal_sessions},
            {"kind":"agent","status":"active","count":resources.agent_sessions}
        ],
        "ports": resources.ports,
        "counts": resources,
    });
    let browser_view = browser.map(|value| {
        json!({
            "serviceVersion": value.service_version,
            "protocolVersion": value.protocol_version,
            "compatible": value.compatible(),
            "healthy": value.healthy,
            "features": value.features,
        })
    });
    let snapshot = json!({
        "process_id": std::process::id(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "uptime_ms": state.uptime().as_millis(),
        "version": env!("CARGO_PKG_VERSION"),
        "overview": {
            "status": if integrity.as_ref().is_some_and(|value| value.healthy) { "healthy" } else { "degraded" },
            "coreStatus": if integrity.as_ref().is_some_and(|value| value.healthy) { "healthy" } else { "degraded" },
            "storageBytes": storage_bytes,
            "version": env!("CARGO_PKG_VERSION"),
            "protocolVersion": 1,
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "uptimeMs": state.uptime().as_millis(),
        },
        "storage": {
            "database": "$RETCON_DATA/retcon.db",
            "artifact_bytes": artifact_bytes,
            "recovery": state.recovery(),
            "databaseAlias": "$RETCON_DATA/retcon.db",
            "schemaVersion": database.schema_version().ok(),
            "healthy": integrity.as_ref().is_some_and(|value| value.healthy),
            "integrityChecks": integrity.as_ref().map_or(0, |value| value.messages.len()),
        },
        "disk": {"databaseBytes": database_bytes, "artifactBytes": artifact_bytes},
        "resources": resource_view,
        "browserService": browser_view,
        "browser_service": browser_view,
        "safePathAliases": state.diagnostics_service().sanitizer().alias_names(),
        "recentErrors": recent_errors,
        "privacy": state.diagnostics_service().privacy(),
    });
    state.diagnostics_service().sanitizer().value(snapshot)
}

fn logs(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let limit = bounded_limit(params, 50)?;
    let severity = optional_token(params, "severity")?;
    let component = optional_token(params, "component")?;
    let entries = state
        .storage()
        .database()
        .diagnostics()
        .logs(&DiagnosticLogQuery {
            severity,
            component,
            since: None,
            limit,
        })?;
    Ok(json!({"logs": entries.into_iter().map(|entry| json!({
        "id":entry.id,"timestamp":entry.timestamp,"component":entry.component,
        "code":entry.code,"severity":entry.severity,"message":entry.message
    })).collect::<Vec<_>>() }))
}

fn metrics(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let window = params
        .get("window")
        .and_then(Value::as_str)
        .unwrap_or("session");
    let since = match window {
        "session" => 0,
        "hour" => now_ms().saturating_sub(3_600_000),
        "day" => now_ms().saturating_sub(86_400_000),
        _ => return Err(invalid("window must be session, hour, or day")),
    };
    let values = state
        .storage()
        .database()
        .diagnostics()
        .metrics(&PerformanceMetricQuery {
            since: Some(since),
            limit: 500,
            ..PerformanceMetricQuery::default()
        })?;
    let ipc = values
        .iter()
        .filter(|value| value.name == "ipc.duration")
        .map(|value| value.value)
        .collect::<Vec<_>>();
    let frames = values
        .iter()
        .filter(|value| matches!(value.name.as_str(), "ui.frame" | "ui.frame.duration"))
        .map(|value| value.value)
        .collect::<Vec<_>>();
    let mut ui = aggregate(frames.clone());
    if let Some(object) = ui.as_object_mut() {
        object.insert(
            "jankCount".into(),
            json!(frames.iter().filter(|value| **value > 16.67).count()),
        );
    }
    Ok(json!({"performance":{"ipc":aggregate(ipc),"uiFrames":ui}}))
}

fn ingest_logs(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let records = batch(params, "records")?;
    for value in records {
        validate_timestamp(value.get("timestamp"))?;
        let component = required_token(value, "component")?;
        let code = required_token(value, "code")?;
        let severity = parse_severity(value.get("severity"))?;
        let count = value.get("count").and_then(Value::as_u64).unwrap_or(1);
        if !(1..=10_000).contains(&count) {
            return Err(invalid("log count must be between 1 and 10000"));
        }
        state
            .diagnostics_service()
            .ingest_log(NewLogRecord {
                severity,
                component,
                event: code,
                message: "Desktop operational event".into(),
                fields: json!({"count":count}),
            })
            .map_err(diagnostics_error)?;
    }
    Ok(json!({"accepted":records.len()}))
}

fn ingest_metrics(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let records = batch(params, "metrics")?;
    if !state.diagnostics_service().privacy().telemetry_enabled {
        return Ok(json!({"accepted":0,"telemetryEnabled":false}));
    }
    for value in records {
        validate_timestamp(value.get("timestamp"))?;
        let name = required_token(value, "name")?;
        if !matches!(
            name.as_str(),
            "ipc.duration" | "ui.frame" | "ui.frame.duration"
        ) {
            return Err(invalid("metric name is not allowlisted"));
        }
        let metric_value = value
            .get("value")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| invalid("metric value must be a finite non-negative number"))?;
        let mut tags = Map::new();
        if let Some(dimensions) = value.get("dimensions").and_then(Value::as_object) {
            for key in ["method", "outcome", "jank"] {
                if let Some(tag) = dimensions.get(key) {
                    tags.insert(key.into(), tag.clone());
                }
            }
            if dimensions
                .keys()
                .any(|key| !matches!(key.as_str(), "method" | "outcome" | "jank"))
            {
                return Err(invalid("metric dimensions contain a non-allowlisted key"));
            }
        }
        state
            .diagnostics_service()
            .ingest_metric(NewMetricSample {
                component: "desktop".into(),
                name,
                value: metric_value,
                unit: MetricUnit::Milliseconds,
                tags: Value::Object(tags),
            })
            .map_err(diagnostics_error)?;
    }
    Ok(json!({"accepted":records.len()}))
}

fn update_privacy(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let enabled = params
        .get("telemetryEnabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("telemetryEnabled must be a boolean"))?;
    let privacy = state.diagnostics_service().set_privacy(enabled);
    if !enabled || privacy.is_ok() {
        state.configure_runtime_observability();
    }
    let privacy = privacy?;
    Ok(json!({"privacy": privacy}))
}

async fn create_bundle(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    let context = params
        .get("context")
        .and_then(Value::as_str)
        .unwrap_or("diagnostics");
    if !matches!(context, "diagnostics" | "provider_doctor") {
        return Err(invalid("context must be diagnostics or provider_doctor"));
    }
    let mut bundle = json!({
        "formatVersion":1,
        "createdAt":now_ms(),
        "context":context,
        "snapshot":snapshot(state, 20).await,
        "privacy":state.diagnostics_service().privacy(),
        "collectedFields":collected_fields(),
    });
    if flag(params, "includeLogs", true) {
        bundle["logs"] = logs(state, &json!({"limit":500}))?["logs"].clone();
    }
    if flag(params, "includeMetrics", true) {
        bundle["metrics"] = metrics(state, &json!({"window":"session"}))?["performance"].clone();
    }
    if !flag(params, "includeRecentErrors", true) {
        bundle["snapshot"]
            .as_object_mut()
            .map(|value| value.remove("recentErrors"));
    }
    bundle = state.diagnostics_service().sanitizer().value(bundle);
    let encoded = serde_json::to_vec_pretty(&bundle)
        .map_err(|error| internal(format!("encode support bundle: {error}")))?;
    if encoded.len() > MAX_SUPPORT_BYTES {
        return Err(invalid("support bundle exceeded the local size limit"));
    }
    let scan = retcon_secrets::scan_text(&String::from_utf8_lossy(&encoded));
    if !scan.is_clean() {
        return Err(internal(
            "support bundle failed the final secret canary scan",
        ));
    }
    let artifact = state.storage().artifacts().store_bytes(&encoded)?;
    let bundle_id = Uuid::new_v4();
    state.storage().database().diagnostics().record_logs(
        &[NewDiagnosticLog {
            id: bundle_id,
            timestamp: now_ms(),
            session_id: None,
            component: "core".into(),
            code: "support_bundle.created".into(),
            severity: "info".into(),
            message: "Sanitized support bundle created".into(),
            fields: json!({"context":context}),
            artifact_hash: Some(artifact.hash),
        }],
        state.diagnostics_service().privacy().retention_days,
    )?;
    Ok(json!({"bundle":{
        "id":bundle_id,
        "fileName":format!("retcon-support-{bundle_id}.json"),
        "sizeBytes":artifact.size,
        "createdAt":now_ms(),
    }}))
}

fn delete_data(state: &CoreState, params: &Value) -> Result<Value, CoreError> {
    if params.get("scope").and_then(Value::as_str) != Some("diagnostics")
        || params.get("confirmation").and_then(Value::as_str) != Some("delete")
    {
        return Err(invalid(
            "scope diagnostics and confirmation delete are required",
        ));
    }
    let deleted = state.storage().database().diagnostics().delete_all()?;
    let artifacts = deleted
        .artifact_hashes
        .iter()
        .filter(|hash| {
            state
                .storage()
                .artifacts()
                .delete_if_unreferenced(state.storage().database(), hash)
                .unwrap_or(false)
        })
        .count();
    state.emit(
        "diagnostics.data.deleted",
        json!({"scope":"diagnostics","logs":deleted.logs,"metrics":deleted.metrics,"artifacts":artifacts}),
    );
    Ok(json!({"deleted":{"logs":deleted.logs,"metrics":deleted.metrics,"artifacts":artifacts}}))
}

fn collected_fields() -> Vec<Value> {
    vec![
        json!({"name":"system.version","purpose":"compatibility","retention":"snapshot only"}),
        json!({"name":"system.os_arch","purpose":"compatibility","retention":"snapshot only"}),
        json!({"name":"storage.health_sizes","purpose":"local troubleshooting","retention":"snapshot only"}),
        json!({"name":"retcon.resources","purpose":"find owned process and port conflicts","retention":"snapshot only"}),
        json!({"name":"operational.events","purpose":"local troubleshooting without user content","retention":"30 days, bounded to 5000"}),
        json!({"name":"performance.metrics","purpose":"local performance analysis when enabled","retention":"30 days, bounded to 20000"}),
    ]
}

fn aggregate(mut values: Vec<f64>) -> Value {
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let count = values.len();
    json!({
        "count":count,
        "p50Ms":percentile(&values, 0.50),
        "p95Ms":percentile(&values, 0.95),
        "maxMs":values.last().copied().unwrap_or(0.0),
    })
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = (((values.len() - 1) as f64) * percentile).ceil() as usize;
    values[index]
}

fn batch<'a>(params: &'a Value, key: &str) -> Result<&'a Vec<Value>, CoreError> {
    let values = params
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| invalid(format!("{key} must be an array")))?;
    if values.is_empty() || values.len() > MAX_INGEST {
        return Err(invalid(format!(
            "{key} must contain 1 to {MAX_INGEST} records"
        )));
    }
    Ok(values)
}

fn bounded_limit(params: &Value, default: i64) -> Result<i64, CoreError> {
    let limit = params
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(default);
    if (1..=500).contains(&limit) {
        Ok(limit)
    } else {
        Err(invalid("limit must be between 1 and 500"))
    }
}

fn optional_token(params: &Value, key: &str) -> Result<Option<String>, CoreError> {
    params
        .get(key)
        .map(|_| required_token(params, key))
        .transpose()
}

fn required_token(params: &Value, key: &str) -> Result<String, CoreError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        })
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("{key} must be a bounded token")))
}

fn parse_severity(value: Option<&Value>) -> Result<Severity, CoreError> {
    match value.and_then(Value::as_str) {
        Some("debug") => Ok(Severity::Debug),
        Some("info") => Ok(Severity::Info),
        Some("warning") => Ok(Severity::Warning),
        Some("error") => Ok(Severity::Error),
        Some("critical") => Ok(Severity::Critical),
        _ => Err(invalid("severity is invalid")),
    }
}

fn validate_timestamp(value: Option<&Value>) -> Result<(), CoreError> {
    let Some(raw) = value.and_then(Value::as_str) else {
        return Err(invalid("timestamp must be an ISO-8601 string"));
    };
    let parsed = chrono::DateTime::parse_from_rfc3339(raw)
        .map_err(|_| invalid("timestamp must be an ISO-8601 string"))?;
    let timestamp = parsed.timestamp_millis();
    if timestamp < now_ms().saturating_sub(31 * 86_400_000)
        || timestamp > now_ms().saturating_add(300_000)
    {
        return Err(invalid("timestamp is outside the accepted range"));
    }
    Ok(())
}

fn flag(params: &Value, key: &str, default: bool) -> bool {
    params.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn diagnostics_error(error: retcon_diagnostics::DiagnosticsError) -> CoreError {
    invalid(error.to_string())
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidRequest,
        ErrorSource::Rpc,
        "The diagnostics request was invalid.",
        message,
    )
}

fn internal(message: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::Internal,
        ErrorSource::System,
        "Retcon could not complete the diagnostics operation.",
        message,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::io::Read;

    use super::*;
    use retcon_diagnostics::DiagnosticsRecorder;

    fn metric() -> NewMetricSample {
        NewMetricSample {
            component: "core".into(),
            name: "ipc.duration".into(),
            value: 2.0,
            unit: MetricUnit::Milliseconds,
            tags: json!({"operation":"health","outcome":"ok"}),
        }
    }

    #[test]
    fn telemetry_defaults_off_logs_persist_and_metrics_do_not() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        assert!(!state.diagnostics_service().privacy().telemetry_enabled);

        state
            .diagnostics_service()
            .record_log(NewLogRecord {
                severity: Severity::Info,
                component: "core".into(),
                event: "ready".into(),
                message: "Core ready".into(),
                fields: json!({}),
            })
            .unwrap();
        state.diagnostics_service().record_metric(metric()).unwrap();
        assert_eq!(
            state
                .storage()
                .database()
                .diagnostics()
                .logs(&DiagnosticLogQuery {
                    limit: 50,
                    ..DiagnosticLogQuery::default()
                })
                .unwrap()
                .len(),
            1
        );
        assert!(
            state
                .storage()
                .database()
                .diagnostics()
                .metrics(&PerformanceMetricQuery {
                    limit: 50,
                    ..PerformanceMetricQuery::default()
                })
                .unwrap()
                .is_empty()
        );

        state.diagnostics_service().set_privacy(true).unwrap();
        state.diagnostics_service().record_metric(metric()).unwrap();
        state.diagnostics_service().set_privacy(false).unwrap();
        state.diagnostics_service().record_metric(metric()).unwrap();
        assert_eq!(
            state
                .storage()
                .database()
                .diagnostics()
                .metrics(&PerformanceMetricQuery {
                    limit: 50,
                    ..PerformanceMetricQuery::default()
                })
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn support_bundle_and_legacy_snapshot_remove_canary_and_raw_paths() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        let canary = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
        state
            .diagnostics_service()
            .record_log(NewLogRecord {
                severity: Severity::Error,
                component: "core".into(),
                event: "canary.failed".into(),
                message: format!(
                    "failed at {} token={canary}",
                    directory.path().join("private.txt").display()
                ),
                fields: json!({"prompt":"private user prompt","outcome":"error"}),
            })
            .unwrap();

        let response = create_bundle(&state, &json!({"context":"diagnostics"}))
            .await
            .unwrap();
        assert!(response["bundle"]["id"].is_string());
        assert!(response["bundle"].get("hash").is_none());
        assert!(response["bundle"].get("path").is_none());
        let records = state
            .storage()
            .database()
            .diagnostics()
            .logs(&DiagnosticLogQuery {
                component: Some("core".into()),
                limit: 50,
                ..DiagnosticLogQuery::default()
            })
            .unwrap();
        let hash = records
            .iter()
            .find(|record| record.code == "support_bundle.created")
            .and_then(|record| record.artifact_hash.as_deref())
            .unwrap();
        let mut encoded = String::new();
        state
            .storage()
            .artifacts()
            .get(hash)
            .unwrap()
            .read_to_string(&mut encoded)
            .unwrap();
        assert!(!encoded.contains(canary));
        assert!(!encoded.contains("private user prompt"));
        assert!(!encoded.contains(&directory.path().display().to_string()));

        let legacy = serde_json::to_string(&state.diagnostics().await).unwrap();
        assert!(!legacy.contains(&directory.path().display().to_string()));
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            assert!(!legacy.contains(&home.to_string_lossy().to_string()));
        }
        if let Ok(username) = std::env::var("USERNAME") {
            assert!(!username.is_empty());
            assert!(
                !legacy
                    .to_ascii_lowercase()
                    .contains(&username.to_ascii_lowercase())
            );
        }
    }

    #[test]
    fn desktop_wire_records_ingest_and_aggregate_with_canonical_names() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        state.diagnostics_service().set_privacy(true).unwrap();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let logs_result = ingest_logs(
            &state,
            &json!({"records":[{
                "timestamp":timestamp,"component":"desktop","severity":"error",
                "code":"rpc.failed","count":2
            }]}),
        )
        .unwrap();
        assert_eq!(logs_result["accepted"], 1);
        let metrics_result = ingest_metrics(
            &state,
            &json!({"metrics":[
                {"timestamp":timestamp,"name":"ipc.duration","value":8.0,"dimensions":{"method":"core.health","outcome":"ok"}},
                {"timestamp":timestamp,"name":"ui.frame.duration","value":20.0,"dimensions":{"jank":true}}
            ]}),
        )
        .unwrap();
        assert_eq!(metrics_result["accepted"], 2);
        let aggregate = metrics(&state, &json!({"window":"session"})).unwrap();
        assert_eq!(aggregate["performance"]["ipc"]["count"], 1);
        assert_eq!(aggregate["performance"]["uiFrames"]["jankCount"], 1);
    }

    #[test]
    fn delete_requires_exact_confirmation_and_leaves_audit_event() {
        let directory = tempfile::tempdir().unwrap();
        let state = CoreState::new(directory.path()).unwrap();
        state
            .diagnostics_service()
            .record_log(NewLogRecord {
                severity: Severity::Info,
                component: "core".into(),
                event: "ready".into(),
                message: "Core ready".into(),
                fields: json!({}),
            })
            .unwrap();
        assert!(delete_data(&state, &json!({"scope":"diagnostics"})).is_err());
        let result = delete_data(
            &state,
            &json!({"scope":"diagnostics","confirmation":"delete"}),
        )
        .unwrap();
        assert_eq!(result["deleted"]["logs"], 1);
        assert!(
            state
                .events()
                .replay(0, 100)
                .iter()
                .any(|event| event.kind == "diagnostics.data.deleted")
        );
    }
}
