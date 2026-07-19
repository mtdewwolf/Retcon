//! Durable managed-browser sessions, profiles, tabs, evidence, and history.

#![allow(missing_docs)]

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::repositories::now_ms;
use crate::task_planning::map_validation;
use crate::{Database, Result, StorageError};

const VALIDATION_PREFIX: &str = "retcon_validation:";
const MAX_HISTORY_ENTRIES: i64 = 1_000;

#[derive(Clone, Debug)]
pub struct NewDurableBrowserSession {
    pub id: Uuid,
    pub project_id: Uuid,
    pub task_id: Option<Uuid>,
    pub dev_server_instance_id: Option<Uuid>,
    pub profile_id: Uuid,
    pub profile_path: String,
    pub persistent_profile: bool,
    pub network_policy: String,
}

impl NewDurableBrowserSession {
    #[must_use]
    pub fn new(project_id: Uuid, profile_path: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            task_id: None,
            dev_server_instance_id: None,
            profile_id: Uuid::new_v4(),
            profile_path: profile_path.into(),
            persistent_profile: false,
            network_policy: "loopback".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableBrowserSession {
    pub id: Uuid,
    pub project_id: Uuid,
    pub task_id: Option<Uuid>,
    pub worktree_id: Option<Uuid>,
    pub dev_server_instance_id: Option<Uuid>,
    pub profile_id: Uuid,
    pub status: String,
    pub service_session_id: Option<String>,
    pub service_version: Option<String>,
    pub service_protocol: Option<i64>,
    pub network_policy: String,
    pub failure: Option<String>,
    pub started_at: i64,
    pub updated_at: i64,
    pub ended_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProfile {
    pub id: Uuid,
    pub project_id: Uuid,
    pub worktree_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub path: String,
    pub persistent: bool,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub released_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub id: Uuid,
    pub browser_session_id: Uuid,
    pub service_tab_id: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserObservation {
    pub id: Uuid,
    pub browser_session_id: Uuid,
    pub tab_id: Option<Uuid>,
    pub kind: String,
    pub artifact_hash: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub metadata: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHistoryEvent {
    pub id: i64,
    pub browser_session_id: Uuid,
    pub project_id: Uuid,
    pub kind: String,
    pub actor: String,
    pub payload: Value,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTakeover {
    pub id: Uuid,
    pub browser_session_id: Uuid,
    pub actor: String,
    pub reason: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub end_reason: Option<String>,
}

pub struct BrowserRepository<'a>(&'a Database);

impl Database {
    #[must_use]
    pub fn durable_browsers(&self) -> BrowserRepository<'_> {
        BrowserRepository(self)
    }
}

impl BrowserRepository<'_> {
    pub fn prepare_session(
        &self,
        input: &NewDurableBrowserSession,
        actor: &str,
    ) -> Result<DurableBrowserSession> {
        validate_new_session(input, actor)?;
        map_validation(self.0.transaction(|tx| {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [input.project_id.as_bytes()],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(validation_error("browser project does not exist"));
            }
            let (task_id, worktree_id) = browser_scope(
                tx,
                input.project_id,
                input.task_id,
                input.dev_server_instance_id,
            )?;
            let now = now_ms();
            tx.execute(
                "INSERT INTO browser_profiles(id,project_id,worktree_id,task_id,path,persistent,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,'active',?7,?7)",
                params![input.profile_id.as_bytes(),input.project_id.as_bytes(),optional_uuid_bytes(worktree_id),optional_uuid_bytes(task_id),input.profile_path,input.persistent_profile,now],
            )?;
            tx.execute(
                "INSERT INTO browser_sessions(id,session_id,status,started_at,metadata_json,project_id,task_id,worktree_id,dev_server_instance_id,profile_id,network_policy,updated_at) VALUES (?1,NULL,'starting',?2,'{}',?3,?4,?5,?6,?7,?8,?2)",
                params![input.id.as_bytes(),now,input.project_id.as_bytes(),optional_uuid_bytes(task_id),optional_uuid_bytes(worktree_id),optional_uuid_bytes(input.dev_server_instance_id),input.profile_id.as_bytes(),input.network_policy],
            )?;
            record_event(tx,input.id,input.project_id,"starting",actor,&json!({"taskId":task_id,"worktreeId":worktree_id,"profileId":input.profile_id,"networkPolicy":input.network_policy}))?;
            Ok(())
        }))?;
        self.session(input.id)?
            .ok_or_else(|| StorageError::Validation("created browser session disappeared".into()))
    }

    pub fn mark_running(
        &self,
        id: Uuid,
        service_session_id: &str,
        service_version: &str,
        service_protocol: i64,
        actor: &str,
    ) -> Result<Option<DurableBrowserSession>> {
        if service_session_id.trim().is_empty()
            || service_session_id.len() > 256
            || service_session_id.chars().any(char::is_control)
            || service_version.trim().is_empty()
            || service_version.len() > 128
            || service_version.chars().any(char::is_control)
            || service_protocol <= 0
        {
            return Err(StorageError::Validation(
                "browser service identity cannot be empty".into(),
            ));
        }
        self.transition(
            id,
            "running",
            None,
            Some((service_session_id, service_version, service_protocol)),
            actor,
        )
    }

    pub fn begin_stop(&self, id: Uuid, actor: &str) -> Result<Option<DurableBrowserSession>> {
        self.transition(id, "stopping", None, None, actor)
    }

    pub fn mark_stopped(&self, id: Uuid, actor: &str) -> Result<Option<DurableBrowserSession>> {
        self.transition(id, "stopped", None, None, actor)
    }

    pub fn mark_failed(
        &self,
        id: Uuid,
        failure: &str,
        actor: &str,
    ) -> Result<Option<DurableBrowserSession>> {
        self.transition(id, "failed", Some(failure), None, actor)
    }

    pub fn mark_orphaned(
        &self,
        id: Uuid,
        failure: &str,
        actor: &str,
    ) -> Result<Option<DurableBrowserSession>> {
        self.transition(id, "orphaned", Some(failure), None, actor)
    }

    fn transition(
        &self,
        id: Uuid,
        next: &str,
        failure: Option<&str>,
        service: Option<(&str, &str, i64)>,
        actor: &str,
    ) -> Result<Option<DurableBrowserSession>> {
        validate_actor(actor)?;
        map_validation(self.0.transaction(|tx| {
            let Some((project_id,profile_id,current))=tx.query_row("SELECT project_id,profile_id,status FROM browser_sessions WHERE id=?1 AND project_id IS NOT NULL AND profile_id IS NOT NULL",[id.as_bytes()],|row|Ok((uuid(row,0)?,uuid(row,1)?,row.get::<_,String>(2)?))).optional()? else{return Ok(false)};
            let valid=match next {"running"=>current=="starting","stopping"=>current=="running","stopped"=>current=="stopping","failed"|"orphaned"=>matches!(current.as_str(),"starting"|"running"|"stopping"),_=>false};
            if !valid{return Err(validation_error(format!("cannot transition browser session from '{current}' to '{next}'")));}
            let now=now_ms();
            let (service_id,version,protocol)=service.map_or((None,None,None),|(id,version,protocol)|(Some(id),Some(version),Some(protocol)));
            tx.execute("UPDATE browser_sessions SET status=?2,service_session_id=COALESCE(?3,service_session_id),service_version=COALESCE(?4,service_version),service_protocol=COALESCE(?5,service_protocol),failure=?6,updated_at=?7,ended_at=CASE WHEN ?2 IN ('stopped','failed','orphaned') THEN ?7 ELSE ended_at END WHERE id=?1",params![id.as_bytes(),next,service_id,version,protocol,failure,now])?;
            if matches!(next,"stopped"|"failed") {
                tx.execute("UPDATE browser_profiles SET status=CASE WHEN persistent=1 THEN 'available' ELSE 'released' END,updated_at=?2,released_at=?2 WHERE id=?1 AND status='active'",params![profile_id.as_bytes(),now])?;
            }
            if matches!(next,"stopped"|"failed"|"orphaned") { tx.execute("UPDATE browser_takeovers SET ended_at=?2,end_reason=?3 WHERE browser_session_id=?1 AND ended_at IS NULL",params![id.as_bytes(),now,next])?; }
            record_event(tx,id,project_id,next,actor,&json!({"failure":failure}))?;
            Ok(true)
        }))?;
        self.session(id)
    }

    pub fn session(&self, id: Uuid) -> Result<Option<DurableBrowserSession>> {
        self.0.read(|db|db.query_row("SELECT id,project_id,task_id,worktree_id,dev_server_instance_id,profile_id,status,service_session_id,service_version,service_protocol,network_policy,failure,started_at,COALESCE(updated_at,started_at),ended_at FROM browser_sessions WHERE id=?1 AND project_id IS NOT NULL AND profile_id IS NOT NULL",[id.as_bytes()],row_session).optional())
    }

    pub fn list_sessions(&self, project_id: Uuid) -> Result<Vec<DurableBrowserSession>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,project_id,task_id,worktree_id,dev_server_instance_id,profile_id,status,service_session_id,service_version,service_protocol,network_policy,failure,started_at,COALESCE(updated_at,started_at),ended_at FROM browser_sessions WHERE project_id=?1 ORDER BY started_at DESC,id DESC")?;statement.query_map([project_id.as_bytes()],row_session)?.collect()})
    }

    pub fn profile(&self, id: Uuid) -> Result<Option<BrowserProfile>> {
        self.0.read(|db|db.query_row("SELECT id,project_id,worktree_id,task_id,path,persistent,status,created_at,updated_at,released_at FROM browser_profiles WHERE id=?1",[id.as_bytes()],row_profile).optional())
    }

    pub fn upsert_tab(
        &self,
        session_id: Uuid,
        service_tab_id: &str,
        url: Option<&str>,
        title: Option<&str>,
        actor: &str,
    ) -> Result<BrowserTab> {
        if service_tab_id.trim().is_empty()
            || service_tab_id.len() > 256
            || service_tab_id.chars().any(char::is_control)
            || url.is_some_and(|value| value.len() > 8_192 || value.chars().any(char::is_control))
            || title.is_some_and(|value| value.len() > 4_096 || value.chars().any(char::is_control))
        {
            return Err(StorageError::Validation(
                "browser tab id cannot be empty".into(),
            ));
        }
        validate_actor(actor)?;
        let tab_id = Uuid::new_v4();
        map_validation(self.0.transaction(|tx|{
            let project_id=session_project(tx,session_id)?;let now=now_ms();
            tx.execute("INSERT INTO browser_tabs(id,browser_session_id,service_tab_id,url,title,status,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,'open',?6,?6) ON CONFLICT(browser_session_id,service_tab_id) DO UPDATE SET url=COALESCE(excluded.url,url),title=COALESCE(excluded.title,title),status='open',updated_at=excluded.updated_at,closed_at=NULL",params![tab_id.as_bytes(),session_id.as_bytes(),service_tab_id,url,title,now])?;
            record_event(tx,session_id,project_id,"tab_updated",actor,&json!({"serviceTabId":service_tab_id,"url":url,"title":title}))?;Ok(())
        }))?;
        self.tab_by_service_id(session_id, service_tab_id)?
            .ok_or_else(|| StorageError::Validation("browser tab disappeared".into()))
    }

    pub fn close_tab(&self, session_id: Uuid, tab_id: Uuid, actor: &str) -> Result<bool> {
        validate_actor(actor)?;
        map_validation(self.0.transaction(|tx|{let project_id=session_project(tx,session_id)?;let now=now_ms();let changed=tx.execute("UPDATE browser_tabs SET status='closed',updated_at=?3,closed_at=?3 WHERE id=?1 AND browser_session_id=?2 AND status='open'",params![tab_id.as_bytes(),session_id.as_bytes(),now])?;if changed>0{record_event(tx,session_id,project_id,"tab_closed",actor,&json!({"tabId":tab_id}))?;}Ok(changed>0)}))
    }

    pub fn tab(&self, id: Uuid) -> Result<Option<BrowserTab>> {
        self.0.read(|db|db.query_row("SELECT id,browser_session_id,service_tab_id,url,title,status,created_at,updated_at,closed_at FROM browser_tabs WHERE id=?1",[id.as_bytes()],row_tab).optional())
    }

    pub fn list_tabs(&self, session_id: Uuid) -> Result<Vec<BrowserTab>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,browser_session_id,service_tab_id,url,title,status,created_at,updated_at,closed_at FROM browser_tabs WHERE browser_session_id=?1 ORDER BY created_at,id")?;statement.query_map([session_id.as_bytes()],row_tab)?.collect()})
    }

