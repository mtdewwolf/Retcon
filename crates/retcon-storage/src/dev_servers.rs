//! Durable development-server configuration, lifecycle, logs, and port ownership.

#![allow(missing_docs)]

use std::collections::{BTreeMap, HashSet};

use rusqlite::{OptionalExtension, Row, Transaction, params};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::repositories::now_ms;
use crate::task_planning::map_validation;
use crate::{Database, Result, StorageError};

const VALIDATION_PREFIX: &str = "retcon_validation:";

#[derive(Clone, Debug)]
pub struct NewDevServerConfig {
    pub id: Uuid,
    pub project_id: Uuid,
    pub worktree_id: Option<Uuid>,
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub host: String,
    pub preferred_port: Option<i64>,
    pub auto_start: bool,
    pub env_allowlist: Vec<String>,
    pub environment: BTreeMap<String, String>,
}

impl NewDevServerConfig {
    #[must_use]
    pub fn new(
        project_id: Uuid,
        name: impl Into<String>,
        command: impl Into<String>,
        cwd: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            worktree_id: None,
            name: name.into(),
            command: command.into(),
            cwd: cwd.into(),
            host: "127.0.0.1".into(),
            preferred_port: None,
            auto_start: false,
            env_allowlist: Vec::new(),
            environment: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerConfig {
    pub id: Uuid,
    pub project_id: Uuid,
    pub worktree_id: Option<Uuid>,
    pub name: String,
    pub command: String,
    pub cwd: String,
    pub host: String,
    pub preferred_port: Option<i64>,
    pub auto_start: bool,
    pub env_allowlist: Vec<String>,
    pub environment_keys: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct DevServerLaunchConfig {
    pub config: DevServerConfig,
    pub environment: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerInstance {
    pub id: Uuid,
    pub config_id: Uuid,
    pub project_id: Uuid,
    pub worktree_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub port: i64,
    pub status: String,
    pub pid: Option<i64>,
    pub url: Option<String>,
    pub preview: Value,
    pub log_artifact_hash: Option<String>,
    pub failure: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub stopped_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerPortLease {
    pub id: Uuid,
    pub port: i64,
    pub project_id: Uuid,
    pub worktree_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub config_id: Uuid,
    pub instance_id: Option<Uuid>,
    pub status: String,
    pub leased_at: i64,
    pub released_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevServerEvent {
    pub id: i64,
    pub instance_id: Uuid,
    pub config_id: Uuid,
    pub project_id: Uuid,
    pub kind: String,
    pub actor: String,
    pub payload: Value,
    pub created_at: i64,
}

pub struct DevServerRepository<'a>(&'a Database);

impl Database {
    #[must_use]
    pub fn dev_servers(&self) -> DevServerRepository<'_> {
        DevServerRepository(self)
    }
}

impl DevServerRepository<'_> {
    pub fn save_config(&self, input: &NewDevServerConfig) -> Result<DevServerConfig> {
        validate_config(input)?;
        let allowlist = serde_json::to_string(&input.env_allowlist)
            .map_err(json_error("encode dev server allowlist"))?;
        let environment = serde_json::to_string(&input.environment)
            .map_err(json_error("encode dev server environment"))?;
        map_validation(self.0.transaction(|tx| {
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)", [input.project_id.as_bytes()], |row| row.get(0))?;
            if !exists { return Err(validation_error("dev server project does not exist")); }
            if let Some(worktree_id) = input.worktree_id {
                let project = tx.query_row("SELECT r.project_id FROM git_worktrees w JOIN repository_locations r ON r.id=w.repository_location_id WHERE w.id=?1", [worktree_id.as_bytes()], |row| uuid(row, 0)).optional()?.ok_or_else(|| validation_error("dev server worktree does not exist"))?;
                if project != input.project_id { return Err(validation_error("dev server worktree belongs to a different project")); }
            }
            let now = now_ms();
            let changed = tx.execute("INSERT INTO dev_server_configs(id,project_id,worktree_id,name,command,cwd,host,preferred_port,auto_start,env_allowlist_json,environment_json,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12) ON CONFLICT(id) DO UPDATE SET worktree_id=excluded.worktree_id,name=excluded.name,command=excluded.command,cwd=excluded.cwd,host=excluded.host,preferred_port=excluded.preferred_port,auto_start=excluded.auto_start,env_allowlist_json=excluded.env_allowlist_json,environment_json=excluded.environment_json,updated_at=excluded.updated_at WHERE project_id=excluded.project_id", params![input.id.as_bytes(), input.project_id.as_bytes(), optional_uuid_bytes(input.worktree_id), input.name.trim(), input.command.trim(), input.cwd.trim(), input.host.trim(), input.preferred_port, input.auto_start, allowlist, environment, now])?;
            if changed == 0 {
                return Err(validation_error(
                    "dev server config belongs to a different project",
                ));
            }
            Ok(())
        }))?;
        self.config(input.id)?
            .ok_or_else(|| StorageError::Validation("saved dev server config disappeared".into()))
    }

    pub fn config(&self, id: Uuid) -> Result<Option<DevServerConfig>> {
        self.0.read(|db| db.query_row("SELECT id,project_id,worktree_id,name,command,cwd,host,preferred_port,auto_start,env_allowlist_json,environment_json,created_at,updated_at FROM dev_server_configs WHERE id=?1", [id.as_bytes()], row_config).optional())
    }

    pub fn launch_config(&self, id: Uuid) -> Result<Option<DevServerLaunchConfig>> {
        self.0.read(|db| db.query_row("SELECT id,project_id,worktree_id,name,command,cwd,host,preferred_port,auto_start,env_allowlist_json,environment_json,created_at,updated_at FROM dev_server_configs WHERE id=?1", [id.as_bytes()], |row| {
            let config = row_config(row)?;
            let raw: String = row.get(10)?;
            let environment = serde_json::from_str(&raw).map_err(|error| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(error)))?;
            Ok(DevServerLaunchConfig { config, environment })
        }).optional())
    }

    pub fn list_configs(&self, project_id: Uuid) -> Result<Vec<DevServerConfig>> {
        self.0.read(|db| { let mut s=db.prepare("SELECT id,project_id,worktree_id,name,command,cwd,host,preferred_port,auto_start,env_allowlist_json,environment_json,created_at,updated_at FROM dev_server_configs WHERE project_id=?1 ORDER BY name,id")?; s.query_map([project_id.as_bytes()], row_config)?.collect() })
    }

    pub fn set_auto_start(&self, id: Uuid, enabled: bool) -> Result<Option<DevServerConfig>> {
        let changed = self.0.execute(
            "UPDATE dev_server_configs SET auto_start=?2,updated_at=?3 WHERE id=?1",
            &[&id.as_bytes(), &enabled, &now_ms()],
        )? > 0;
        if changed { self.config(id) } else { Ok(None) }
    }

    pub fn assign_port(
        &self,
        config_id: Uuid,
        task_id: Option<Uuid>,
        port: i64,
        actor: &str,
    ) -> Result<DevServerPortLease> {
        validate_port(port)?;
        validate_actor(actor)?;
        let lease_id = Uuid::new_v4();
        map_validation(self.0.transaction(|tx| {
            let (project_id, config_worktree_id) = config_identity(tx, config_id)?;
            let worktree_id =
                validate_task_scope(tx, task_id, project_id, config_worktree_id)?;
            let active_instance: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM dev_server_instances i WHERE i.config_id=?1 AND ifnull(hex(i.task_id),'')=ifnull(hex(?2),'') AND (i.status IN ('starting','running','stopping') OR (i.status='orphaned' AND EXISTS(SELECT 1 FROM dev_server_port_leases l WHERE l.instance_id=i.id AND l.status='active'))))",params![config_id.as_bytes(),optional_uuid_bytes(task_id)],|row|row.get(0))?;
            if active_instance {
                return Err(validation_error(
                    "cannot change the port of an active development server",
                ));
            }
            let occupied: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM dev_server_port_leases WHERE port=?1 AND status='active')", [port], |row| row.get(0))?;
            if occupied { return Err(validation_error(format!("port {port} is already leased"))); }
            let now=now_ms();
            tx.execute("UPDATE dev_server_port_leases SET status='released',released_at=?1 WHERE project_id=?2 AND ifnull(hex(worktree_id),'')=ifnull(hex(?3),'') AND ifnull(hex(task_id),'')=ifnull(hex(?4),'') AND config_id=?5 AND status='active'", params![now, project_id.as_bytes(), optional_uuid_bytes(worktree_id), optional_uuid_bytes(task_id), config_id.as_bytes()])?;
            tx.execute("INSERT INTO dev_server_port_leases(id,port,project_id,worktree_id,task_id,config_id,status,leased_at) VALUES (?1,?2,?3,?4,?5,?6,'active',?7)", params![lease_id.as_bytes(),port,project_id.as_bytes(),optional_uuid_bytes(worktree_id),optional_uuid_bytes(task_id),config_id.as_bytes(),now])?;
            Ok(())
        }))?;
        self.lease(lease_id)?
            .ok_or_else(|| StorageError::Validation("created port lease disappeared".into()))
    }

    pub fn release_port(&self, config_id: Uuid, task_id: Option<Uuid>) -> Result<bool> {
        map_validation(self.0.transaction(|tx| {
            let (project_id, config_worktree_id) = config_identity(tx, config_id)?;
            let worktree_id =
                validate_task_scope(tx, task_id, project_id, config_worktree_id)?;
            let active_instance: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM dev_server_instances i WHERE i.config_id=?1 AND ifnull(hex(i.task_id),'')=ifnull(hex(?2),'') AND (i.status IN ('starting','running','stopping') OR (i.status='orphaned' AND EXISTS(SELECT 1 FROM dev_server_port_leases l WHERE l.instance_id=i.id AND l.status='active'))))",params![config_id.as_bytes(),optional_uuid_bytes(task_id)],|row|row.get(0))?;
            if active_instance {
                return Err(validation_error(
                    "cannot release the port of an active development server",
                ));
            }
            let now = now_ms();
            Ok(tx.execute("UPDATE dev_server_port_leases SET status='released',released_at=?1 WHERE project_id=?2 AND ifnull(hex(worktree_id),'')=ifnull(hex(?3),'') AND ifnull(hex(task_id),'')=ifnull(hex(?4),'') AND config_id=?5 AND status='active'", params![now,project_id.as_bytes(),optional_uuid_bytes(worktree_id),optional_uuid_bytes(task_id),config_id.as_bytes()])?>0)
        }))
    }

    pub fn prepare_start(
        &self,
        config_id: Uuid,
        task_id: Option<Uuid>,
        actor: &str,
    ) -> Result<DevServerInstance> {
        validate_actor(actor)?;
        let instance_id = Uuid::new_v4();
        map_validation(self.0.transaction(|tx| {
            let (project_id,config_worktree_id)=config_identity(tx,config_id)?;
            let worktree_id=validate_task_scope(tx,task_id,project_id,config_worktree_id)?;
            let already_active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM dev_server_instances i WHERE i.config_id=?1 AND ifnull(hex(i.task_id),'')=ifnull(hex(?2),'') AND (i.status IN ('starting','running','stopping') OR (i.status='orphaned' AND EXISTS(SELECT 1 FROM dev_server_port_leases l WHERE l.instance_id=i.id AND l.status='active'))))",params![config_id.as_bytes(),optional_uuid_bytes(task_id)],|row|row.get(0))?;
            if already_active { return Err(validation_error("this development server already has an active instance")); }
            let preferred: Option<i64>=tx.query_row("SELECT preferred_port FROM dev_server_configs WHERE id=?1",[config_id.as_bytes()],|row|row.get(0))?;
            let existing: Option<(Vec<u8>,i64)>=tx.query_row("SELECT id,port FROM dev_server_port_leases WHERE project_id=?1 AND ifnull(hex(worktree_id),'')=ifnull(hex(?2),'') AND ifnull(hex(task_id),'')=ifnull(hex(?3),'') AND config_id=?4 AND status='active'",params![project_id.as_bytes(),optional_uuid_bytes(worktree_id),optional_uuid_bytes(task_id),config_id.as_bytes()],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
            let (lease_id,port)=if let Some(value)=existing { value } else {
                let port=find_available_port(tx,preferred)?; let id=Uuid::new_v4();
                tx.execute("INSERT INTO dev_server_port_leases(id,port,project_id,worktree_id,task_id,config_id,status,leased_at) VALUES (?1,?2,?3,?4,?5,?6,'active',?7)",params![id.as_bytes(),port,project_id.as_bytes(),optional_uuid_bytes(worktree_id),optional_uuid_bytes(task_id),config_id.as_bytes(),now_ms()])?;
                (id.as_bytes().to_vec(),port)
            };
            let now=now_ms();
            tx.execute("INSERT INTO dev_server_instances(id,config_id,project_id,worktree_id,task_id,port,status,created_at) VALUES (?1,?2,?3,?4,?5,?6,'starting',?7)",params![instance_id.as_bytes(),config_id.as_bytes(),project_id.as_bytes(),optional_uuid_bytes(worktree_id),optional_uuid_bytes(task_id),port,now])?;
            tx.execute("UPDATE dev_server_port_leases SET instance_id=?1 WHERE id=?2",params![instance_id.as_bytes(),lease_id])?;
            record_event(tx,instance_id,config_id,project_id,"starting",actor,&json!({"port":port}))?; Ok(())
        }))?;
        self.instance(instance_id)?
            .ok_or_else(|| StorageError::Validation("created server instance disappeared".into()))
    }

    pub fn mark_running(
        &self,
        id: Uuid,
        pid: Option<i64>,
        url: &str,
        preview: &Value,
        actor: &str,
    ) -> Result<Option<DevServerInstance>> {
        self.transition(
            id,
            "running",
            pid,
            Some(url),
            Some(preview),
            None,
            actor,
            false,
        )
    }
    pub fn begin_stop(&self, id: Uuid, actor: &str) -> Result<Option<DevServerInstance>> {
        self.transition(id, "stopping", None, None, None, None, actor, false)
    }
    pub fn mark_stopped(&self, id: Uuid, actor: &str) -> Result<Option<DevServerInstance>> {
        self.transition(id, "stopped", None, None, None, None, actor, true)
    }
    pub fn mark_failed(
        &self,
        id: Uuid,
        failure: &str,
        actor: &str,
    ) -> Result<Option<DevServerInstance>> {
        self.transition(id, "failed", None, None, None, Some(failure), actor, true)
    }

    pub fn mark_orphaned(
        &self,
        id: Uuid,
        failure: &str,
        actor: &str,
    ) -> Result<Option<DevServerInstance>> {
        self.transition(
            id,
            "orphaned",
            None,
            None,
            None,
            Some(failure),
            actor,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn transition(
        &self,
        id: Uuid,
        status: &str,
        pid: Option<i64>,
        url: Option<&str>,
        preview: Option<&Value>,
        failure: Option<&str>,
        actor: &str,
        release: bool,
    ) -> Result<Option<DevServerInstance>> {
        validate_actor(actor)?;
        let encoded = preview
            .map(serde_json::to_string)
            .transpose()
            .map_err(json_error("encode dev server preview"))?;
        map_validation(self.0.transaction(|tx| {
            let Some((config_id,project_id,current_status,port))=tx.query_row("SELECT config_id,project_id,status,port FROM dev_server_instances WHERE id=?1",[id.as_bytes()],|row|Ok((uuid(row,0)?,uuid(row,1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?))).optional()? else{return Ok(false)};
            let valid=match status {"running"=>current_status=="starting","stopping"=>matches!(current_status.as_str(),"starting"|"running"),"stopped"=>current_status=="stopping","failed"|"orphaned"=>matches!(current_status.as_str(),"starting"|"running"|"stopping"),_=>false};
            if !valid{return Err(validation_error(format!("cannot transition development server from '{current_status}' to '{status}'")));}
            if status == "running" {
                if pid.is_some_and(|value| value <= 0) {
                    return Err(validation_error(
                        "development server runtime returned an invalid process identifier",
                    ));
                }
                let Some(url) = url else {
                    return Err(validation_error(
                        "development server runtime did not return a preview URL",
                    ));
                };
                validate_local_preview_url(url, port)?;
            }
            let now=now_ms(); tx.execute("UPDATE dev_server_instances SET status=?2,pid=COALESCE(?3,pid),url=COALESCE(?4,url),preview_json=COALESCE(?5,preview_json),failure=?6,started_at=CASE WHEN ?2='running' THEN COALESCE(started_at,?7) ELSE started_at END,stopped_at=CASE WHEN ?2 IN ('stopped','failed') THEN ?7 ELSE stopped_at END WHERE id=?1",params![id.as_bytes(),status,pid,url,encoded,failure,now])?; if release {tx.execute("UPDATE dev_server_port_leases SET status='released',released_at=?2 WHERE instance_id=?1 AND status='active'",params![id.as_bytes(),now])?;} record_event(tx,id,config_id,project_id,status,actor,&json!({"failure":failure}))?; Ok(true)
        }))?;
        self.instance(id)
    }

    pub fn set_log_artifact(&self, id: Uuid, hash: &str) -> Result<bool> {
        validate_hash(hash)?;
        Ok(self.0.execute(
            "UPDATE dev_server_instances SET log_artifact_hash=?2 WHERE id=?1",
            &[&id.as_bytes(), &hash],
        )? > 0)
    }
    pub fn instance(&self, id: Uuid) -> Result<Option<DevServerInstance>> {
        self.0.read(|db|db.query_row("SELECT id,config_id,project_id,worktree_id,task_id,port,status,pid,url,preview_json,log_artifact_hash,failure,created_at,started_at,stopped_at FROM dev_server_instances WHERE id=?1",[id.as_bytes()],row_instance).optional())
    }
    pub fn list_instances(&self, project_id: Uuid) -> Result<Vec<DevServerInstance>> {
        self.0.read(|db|{let mut s=db.prepare("SELECT id,config_id,project_id,worktree_id,task_id,port,status,pid,url,preview_json,log_artifact_hash,failure,created_at,started_at,stopped_at FROM dev_server_instances WHERE project_id=?1 ORDER BY created_at DESC,id DESC")?;s.query_map([project_id.as_bytes()],row_instance)?.collect()})
    }
    pub fn history(&self, instance_id: Uuid) -> Result<Vec<DevServerEvent>> {
        self.0.read(|db|{let mut s=db.prepare("SELECT id,instance_id,config_id,project_id,kind,actor,payload_json,created_at FROM dev_server_events WHERE instance_id=?1 ORDER BY id")?;s.query_map([instance_id.as_bytes()],row_event)?.collect()})
    }
    fn lease(&self, id: Uuid) -> Result<Option<DevServerPortLease>> {
        self.0.read(|db|db.query_row("SELECT id,port,project_id,worktree_id,task_id,config_id,instance_id,status,leased_at,released_at FROM dev_server_port_leases WHERE id=?1",[id.as_bytes()],row_lease).optional())
    }
}

fn validate_config(c: &NewDevServerConfig) -> Result<()> {
    if c.name.trim().is_empty()
        || c.command.trim().is_empty()
        || c.cwd.trim().is_empty()
        || c.host.trim().is_empty()
    {
        return Err(StorageError::Validation(
            "dev server name, command, cwd, and host cannot be empty".into(),
        ));
    }
    if [
        c.name.as_str(),
        c.command.as_str(),
        c.cwd.as_str(),
        c.host.as_str(),
    ]
    .iter()
    .any(|value| value.contains('\0'))
        || c.environment.values().any(|value| value.contains('\0'))
    {
        return Err(StorageError::Validation(
            "dev server configuration cannot contain NUL bytes".into(),
        ));
    }
    if let Some(p) = c.preferred_port {
        validate_port(p)?;
    }
    let allowed: HashSet<_> = c.env_allowlist.iter().map(String::as_str).collect();
    if allowed.len() != c.env_allowlist.len() {
        return Err(StorageError::Validation(
            "environment allowlist keys must be unique".into(),
        ));
    }
    if c.env_allowlist.iter().any(|key| !valid_env_key(key)) {
        return Err(StorageError::Validation(
            "environment allowlist contains an invalid key".into(),
        ));
    }
    for key in c.environment.keys() {
        if !valid_env_key(key) || !allowed.contains(key.as_str()) {
            return Err(StorageError::Validation(format!(
                "environment key '{key}' is not allowlisted"
            )));
        }
    }
    Ok(())
}
fn valid_env_key(k: &str) -> bool {
    !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && !k.as_bytes()[0].is_ascii_digit()
}
fn validate_port(p: i64) -> Result<()> {
    if (1024..=65535).contains(&p) {
        Ok(())
    } else {
        Err(StorageError::Validation(
            "dev server port must be between 1024 and 65535".into(),
        ))
    }
}
fn validate_local_preview_url(url: &str, port: i64) -> rusqlite::Result<()> {
    let remainder = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| validation_error("development server preview URL must use HTTP or HTTPS"))?;
    let authority = remainder
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let expected = port.to_string();
    let valid_authority = authority == format!("localhost:{expected}")
        || authority == format!("127.0.0.1:{expected}")
        || authority == format!("[::1]:{expected}");
    if !valid_authority || url.chars().any(char::is_control) {
        return Err(validation_error(
            "development server preview URL must target the leased local port",
        ));
    }
    Ok(())
}
fn validate_actor(a: &str) -> Result<()> {
    if a.trim().is_empty() {
        Err(StorageError::Validation(
            "dev server actor cannot be empty".into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_hash(h: &str) -> Result<()> {
    if h.len() == 64
        && h.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(StorageError::Validation(
            "invalid dev server log artifact hash".into(),
        ))
    }
}
fn validation_error(m: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!("{VALIDATION_PREFIX}{}", m.into()))
}
fn config_identity(tx: &Transaction<'_>, id: Uuid) -> rusqlite::Result<(Uuid, Option<Uuid>)> {
    tx.query_row(
        "SELECT project_id,worktree_id FROM dev_server_configs WHERE id=?1",
        [id.as_bytes()],
        |row| Ok((uuid(row, 0)?, optional_uuid(row, 1)?)),
    )
    .optional()?
    .ok_or_else(|| validation_error("dev server config does not exist"))
}
fn validate_task_scope(
    tx: &Transaction<'_>,
    task: Option<Uuid>,
    project: Uuid,
    config_worktree: Option<Uuid>,
) -> rusqlite::Result<Option<Uuid>> {
    if let Some(id) = task {
        let (task_project, task_worktree, worktree_project) = tx
            .query_row(
                "SELECT t.project_id,t.worktree_id,r.project_id FROM tasks t LEFT JOIN git_worktrees w ON w.id=t.worktree_id LEFT JOIN repository_locations r ON r.id=w.repository_location_id WHERE t.id=?1",
                [id.as_bytes()],
                |row| Ok((optional_uuid(row, 0)?, optional_uuid(row, 1)?, optional_uuid(row, 2)?)),
            )
            .optional()?
            .ok_or_else(|| validation_error("dev server task does not exist"))?;
        if task_project != Some(project) {
            return Err(validation_error(
                "dev server task belongs to a different project",
            ));
        }
        if task_worktree.is_some() && worktree_project != Some(project) {
            return Err(validation_error(
                "dev server task worktree belongs to a different project",
            ));
        }
        if config_worktree.is_some() && task_worktree != config_worktree {
            return Err(validation_error(
                "dev server task belongs to a different worktree",
            ));
        }
        return Ok(config_worktree.or(task_worktree));
    }
    Ok(config_worktree)
}
fn find_available_port(tx: &Transaction<'_>, preferred: Option<i64>) -> rusqlite::Result<i64> {
    if let Some(p) = preferred {
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM dev_server_port_leases WHERE port=?1 AND status='active')",
            [p],
            |r| r.get(0),
        )?;
        if !used {
            return Ok(p);
        }
    }
    for p in 3000..10000 {
        let used: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM dev_server_port_leases WHERE port=?1 AND status='active')",
            [p],
            |r| r.get(0),
        )?;
        if !used {
            return Ok(p);
        }
    }
    Err(validation_error("no development server port is available"))
}
fn record_event(
    tx: &Transaction<'_>,
    instance: Uuid,
    config: Uuid,
    project: Uuid,
    kind: &str,
    actor: &str,
    payload: &Value,
) -> rusqlite::Result<()> {
    tx.execute("INSERT INTO dev_server_events(instance_id,config_id,project_id,kind,actor,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",params![instance.as_bytes(),config.as_bytes(),project.as_bytes(),kind,actor,serde_json::to_string(payload).map_err(|e|rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,now_ms()])?;
    Ok(())
}
fn json_error(op: &'static str) -> impl FnOnce(serde_json::Error) -> StorageError {
    move |e| StorageError::database(op, rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}
fn optional_uuid_bytes(v: Option<Uuid>) -> Option<Vec<u8>> {
    v.map(|id| id.as_bytes().to_vec())
}
fn uuid(r: &Row<'_>, i: usize) -> rusqlite::Result<Uuid> {
    let b: Vec<u8> = r.get(i)?;
    Uuid::from_slice(&b).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Blob, Box::new(e))
    })
}
fn optional_uuid(r: &Row<'_>, i: usize) -> rusqlite::Result<Option<Uuid>> {
    r.get::<_, Option<Vec<u8>>>(i)?
        .map(|b| {
            Uuid::from_slice(&b).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    i,
                    rusqlite::types::Type::Blob,
                    Box::new(e),
                )
            })
        })
        .transpose()
}
fn value(r: &Row<'_>, i: usize) -> rusqlite::Result<Value> {
    let s: String = r.get(i)?;
    serde_json::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}
