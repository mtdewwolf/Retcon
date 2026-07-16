//! Observable, cancellable background-job supervision.

#![allow(missing_docs)]

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{CoreError, ErrorCode};
use retcon_storage::Database;

const FINISHED_RETENTION_DAYS: i64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Stuck,
}

impl JobStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Stuck => "stuck",
        }
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    User,
    Provider,
    Process,
    Timeout,
    Cancelled,
    Internal,
}

impl FailureClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Provider => "provider",
            Self::Process => "process",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JobSnapshot {
    pub id: Uuid,
    pub owner: String,
    pub name: String,
    pub status: JobStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub attempts: u32,
    pub max_attempts: u32,
    pub timeout_ms: u64,
    pub failure_class: Option<FailureClass>,
    pub failure: Option<String>,
    pub child_process_ids: Vec<u32>,
    pub elapsed_ms: u64,
}

struct JobEntry {
    snapshot: JobSnapshot,
    cancel: watch::Sender<bool>,
    handle: Option<JoinHandle<()>>,
    dirty: bool,
}

#[derive(Clone)]
pub struct JobSupervisor {
    inner: Arc<Mutex<HashMap<Uuid, JobEntry>>>,
    storage: Option<Database>,
}

impl Default for JobSupervisor {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            storage: None,
        }
    }
}