    fn tab_by_service_id(
        &self,
        session_id: Uuid,
        service_tab_id: &str,
    ) -> Result<Option<BrowserTab>> {
        self.0.read(|db|db.query_row("SELECT id,browser_session_id,service_tab_id,url,title,status,created_at,updated_at,closed_at FROM browser_tabs WHERE browser_session_id=?1 AND service_tab_id=?2",params![session_id.as_bytes(),service_tab_id],row_tab).optional())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_observation(
        &self,
        session_id: Uuid,
        tab_id: Option<Uuid>,
        kind: &str,
        artifact_hash: &str,
        mime_type: &str,
        size_bytes: i64,
        metadata: &Value,
        actor: &str,
    ) -> Result<BrowserObservation> {
        validate_actor(actor)?;
        validate_hash(artifact_hash)?;
        if kind.trim().is_empty()
            || kind.len() > 64
            || mime_type.trim().is_empty()
            || mime_type.len() > 128
            || size_bytes < 0
        {
            return Err(StorageError::Validation(
                "invalid browser observation metadata".into(),
            ));
        }
        let id = Uuid::new_v4();
        let encoded =
            serde_json::to_string(metadata).map_err(json_error("encode browser observation"))?;
        if encoded.len() > 256 * 1024 {
            return Err(StorageError::Validation(
                "browser observation metadata exceeds 256 KiB".into(),
            ));
        }
        map_validation(self.0.transaction(|tx|{let project_id=session_project(tx,session_id)?;if let Some(tab_id)=tab_id{let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM browser_tabs WHERE id=?1 AND browser_session_id=?2)",params![tab_id.as_bytes(),session_id.as_bytes()],|row|row.get(0))?;if !exists{return Err(validation_error("browser observation tab does not belong to session"));}}let now=now_ms();tx.execute("INSERT INTO browser_observations(id,browser_session_id,tab_id,kind,artifact_hash,mime_type,size_bytes,metadata_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id.as_bytes(),session_id.as_bytes(),optional_uuid_bytes(tab_id),kind,artifact_hash,mime_type,size_bytes,encoded,now])?;record_event(tx,session_id,project_id,"observation_recorded",actor,&json!({"observationId":id,"kind":kind,"artifactHash":artifact_hash,"sizeBytes":size_bytes}))?;Ok(())}))?;
        self.observation(id)?
            .ok_or_else(|| StorageError::Validation("browser observation disappeared".into()))
    }