fn row_config(r: &Row<'_>) -> rusqlite::Result<DevServerConfig> {
    let allow: Vec<String> = serde_json::from_str(&r.get::<_, String>(9)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let env: BTreeMap<String, String> =
        serde_json::from_str(&r.get::<_, String>(10)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
        })?;
    Ok(DevServerConfig {
        id: uuid(r, 0)?,
        project_id: uuid(r, 1)?,
        worktree_id: optional_uuid(r, 2)?,
        name: r.get(3)?,
        command: r.get(4)?,
        cwd: r.get(5)?,
        host: r.get(6)?,
        preferred_port: r.get(7)?,
        auto_start: r.get(8)?,
        env_allowlist: allow,
        environment_keys: env.keys().cloned().collect(),
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}
fn row_instance(r: &Row<'_>) -> rusqlite::Result<DevServerInstance> {
    Ok(DevServerInstance {
        id: uuid(r, 0)?,
        config_id: uuid(r, 1)?,
        project_id: uuid(r, 2)?,
        worktree_id: optional_uuid(r, 3)?,
        task_id: optional_uuid(r, 4)?,
        port: r.get(5)?,
        status: r.get(6)?,
        pid: r.get(7)?,
        url: r.get(8)?,
        preview: value(r, 9)?,
        log_artifact_hash: r.get(10)?,
        failure: r.get(11)?,
        created_at: r.get(12)?,
        started_at: r.get(13)?,
        stopped_at: r.get(14)?,
    })
}
fn row_lease(r: &Row<'_>) -> rusqlite::Result<DevServerPortLease> {
    Ok(DevServerPortLease {
        id: uuid(r, 0)?,
        port: r.get(1)?,
        project_id: uuid(r, 2)?,
        worktree_id: optional_uuid(r, 3)?,
        task_id: optional_uuid(r, 4)?,
        config_id: uuid(r, 5)?,
        instance_id: optional_uuid(r, 6)?,
        status: r.get(7)?,
        leased_at: r.get(8)?,
        released_at: r.get(9)?,
    })
}
fn row_event(r: &Row<'_>) -> rusqlite::Result<DevServerEvent> {
    Ok(DevServerEvent {
        id: r.get(0)?,
        instance_id: uuid(r, 1)?,
        config_id: uuid(r, 2)?,
        project_id: uuid(r, 3)?,
        kind: r.get(4)?,
        actor: r.get(5)?,
        payload: value(r, 6)?,
        created_at: r.get(7)?,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::{NewGitWorktree, NewProject, NewTask};
    #[test]
    fn config_ports_lifecycle_and_redaction_round_trip() {
        let db = Database::open_in_memory().unwrap();
        let p = db.projects().create(&NewProject::new("Web")).unwrap();
        let mut c = NewDevServerConfig::new(p.id, "web", "npm run dev", ".");
        c.env_allowlist = vec!["PUBLIC_API".into()];
        c.environment
            .insert("PUBLIC_API".into(), "secret-value".into());
        c.preferred_port = Some(4173);
        let c = db.dev_servers().save_config(&c).unwrap();
        assert_eq!(c.environment_keys, vec!["PUBLIC_API"]);
        assert!(!serde_json::to_string(&c).unwrap().contains("secret-value"));
        let i = db.dev_servers().prepare_start(c.id, None, "test").unwrap();
        assert_eq!(i.port, 4173);
        let i = db
            .dev_servers()
            .mark_running(
                i.id,
                Some(42),
                "http://127.0.0.1:4173",
                &json!({"title":"Web"}),
                "test",
            )
            .unwrap()
            .unwrap();
        assert_eq!(i.status, "running");
        assert!(
            db.dev_servers()
                .assign_port(c.id, None, 4174, "test")
                .is_err()
        );
        assert!(db.dev_servers().release_port(c.id, None).is_err());
        db.dev_servers().begin_stop(i.id, "test").unwrap();
        let i = db
            .dev_servers()
            .mark_stopped(i.id, "test")
            .unwrap()
            .unwrap();
        assert_eq!(i.status, "stopped");
        assert_eq!(db.dev_servers().history(i.id).unwrap().len(), 4);
        assert!(
            db.dev_servers()
                .assign_port(c.id, None, 4174, "test")
                .is_ok()
        );
    }
    #[test]
    fn rejects_unallowlisted_environment_and_duplicate_ports() {
        let db = Database::open_in_memory().unwrap();
        let p = db.projects().create(&NewProject::new("Web")).unwrap();
        let mut bad = NewDevServerConfig::new(p.id, "bad", "serve", ".");
        bad.environment.insert("TOKEN".into(), "x".into());
        assert!(db.dev_servers().save_config(&bad).is_err());
        let a = db
            .dev_servers()
            .save_config(&NewDevServerConfig::new(p.id, "a", "serve", "."))
            .unwrap();
        let b = db
            .dev_servers()
            .save_config(&NewDevServerConfig::new(p.id, "b", "serve", "."))
            .unwrap();
        db.dev_servers()
            .assign_port(a.id, None, 4000, "test")
            .unwrap();
        assert!(
            db.dev_servers()
                .assign_port(b.id, None, 4000, "test")
                .is_err()
        );
    }

    #[test]
    fn prevents_duplicate_instances_and_recovers_process_owned_state() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Web")).unwrap();
        let config = db
            .dev_servers()
            .save_config(&NewDevServerConfig::new(project.id, "web", "serve", "."))
            .unwrap();
        let reserved = db
            .dev_servers()
            .save_config(&NewDevServerConfig::new(
                project.id, "reserved", "serve", ".",
            ))
            .unwrap();
        db.dev_servers()
            .assign_port(reserved.id, None, 4100, "test")
            .unwrap();
        let instance = db
            .dev_servers()
            .prepare_start(config.id, None, "test")
            .unwrap();
        assert!(
            db.dev_servers()
                .prepare_start(config.id, None, "test")
                .is_err()
        );
        db.dev_servers()
            .mark_running(
                instance.id,
                Some(9),
                "http://127.0.0.1:3000",
                &json!({}),
                "test",
            )
            .unwrap();
        let report = db.recover_interrupted().unwrap();
        assert_eq!(report.orphaned_dev_servers, 1);
        assert_eq!(report.stale_port_leases, 1);
        assert_eq!(
            db.dev_servers()
                .instance(instance.id)
                .unwrap()
                .unwrap()
                .status,
            "orphaned"
        );
        assert_eq!(
            db.dev_servers().instance(instance.id).unwrap().unwrap().pid,
            None
        );
        assert_eq!(db.dev_servers().history(instance.id).unwrap().len(), 3);
        assert!(
            db.dev_servers()
                .assign_port(config.id, None, 3000, "test")
                .is_ok()
        );
        assert!(
            db.dev_servers()
                .assign_port(config.id, None, 4100, "test")
                .is_err()
        );
    }

    #[test]
    fn rejects_cross_project_config_overwrite() {
        let db = Database::open_in_memory().unwrap();
        let first = db.projects().create(&NewProject::new("First")).unwrap();
        let second = db.projects().create(&NewProject::new("Second")).unwrap();
        let mut input = NewDevServerConfig::new(first.id, "web", "serve", ".");
        let stored = db.dev_servers().save_config(&input).unwrap();
        input.project_id = second.id;
        input.name = "spoofed".into();
        assert!(db.dev_servers().save_config(&input).is_err());
        let unchanged = db.dev_servers().config(stored.id).unwrap().unwrap();
        assert_eq!(unchanged.project_id, first.id);
        assert_eq!(unchanged.name, "web");
    }

    #[test]
    fn enforces_task_worktree_scope_and_inherits_task_worktree() {
        let db = Database::open_in_memory().unwrap();
        let project = db.projects().create(&NewProject::new("Web")).unwrap();
        db.projects()
            .add_location(project.id, "C:/web-one", None)
            .unwrap();
        db.projects()
            .add_location(project.id, "C:/web-two", None)
            .unwrap();
        let first_location = db
            .projects()
            .location_id_by_path("C:/web-one")
            .unwrap()
            .unwrap();
        let second_location = db
            .projects()
            .location_id_by_path("C:/web-two")
            .unwrap()
            .unwrap();
        let first_worktree = db
            .git_worktrees()
            .create(&NewGitWorktree::new(first_location, "C:/web-one/wt"))
            .unwrap();
        let second_worktree = db
            .git_worktrees()
            .create(&NewGitWorktree::new(second_location, "C:/web-two/wt"))
            .unwrap();
        let mut task = NewTask::new("Run web");
        task.project_id = Some(project.id);
        task.worktree_id = Some(second_worktree.id);
        let task = db.tasks().create(&task).unwrap();

        let mut scoped = NewDevServerConfig::new(project.id, "scoped", "serve", ".");
        scoped.worktree_id = Some(first_worktree.id);
        let scoped = db.dev_servers().save_config(&scoped).unwrap();
        assert!(
            db.dev_servers()
                .prepare_start(scoped.id, Some(task.id), "test")
                .is_err()
        );

        let project_config = db
            .dev_servers()
            .save_config(&NewDevServerConfig::new(
                project.id, "project", "serve", ".",
            ))
            .unwrap();
        let instance = db
            .dev_servers()
            .prepare_start(project_config.id, Some(task.id), "test")
            .unwrap();
        assert_eq!(instance.worktree_id, Some(second_worktree.id));
    }
}