impl JobSupervisor {
    pub fn with_storage(storage: Database) -> Self {
        let supervisor = Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            storage: Some(storage.clone()),
        };
        supervisor.evict_stale_records();
        supervisor
    }

    pub fn spawn<F, Fut>(
        &self,
        owner: impl Into<String>,
        name: impl Into<String>,
        timeout: Duration,
        max_attempts: u32,
        operation: F,
    ) -> Uuid
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), CoreError>> + Send + 'static,
    {
        let id = Uuid::new_v4();
        let (cancel, mut cancelled) = watch::channel(false);
        let snapshot = JobSnapshot {
            id,
            owner: owner.into(),
            name: name.into(),
            status: JobStatus::Queued,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            attempts: 0,
            max_attempts: max_attempts.max(1),
            timeout_ms: timeout.as_millis().try_into().unwrap_or(u64::MAX),
            failure_class: None,
            failure: None,
            child_process_ids: Vec::new(),
            elapsed_ms: 0,
        };
        if let Ok(mut jobs) = self.inner.lock() {
            jobs.insert(
                id,
                JobEntry {
                    snapshot,
                    cancel,
                    handle: None,
                    dirty: true,
                },
            );
        }
        self.flush_persist(id, true);
        let supervisor = self.clone();
        let operation = Arc::new(operation);
        let handle = tokio::spawn(async move {
            supervisor.update(id, |job| {
                job.status = JobStatus::Running;
                job.started_at = Some(Utc::now());
            });
            let mut last_error = None;
            for attempt in 1..=max_attempts.max(1) {
                supervisor.update(id, |job| job.attempts = attempt);
                let run = operation();
                tokio::select! {
                    changed = cancelled.changed() => {
                        if changed.is_ok() && *cancelled.borrow() {
                            supervisor.finish(id, JobStatus::Cancelled, Some(FailureClass::Cancelled), Some("cancelled by user".into()));
                            return;
                        }
                    }
                    result = tokio::time::timeout(timeout, run) => match result {
                        Ok(Ok(())) => { supervisor.finish(id, JobStatus::Succeeded, None, None); return; }
                        Ok(Err(error)) => { last_error = Some((classify(&error), error.to_string())); }
                        Err(_) => { supervisor.finish(id, JobStatus::TimedOut, Some(FailureClass::Timeout), Some(format!("exceeded {} ms", timeout.as_millis()))); return; }
                    }
                }
            }
            let (class, message) = last_error
                .unwrap_or((FailureClass::Internal, "job failed without an error".into()));
            supervisor.finish(id, JobStatus::Failed, Some(class), Some(message));
        });
        if let Ok(mut jobs) = self.inner.lock()
            && let Some(job) = jobs.get_mut(&id)
        {
            job.handle = Some(handle);
        }
        id
    }

    pub fn list(&self, offset: usize, limit: usize) -> Vec<JobSnapshot> {
        self.evict_finished_from_memory();
        let mut jobs = self
            .inner
            .lock()
            .map(|m| {
                m.values()
                    .map(|e| with_elapsed(e.snapshot.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        jobs.into_iter().skip(offset).take(limit.min(500)).collect()
    }

    pub fn get(&self, id: Uuid) -> Option<JobSnapshot> {
        self.inner
            .lock()
            .ok()?
            .get(&id)
            .map(|e| with_elapsed(e.snapshot.clone()))
    }

    pub fn cancel(&self, id: Uuid) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|m| m.get(&id).map(|e| e.cancel.send_replace(true)))
            .is_some()
    }

    pub fn force_stop(&self, id: Uuid) -> bool {
        let stopped = self
            .inner
            .lock()
            .ok()
            .and_then(|mut m| {
                m.get_mut(&id).map(|entry| {
                    entry.cancel.send_replace(true);
                    if let Some(handle) = &entry.handle {
                        handle.abort();
                    }
                })
            })
            .is_some();
        if stopped {
            self.finish(
                id,
                JobStatus::Cancelled,
                Some(FailureClass::Cancelled),
                Some("force-stopped".into()),
            );
        }
        stopped
    }

    pub fn detect_stuck(&self, threshold: Duration) -> Vec<Uuid> {
        let now = Utc::now();
        let mut stuck = Vec::new();
        if let Ok(mut jobs) = self.inner.lock() {
            for (id, entry) in jobs.iter_mut() {
                if entry.snapshot.status == JobStatus::Running
                    && entry.snapshot.started_at.is_some_and(|s| {
                        now.signed_duration_since(s)
                            .to_std()
                            .is_ok_and(|d| d > threshold)
                    })
                {
                    entry.snapshot.status = JobStatus::Stuck;
                    entry.dirty = true;
                    stuck.push(*id);
                }
            }
        }
        for id in &stuck {
            self.flush_persist(*id, true);
        }
        stuck
    }

    pub fn track_child_process(&self, id: Uuid, process_id: u32) -> bool {
        let mut found = false;
        self.update(id, |job| {
            if !job.child_process_ids.contains(&process_id) {
                job.child_process_ids.push(process_id);
            }
            found = true;
        });
        found
    }

    pub fn reconcile_orphans(&self) -> Vec<Uuid> {
        let mut orphaned = Vec::new();
        if let Ok(mut jobs) = self.inner.lock() {
            for (id, entry) in jobs.iter_mut() {
                if matches!(
                    entry.snapshot.status,
                    JobStatus::Queued | JobStatus::Running | JobStatus::Stuck
                ) && entry.handle.as_ref().is_some_and(JoinHandle::is_finished)
                {
                    entry.snapshot.status = JobStatus::Failed;
                    entry.snapshot.failure_class = Some(FailureClass::Internal);
                    entry.snapshot.failure =
                        Some("supervised task exited without reporting completion".into());
                    entry.snapshot.finished_at = Some(Utc::now());
                    entry.dirty = true;
                    orphaned.push(*id);
                }
            }
        }
        for id in &orphaned {
            self.flush_persist(*id, true);
        }
        orphaned
    }

    pub fn shutdown(&self) {
        let ids: Vec<_> = self
            .list(0, usize::MAX)
            .into_iter()
            .filter(|j| {
                matches!(
                    j.status,
                    JobStatus::Queued | JobStatus::Running | JobStatus::Stuck
                )
            })
            .map(|j| j.id)
            .collect();
        for id in ids {
            self.force_stop(id);
        }
    }

    fn update(&self, id: Uuid, action: impl FnOnce(&mut JobSnapshot)) {
        if let Ok(mut jobs) = self.inner.lock()
            && let Some(job) = jobs.get_mut(&id)
        {
            action(&mut job.snapshot);
            job.dirty = true;
        }
        self.flush_persist(id, false);
    }

    fn finish(
        &self,
        id: Uuid,
        status: JobStatus,
        class: Option<FailureClass>,
        failure: Option<String>,
    ) {
        self.update(id, |job| {
            job.status = status;
            job.failure_class = class;
            job.failure = failure;
            job.finished_at = Some(Utc::now());
        });
        self.flush_persist(id, true);
        self.evict_stale_records();
    }

    fn flush_persist(&self, id: Uuid, force: bool) {
        let snapshot = {
            let Ok(mut jobs) = self.inner.lock() else {
                return;
            };
            let Some(entry) = jobs.get_mut(&id) else {
                return;
            };
            if !force && !entry.dirty {
                return;
            }
            if !force && !entry.snapshot.status.is_terminal() {
                return;
            }
            entry.dirty = false;
            Some(entry.snapshot.clone())
        };
        let Some(job) = snapshot else {
            return;
        };
        self.persist_snapshot(&job);
    }

    fn persist_snapshot(&self, job: &JobSnapshot) {
        let Some(storage) = &self.storage else {
            return;
        };
        let status = job.status.as_str();
        let failure_class = job.failure_class.map(FailureClass::as_str);
        let children =
            serde_json::to_string(&job.child_process_ids).unwrap_or_else(|_| "[]".into());
        let created = job.created_at.timestamp_millis();
        let started = job.started_at.map(|value| value.timestamp_millis());
        let finished = job.finished_at.map(|value| value.timestamp_millis());
        let attempts = i64::from(job.attempts);
        let max_attempts = i64::from(job.max_attempts);
        let timeout = i64::try_from(job.timeout_ms).unwrap_or(i64::MAX);
        if let Err(error) = storage.read(|db| {
            let mut statement = db.prepare_cached(
                "INSERT INTO background_jobs (id,owner,name,status,created_at,started_at,finished_at,attempts,max_attempts,timeout_ms,failure_class,failure,child_process_ids_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13) ON CONFLICT(id) DO UPDATE SET status=excluded.status,started_at=excluded.started_at,finished_at=excluded.finished_at,attempts=excluded.attempts,failure_class=excluded.failure_class,failure=excluded.failure,child_process_ids_json=excluded.child_process_ids_json",
            )?;
            statement.execute(rusqlite::params![
                job.id.as_bytes(),
                job.owner,
                job.name,
                status,
                created,
                started,
                finished,
                attempts,
                max_attempts,
                timeout,
                failure_class,
                job.failure,
                children,
            ])
        }) {
            tracing::error!(%error, job.id = %job.id, "failed to persist supervised job");
        }
    }

    fn evict_finished_from_memory(&self) {
        let cutoff = Utc::now() - chrono::Duration::days(FINISHED_RETENTION_DAYS);
        if let Ok(mut jobs) = self.inner.lock() {
            jobs.retain(|_, entry| {
                !entry.snapshot.status.is_terminal()
                    || entry
                        .snapshot
                        .finished_at
                        .is_none_or(|finished| finished > cutoff)
            });
        }
    }

    fn evict_stale_records(&self) {
        self.evict_finished_from_memory();
        let Some(storage) = &self.storage else {
            return;
        };
        let cutoff =
            (Utc::now() - chrono::Duration::days(FINISHED_RETENTION_DAYS)).timestamp_millis();
        if let Err(error) = storage.execute(
            "DELETE FROM background_jobs WHERE finished_at IS NOT NULL AND finished_at < ?1",
            &[&cutoff],
        ) {
            tracing::warn!(%error, "failed to prune finished background jobs");
        }
    }
}

fn with_elapsed(mut snapshot: JobSnapshot) -> JobSnapshot {
    let end = snapshot.finished_at.unwrap_or_else(Utc::now);
    snapshot.elapsed_ms = snapshot
        .started_at
        .and_then(|start| end.signed_duration_since(start).to_std().ok())
        .map_or(0, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        });
    snapshot
}