    pub fn observation(&self, id: Uuid) -> Result<Option<BrowserObservation>> {
        self.0.read(|db|db.query_row("SELECT id,browser_session_id,tab_id,kind,artifact_hash,mime_type,size_bytes,metadata_json,created_at FROM browser_observations WHERE id=?1",[id.as_bytes()],row_observation).optional())
    }

    pub fn list_observations(&self, session_id: Uuid) -> Result<Vec<BrowserObservation>> {
        self.0.read(|db|{let mut statement=db.prepare("SELECT id,browser_session_id,tab_id,kind,artifact_hash,mime_type,size_bytes,metadata_json,created_at FROM browser_observations WHERE browser_session_id=?1 ORDER BY created_at DESC,id DESC")?;statement.query_map([session_id.as_bytes()],row_observation)?.collect()})
    }

    pub fn append_console(&self, session_id: Uuid, entries: &[Value]) -> Result<usize> {
        self.append_bounded("browser_console_entries", session_id, entries)
    }
    pub fn append_network(&self, session_id: Uuid, entries: &[Value]) -> Result<usize> {
        self.append_bounded("browser_network_entries", session_id, entries)
    }

    fn append_bounded(&self, table: &str, session_id: Uuid, entries: &[Value]) -> Result<usize> {
        if !matches!(table, "browser_console_entries" | "browser_network_entries") {
            return Err(StorageError::Validation(
                "invalid browser history table".into(),
            ));
        }
        map_validation(self.0.transaction(|tx|{session_project(tx,session_id)?;let now=now_ms();for entry in entries{let encoded=serde_json::to_string(entry).map_err(|error|rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;tx.execute(&format!("INSERT INTO {table}(browser_session_id,payload_json,created_at) VALUES (?1,?2,?3)"),params![session_id.as_bytes(),encoded,now])?;}tx.execute(&format!("DELETE FROM {table} WHERE browser_session_id=?1 AND id NOT IN (SELECT id FROM {table} WHERE browser_session_id=?1 ORDER BY id DESC LIMIT ?2)"),params![session_id.as_bytes(),MAX_HISTORY_ENTRIES])?;Ok(entries.len())}))
    }

    pub fn console(&self, session_id: Uuid, limit: usize) -> Result<Vec<Value>> {
        self.bounded_entries("browser_console_entries", session_id, limit)
    }
    pub fn network(&self, session_id: Uuid, limit: usize) -> Result<Vec<Value>> {
        self.bounded_entries("browser_network_entries", session_id, limit)
    }
    fn bounded_entries(&self, table: &str, session_id: Uuid, limit: usize) -> Result<Vec<Value>> {
        if !matches!(table, "browser_console_entries" | "browser_network_entries") {
            return Err(StorageError::Validation(
                "invalid browser history table".into(),
            ));
        }
        let limit = limit.clamp(1, MAX_HISTORY_ENTRIES as usize) as i64;
        self.0.read(|db|{let mut statement=db.prepare(&format!("SELECT payload_json FROM {table} WHERE browser_session_id=?1 ORDER BY id DESC LIMIT ?2"))?;statement.query_map(params![session_id.as_bytes(),limit],|row|json_value(row,0))?.collect()})
    }

    pub fn start_takeover(
        &self,
        session_id: Uuid,
        actor: &str,
        reason: Option<&str>,
    ) -> Result<BrowserTakeover> {
        validate_actor(actor)?;
        if reason.is_some_and(|value| value.len() > 4_096 || value.contains('\0')) {
            return Err(StorageError::Validation(
                "browser takeover reason is invalid".into(),
            ));
        }
        let id = Uuid::new_v4();
        map_validation(self.0.transaction(|tx|{let project_id=session_project(tx,session_id)?;let status:String=tx.query_row("SELECT status FROM browser_sessions WHERE id=?1",[session_id.as_bytes()],|row|row.get(0))?;if status!="running"{return Err(validation_error("browser takeover requires a running session"));}let now=now_ms();tx.execute("INSERT INTO browser_takeovers(id,browser_session_id,actor,reason,started_at) VALUES (?1,?2,?3,?4,?5)",params![id.as_bytes(),session_id.as_bytes(),actor,reason,now])?;record_event(tx,session_id,project_id,"takeover_started",actor,&json!({"takeoverId":id,"reason":reason}))?;Ok(())}))?;
        self.takeover(id)?
            .ok_or_else(|| StorageError::Validation("browser takeover disappeared".into()))
    }

    pub fn stop_takeover(
        &self,
        session_id: Uuid,
        actor: &str,
        end_reason: &str,
    ) -> Result<Option<BrowserTakeover>> {
        validate_actor(actor)?;
        map_validation(self.0.transaction(|tx|{let project_id=session_project(tx,session_id)?;let current:Option<Uuid>=tx.query_row("SELECT id FROM browser_takeovers WHERE browser_session_id=?1 AND ended_at IS NULL",[session_id.as_bytes()],|row|uuid(row,0)).optional()?;let Some(id)=current else{return Ok(None)};let now=now_ms();tx.execute("UPDATE browser_takeovers SET ended_at=?2,end_reason=?3 WHERE id=?1",params![id.as_bytes(),now,end_reason])?;record_event(tx,session_id,project_id,"takeover_stopped",actor,&json!({"takeoverId":id,"reason":end_reason}))?;Ok(Some(id))}))?.map_or(Ok(None),|id|self.takeover(id))
    }

    pub fn active_takeover(&self, session_id: Uuid) -> Result<Option<BrowserTakeover>> {
        self.0.read(|db|db.query_row("SELECT id,browser_session_id,actor,reason,started_at,ended_at,end_reason FROM browser_takeovers WHERE browser_session_id=?1 AND ended_at IS NULL",[session_id.as_bytes()],row_takeover).optional())
    }
    fn takeover(&self, id: Uuid) -> Result<Option<BrowserTakeover>> {
        self.0.read(|db|db.query_row("SELECT id,browser_session_id,actor,reason,started_at,ended_at,end_reason FROM browser_takeovers WHERE id=?1",[id.as_bytes()],row_takeover).optional())
    }

    pub fn history(&self, session_id: Uuid) -> Result<Vec<BrowserHistoryEvent>> {
        self.history_limited(session_id, MAX_HISTORY_ENTRIES as usize)
    }

    pub fn history_limited(
        &self,
        session_id: Uuid,
        limit: usize,
    ) -> Result<Vec<BrowserHistoryEvent>> {
        let limit = limit.clamp(1, MAX_HISTORY_ENTRIES as usize) as i64;
        self.0.read(|db|{
            let mut statement=db.prepare(
                "SELECT id,browser_session_id,project_id,kind,actor,payload_json,created_at FROM browser_history WHERE browser_session_id=?1 ORDER BY id DESC LIMIT ?2",
            )?;
            let mut rows: Vec<BrowserHistoryEvent> = statement
                .query_map(params![session_id.as_bytes(), limit], row_history)?
                .collect::<rusqlite::Result<_>>()?;
            rows.reverse();
            Ok(rows)
        })
    }

    pub fn record_event(
        &self,
        session_id: Uuid,
        kind: &str,
        actor: &str,
        payload: &Value,
    ) -> Result<()> {
        validate_actor(actor)?;
        map_validation(self.0.transaction(|tx| {
            let project_id = session_project(tx, session_id)?;
            record_event(tx, session_id, project_id, kind, actor, payload)
        }))
    }
}

fn validate_new_session(input: &NewDurableBrowserSession, actor: &str) -> Result<()> {
    validate_actor(actor)?;
    if input.profile_path.trim().is_empty()
        || input.profile_path.len() > 32_768
        || input.profile_path.contains('\0')
    {
        return Err(StorageError::Validation(
            "browser profile path is invalid".into(),
        ));
    }
    if !matches!(input.network_policy.as_str(), "loopback" | "network") {
        return Err(StorageError::Validation(
            "browser network policy must be 'loopback' or 'network'".into(),
        ));
    }
    Ok(())
}
fn validate_actor(actor: &str) -> Result<()> {
    if actor.trim().is_empty() {
        Err(StorageError::Validation(
            "browser actor cannot be empty".into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(StorageError::Validation(
            "invalid browser artifact hash".into(),
        ))
    }
}

fn browser_scope(
    tx: &Transaction<'_>,
    project_id: Uuid,
    requested_task: Option<Uuid>,
    dev_server: Option<Uuid>,
) -> rusqlite::Result<(Option<Uuid>, Option<Uuid>)> {
    let mut task_id = requested_task;
    let mut worktree_id = None;
    if let Some(task) = requested_task {
        let (project, worktree) = tx
            .query_row(
                "SELECT project_id,worktree_id FROM tasks WHERE id=?1",
                [task.as_bytes()],
                |row| Ok((optional_uuid(row, 0)?, optional_uuid(row, 1)?)),
            )
            .optional()?
            .ok_or_else(|| validation_error("browser task does not exist"))?;
        if project != Some(project_id) {
            return Err(validation_error(
                "browser task belongs to a different project",
            ));
        }
        worktree_id = worktree;
    }
    if let Some(instance) = dev_server {
        let (project,task,worktree,status)=tx.query_row("SELECT project_id,task_id,worktree_id,status FROM dev_server_instances WHERE id=?1",[instance.as_bytes()],|row|Ok((uuid(row,0)?,optional_uuid(row,1)?,optional_uuid(row,2)?,row.get::<_,String>(3)?))).optional()?.ok_or_else(||validation_error("browser development server does not exist"))?;
        if project != project_id {
            return Err(validation_error(
                "browser development server belongs to a different project",
            ));
        }
        if status != "running" {
            return Err(validation_error(
                "browser development server is not running",
            ));
        }
        if task_id.is_some() && task_id != task {
            return Err(validation_error(
                "browser task and development server task scope differ",
            ));
        }
        if worktree_id.is_some() && worktree.is_some() && worktree_id != worktree {
            return Err(validation_error(
                "browser task and development server worktree differ",
            ));
        }
        task_id = task_id.or(task);
        worktree_id = worktree_id.or(worktree);
    }
    Ok((task_id, worktree_id))
}
fn session_project(tx: &Transaction<'_>, id: Uuid) -> rusqlite::Result<Uuid> {
    tx.query_row(
        "SELECT project_id FROM browser_sessions WHERE id=?1 AND project_id IS NOT NULL",
        [id.as_bytes()],
        |row| uuid(row, 0),
    )
    .optional()?
    .ok_or_else(|| validation_error("durable browser session does not exist"))
}
fn record_event(
    tx: &Transaction<'_>,
    session: Uuid,
    project: Uuid,
    kind: &str,
    actor: &str,
    payload: &Value,
) -> rusqlite::Result<()> {
    tx.execute("INSERT INTO browser_history(browser_session_id,project_id,kind,actor,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",params![session.as_bytes(),project.as_bytes(),kind,actor,serde_json::to_string(payload).map_err(|error|rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?,now_ms()])?;
    tx.execute(
        "DELETE FROM browser_history WHERE browser_session_id=?1 AND id NOT IN (SELECT id FROM browser_history WHERE browser_session_id=?1 ORDER BY id DESC LIMIT ?2)",
        params![session.as_bytes(), MAX_HISTORY_ENTRIES],
    )?;
    Ok(())
}
fn validation_error(message: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{VALIDATION_PREFIX}{}", message.into()))
}
fn json_error(operation: &'static str) -> impl FnOnce(serde_json::Error) -> StorageError {
    move |error| {
        StorageError::database(
            operation,
            rusqlite::Error::ToSqlConversionFailure(Box::new(error)),
        )
    }
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
fn json_value(row: &Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}
fn row_session(row: &Row<'_>) -> rusqlite::Result<DurableBrowserSession> {
    Ok(DurableBrowserSession {
        id: uuid(row, 0)?,
        project_id: uuid(row, 1)?,
        task_id: optional_uuid(row, 2)?,
        worktree_id: optional_uuid(row, 3)?,
        dev_server_instance_id: optional_uuid(row, 4)?,
        profile_id: uuid(row, 5)?,
        status: row.get(6)?,
        service_session_id: row.get(7)?,
        service_version: row.get(8)?,
        service_protocol: row.get(9)?,
        network_policy: row.get(10)?,
        failure: row.get(11)?,
        started_at: row.get(12)?,
        updated_at: row.get(13)?,
        ended_at: row.get(14)?,
    })
}
fn row_profile(row: &Row<'_>) -> rusqlite::Result<BrowserProfile> {
    Ok(BrowserProfile {
        id: uuid(row, 0)?,
        project_id: uuid(row, 1)?,
        worktree_id: optional_uuid(row, 2)?,
        task_id: optional_uuid(row, 3)?,
        path: row.get(4)?,
        persistent: row.get(5)?,
        status: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        released_at: row.get(9)?,
    })
}
fn row_tab(row: &Row<'_>) -> rusqlite::Result<BrowserTab> {
    Ok(BrowserTab {
        id: uuid(row, 0)?,
        browser_session_id: uuid(row, 1)?,
        service_tab_id: row.get(2)?,
        url: row.get(3)?,
        title: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        closed_at: row.get(8)?,
    })
}
fn row_observation(row: &Row<'_>) -> rusqlite::Result<BrowserObservation> {
    Ok(BrowserObservation {
        id: uuid(row, 0)?,
        browser_session_id: uuid(row, 1)?,
        tab_id: optional_uuid(row, 2)?,
        kind: row.get(3)?,
        artifact_hash: row.get(4)?,
        mime_type: row.get(5)?,
        size_bytes: row.get(6)?,
        metadata: json_value(row, 7)?,
        created_at: row.get(8)?,
    })
}
fn row_history(row: &Row<'_>) -> rusqlite::Result<BrowserHistoryEvent> {
    Ok(BrowserHistoryEvent {
        id: row.get(0)?,
        browser_session_id: uuid(row, 1)?,
        project_id: uuid(row, 2)?,
        kind: row.get(3)?,
        actor: row.get(4)?,
        payload: json_value(row, 5)?,
        created_at: row.get(6)?,
    })
}
fn row_takeover(row: &Row<'_>) -> rusqlite::Result<BrowserTakeover> {
    Ok(BrowserTakeover {
        id: uuid(row, 0)?,
        browser_session_id: uuid(row, 1)?,
        actor: row.get(2)?,
        reason: row.get(3)?,
        started_at: row.get(4)?,
        ended_at: row.get(5)?,
        end_reason: row.get(6)?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{NewDevServerConfig, NewProject, NewTask};

    #[test]
    fn lifecycle_tabs_observations_takeover_and_bounded_logs_round_trip() {
        let database = Database::open_in_memory().unwrap();
        let project = database.projects().create(&NewProject::new("Web")).unwrap();
        let mut task = NewTask::new("Observe");
        task.project_id = Some(project.id);
        let task = database.tasks().create(&task).unwrap();
        let mut input = NewDurableBrowserSession::new(project.id, "C:/profiles/one");
        input.task_id = Some(task.id);
        let repository = database.durable_browsers();
        let session = repository.prepare_session(&input, "test").unwrap();
        assert_eq!(session.status, "starting");
        let session = repository
            .mark_running(session.id, "svc-1", "0.1.0", 1, "test")
            .unwrap()
            .unwrap();
        assert_eq!(session.status, "running");
        let tab = repository
            .upsert_tab(
                session.id,
                "tab-1",
                Some("http://localhost:3000"),
                Some("Web"),
                "test",
            )
            .unwrap();
        assert_eq!(repository.list_tabs(session.id).unwrap().len(), 1);
        repository
            .append_console(
                session.id,
                &(0..1005)
                    .map(|index| json!({"index":index}))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(repository.console(session.id, 1000).unwrap().len(), 1000);
        let observation = repository
            .record_observation(
                session.id,
                Some(tab.id),
                "screenshot",
                &"a".repeat(64),
                "image/png",
                4,
                &json!({}),
                "test",
            )
            .unwrap();
        assert_eq!(observation.kind, "screenshot");
        repository
            .start_takeover(session.id, "user", Some("inspect"))
            .unwrap();
        assert!(repository.active_takeover(session.id).unwrap().is_some());
        repository
            .stop_takeover(session.id, "user", "done")
            .unwrap();
        repository.begin_stop(session.id, "test").unwrap();
        let session = repository
            .mark_stopped(session.id, "test")
            .unwrap()
            .unwrap();
        assert_eq!(session.status, "stopped");
        assert_eq!(
            repository
                .profile(session.profile_id)
                .unwrap()
                .unwrap()
                .status,
            "released"
        );
        assert!(repository.history(session.id).unwrap().len() >= 7);
    }

    #[test]
    fn rejects_cross_project_task_and_invalid_transitions() {
        let database = Database::open_in_memory().unwrap();
        let first = database.projects().create(&NewProject::new("One")).unwrap();
        let second = database.projects().create(&NewProject::new("Two")).unwrap();
        let mut task = NewTask::new("Other");
        task.project_id = Some(second.id);
        let task = database.tasks().create(&task).unwrap();
        let mut input = NewDurableBrowserSession::new(first.id, "C:/profiles/two");
        input.task_id = Some(task.id);
        assert!(
            database
                .durable_browsers()
                .prepare_session(&input, "test")
                .is_err()
        );
        let input = NewDurableBrowserSession::new(first.id, "C:/profiles/three");
        let session = database
            .durable_browsers()
            .prepare_session(&input, "test")
            .unwrap();
        assert!(
            database
                .durable_browsers()
                .mark_stopped(session.id, "test")
                .is_err()
        );
    }

    #[test]
    fn rejects_task_scoped_browser_for_project_scoped_development_server() {
        let database = Database::open_in_memory().unwrap();
        let project = database.projects().create(&NewProject::new("Web")).unwrap();
        let mut task = NewTask::new("Task preview");
        task.project_id = Some(project.id);
        let task = database.tasks().create(&task).unwrap();
        let config = database
            .dev_servers()
            .save_config(&NewDevServerConfig::new(project.id, "web", "serve", "."))
            .unwrap();
        let instance = database
            .dev_servers()
            .prepare_start(config.id, None, "test")
            .unwrap();
        database
            .dev_servers()
            .mark_running(
                instance.id,
                Some(42),
                "http://127.0.0.1:3000",
                &json!({}),
                "test",
            )
            .unwrap();
        let mut input = NewDurableBrowserSession::new(project.id, "C:/profiles/mismatch");
        input.task_id = Some(task.id);
        input.dev_server_instance_id = Some(instance.id);
        assert!(
            database
                .durable_browsers()
                .prepare_session(&input, "test")
                .is_err()
        );
    }

    #[test]
    fn recovery_interrupts_sessions_takeovers_and_profiles() {
        let database = Database::open_in_memory().unwrap();
        let project = database
            .projects()
            .create(&NewProject::new("Recovery"))
            .unwrap();
        let input = NewDurableBrowserSession::new(project.id, "C:/profiles/recovery");
        let repository = database.durable_browsers();
        let session = repository.prepare_session(&input, "test").unwrap();
        repository
            .mark_running(session.id, "service", "0.1.0", 1, "test")
            .unwrap();
        repository.start_takeover(session.id, "user", None).unwrap();
        let report = database.recover_interrupted().unwrap();
        assert_eq!(report.interrupted_browsers, 1);
        assert_eq!(report.orphaned_browser_profiles, 1);
        assert_eq!(
            repository.session(session.id).unwrap().unwrap().status,
            "interrupted"
        );
        assert_eq!(
            repository
                .profile(input.profile_id)
                .unwrap()
                .unwrap()
                .status,
            "orphaned"
        );
        assert!(repository.active_takeover(session.id).unwrap().is_none());
    }

    #[test]
    fn orphaning_session_closes_takeover_but_preserves_profile_ownership() {
        let database = Database::open_in_memory().unwrap();
        let project = database.projects().create(&NewProject::new("Web")).unwrap();
        let input = NewDurableBrowserSession::new(project.id, "C:/profiles/orphaned");
        let repository = database.durable_browsers();
        let session = repository.prepare_session(&input, "test").unwrap();
        repository
            .mark_running(session.id, "service", "0.1.0", 1, "test")
            .unwrap();
        repository
            .start_takeover(session.id, "user", Some("inspect"))
            .unwrap();

        let orphaned = repository
            .mark_orphaned(session.id, "transport closed", "service")
            .unwrap()
            .unwrap();

        assert_eq!(orphaned.status, "orphaned");
        assert!(orphaned.ended_at.is_some());
        assert!(repository.active_takeover(session.id).unwrap().is_none());
        assert_eq!(
            repository
                .profile(session.profile_id)
                .unwrap()
                .unwrap()
                .status,
            "active"
        );
    }
}
