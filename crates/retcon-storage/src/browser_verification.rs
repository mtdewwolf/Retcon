//! Durable browser-verification definitions, runs, evidence, review, and baselines.

#![allow(missing_docs)]

use std::collections::HashSet;

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::repositories::now_ms;
use crate::task_planning::map_validation;
use crate::{Database, Result, StorageError};

const VALIDATION_PREFIX: &str = "retcon_validation:";
const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_COLLECTION: usize = 1_000;

#[derive(Clone, Debug)]
pub struct NewBrowserVerificationVariant {
    pub id: Uuid,
    pub key: String,
    pub width: i64,
    pub height: i64,
    pub device_scale: f64,
    pub device_name: Option<String>,
    pub sort_order: i64,
}

#[derive(Clone, Debug)]
pub struct NewBrowserVerificationDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub task_id: Option<Uuid>,
    pub dev_server_config_id: Option<Uuid>,
    pub name: String,
    pub target_url: String,
    pub steps: Value,
    pub assertions: Value,
    pub visual_policy: Value,
    pub fail_on_accessibility: bool,
    pub variants: Vec<NewBrowserVerificationVariant>,
    pub status: String,
    pub required: bool,
    pub timeout_ms: i64,
    pub max_retries: i64,
}

impl NewBrowserVerificationDefinition {
    #[must_use]
    pub fn new(project_id: Uuid, name: impl Into<String>, target_url: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            task_id: None,
            dev_server_config_id: None,
            name: name.into(),
            target_url: target_url.into(),
            steps: json!([{"id":"visual","kind":"screenshot","compare":true,"value":"visual"}]),
            assertions: json!([]),
            visual_policy: json!({"pixelThreshold":0.01,"maxDiffPixelRatio":0.01,"perceptualThreshold":0.01,"dynamicRegions":[]}),
            fail_on_accessibility: true,
            variants: vec![NewBrowserVerificationVariant {
                id: Uuid::new_v4(),
                key: "desktop".into(),
                width: 1280,
                height: 720,
                device_scale: 1.0,
                device_name: None,
                sort_order: 0,
            }],
            status: "active".into(),
            required: true,
            timeout_ms: 60_000,
            max_retries: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationVariant {
    pub id: Uuid,
    pub definition_id: Uuid,
    pub key: String,
    pub width: i64,
    pub height: i64,
    pub device_scale: f64,
    pub device_name: Option<String>,
    pub sort_order: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationDefinition {
    pub id: Uuid,
    pub project_id: Uuid,
    pub task_id: Option<Uuid>,
    pub dev_server_config_id: Option<Uuid>,
    pub name: String,
    pub target_url: String,
    pub steps: Value,
    pub assertions: Value,
    pub visual_policy: Value,
    pub fail_on_accessibility: bool,
    pub variants: Vec<BrowserVerificationVariant>,
    pub status: String,
    pub required: bool,
    pub timeout_ms: i64,
    pub max_retries: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewBrowserVerificationRun {
    pub id: Uuid,
    pub definition_id: Uuid,
    pub task_id: Uuid,
    pub dev_server_instance_id: Option<Uuid>,
    pub browser_session_id: Option<Uuid>,
    pub idempotency_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationRun {
    pub id: Uuid,
    pub definition_id: Uuid,
    pub project_id: Uuid,
    pub task_id: Uuid,
    pub dev_server_instance_id: Option<Uuid>,
    pub browser_session_id: Option<Uuid>,
    pub status: String,
    pub attempt: i64,
    pub max_attempts: i64,
    pub timeout_ms: i64,
    pub idempotency_key: Option<String>,
    pub runner_version: Option<String>,
    pub blocking_failures: i64,
    pub critical_accessibility: i64,
    pub warning_count: i64,
    pub visual_differences: i64,
    pub console_errors: i64,
    pub summary: Value,
    pub review: Value,
    pub failure: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationEvent {
    pub id: i64,
    pub run_id: Uuid,
    pub sequence: i64,
    pub kind: String,
    pub severity: String,
    pub actor: String,
    pub payload: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewBrowserVerificationEvent {
    pub kind: String,
    pub severity: String,
    pub payload: Value,
}

#[derive(Clone, Debug)]
pub struct NewBrowserAssertionResult {
    pub name: String,
    pub required: bool,
    pub status: String,
    pub message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewBrowserVerificationArtifact {
    pub id: Uuid,
    pub kind: String,
    pub hash: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationArtifact {
    pub id: Uuid,
    pub run_id: Uuid,
    pub kind: String,
    pub hash: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub metadata: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewVisualComparison {
    pub id: Uuid,
    pub baseline_id: Option<Uuid>,
    pub variant_key: String,
    pub current_hash: String,
    pub difference_hash: Option<String>,
    pub pixel_difference_ratio: f64,
    pub perceptual_difference_ratio: Option<f64>,
    pub threshold_ratio: f64,
    pub status: String,
    pub ignore_regions: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVisualComparison {
    pub id: Uuid,
    pub run_id: Uuid,
    pub baseline_id: Option<Uuid>,
    pub variant_key: String,
    pub current_hash: String,
    pub difference_hash: Option<String>,
    pub pixel_difference_ratio: f64,
    pub perceptual_difference_ratio: Option<f64>,
    pub threshold_ratio: f64,
    pub status: String,
    pub ignore_regions: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewConsoleEvidence {
    pub level: String,
    pub message: String,
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserConsoleEvidence {
    pub id: i64,
    pub run_id: Uuid,
    pub sequence: i64,
    pub level: String,
    pub message: String,
    pub source: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewNetworkEvidence {
    pub method: String,
    pub url: String,
    pub status_code: Option<i64>,
    pub failure: Option<String>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserNetworkEvidence {
    pub id: i64,
    pub run_id: Uuid,
    pub sequence: i64,
    pub method: String,
    pub url: String,
    pub status_code: Option<i64>,
    pub failure: Option<String>,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewAccessibilityFinding {
    pub id: Uuid,
    pub rule_id: String,
    pub severity: String,
    pub message: String,
    pub selector: Option<String>,
    pub help_url: Option<String>,
    pub artifact_hash: Option<String>,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserAccessibilityFinding {
    pub id: Uuid,
    pub run_id: Uuid,
    pub rule_id: String,
    pub severity: String,
    pub message: String,
    pub selector: Option<String>,
    pub help_url: Option<String>,
    pub status: String,
    pub artifact_hash: Option<String>,
    pub metadata: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct BrowserVerificationOutcome {
    pub runner_version: String,
    pub summary: Value,
    pub timeline: Vec<NewBrowserVerificationEvent>,
    pub assertions: Vec<NewBrowserAssertionResult>,
    pub artifacts: Vec<NewBrowserVerificationArtifact>,
    pub visual_comparisons: Vec<NewVisualComparison>,
    pub console: Vec<NewConsoleEvidence>,
    pub network: Vec<NewNetworkEvidence>,
    pub accessibility: Vec<NewAccessibilityFinding>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationDetails {
    pub run: BrowserVerificationRun,
    pub definition: BrowserVerificationDefinition,
    pub events: Vec<BrowserVerificationEvent>,
    pub artifacts: Vec<BrowserVerificationArtifact>,
    pub visual_comparisons: Vec<BrowserVisualComparison>,
    pub console: Vec<BrowserConsoleEvidence>,
    pub network: Vec<BrowserNetworkEvidence>,
    pub accessibility: Vec<BrowserAccessibilityFinding>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationBaseline {
    pub id: Uuid,
    pub definition_id: Uuid,
    pub variant_key: String,
    pub artifact_hash: String,
    pub source_run_id: Option<Uuid>,
    pub status: String,
    pub approved_by: String,
    pub approved_at: i64,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVerificationMutation<T> {
    pub task_id: Uuid,
    pub reopened: bool,
    pub value: T,
}

pub struct BrowserVerificationRepository<'a>(&'a Database);

impl Database {
    #[must_use]
    pub fn browser_verification(&self) -> BrowserVerificationRepository<'_> {
        BrowserVerificationRepository(self)
    }
}

impl BrowserVerificationRepository<'_> {
    pub fn save_definition(
        &self,
        input: &NewBrowserVerificationDefinition,
        actor: &str,
    ) -> Result<BrowserVerificationDefinition> {
        validate_definition(input, actor)?;
        let steps = encode_json(&input.steps, "browser verification steps")?;
        let assertions = encode_json(&input.assertions, "browser verification assertions")?;
        let visual_policy =
            encode_json(&input.visual_policy, "browser verification visual policy")?;
        map_validation(self.0.transaction(|tx| {
            require_project(tx, input.project_id)?;
            validate_definition_scope(tx, input)?;
            let now = now_ms();
            let changed = tx.execute(
                "INSERT INTO browser_verification_definitions(id,project_id,task_id,dev_server_config_id,name,target_url,steps_json,assertions_json,visual_policy_json,fail_on_accessibility,status,is_required,timeout_ms,max_retries,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15) ON CONFLICT(id) DO UPDATE SET task_id=excluded.task_id,dev_server_config_id=excluded.dev_server_config_id,name=excluded.name,target_url=excluded.target_url,steps_json=excluded.steps_json,assertions_json=excluded.assertions_json,visual_policy_json=excluded.visual_policy_json,fail_on_accessibility=excluded.fail_on_accessibility,status=excluded.status,is_required=excluded.is_required,timeout_ms=excluded.timeout_ms,max_retries=excluded.max_retries,updated_at=excluded.updated_at WHERE project_id=excluded.project_id",
                params![input.id.as_bytes(),input.project_id.as_bytes(),optional_uuid_bytes(input.task_id),optional_uuid_bytes(input.dev_server_config_id),input.name.trim(),input.target_url.trim(),steps,assertions,visual_policy,input.fail_on_accessibility,input.status,input.required,input.timeout_ms,input.max_retries,now],
            )?;
            if changed == 0 { return Err(validation_error("browser verification definition belongs to a different project")); }
            tx.execute("DELETE FROM browser_verification_variants WHERE definition_id=?1", [input.id.as_bytes()])?;
            for variant in &input.variants {
                tx.execute("INSERT INTO browser_verification_variants(id,definition_id,variant_key,width,height,device_scale,device_name,sort_order) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![variant.id.as_bytes(),input.id.as_bytes(),variant.key.trim(),variant.width,variant.height,variant.device_scale,variant.device_name,variant.sort_order])?;
            }
            Ok(())
        }))?;
        self.definition(input.id)?.ok_or_else(|| {
            StorageError::Validation("saved browser verification definition disappeared".into())
        })
    }

    pub fn definition(&self, id: Uuid) -> Result<Option<BrowserVerificationDefinition>> {
        let Some(mut definition) = self.0.read(|db| db.query_row("SELECT id,project_id,task_id,dev_server_config_id,name,target_url,steps_json,assertions_json,visual_policy_json,fail_on_accessibility,status,is_required,timeout_ms,max_retries,created_at,updated_at FROM browser_verification_definitions WHERE id=?1",[id.as_bytes()],row_definition).optional())? else { return Ok(None); };
        definition.variants = self.variants(id)?;
        Ok(Some(definition))
    }

    pub fn definitions(&self, project_id: Uuid) -> Result<Vec<BrowserVerificationDefinition>> {
        let ids: Vec<Uuid> = self.0.read(|db| { let mut statement=db.prepare("SELECT id FROM browser_verification_definitions WHERE project_id=?1 ORDER BY name,id")?; statement.query_map([project_id.as_bytes()],|row|uuid(row,0))?.collect() })?;
        ids.into_iter()
            .map(|id| {
                self.definition(id)?.ok_or_else(|| {
                    StorageError::Validation("browser verification definition disappeared".into())
                })
            })
            .collect()
    }

    pub fn queue_run(
        &self,
        input: &NewBrowserVerificationRun,
        actor: &str,
    ) -> Result<BrowserVerificationMutation<BrowserVerificationDetails>> {
        validate_actor(actor)?;
        if input.idempotency_key.as_ref().is_some_and(|key| {
            key.is_empty() || key.len() > 128 || key.chars().any(char::is_control)
        }) {
            return Err(StorageError::Validation(
                "browser verification idempotency key is invalid".into(),
            ));
        }
        if let Some(key)=&input.idempotency_key
            && let Some(existing)=self.0.read(|db|db.query_row("SELECT id FROM browser_verification_runs WHERE definition_id=?1 AND task_id=?2 AND idempotency_key=?3",params![input.definition_id.as_bytes(),input.task_id.as_bytes(),key],|row|uuid(row,0)).optional())? {
            let value=self.get(existing)?.ok_or_else(||StorageError::Validation("idempotent browser verification run disappeared".into()))?;
            return Ok(BrowserVerificationMutation{task_id:input.task_id,reopened:false,value});
        }
        let reopened=map_validation(self.0.transaction(|tx|{
            let (project_id,definition_task,server_config,status,required,timeout,retries)=tx.query_row("SELECT project_id,task_id,dev_server_config_id,status,is_required,timeout_ms,max_retries FROM browser_verification_definitions WHERE id=?1",[input.definition_id.as_bytes()],|row|Ok((uuid(row,0)?,optional_uuid(row,1)?,optional_uuid(row,2)?,row.get::<_,String>(3)?,row.get::<_,bool>(4)?,row.get::<_,i64>(5)?,row.get::<_,i64>(6)?))).optional()?.ok_or_else(||validation_error("browser verification definition does not exist"))?;
            if status!="active" { return Err(validation_error("browser verification definition is not active")); }
            let task_project=tx.query_row("SELECT project_id FROM tasks WHERE id=?1",[input.task_id.as_bytes()],|row|optional_uuid(row,0)).optional()?.ok_or_else(||validation_error("browser verification task does not exist"))?.ok_or_else(||validation_error("browser verification task has no project"))?;
            if task_project!=project_id || definition_task.is_some_and(|task|task!=input.task_id) { return Err(validation_error("browser verification task is outside the definition scope")); }
            validate_run_owners(tx,project_id,input.task_id,server_config,input.dev_server_instance_id,input.browser_session_id)?;
            let now=now_ms();
            tx.execute("INSERT INTO browser_verification_runs(id,definition_id,project_id,task_id,dev_server_instance_id,browser_session_id,status,attempt,max_attempts,timeout_ms,idempotency_key,summary_json,review_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,'queued',1,?7,?8,?9,'{}','{}',?10,?10)",params![input.id.as_bytes(),input.definition_id.as_bytes(),project_id.as_bytes(),input.task_id.as_bytes(),optional_uuid_bytes(input.dev_server_instance_id),optional_uuid_bytes(input.browser_session_id),retries+1,timeout,input.idempotency_key,now])?;
            append_event(tx,input.id,"queued","info",actor,&json!({"definitionId":input.definition_id,"attempt":1,"maxAttempts":retries+1}))?;
            if required { reopen_terminal(tx,input.task_id) } else { Ok(false) }
        }))?;
        let value = self.get(input.id)?.ok_or_else(|| {
            StorageError::Validation("created browser verification run disappeared".into())
        })?;
        Ok(BrowserVerificationMutation {
            task_id: input.task_id,
            reopened,
            value,
        })
    }

    pub fn start(&self, id: Uuid, actor: &str) -> Result<Option<BrowserVerificationDetails>> {
        validate_actor(actor)?;
        let changed=map_validation(self.0.transaction(|tx|{let Some(status)=tx.query_row("SELECT status FROM browser_verification_runs WHERE id=?1",[id.as_bytes()],|row|row.get::<_,String>(0)).optional()? else{return Ok(false)};if status!="queued"{return Err(validation_error("only queued browser verification runs can start"));}let now=now_ms();tx.execute("UPDATE browser_verification_runs SET status='running',started_at=?2,updated_at=?2 WHERE id=?1",params![id.as_bytes(),now])?;append_event(tx,id,"started","info",actor,&json!({}))?;Ok(true)}))?;
        if changed { self.get(id) } else { Ok(None) }
    }

    pub fn cancel(
        &self,
        id: Uuid,
        actor: &str,
    ) -> Result<Option<BrowserVerificationMutation<BrowserVerificationDetails>>> {
        validate_actor(actor)?;
        let outcome=map_validation(self.0.transaction(|tx|{let Some((task,status))=tx.query_row("SELECT task_id,status FROM browser_verification_runs WHERE id=?1",[id.as_bytes()],|row|Ok((uuid(row,0)?,row.get::<_,String>(1)?))).optional()? else{return Ok(None)};if !matches!(status.as_str(),"queued"|"running"){return Err(validation_error("only queued or running browser verification runs can be cancelled"));}let now=now_ms();tx.execute("UPDATE browser_verification_runs SET status='cancelled',completed_at=?2,updated_at=?2 WHERE id=?1",params![id.as_bytes(),now])?;append_event(tx,id,"cancelled","warning",actor,&json!({}))?;let reopened=reopen_terminal(tx,task)?;Ok(Some((task,reopened)))}))?;
        let Some((task_id, reopened)) = outcome else {
            return Ok(None);
        };
        Ok(self.get(id)?.map(|value| BrowserVerificationMutation {
            task_id,
            reopened,
            value,
        }))
    }

    pub fn fail(
        &self,
        id: Uuid,
        failure: &str,
        actor: &str,
    ) -> Result<Option<BrowserVerificationMutation<BrowserVerificationDetails>>> {
        validate_actor(actor)?;
        if failure.trim().is_empty() || failure.len() > 4096 {
            return Err(StorageError::Validation(
                "browser verification failure is invalid".into(),
            ));
        }
        let outcome=map_validation(self.0.transaction(|tx|{let Some((task,status))=tx.query_row("SELECT task_id,status FROM browser_verification_runs WHERE id=?1",[id.as_bytes()],|row|Ok((uuid(row,0)?,row.get::<_,String>(1)?))).optional()? else{return Ok(None)};if !matches!(status.as_str(),"queued"|"running"){return Err(validation_error("browser verification run is already terminal"));}let now=now_ms();tx.execute("UPDATE browser_verification_runs SET status='error',failure=?2,completed_at=?3,updated_at=?3 WHERE id=?1",params![id.as_bytes(),failure,now])?;append_event(tx,id,"error","error",actor,&json!({"failure":failure}))?;let reopened=reopen_terminal(tx,task)?;Ok(Some((task,reopened)))}))?;
        let Some((task_id, reopened)) = outcome else {
            return Ok(None);
        };
        Ok(self.get(id)?.map(|value| BrowserVerificationMutation {
            task_id,
            reopened,
            value,
        }))
    }

    pub fn complete(
        &self,
        id: Uuid,
        outcome: &BrowserVerificationOutcome,
        actor: &str,
    ) -> Result<Option<BrowserVerificationMutation<BrowserVerificationDetails>>> {
        validate_outcome(outcome, actor)?;
        let summary = encode_json(&outcome.summary, "browser verification summary")?;
        let result=map_validation(self.0.transaction(|tx|{
            let Some((task,status))=tx.query_row("SELECT task_id,status FROM browser_verification_runs WHERE id=?1",[id.as_bytes()],|row|Ok((uuid(row,0)?,row.get::<_,String>(1)?))).optional()? else{return Ok(None)};
            if status!="running"{return Err(validation_error("browser verification result requires a running run"));}
            let blocking=outcome.assertions.iter().filter(|value|value.required&&!matches!(value.status.as_str(),"passed"|"approved")).count() as i64;
            let critical=outcome.accessibility.iter().filter(|value|value.severity=="critical").count() as i64;
            let accessibility_warnings=outcome.accessibility.iter().filter(|value|value.severity=="warning").count() as i64;
            let console_errors=outcome.console.iter().filter(|value|matches!(value.level.as_str(),"error"|"critical")).count() as i64;
            let console_warnings=outcome.console.iter().filter(|value|value.level=="warning").count() as i64;
            let visual=outcome.visual_comparisons.iter().filter(|value|matches!(value.status.as_str(),"different"|"missing_baseline")).count() as i64;
            let warnings=accessibility_warnings+console_warnings;
            let final_status=if blocking>0||critical>0{"failed"}else if visual>0||warnings>0{"needs_review"}else{"passed"};
            let now=now_ms();
            for event in &outcome.timeline { append_event(tx,id,&event.kind,&event.severity,actor,&event.payload)?; }
            for assertion in &outcome.assertions { append_event(tx,id,"assertion",if assertion.required&&assertion.status!="passed"{"error"}else{"info"},actor,&json!({"name":assertion.name,"required":assertion.required,"status":assertion.status,"message":assertion.message}))?; }
            for artifact in &outcome.artifacts { let metadata=encode_json_sql(&artifact.metadata)?;tx.execute("INSERT INTO browser_verification_artifacts(id,run_id,kind,artifact_hash,mime_type,size_bytes,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![artifact.id.as_bytes(),id.as_bytes(),artifact.kind,artifact.hash,artifact.mime_type,artifact.size_bytes,metadata,now])?; }
            for comparison in &outcome.visual_comparisons { let ignore=encode_json_sql(&comparison.ignore_regions)?;tx.execute("INSERT INTO browser_visual_comparisons(id,run_id,baseline_id,variant_key,current_artifact_hash,difference_artifact_hash,pixel_difference_ratio,perceptual_difference_ratio,threshold_ratio,status,ignore_regions_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![comparison.id.as_bytes(),id.as_bytes(),optional_uuid_bytes(comparison.baseline_id),comparison.variant_key,comparison.current_hash,comparison.difference_hash,comparison.pixel_difference_ratio,comparison.perceptual_difference_ratio,comparison.threshold_ratio,comparison.status,ignore,now])?; }
            for (index,entry) in outcome.console.iter().enumerate(){tx.execute("INSERT INTO browser_console_evidence(run_id,sequence,level,message,source,created_at) VALUES (?1,?2,?3,?4,?5,?6)",params![id.as_bytes(),index as i64+1,entry.level,entry.message,entry.source,now])?;}
            for (index,entry) in outcome.network.iter().enumerate(){tx.execute("INSERT INTO browser_network_evidence(run_id,sequence,method,url,status_code,failure,duration_ms,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![id.as_bytes(),index as i64+1,entry.method,entry.url,entry.status_code,entry.failure,entry.duration_ms,now])?;}
            for finding in &outcome.accessibility { let metadata=encode_json_sql(&finding.metadata)?;tx.execute("INSERT INTO browser_accessibility_findings(id,run_id,rule_id,severity,message,selector,help_url,status,artifact_hash,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'open',?8,?9,?10)",params![finding.id.as_bytes(),id.as_bytes(),finding.rule_id,finding.severity,finding.message,finding.selector,finding.help_url,finding.artifact_hash,metadata,now])?; }
            tx.execute("UPDATE browser_verification_runs SET status=?2,runner_version=?3,blocking_failures=?4,critical_accessibility=?5,warning_count=?6,visual_differences=?7,console_errors=?8,summary_json=?9,completed_at=?10,updated_at=?10 WHERE id=?1",params![id.as_bytes(),final_status,outcome.runner_version,blocking,critical,warnings,visual,console_errors,summary,now])?;
            append_event(tx,id,"completed",if final_status=="failed"{"error"}else if final_status=="needs_review"{"warning"}else{"info"},actor,&json!({"status":final_status,"blockingFailures":blocking,"criticalAccessibility":critical,"warnings":warnings,"visualDifferences":visual,"consoleErrors":console_errors}))?;
            let reopened=if final_status=="passed"{false}else{reopen_terminal(tx,task)?};Ok(Some((task,reopened)))
        }))?;
        let Some((task_id, reopened)) = result else {
            return Ok(None);
        };
        Ok(self.get(id)?.map(|value| BrowserVerificationMutation {
            task_id,
            reopened,
            value,
        }))
    }

    pub fn review(
        &self,
        id: Uuid,
        decision: &str,
        reason: &str,
        actor: &str,
    ) -> Result<Option<BrowserVerificationMutation<BrowserVerificationDetails>>> {
        validate_actor(actor)?;
        if !matches!(decision, "approve" | "reject")
            || reason.trim().is_empty()
            || reason.len() > 4096
        {
            return Err(StorageError::Validation(
                "browser verification review is invalid".into(),
            ));
        }
        let result=map_validation(self.0.transaction(|tx|{
            let Some((task,status,blocking,critical))=tx.query_row("SELECT task_id,status,blocking_failures,critical_accessibility FROM browser_verification_runs WHERE id=?1",[id.as_bytes()],|row|Ok((uuid(row,0)?,row.get::<_,String>(1)?,row.get::<_,i64>(2)?,row.get::<_,i64>(3)?))).optional()? else{return Ok(None)};
            if !matches!(status.as_str(),"needs_review"|"failed"){return Err(validation_error("browser verification run is not reviewable"));}
            if decision=="approve"&&(blocking>0||critical>0){return Err(validation_error("blocking assertions and critical accessibility findings cannot be approved away"));}
            let next=if decision=="approve"{"approved"}else{"failed"};let now=now_ms();let review=encode_json_sql(&json!({"decision":decision,"reason":reason,"actor":actor,"reviewedAt":now}))?;tx.execute("UPDATE browser_verification_runs SET status=?2,review_json=?3,updated_at=?4 WHERE id=?1",params![id.as_bytes(),next,review,now])?;append_event(tx,id,"reviewed",if decision=="approve"{"info"}else{"warning"},actor,&json!({"decision":decision,"reason":reason}))?;let reopened=if next=="approved"{false}else{reopen_terminal(tx,task)?};Ok(Some((task,reopened)))
        }))?;
        let Some((task_id, reopened)) = result else {
            return Ok(None);
        };
        Ok(self.get(id)?.map(|value| BrowserVerificationMutation {
            task_id,
            reopened,
            value,
        }))
    }

    pub fn approve_baseline(
        &self,
        comparison_id: Uuid,
        actor: &str,
    ) -> Result<Option<BrowserVerificationBaseline>> {
        validate_actor(actor)?;
        let baseline_id = Uuid::new_v4();
        let changed=map_validation(self.0.transaction(|tx|{let Some((run,definition,variant,hash))=tx.query_row("SELECT c.run_id,r.definition_id,c.variant_key,c.current_artifact_hash FROM browser_visual_comparisons c JOIN browser_verification_runs r ON r.id=c.run_id WHERE c.id=?1",[comparison_id.as_bytes()],|row|Ok((uuid(row,0)?,uuid(row,1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?))).optional()? else{return Ok(false)};let now=now_ms();tx.execute("UPDATE browser_verification_baselines SET status='superseded' WHERE definition_id=?1 AND variant_key=?2 AND status='active'",params![definition.as_bytes(),variant])?;tx.execute("INSERT INTO browser_verification_baselines(id,definition_id,variant_key,artifact_hash,source_run_id,status,approved_by,approved_at,metadata_json) VALUES (?1,?2,?3,?4,?5,'active',?6,?7,'{\"keying\":\"single_compared_screenshot_per_variant\"}')",params![baseline_id.as_bytes(),definition.as_bytes(),variant,hash,run.as_bytes(),actor,now])?;tx.execute("UPDATE browser_visual_comparisons SET baseline_id=?2,status='approved' WHERE id=?1",params![comparison_id.as_bytes(),baseline_id.as_bytes()])?;append_event(tx,run,"baseline_approved","info",actor,&json!({"baselineId":baseline_id,"comparisonId":comparison_id,"variantKey":variant,"artifactHash":hash}))?;Ok(true)}))?;
        if changed {
            self.baseline(baseline_id)
        } else {
            Ok(None)
        }
    }

    pub fn get(&self, id: Uuid) -> Result<Option<BrowserVerificationDetails>> {
        let Some(run) = self.run(id)? else {
            return Ok(None);
        };
        let definition = self.definition(run.definition_id)?.ok_or_else(|| {
            StorageError::Validation("browser verification definition disappeared".into())
        })?;
        Ok(Some(BrowserVerificationDetails {
            run,
            definition,
            events: self.events(id)?,
            artifacts: self.artifacts(id)?,
            visual_comparisons: self.comparisons(id)?,
            console: self.console(id)?,
            network: self.network(id)?,
            accessibility: self.accessibility(id)?,
        }))
    }
    pub fn run(&self, id: Uuid) -> Result<Option<BrowserVerificationRun>> {
        self.0.read(|db|db.query_row("SELECT id,definition_id,project_id,task_id,dev_server_instance_id,browser_session_id,status,attempt,max_attempts,timeout_ms,idempotency_key,runner_version,blocking_failures,critical_accessibility,warning_count,visual_differences,console_errors,summary_json,review_json,failure,created_at,started_at,completed_at,updated_at FROM browser_verification_runs WHERE id=?1",[id.as_bytes()],row_run).optional())
    }
    pub fn comparison_project_id(&self, id: Uuid) -> Result<Option<Uuid>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT r.project_id FROM browser_visual_comparisons c JOIN browser_verification_runs r ON r.id=c.run_id WHERE c.id=?1",
                [id.as_bytes()],
                |row| uuid(row, 0),
            )
            .optional()
        })
    }
    pub fn comparison_artifact_hash(&self, id: Uuid) -> Result<Option<String>> {
        self.0.read(|db| {
            db.query_row(
                "SELECT current_artifact_hash FROM browser_visual_comparisons WHERE id=?1",
                [id.as_bytes()],
                |row| row.get(0),
            )
            .optional()
        })
    }
    pub fn active_baselines(
        &self,
        definition_id: Uuid,
    ) -> Result<Vec<BrowserVerificationBaseline>> {
        self.0.read(|db| { let mut statement=db.prepare("SELECT id,definition_id,variant_key,artifact_hash,source_run_id,status,approved_by,approved_at,metadata_json FROM browser_verification_baselines WHERE definition_id=?1 AND status='active' ORDER BY variant_key,id")?; statement.query_map([definition_id.as_bytes()],row_baseline)?.collect() })
    }
    pub fn runs(&self, task_id: Uuid) -> Result<Vec<BrowserVerificationRun>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,definition_id,project_id,task_id,dev_server_instance_id,browser_session_id,status,attempt,max_attempts,timeout_ms,idempotency_key,runner_version,blocking_failures,critical_accessibility,warning_count,visual_differences,console_errors,summary_json,review_json,failure,created_at,started_at,completed_at,updated_at FROM browser_verification_runs WHERE task_id=?1 ORDER BY created_at DESC,id DESC")?;statement.query_map([task_id.as_bytes()],row_run)?.collect()})
    }
    pub fn events(&self, id: Uuid) -> Result<Vec<BrowserVerificationEvent>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,run_id,sequence,kind,severity,actor,payload_json,created_at FROM browser_verification_events WHERE run_id=?1 ORDER BY sequence")?;statement.query_map([id.as_bytes()],row_event)?.collect()})
    }
    fn variants(&self, id: Uuid) -> Result<Vec<BrowserVerificationVariant>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,definition_id,variant_key,width,height,device_scale,device_name,sort_order FROM browser_verification_variants WHERE definition_id=?1 ORDER BY sort_order,id")?;statement.query_map([id.as_bytes()],row_variant)?.collect()})
    }
    fn artifacts(&self, id: Uuid) -> Result<Vec<BrowserVerificationArtifact>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,run_id,kind,artifact_hash,mime_type,size_bytes,metadata_json,created_at FROM browser_verification_artifacts WHERE run_id=?1 ORDER BY created_at,id")?;statement.query_map([id.as_bytes()],row_artifact)?.collect()})
    }
    fn comparisons(&self, id: Uuid) -> Result<Vec<BrowserVisualComparison>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,run_id,baseline_id,variant_key,current_artifact_hash,difference_artifact_hash,pixel_difference_ratio,perceptual_difference_ratio,threshold_ratio,status,ignore_regions_json,created_at FROM browser_visual_comparisons WHERE run_id=?1 ORDER BY variant_key,id")?;statement.query_map([id.as_bytes()],row_comparison)?.collect()})
    }
    fn console(&self, id: Uuid) -> Result<Vec<BrowserConsoleEvidence>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,run_id,sequence,level,message,source,created_at FROM browser_console_evidence WHERE run_id=?1 ORDER BY sequence")?;statement.query_map([id.as_bytes()],row_console)?.collect()})
    }
    fn network(&self, id: Uuid) -> Result<Vec<BrowserNetworkEvidence>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,run_id,sequence,method,url,status_code,failure,duration_ms,created_at FROM browser_network_evidence WHERE run_id=?1 ORDER BY sequence")?;statement.query_map([id.as_bytes()],row_network)?.collect()})
    }
    fn accessibility(&self, id: Uuid) -> Result<Vec<BrowserAccessibilityFinding>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,run_id,rule_id,severity,message,selector,help_url,status,artifact_hash,metadata_json,created_at FROM browser_accessibility_findings WHERE run_id=?1 ORDER BY CASE severity WHEN 'critical' THEN 0 ELSE 1 END,rule_id,id")?;statement.query_map([id.as_bytes()],row_accessibility)?.collect()})
    }
    fn baseline(&self, id: Uuid) -> Result<Option<BrowserVerificationBaseline>> {
        self.0.read(|db|db.query_row("SELECT id,definition_id,variant_key,artifact_hash,source_run_id,status,approved_by,approved_at,metadata_json FROM browser_verification_baselines WHERE id=?1",[id.as_bytes()],row_baseline).optional())
    }
}

fn validate_definition(input: &NewBrowserVerificationDefinition, actor: &str) -> Result<()> {
    validate_actor(actor)?;
    if input.name.trim().is_empty()
        || input.name.len() > 256
        || !valid_url(&input.target_url)
        || !matches!(input.status.as_str(), "active" | "disabled" | "archived")
        || !(500..=120_000).contains(&input.timeout_ms)
        || !(0..=3).contains(&input.max_retries)
        || input.variants.is_empty()
        || input.variants.len() > 32
    {
        return Err(StorageError::Validation(
            "browser verification definition is invalid".into(),
        ));
    }
    if !input.steps.is_array() || !input.assertions.is_array() {
        return Err(StorageError::Validation(
            "browser verification steps and assertions must be arrays".into(),
        ));
    }
    let Some(steps) = input.steps.as_array() else {
        return Err(StorageError::Validation(
            "browser verification steps must be an array".into(),
        ));
    };
    if steps.len() > 256
        || steps
            .iter()
            .filter(|step| step.get("enabled").and_then(Value::as_bool) != Some(false))
            .any(|step| {
                !matches!(
                    step.get("kind").and_then(Value::as_str),
                    Some(
                        "navigate"
                            | "click"
                            | "fill"
                            | "press"
                            | "wait"
                            | "assert.text"
                            | "assert.element"
                            | "assert.status"
                            | "assert.console"
                            | "screenshot"
                            | "accessibility"
                    )
                )
            })
    {
        return Err(StorageError::Validation(
            "browser verification contains an unsupported step".into(),
        ));
    }
    let compared: Vec<_> = steps
        .iter()
        .filter(|step| {
            step.get("enabled").and_then(Value::as_bool) != Some(false)
                && step.get("kind").and_then(Value::as_str) == Some("screenshot")
                && step.get("compare").and_then(Value::as_bool) != Some(false)
        })
        .collect();
    if compared.len() > 1
        || compared.first().is_some_and(|step| {
            step.get("id").and_then(Value::as_str).is_none_or(|id| {
                id.is_empty()
                    || id.len() > 128
                    || !id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            })
        })
    {
        return Err(StorageError::Validation(
            "browser verification allows at most one compare screenshot and requires a safe step id"
                .into(),
        ));
    }
    encode_json(&input.steps, "browser verification steps")?;
    encode_json(&input.assertions, "browser verification assertions")?;
    validate_visual_policy(&input.visual_policy)?;
    let mut keys = HashSet::new();
    let mut orders = HashSet::new();
    for variant in &input.variants {
        if variant.key.trim().is_empty()
            || variant.key.len() > 128
            || !variant
                .key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || !keys.insert(variant.key.trim())
            || !orders.insert(variant.sort_order)
            || !(1..=16384).contains(&variant.width)
            || !(1..=16384).contains(&variant.height)
            || !(0.0..=8.0).contains(&variant.device_scale)
            || variant.device_scale == 0.0
        {
            return Err(StorageError::Validation(
                "browser verification responsive variant is invalid".into(),
            ));
        }
    }
    Ok(())
}
fn validate_visual_policy(value: &Value) -> Result<()> {
    let Some(policy) = value.as_object() else {
        return Err(StorageError::Validation(
            "browser verification visual policy must be an object".into(),
        ));
    };
    for key in ["pixelThreshold", "maxDiffPixelRatio", "perceptualThreshold"] {
        if policy
            .get(key)
            .and_then(Value::as_f64)
            .is_some_and(|ratio| !(0.0..=1.0).contains(&ratio))
        {
            return Err(StorageError::Validation(
                "browser verification visual thresholds must be between 0 and 1".into(),
            ));
        }
    }
    for key in ["dynamicRegions", "maskSelectors", "ignoreSelectors"] {
        if let Some(selectors) = policy.get(key) {
            let Some(selectors) = selectors.as_array() else {
                return Err(StorageError::Validation(
                    "browser verification dynamic regions must be arrays".into(),
                ));
            };
            if selectors.len() > 64
                || selectors.iter().any(|value| {
                    value.as_str().is_none_or(|selector| {
                        selector.trim().is_empty()
                            || selector.len() > 512
                            || selector.chars().any(char::is_control)
                    })
                })
            {
                return Err(StorageError::Validation(
                    "browser verification dynamic region is invalid".into(),
                ));
            }
        }
    }
    encode_json(value, "browser verification visual policy")?;
    Ok(())
}
fn validate_definition_scope(
    tx: &Transaction<'_>,
    input: &NewBrowserVerificationDefinition,
) -> rusqlite::Result<()> {
    if let Some(task) = input.task_id {
        let project = tx
            .query_row(
                "SELECT project_id FROM tasks WHERE id=?1",
                [task.as_bytes()],
                |row| optional_uuid(row, 0),
            )
            .optional()?
            .ok_or_else(|| validation_error("browser verification task does not exist"))?;
        if project != Some(input.project_id) {
            return Err(validation_error(
                "browser verification task belongs to a different project",
            ));
        }
    }
    if let Some(config) = input.dev_server_config_id {
        let project = tx
            .query_row(
                "SELECT project_id FROM dev_server_configs WHERE id=?1",
                [config.as_bytes()],
                |row| uuid(row, 0),
            )
            .optional()?
            .ok_or_else(|| {
                validation_error("browser verification development server config does not exist")
            })?;
        if project != input.project_id {
            return Err(validation_error(
                "browser verification development server belongs to a different project",
            ));
        }
    }
    Ok(())
}
fn validate_run_owners(
    tx: &Transaction<'_>,
    project: Uuid,
    task: Uuid,
    config: Option<Uuid>,
    instance: Option<Uuid>,
    browser: Option<Uuid>,
) -> rusqlite::Result<()> {
    if config.is_some() || instance.is_some() {
        let Some(instance) = instance else {
            return Err(validation_error(
                "browser verification requires a running development server",
            ));
        };
        let (actual_config, actual_project, actual_task, status) = tx
            .query_row(
                "SELECT config_id,project_id,task_id,status FROM dev_server_instances WHERE id=?1",
                [instance.as_bytes()],
                |row| {
                    Ok((
                        uuid(row, 0)?,
                        uuid(row, 1)?,
                        optional_uuid(row, 2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                validation_error("browser verification development server instance does not exist")
            })?;
        if config.is_some_and(|required| actual_config != required)
            || actual_project != project
            || status != "running"
            || actual_task != Some(task)
        {
            return Err(validation_error(
                "browser verification development server ownership or state is invalid",
            ));
        }
    }
    if let Some(browser) = browser {
        let (actual_project,actual_task,status)=tx.query_row("SELECT project_id,task_id,status FROM browser_sessions WHERE id=?1 AND project_id IS NOT NULL",[browser.as_bytes()],|row|Ok((uuid(row,0)?,optional_uuid(row,1)?,row.get::<_,String>(2)?))).optional()?.ok_or_else(||validation_error("browser verification browser session does not exist"))?;
        if actual_project != project
            || actual_task.is_some_and(|value| value != task)
            || status != "running"
        {
            return Err(validation_error(
                "browser verification browser session ownership or state is invalid",
            ));
        }
    }
    Ok(())
}
fn validate_outcome(value: &BrowserVerificationOutcome, actor: &str) -> Result<()> {
    validate_actor(actor)?;
    if value.runner_version.trim().is_empty()
        || value.runner_version.len() > 128
        || value.timeline.len() > MAX_COLLECTION
        || value.assertions.len() > MAX_COLLECTION
        || value.artifacts.len() > 64
        || value.visual_comparisons.len() > 128
        || value.console.len() > MAX_COLLECTION
        || value.network.len() > MAX_COLLECTION
        || value.accessibility.len() > MAX_COLLECTION
    {
        return Err(StorageError::Validation(
            "browser verification result exceeds configured limits".into(),
        ));
    }
    encode_json(&value.summary, "browser verification summary")?;
    for assertion in &value.assertions {
        if assertion.name.trim().is_empty()
            || assertion.name.len() > 512
            || !matches!(
                assertion.status.as_str(),
                "passed" | "failed" | "error" | "approved"
            )
        {
            return Err(StorageError::Validation(
                "browser verification assertion is invalid".into(),
            ));
        }
    }
    for event in &value.timeline {
        if event.kind.trim().is_empty()
            || event.kind.len() > 128
            || !matches!(
                event.severity.as_str(),
                "info" | "warning" | "error" | "critical"
            )
        {
            return Err(StorageError::Validation(
                "browser verification timeline event is invalid".into(),
            ));
        }
        encode_json(&event.payload, "browser verification timeline payload")?;
    }
    for artifact in &value.artifacts {
        validate_hash(&artifact.hash)?;
        if artifact.kind.trim().is_empty()
            || artifact.kind.len() > 64
            || artifact.mime_type.trim().is_empty()
            || artifact.mime_type.len() > 128
            || artifact.size_bytes < 0
        {
            return Err(StorageError::Validation(
                "browser verification artifact is invalid".into(),
            ));
        }
        encode_json(&artifact.metadata, "browser verification artifact metadata")?;
    }
    for comparison in &value.visual_comparisons {
        validate_hash(&comparison.current_hash)?;
        if let Some(hash) = &comparison.difference_hash {
            validate_hash(hash)?;
        }
        if !matches!(
            comparison.status.as_str(),
            "matched" | "different" | "missing_baseline" | "approved"
        ) || !(0.0..=1.0).contains(&comparison.pixel_difference_ratio)
            || comparison
                .perceptual_difference_ratio
                .is_some_and(|ratio| !(0.0..=1.0).contains(&ratio))
            || !(0.0..=1.0).contains(&comparison.threshold_ratio)
        {
            return Err(StorageError::Validation(
                "browser visual comparison ratios must be between 0 and 1".into(),
            ));
        }
    }
    for entry in &value.console {
        if entry.message.len() > 65536
            || !matches!(
                entry.level.as_str(),
                "debug" | "info" | "warning" | "error" | "critical"
            )
        {
            return Err(StorageError::Validation(
                "browser console evidence is invalid".into(),
            ));
        }
    }
    for entry in &value.network {
        if entry.method.is_empty()
            || entry.method.len() > 32
            || !valid_url(&entry.url)
            || entry.duration_ms.is_some_and(|duration| duration < 0)
        {
            return Err(StorageError::Validation(
                "browser network evidence is invalid".into(),
            ));
        }
    }
    for finding in &value.accessibility {
        if finding.rule_id.trim().is_empty()
            || finding.message.trim().is_empty()
            || !matches!(finding.severity.as_str(), "warning" | "critical")
        {
            return Err(StorageError::Validation(
                "browser accessibility finding is invalid".into(),
            ));
        }
        if let Some(hash) = &finding.artifact_hash {
            validate_hash(hash)?;
        }
        encode_json(&finding.metadata, "browser accessibility metadata")?;
    }
    Ok(())
}
fn append_event(
    tx: &Transaction<'_>,
    run: Uuid,
    kind: &str,
    severity: &str,
    actor: &str,
    payload: &Value,
) -> rusqlite::Result<()> {
    let sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(sequence),0)+1 FROM browser_verification_events WHERE run_id=?1",
        [run.as_bytes()],
        |row| row.get(0),
    )?;
    tx.execute("INSERT INTO browser_verification_events(run_id,sequence,kind,severity,actor,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![run.as_bytes(),sequence,kind,severity,actor,encode_json_sql(payload)?,now_ms()])?;
    Ok(())
}
fn reopen_terminal(tx: &Transaction<'_>, task: Uuid) -> rusqlite::Result<bool> {
    Ok(tx.execute("UPDATE tasks SET status='review',completed_at=NULL,updated_at=?2 WHERE id=?1 AND status IN ('completed','done')",params![task.as_bytes(),now_ms()])?>0)
}
fn require_project(tx: &Transaction<'_>, project: Uuid) -> rusqlite::Result<()> {
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
        [project.as_bytes()],
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(validation_error(
            "browser verification project does not exist",
        ))
    }
}
fn valid_url(value: &str) -> bool {
    value.len() <= 8192
        && !value.chars().any(char::is_control)
        && (value.starts_with("http://") || value.starts_with("https://"))
}
fn validate_actor(actor: &str) -> Result<()> {
    if actor.trim().is_empty() || actor.len() > 128 {
        Err(StorageError::Validation(
            "browser verification actor is invalid".into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        Ok(())
    } else {
        Err(StorageError::Validation(
            "browser verification artifact hash is invalid".into(),
        ))
    }
}
fn encode_json(value: &Value, label: &str) -> Result<String> {
    let encoded = serde_json::to_string(value)
        .map_err(|error| StorageError::Validation(format!("could not encode {label}: {error}")))?;
    if encoded.len() > MAX_JSON_BYTES {
        Err(StorageError::Validation(format!("{label} exceeds 1 MiB")))
    } else {
        Ok(encoded)
    }
}
fn encode_json_sql(value: &Value) -> rusqlite::Result<String> {
    serde_json::to_string(value)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}
fn validation_error(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{VALIDATION_PREFIX}{}", message.into()))
}
fn optional_uuid_bytes(value: Option<Uuid>) -> Option<Vec<u8>> {
    value.map(|id| id.as_bytes().to_vec())
}
fn uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Uuid> {
    let bytes: Vec<u8> = row.get(index)?;
    Uuid::from_slice(&bytes).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Blob,
            Box::new(error),
        )
    })
}
fn optional_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<Uuid>> {
    row.get::<_, Option<Vec<u8>>>(index)?
        .map(|bytes| {
            Uuid::from_slice(&bytes).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    index,
                    rusqlite::types::Type::Blob,
                    Box::new(error),
                )
            })
        })
        .transpose()
}
fn value(row: &Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn row_definition(row: &Row<'_>) -> rusqlite::Result<BrowserVerificationDefinition> {
    Ok(BrowserVerificationDefinition {
        id: uuid(row, 0)?,
        project_id: uuid(row, 1)?,
        task_id: optional_uuid(row, 2)?,
        dev_server_config_id: optional_uuid(row, 3)?,
        name: row.get(4)?,
        target_url: row.get(5)?,
        steps: value(row, 6)?,
        assertions: value(row, 7)?,
        visual_policy: value(row, 8)?,
        fail_on_accessibility: row.get(9)?,
        variants: Vec::new(),
        status: row.get(10)?,
        required: row.get(11)?,
        timeout_ms: row.get(12)?,
        max_retries: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}
fn row_variant(row: &Row<'_>) -> rusqlite::Result<BrowserVerificationVariant> {
    Ok(BrowserVerificationVariant {
        id: uuid(row, 0)?,
        definition_id: uuid(row, 1)?,
        key: row.get(2)?,
        width: row.get(3)?,
        height: row.get(4)?,
        device_scale: row.get(5)?,
        device_name: row.get(6)?,
        sort_order: row.get(7)?,
    })
}
fn row_run(row: &Row<'_>) -> rusqlite::Result<BrowserVerificationRun> {
    Ok(BrowserVerificationRun {
        id: uuid(row, 0)?,
        definition_id: uuid(row, 1)?,
        project_id: uuid(row, 2)?,
        task_id: uuid(row, 3)?,
        dev_server_instance_id: optional_uuid(row, 4)?,
        browser_session_id: optional_uuid(row, 5)?,
        status: row.get(6)?,
        attempt: row.get(7)?,
        max_attempts: row.get(8)?,
        timeout_ms: row.get(9)?,
        idempotency_key: row.get(10)?,
        runner_version: row.get(11)?,
        blocking_failures: row.get(12)?,
        critical_accessibility: row.get(13)?,
        warning_count: row.get(14)?,
        visual_differences: row.get(15)?,
        console_errors: row.get(16)?,
        summary: value(row, 17)?,
        review: value(row, 18)?,
        failure: row.get(19)?,
        created_at: row.get(20)?,
        started_at: row.get(21)?,
        completed_at: row.get(22)?,
        updated_at: row.get(23)?,
    })
}
fn row_event(row: &Row<'_>) -> rusqlite::Result<BrowserVerificationEvent> {
    Ok(BrowserVerificationEvent {
        id: row.get(0)?,
        run_id: uuid(row, 1)?,
        sequence: row.get(2)?,
        kind: row.get(3)?,
        severity: row.get(4)?,
        actor: row.get(5)?,
        payload: value(row, 6)?,
        created_at: row.get(7)?,
    })
}
fn row_artifact(row: &Row<'_>) -> rusqlite::Result<BrowserVerificationArtifact> {
    Ok(BrowserVerificationArtifact {
        id: uuid(row, 0)?,
        run_id: uuid(row, 1)?,
        kind: row.get(2)?,
        hash: row.get(3)?,
        mime_type: row.get(4)?,
        size_bytes: row.get(5)?,
        metadata: value(row, 6)?,
        created_at: row.get(7)?,
    })
}
fn row_comparison(row: &Row<'_>) -> rusqlite::Result<BrowserVisualComparison> {
    Ok(BrowserVisualComparison {
        id: uuid(row, 0)?,
        run_id: uuid(row, 1)?,
        baseline_id: optional_uuid(row, 2)?,
        variant_key: row.get(3)?,
        current_hash: row.get(4)?,
        difference_hash: row.get(5)?,
        pixel_difference_ratio: row.get(6)?,
        perceptual_difference_ratio: row.get(7)?,
        threshold_ratio: row.get(8)?,
        status: row.get(9)?,
        ignore_regions: value(row, 10)?,
        created_at: row.get(11)?,
    })
}
fn row_console(row: &Row<'_>) -> rusqlite::Result<BrowserConsoleEvidence> {
    Ok(BrowserConsoleEvidence {
        id: row.get(0)?,
        run_id: uuid(row, 1)?,
        sequence: row.get(2)?,
        level: row.get(3)?,
        message: row.get(4)?,
        source: row.get(5)?,
        created_at: row.get(6)?,
    })
}
fn row_network(row: &Row<'_>) -> rusqlite::Result<BrowserNetworkEvidence> {
    Ok(BrowserNetworkEvidence {
        id: row.get(0)?,
        run_id: uuid(row, 1)?,
        sequence: row.get(2)?,
        method: row.get(3)?,
        url: row.get(4)?,
        status_code: row.get(5)?,
        failure: row.get(6)?,
        duration_ms: row.get(7)?,
        created_at: row.get(8)?,
    })
}
fn row_accessibility(row: &Row<'_>) -> rusqlite::Result<BrowserAccessibilityFinding> {
    Ok(BrowserAccessibilityFinding {
        id: uuid(row, 0)?,
        run_id: uuid(row, 1)?,
        rule_id: row.get(2)?,
        severity: row.get(3)?,
        message: row.get(4)?,
        selector: row.get(5)?,
        help_url: row.get(6)?,
        status: row.get(7)?,
        artifact_hash: row.get(8)?,
        metadata: value(row, 9)?,
        created_at: row.get(10)?,
    })
}
fn row_baseline(row: &Row<'_>) -> rusqlite::Result<BrowserVerificationBaseline> {
    Ok(BrowserVerificationBaseline {
        id: uuid(row, 0)?,
        definition_id: uuid(row, 1)?,
        variant_key: row.get(2)?,
        artifact_hash: row.get(3)?,
        source_run_id: optional_uuid(row, 4)?,
        status: row.get(5)?,
        approved_by: row.get(6)?,
        approved_at: row.get(7)?,
        metadata: value(row, 8)?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{NewDevServerConfig, NewProject, NewTask};

    fn running_run(db: &Database) -> Uuid {
        let project = db
            .projects()
            .create(&NewProject::new("Browser verification"))
            .unwrap();
        let mut task = NewTask::new("Verify UI");
        task.project_id = Some(project.id);
        let task = db.tasks().create(&task).unwrap();
        let mut definition =
            NewBrowserVerificationDefinition::new(project.id, "Homepage", "http://127.0.0.1:3000");
        definition.task_id = Some(task.id);
        definition.steps = json!([{"kind":"navigate"},{"kind":"assert.text","text":"Ready"},{"id":"visual","kind":"screenshot","compare":true,"value":"visual"}]);
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
                    idempotency_key: Some("one".into()),
                },
                "test",
            )
            .unwrap();
        db.browser_verification()
            .start(queued.value.run.id, "test")
            .unwrap();
        queued.value.run.id
    }

    #[test]
    fn cancelled_run_rejects_late_runner_result() {
        let db = Database::open_in_memory().unwrap();
        let run_id = running_run(&db);
        db.browser_verification().cancel(run_id, "test").unwrap();
        let outcome = BrowserVerificationOutcome {
            runner_version: "late".into(),
            summary: json!({}),
            timeline: vec![],
            assertions: vec![],
            artifacts: vec![],
            visual_comparisons: vec![],
            console: vec![],
            network: vec![],
            accessibility: vec![],
        };
        assert!(matches!(
            db.browser_verification()
                .complete(run_id, &outcome, "runner"),
            Err(StorageError::Validation(_))
        ));
        assert_eq!(
            db.browser_verification()
                .run(run_id)
                .unwrap()
                .unwrap()
                .status,
            "cancelled"
        );
    }

    #[test]
    fn restart_interrupts_queued_and_running_browser_verification() {
        let db = Database::open_in_memory().unwrap();
        let run_id = running_run(&db);
        let report = db.recover_interrupted().unwrap();
        assert_eq!(report.interrupted_browser_verifications, 1);
        assert_eq!(
            db.browser_verification()
                .run(run_id)
                .unwrap()
                .unwrap()
                .status,
            "interrupted"
        );
        assert_eq!(
            db.browser_verification()
                .events(run_id)
                .unwrap()
                .last()
                .unwrap()
                .kind,
            "interrupted"
        );
    }

    #[test]
    fn required_warning_run_blocks_until_reviewed_and_approved() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Gating")).unwrap();
        let mut task = NewTask::new("Gate task");
        task.project_id = Some(project.id);
        let task = db.tasks().create(&task).unwrap();
        let mut definition = NewBrowserVerificationDefinition::new(
            project.id,
            "Required UI",
            "http://127.0.0.1:3000",
        );
        definition.task_id = Some(task.id);
        let definition = db
            .browser_verification()
            .save_definition(&definition, "test")
            .unwrap();
        assert!(matches!(
            db.tasks().set_status(task.id, "completed"),
            Err(StorageError::Validation(_))
        ));
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
            runner_version: "test".into(),
            summary: json!({}),
            timeline: vec![],
            assertions: vec![],
            artifacts: vec![],
            visual_comparisons: vec![],
            console: vec![],
            network: vec![],
            accessibility: vec![NewAccessibilityFinding {
                id: Uuid::new_v4(),
                rule_id: "label".into(),
                severity: "warning".into(),
                message: "label could be clearer".into(),
                selector: Some("#name".into()),
                help_url: None,
                artifact_hash: None,
                metadata: json!({}),
            }],
        };
        let completed = db
            .browser_verification()
            .complete(queued.value.run.id, &outcome, "runner")
            .unwrap()
            .unwrap();
        assert_eq!(completed.value.run.status, "needs_review");
        assert!(db.tasks().set_status(task.id, "completed").is_err());
        db.browser_verification()
            .review(
                queued.value.run.id,
                "approve",
                "warning reviewed",
                "reviewer",
            )
            .unwrap();
        assert!(db.tasks().set_status(task.id, "completed").unwrap());
    }

    #[test]
    fn configless_definition_rejects_foreign_development_server_instance() {
        let db = Database::open_in_memory().unwrap();
        let owner = db.projects().create(&NewProject::new("Owner")).unwrap();
        let foreign = db.projects().create(&NewProject::new("Foreign")).unwrap();
        let mut owner_task = NewTask::new("Owner task");
        owner_task.project_id = Some(owner.id);
        let owner_task = db.tasks().create(&owner_task).unwrap();
        let mut foreign_task = NewTask::new("Foreign task");
        foreign_task.project_id = Some(foreign.id);
        let foreign_task = db.tasks().create(&foreign_task).unwrap();
        let config = db
            .dev_servers()
            .save_config(&NewDevServerConfig::new(foreign.id, "web", "serve", "."))
            .unwrap();
        let instance = db
            .dev_servers()
            .prepare_start(config.id, Some(foreign_task.id), "test")
            .unwrap();
        db.dev_servers()
            .mark_running(
                instance.id,
                Some(1),
                "http://127.0.0.1:3000",
                &json!({}),
                "test",
            )
            .unwrap();
        let mut definition =
            NewBrowserVerificationDefinition::new(owner.id, "UI", "http://127.0.0.1:3000");
        definition.task_id = Some(owner_task.id);
        definition.dev_server_config_id = None;
        let definition = db
            .browser_verification()
            .save_definition(&definition, "test")
            .unwrap();
        let error = db
            .browser_verification()
            .queue_run(
                &NewBrowserVerificationRun {
                    id: Uuid::new_v4(),
                    definition_id: definition.id,
                    task_id: owner_task.id,
                    dev_server_instance_id: Some(instance.id),
                    browser_session_id: None,
                    idempotency_key: None,
                },
                "test",
            )
            .unwrap_err();
        assert!(matches!(error, StorageError::Validation(_)));
    }
}