fn classify(error: &CoreError) -> FailureClass {
    match error.code {
        ErrorCode::Shutdown => FailureClass::Cancelled,
        ErrorCode::Io => FailureClass::Process,
        ErrorCode::AuthenticationFailed | ErrorCode::InvalidRequest | ErrorCode::NotFound => {
            FailureClass::User
        }
        _ => FailureClass::Internal,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn jobs_complete_and_cancel() {
        let jobs = JobSupervisor::default();
        let completed = jobs.spawn("test", "complete", Duration::from_secs(1), 1, || async {
            Ok(())
        });
        let cancelled = jobs.spawn("test", "cancel", Duration::from_secs(5), 1, || async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(())
        });
        assert!(jobs.cancel(cancelled));
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(jobs.get(completed).unwrap().status, JobStatus::Succeeded);
        assert_eq!(jobs.get(cancelled).unwrap().status, JobStatus::Cancelled);
    }

    #[tokio::test]
    async fn job_transitions_are_persisted() {
        let database = Database::open_in_memory().unwrap();
        let jobs = JobSupervisor::with_storage(database.clone());
        let id = jobs.spawn("test", "durable", Duration::from_secs(1), 1, || async {
            Ok(())
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        let status: String = database
            .read(|db| {
                db.query_row(
                    "SELECT status FROM background_jobs WHERE id=?1",
                    [id.as_bytes()],
                    |row| row.get(0),
                )
            })
            .unwrap();
        assert_eq!(status, "succeeded");
    }

    #[tokio::test]
    async fn list_supports_pagination() {
        let jobs = JobSupervisor::default();
        for index in 0..5 {
            jobs.spawn(
                "test",
                format!("job-{index}"),
                Duration::from_millis(1),
                1,
                || async { Ok(()) },
            );
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(jobs.list(0, 2).len(), 2);
        assert_eq!(jobs.list(2, 10).len(), 3);
    }
}
