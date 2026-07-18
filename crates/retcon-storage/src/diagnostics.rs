//! Bounded local operational logs, metrics, and diagnostics resources.

#![allow(missing_docs)]

use rusqlite::{Connection, Row, params};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::task_planning::map_validation;
use crate::{Database, Result, StorageError};

pub const MAX_LOCAL_LOGS: i64 = 5_000;
pub const MAX_LOCAL_METRICS: i64 = 20_000;
pub const MAX_QUERY_LIMIT: i64 = 500;
pub const MAX_BATCH: usize = 64;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticLog {
    pub id: Uuid,
    pub timestamp: i64,
    pub component: String,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub fields: Value,
    pub artifact_hash: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewDiagnosticLog {
    pub id: Uuid,
    pub timestamp: i64,
    pub session_id: Option<Uuid>,
    pub component: String,
    pub code: String,
    pub severity: String,
    pub message: String,
    pub fields: Value,
    pub artifact_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMetric {
    pub id: Uuid,
    pub timestamp: i64,
    pub component: String,
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub dimensions: Value,
}

#[derive(Clone, Debug)]
pub struct NewPerformanceMetric {
    pub id: Uuid,
    pub timestamp: i64,
    pub session_id: Option<Uuid>,
    pub component: String,
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub dimensions: Value,
}

#[derive(Clone, Debug, Default)]
pub struct DiagnosticLogQuery {
    pub severity: Option<String>,
    pub component: Option<String>,
    pub since: Option<i64>,
    pub limit: i64,
}

#[derive(Clone, Debug, Default)]
pub struct PerformanceMetricQuery {
    pub component: Option<String>,
    pub name: Option<String>,
    pub since: Option<i64>,
    pub limit: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsResources {
    pub dev_server_processes: i64,
    pub browser_sessions: i64,
    pub terminal_sessions: i64,
    pub agent_sessions: i64,
    pub ports: Vec<DiagnosticsPort>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsPort {
    pub port: i64,
    pub owner_kind: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsDeletion {
    pub logs: u64,
    pub metrics: u64,
    pub artifact_hashes: Vec<String>,
}

pub struct DiagnosticsRepository<'a>(&'a Database);

impl Database {
    #[must_use]
    pub fn diagnostics(&self) -> DiagnosticsRepository<'_> {
        DiagnosticsRepository(self)
    }
}

impl DiagnosticsRepository<'_> {
    pub fn record_logs(&self, values: &[NewDiagnosticLog], retention_days: i64) -> Result<()> {
        if values.is_empty() || values.len() > MAX_BATCH || !(1..=365).contains(&retention_days) {
            return Err(StorageError::Validation(
                "diagnostic log batch is invalid".into(),
            ));
        }
        for value in values {
            validate_log(value)?;
        }
        let cutoff = now_ms().saturating_sub(retention_days.saturating_mul(86_400_000));
        map_validation(self.0.transaction(|tx| {
            for value in values {
                tx.execute(
                    "INSERT INTO diagnostics(id,session_id,severity,source,message,details_json,artifact_hash,created_at,event,fields_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![value.id.as_bytes(),optional_uuid_bytes(value.session_id),value.severity,value.component,value.message,encode(&value.fields)?,value.artifact_hash,value.timestamp,value.code,encode(&value.fields)?],
                )?;
            }
            tx.execute("DELETE FROM diagnostics WHERE created_at<?1", [cutoff])?;
            tx.execute("DELETE FROM diagnostics WHERE id IN (SELECT id FROM diagnostics ORDER BY created_at DESC,id DESC LIMIT -1 OFFSET ?1)",[MAX_LOCAL_LOGS])?;
            Ok(())
        }))
    }

    pub fn record_metrics(
        &self,
        values: &[NewPerformanceMetric],
        retention_days: i64,
    ) -> Result<()> {
        if values.is_empty() || values.len() > MAX_BATCH || !(1..=365).contains(&retention_days) {
            return Err(StorageError::Validation(
                "performance metric batch is invalid".into(),
            ));
        }
        for value in values {
            validate_metric(value)?;
        }
        let cutoff = now_ms().saturating_sub(retention_days.saturating_mul(86_400_000));
        map_validation(self.0.transaction(|tx| {
            for value in values {
                tx.execute(
                    "INSERT INTO diagnostic_metrics(id,session_id,recorded_at,component,name,value,unit,dimensions_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                    params![value.id.as_bytes(),optional_uuid_bytes(value.session_id),value.timestamp,value.component,value.name,value.value,value.unit,encode(&value.dimensions)?],
                )?;
            }
            tx.execute("DELETE FROM diagnostic_metrics WHERE recorded_at<?1", [cutoff])?;
            tx.execute("DELETE FROM diagnostic_metrics WHERE id IN (SELECT id FROM diagnostic_metrics ORDER BY recorded_at DESC,id DESC LIMIT -1 OFFSET ?1)",[MAX_LOCAL_METRICS])?;
            Ok(())
        }))
    }

    pub fn logs(&self, query: &DiagnosticLogQuery) -> Result<Vec<DiagnosticLog>> {
        validate_query(
            query.limit,
            query.severity.as_deref(),
            query.component.as_deref(),
        )?;
        self.0.read(|db| {
            let mut statement=db.prepare("SELECT id,created_at,source,event,severity,message,fields_json,artifact_hash FROM diagnostics WHERE (?1 IS NULL OR severity=?1) AND (?2 IS NULL OR source=?2) AND created_at>=?3 ORDER BY created_at DESC,id DESC LIMIT ?4")?;
            statement.query_map(params![query.severity,query.component,query.since.unwrap_or(0),query.limit],row_log)?.collect()
        })
    }

    pub fn metrics(&self, query: &PerformanceMetricQuery) -> Result<Vec<PerformanceMetric>> {
        validate_query(
            query.limit,
            query.name.as_deref(),
            query.component.as_deref(),
        )?;
        self.0.read(|db| {
            let mut statement=db.prepare("SELECT id,recorded_at,component,name,value,unit,dimensions_json FROM diagnostic_metrics WHERE (?1 IS NULL OR component=?1) AND (?2 IS NULL OR name=?2) AND recorded_at>=?3 ORDER BY recorded_at DESC,id DESC LIMIT ?4")?;
            statement.query_map(params![query.component,query.name,query.since.unwrap_or(0),query.limit],row_metric)?.collect()
        })
    }

    pub fn resources(&self) -> Result<DiagnosticsResources> {
        self.0.read(|db| {
            let mut ports=db.prepare("SELECT port,'dev_server',status FROM dev_server_port_leases WHERE status IN ('active','stale') ORDER BY port LIMIT 256")?;
            Ok(DiagnosticsResources {
                dev_server_processes: count(db,"SELECT count(*) FROM dev_server_instances WHERE status IN ('starting','running','stopping')")?,
                browser_sessions: count(db,"SELECT count(*) FROM browser_sessions WHERE status='running'")?,
                terminal_sessions: count(db,"SELECT count(*) FROM terminal_sessions WHERE status IN ('starting','running')")?,
                agent_sessions: count(db,"SELECT count(*) FROM sessions WHERE status IN ('starting','running','active')")?,
                ports: ports.query_map([],|row|Ok(DiagnosticsPort{port:row.get(0)?,owner_kind:row.get(1)?,status:row.get(2)?}))?.collect::<rusqlite::Result<_>>()?,
            })
        })
    }

    pub fn delete_all(&self) -> Result<DiagnosticsDeletion> {
        self.0.transaction(|tx| {
            let artifact_hashes = tx
                .prepare("SELECT artifact_hash FROM diagnostics WHERE artifact_hash IS NOT NULL")?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let logs = tx.execute("DELETE FROM diagnostics", [])? as u64;
            let metrics = tx.execute("DELETE FROM diagnostic_metrics", [])? as u64;
            Ok(DiagnosticsDeletion {
                logs,
                metrics,
                artifact_hashes,
            })
        })
    }
}

fn validate_log(value: &NewDiagnosticLog) -> Result<()> {
    if !matches!(
        value.severity.as_str(),
        "debug" | "info" | "warning" | "error" | "critical"
    ) || !valid_name(&value.component)
        || !valid_name(&value.code)
        || value.message.is_empty()
        || value.message.len() > 4096
        || !value.fields.is_object()
        || encoded_len(&value.fields) > 16_384
        || value.timestamp < 0
        || value
            .artifact_hash
            .as_ref()
            .is_some_and(|hash| !valid_hash(hash))
    {
        return Err(StorageError::Validation("diagnostic log is invalid".into()));
    }
    Ok(())
}

fn validate_metric(value: &NewPerformanceMetric) -> Result<()> {
    if !valid_name(&value.component)
        || !valid_name(&value.name)
        || !value.value.is_finite()
        || !matches!(
            value.unit.as_str(),
            "milliseconds" | "count" | "bytes" | "ratio"
        )
        || (value.unit == "ratio" && !(0.0..=1.0).contains(&value.value))
        || !value.dimensions.is_object()
        || encoded_len(&value.dimensions) > 16_384
        || value.timestamp < 0
    {
        return Err(StorageError::Validation(
            "performance metric is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_query(limit: i64, first: Option<&str>, second: Option<&str>) -> Result<()> {
    if !(1..=MAX_QUERY_LIMIT).contains(&limit)
        || first.is_some_and(|value| !valid_name(value))
        || second.is_some_and(|value| !valid_name(value))
    {
        Err(StorageError::Validation(
            "diagnostics query is invalid".into(),
        ))
    } else {
        Ok(())
    }
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
fn encode(value: &Value) -> rusqlite::Result<String> {
    serde_json::to_string(value)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}
fn encoded_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |encoded| encoded.len())
}
fn decode(row: &Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    let raw: Vec<u8> = row.get(index)?;
    Uuid::from_slice(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Blob,
            Box::new(error),
        )
    })
}
fn row_log(row: &Row<'_>) -> rusqlite::Result<DiagnosticLog> {
    Ok(DiagnosticLog {
        id: uuid(row, 0)?,
        timestamp: row.get(1)?,
        component: row.get(2)?,
        code: row.get(3)?,
        severity: row.get(4)?,
        message: row.get(5)?,
        fields: decode(row, 6)?,
        artifact_hash: row.get(7)?,
    })
}
fn row_metric(row: &Row<'_>) -> rusqlite::Result<PerformanceMetric> {
    Ok(PerformanceMetric {
        id: uuid(row, 0)?,
        timestamp: row.get(1)?,
        component: row.get(2)?,
        name: row.get(3)?,
        value: row.get(4)?,
        unit: row.get(5)?,
        dimensions: decode(row, 6)?,
    })
}
fn optional_uuid_bytes(value: Option<Uuid>) -> Option<Vec<u8>> {
    value.map(|value| value.as_bytes().to_vec())
}
fn count(db: &Connection, sql: &str) -> rusqlite::Result<i64> {
    db.query_row(sql, [], |row| row.get(0))
}
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bounded_logs_metrics_queries_and_delete_round_trip() {
        let db = Database::open_in_memory().unwrap();
        let now = now_ms();
        db.diagnostics()
            .record_logs(
                &[NewDiagnosticLog {
                    id: Uuid::new_v4(),
                    timestamp: now,
                    session_id: None,
                    component: "core".into(),
                    code: "request.failed".into(),
                    severity: "error".into(),
                    message: "safe".into(),
                    fields: json!({"outcome":"error"}),
                    artifact_hash: None,
                }],
                30,
            )
            .unwrap();
        db.diagnostics()
            .record_metrics(
                &[NewPerformanceMetric {
                    id: Uuid::new_v4(),
                    timestamp: now,
                    session_id: None,
                    component: "desktop".into(),
                    name: "ipc.duration".into(),
                    value: 12.5,
                    unit: "milliseconds".into(),
                    dimensions: json!({"operation":"health"}),
                }],
                30,
            )
            .unwrap();
        assert_eq!(
            db.diagnostics()
                .logs(&DiagnosticLogQuery {
                    severity: Some("error".into()),
                    limit: 50,
                    ..Default::default()
                })
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            db.diagnostics()
                .metrics(&PerformanceMetricQuery {
                    name: Some("ipc.duration".into()),
                    limit: 50,
                    ..Default::default()
                })
                .unwrap()
                .len(),
            1
        );
        let deleted = db.diagnostics().delete_all().unwrap();
        assert_eq!((deleted.logs, deleted.metrics), (1, 1));
    }

    #[test]
    fn rejects_unbounded_batches_and_invalid_values() {
        let db = Database::open_in_memory().unwrap();
        let value = NewPerformanceMetric {
            id: Uuid::new_v4(),
            timestamp: 0,
            session_id: None,
            component: "core".into(),
            name: "ratio".into(),
            value: 2.0,
            unit: "ratio".into(),
            dimensions: json!({}),
        };
        assert!(db.diagnostics().record_metrics(&[value], 30).is_err());
        assert!(
            db.diagnostics()
                .logs(&DiagnosticLogQuery {
                    limit: 501,
                    ..Default::default()
                })
                .is_err()
        );
    }

    #[test]
    fn diagnostics_retention_and_delete_preserve_usage_history() {
        let db = Database::open_in_memory().unwrap();
        let usage_id = Uuid::new_v4();
        db.transaction(|tx| {
            tx.execute(
                "INSERT INTO usage_records(id,metric,quantity,unit,recorded_at,metadata_json) VALUES (?1,'tokens',42.0,'count',0,'{}')",
                [usage_id.as_bytes()],
            )?;
            Ok(())
        })
        .unwrap();

        db.diagnostics()
            .record_metrics(
                &[NewPerformanceMetric {
                    id: Uuid::new_v4(),
                    timestamp: now_ms(),
                    session_id: None,
                    component: "core".into(),
                    name: "ipc.duration".into(),
                    value: 1.0,
                    unit: "milliseconds".into(),
                    dimensions: json!({"outcome":"ok"}),
                }],
                1,
            )
            .unwrap();
        db.diagnostics().delete_all().unwrap();

        let remaining = db
            .read(|connection| {
                connection.query_row(
                    "SELECT count(*) FROM usage_records WHERE id=?1",
                    [usage_id.as_bytes()],
                    |row| row.get::<_, i64>(0),
                )
            })
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn schema_rejects_invalid_diagnostics_shapes() {
        let db = Database::open_in_memory().unwrap();
        assert!(db.transaction(|tx| {
            tx.execute(
                "INSERT INTO diagnostic_metrics(id,recorded_at,component,name,value,unit,dimensions_json) VALUES (?1,0,'core','ipc.duration',1.0,'milliseconds','[]')",
                [Uuid::new_v4().as_bytes()],
            )?;
            Ok(())
        }).is_err());
        assert!(db.transaction(|tx| {
            tx.execute(
                "INSERT INTO diagnostics(id,severity,source,message,created_at,event,fields_json) VALUES (?1,'info','core','safe',0,'ready','[]')",
                [Uuid::new_v4().as_bytes()],
            )?;
            Ok(())
        }).is_err());
    }
}
