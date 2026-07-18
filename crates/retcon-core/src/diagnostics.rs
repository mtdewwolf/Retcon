//! Privacy-safe diagnostics recording shared by Core and runtime components.

#![allow(missing_docs)]

use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use retcon_diagnostics::{
    DiagnosticsError, DiagnosticsRecorder, NewLogRecord, NewMetricSample, Sanitizer,
    SharedDiagnosticsRecorder, validate_log, validate_metric,
};
use retcon_storage::{NewDiagnosticLog, NewPerformanceMetric, Storage};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::CoreError;

const PRIVACY_SCOPE: &str = "global";
const PRIVACY_KEY: &str = "diagnostics.privacy";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsPrivacy {
    #[serde(default)]
    pub telemetry_enabled: bool,
    #[serde(default = "default_retention_days")]
    pub retention_days: i64,
}

impl Default for DiagnosticsPrivacy {
    fn default() -> Self {
        Self {
            telemetry_enabled: false,
            retention_days: default_retention_days(),
        }
    }
}

pub struct DiagnosticsService {
    storage: Storage,
    sanitizer: Sanitizer,
    privacy: RwLock<DiagnosticsPrivacy>,
}

impl DiagnosticsService {
    pub fn open(storage: Storage) -> Result<Arc<Self>, CoreError> {
        let privacy = storage
            .database()
            .settings()
            .get(PRIVACY_SCOPE, PRIVACY_KEY)?
            .and_then(|setting| serde_json::from_value(setting.value).ok())
            .filter(valid_privacy)
            .unwrap_or_default();
        let sanitizer = Sanitizer::discover().with_alias(storage.data_dir(), "$RETCON_DATA");
        Ok(Arc::new(Self {
            storage,
            sanitizer,
            privacy: RwLock::new(privacy),
        }))
    }

    #[must_use]
    pub fn recorder(self: &Arc<Self>) -> SharedDiagnosticsRecorder {
        self.clone()
    }

    #[must_use]
    pub fn sanitizer(&self) -> &Sanitizer {
        &self.sanitizer
    }

    #[must_use]
    pub fn privacy(&self) -> DiagnosticsPrivacy {
        self.privacy
            .read()
            .map_or_else(|_| DiagnosticsPrivacy::default(), |value| *value)
    }

    pub fn set_privacy(&self, telemetry_enabled: bool) -> Result<DiagnosticsPrivacy, CoreError> {
        let mut privacy = self.privacy();
        privacy.telemetry_enabled = telemetry_enabled;
        // Disabling is immediate and fail-closed even when persistence fails.
        if !telemetry_enabled && let Ok(mut current) = self.privacy.write() {
            current.telemetry_enabled = false;
        }
        self.storage.database().settings().set(
            PRIVACY_SCOPE,
            PRIVACY_KEY,
            &serde_json::to_value(privacy).unwrap_or_else(|_| {
                json!({
                    "telemetryEnabled": false,
                    "retentionDays": default_retention_days()
                })
            }),
        )?;
        // Enabling happens only after the persisted privacy choice succeeds.
        if let Ok(mut current) = self.privacy.write() {
            *current = privacy;
        }
        Ok(privacy)
    }

    pub fn ingest_log(&self, record: NewLogRecord) -> Result<(), DiagnosticsError> {
        validate_log(&record)?;
        let fields = self.sanitizer.value(record.fields);
        let message = self.sanitizer.text(&record.message);
        self.storage
            .database()
            .diagnostics()
            .record_logs(
                &[NewDiagnosticLog {
                    id: Uuid::new_v4(),
                    timestamp: now_ms(),
                    session_id: None,
                    component: record.component,
                    code: record.event,
                    severity: record.severity.as_str().to_owned(),
                    message,
                    fields,
                    artifact_hash: None,
                }],
                self.privacy().retention_days,
            )
            .map_err(|error| DiagnosticsError::Persistence(error.to_string()))
    }

    pub fn ingest_metric(&self, metric: NewMetricSample) -> Result<(), DiagnosticsError> {
        if !self.privacy().telemetry_enabled {
            return Ok(());
        }
        validate_metric(&metric)?;
        let dimensions = self.sanitizer.value(metric.tags);
        self.storage
            .database()
            .diagnostics()
            .record_metrics(
                &[NewPerformanceMetric {
                    id: Uuid::new_v4(),
                    timestamp: now_ms(),
                    session_id: None,
                    component: metric.component,
                    name: metric.name,
                    value: metric.value,
                    unit: metric.unit.as_str().to_owned(),
                    dimensions,
                }],
                self.privacy().retention_days,
            )
            .map_err(|error| DiagnosticsError::Persistence(error.to_string()))
    }
}

impl DiagnosticsRecorder for DiagnosticsService {
    fn record_log(&self, record: NewLogRecord) -> Result<(), DiagnosticsError> {
        self.ingest_log(record)
    }

    fn record_metric(&self, metric: NewMetricSample) -> Result<(), DiagnosticsError> {
        self.ingest_metric(metric)
    }
}

fn valid_privacy(value: &DiagnosticsPrivacy) -> bool {
    (1..=365).contains(&value.retention_days)
}

const fn default_retention_days() -> i64 {
    30
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}
