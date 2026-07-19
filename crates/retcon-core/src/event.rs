//! Durable, sequence-numbered core event bus backed by SQLite.

#![allow(missing_docs)]

use std::collections::VecDeque;
use std::mem::ManuallyDrop;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{CoreError, ErrorCode, ErrorSource};
use retcon_storage::Database;

const DEFAULT_MAX_EVENTS: usize = 10_000;
const DEFAULT_MAX_AGE_DAYS: i64 = 7;
const PRUNE_EVERY_N_INSERTS: u64 = 100;
/// Bound durable persist backlog; emitters block under backpressure instead of OOM.
const PERSIST_QUEUE_CAP: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    Session,
    Terminal,
    Git,
    File,
    Browser,
    Approval,
    Task,
    Agent,
    Job,
    System,
}

impl EventCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Terminal => "terminal",
            Self::Git => "git",
            Self::File => "file",
            Self::Browser => "browser",
            Self::Approval => "approval",
            Self::Task => "task",
            Self::Agent => "agent",
            Self::Job => "job",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub id: Uuid,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub category: EventCategory,
    pub kind: String,
    pub payload: Value,
}

struct EventLog {
    events: VecDeque<Arc<EventEnvelope>>,
    next_sequence: u64,
}

struct PersistJob {
    event: Arc<EventEnvelope>,
    encoded_payload: String,
}

enum PersistMessage {
    Job(PersistJob),
    /// Reply when the writer has drained earlier jobs (best-effort flush).
    Flush(mpsc::Sender<()>),
}

struct EventBusInner {
    log: Mutex<EventLog>,
    sender: broadcast::Sender<Arc<EventEnvelope>>,
    persist_tx: SyncSender<PersistMessage>,
    _writer: JoinHandle<()>,
}

/// Sequence-numbered event bus with in-memory replay and durable persistence.
pub struct EventBus {
    inner: ManuallyDrop<Arc<EventBusInner>>,
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            inner: ManuallyDrop::new(Arc::clone(&self.inner)),
        }
    }
}

impl Drop for EventBus {
    fn drop(&mut self) {
        // Take ownership of the Arc so we can join the writer when we are the last clone.
        #[allow(unsafe_code)]
        let inner = unsafe { ManuallyDrop::take(&mut self.inner) };
        if let Ok(inner) = Arc::try_unwrap(inner) {
            drop(inner.persist_tx);
            if let Err(error) = inner._writer.join() {
                tracing::warn!(?error, "event writer thread panicked during shutdown");
            }
        }
    }
}

impl EventBus {
    /// Open or create the event bus, hydrating only the retained in-memory window.
    pub fn open(database: Database) -> Result<Self, CoreError> {
        prune_database(&database)?;
        let events = load_recent_events(&database)?;
        let next_sequence = database.read(|db| {
            db.query_row(
                "SELECT coalesce(max(id), 0) + 1 FROM agent_events",
                [],
                |row| row.get::<_, u64>(0),
            )
        })?;
        let (sender, _) = broadcast::channel(1024);
        let (persist_tx, persist_rx) = mpsc::sync_channel(PERSIST_QUEUE_CAP);
        let writer_db = database.clone();
        let writer = thread::Builder::new()
            .name("event-writer".into())
            .spawn(move || event_writer_loop(persist_rx, writer_db))
            .map_err(|error| internal_io(error.to_string()))?;
        let inner = Arc::new(EventBusInner {
            log: Mutex::new(EventLog {
                events,
                next_sequence,
            }),
            sender,
            persist_tx,
            _writer: writer,
        });
        Ok(Self {
            inner: ManuallyDrop::new(inner),
        })
    }

    /// Persist and broadcast a durable event.
    pub fn emit(
        &self,
        kind: impl Into<String>,
        payload: Value,
    ) -> Result<EventEnvelope, CoreError> {
        self.emit_internal(kind, payload, true)
    }

    /// Broadcast an event without SQLite persistence (heartbeats, high-frequency noise).
    pub fn emit_volatile(
        &self,
        kind: impl Into<String>,
        payload: Value,
    ) -> Result<EventEnvelope, CoreError> {
        self.emit_internal(kind, payload, false)
    }

    fn emit_internal(
        &self,
        kind: impl Into<String>,
        payload: Value,
        durable: bool,
    ) -> Result<EventEnvelope, CoreError> {
        let kind = kind.into();
        let category = category_for(&kind);
        let encoded = serde_json::to_string(&payload).map_err(internal)?;
        let event = {
            let mut inner = self.inner.log.lock().map_err(|_| poisoned())?;
            let sequence = inner.next_sequence;
            inner.next_sequence = sequence.saturating_add(1);
            let event = Arc::new(EventEnvelope {
                id: Uuid::new_v4(),
                sequence,
                timestamp: Utc::now(),
                category,
                kind,
                payload,
            });
            inner.events.push_back(Arc::clone(&event));
            while inner.events.len() > DEFAULT_MAX_EVENTS {
                inner.events.pop_front();
            }
            event
        };

        if durable {
            self.inner
                .persist_tx
                .send(PersistMessage::Job(PersistJob {
                    event: Arc::clone(&event),
                    encoded_payload: encoded,
                }))
                .map_err(|error| internal_io(error.to_string()))?;
        }

        let _ = self.inner.sender.send(Arc::clone(&event));
        Ok((*event).clone())
    }

    /// Block until durable events queued before this call have been written (or the writer stopped).
    pub fn flush(&self) -> Result<(), CoreError> {
        let (tx, rx) = mpsc::channel();
        self.inner
            .persist_tx
            .send(PersistMessage::Flush(tx))
            .map_err(|error| internal_io(error.to_string()))?;
        rx.recv().map_err(|error| internal_io(error.to_string()))
    }

    pub fn replay(&self, after_sequence: u64, limit: usize) -> Vec<EventEnvelope> {
        self.inner
            .log
            .lock()
            .map(|inner| {
                let events = &inner.events;
                let start = partition_after(events, after_sequence);
                events
                    .iter()
                    .skip(start)
                    .take(limit.min(1000))
                    .map(|event| (**event).clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EventEnvelope>> {
        self.inner.sender.subscribe()
    }

    pub fn latest_sequence(&self) -> u64 {
        self.inner
            .log
            .lock()
            .ok()
            .map(|inner| inner.next_sequence.saturating_sub(1))
            .unwrap_or(0)
    }
}

fn partition_after(events: &VecDeque<Arc<EventEnvelope>>, after_sequence: u64) -> usize {
    if events.is_empty() {
        return 0;
    }
    let mut low = 0_usize;
    let mut high = events.len();
    while low < high {
        let mid = low + (high - low) / 2;
        if events[mid].sequence <= after_sequence {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    low
}

fn event_writer_loop(rx: mpsc::Receiver<PersistMessage>, database: Database) {
    let mut inserts_since_prune = 0_u64;
    while let Ok(message) = rx.recv() {
        match message {
            PersistMessage::Flush(reply) => {
                let _ = reply.send(());
            }
            PersistMessage::Job(job) => {
                if let Err(error) = database.read(|db| {
                    let mut statement = db.prepare_cached(
                        "INSERT INTO agent_events (id,event_id,category,kind,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                    )?;
                    statement.execute(rusqlite::params![
                        job.event.sequence as i64,
                        job.event.id.as_bytes(),
                        job.event.category.as_str(),
                        job.event.kind,
                        job.encoded_payload,
                        job.event.timestamp.timestamp_millis(),
                    ])
                }) {
                    tracing::error!(%error, sequence = job.event.sequence, "failed to persist event");
                }
                inserts_since_prune = inserts_since_prune.saturating_add(1);
                if inserts_since_prune.is_multiple_of(PRUNE_EVERY_N_INSERTS) {
                    if let Err(error) = prune_database(&database) {
                        tracing::warn!(%error, "failed to prune agent_events");
                    }
                    inserts_since_prune = 0;
                }
            }
        }
    }
}

fn prune_database(database: &Database) -> Result<(), CoreError> {
    let cutoff = (Utc::now() - Duration::days(DEFAULT_MAX_AGE_DAYS)).timestamp_millis();
    database
        .execute(
            "DELETE FROM agent_events WHERE created_at < ?1 OR id NOT IN (SELECT id FROM agent_events ORDER BY id DESC LIMIT ?2)",
            &[&cutoff, &(DEFAULT_MAX_EVENTS as i64)],
        )
        .map(|_| ())
        .map_err(storage)
}

fn load_recent_events(database: &Database) -> Result<VecDeque<Arc<EventEnvelope>>, CoreError> {
    database
        .read(|db| {
            let mut statement = db.prepare(
                "SELECT id,event_id,category,kind,payload_json,created_at FROM agent_events WHERE event_id IS NOT NULL ORDER BY id DESC LIMIT ?1",
            )?;
            let mut rows: Vec<EventEnvelope> = statement
                .query_map([DEFAULT_MAX_EVENTS as i64], |row| {
                    let sequence: i64 = row.get(0)?;
                    let id_bytes: Vec<u8> = row.get(1)?;
                    let kind: String = row.get(3)?;
                    let payload_text: String = row.get(4)?;
                    let timestamp_ms: i64 = row.get(5)?;
                    let id = Uuid::from_slice(&id_bytes).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            16,
                            rusqlite::types::Type::Blob,
                            Box::new(e),
                        )
                    })?;
                    let payload = serde_json::from_str(&payload_text).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            payload_text.len(),
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    let timestamp = DateTime::from_timestamp_millis(timestamp_ms).ok_or_else(
                        || rusqlite::Error::IntegralValueOutOfRange(5, timestamp_ms),
                    )?;
                    Ok(EventEnvelope {
                        id,
                        sequence: sequence as u64,
                        timestamp,
                        category: category_for(&kind),
                        kind,
                        payload,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows.reverse();
            Ok(rows.into_iter().map(Arc::new).collect())
        })
        .map_err(storage)
}

fn category_for(kind: &str) -> EventCategory {
    match kind.split('.').next().unwrap_or("system") {
        "session" => EventCategory::Session,
        "terminal" => EventCategory::Terminal,
        "git" => EventCategory::Git,
        "file" => EventCategory::File,
        "browser" => EventCategory::Browser,
        "approval" => EventCategory::Approval,
        "task" => EventCategory::Task,
        "agent" => EventCategory::Agent,
        "job" => EventCategory::Job,
        _ => EventCategory::System,
    }
}

fn poisoned() -> CoreError {
    CoreError::new(
        ErrorCode::Internal,
        ErrorSource::Storage,
        "The event store is unavailable.",
        "event bus mutex poisoned",
    )
}

fn internal(error: serde_json::Error) -> CoreError {
    CoreError::new(
        ErrorCode::Internal,
        ErrorSource::Storage,
        "Retcon could not encode an event.",
        error.to_string(),
    )
}

fn internal_io(message: String) -> CoreError {
    CoreError::new(
        ErrorCode::Internal,
        ErrorSource::Storage,
        "The event store is unavailable.",
        message,
    )
}

fn storage(error: retcon_storage::StorageError) -> CoreError {
    CoreError::new(
        ErrorCode::Internal,
        ErrorSource::Storage,
        "Retcon could not access the event store.",
        error.to_string(),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn events_persist_replay_and_continue_sequences() {
        let database = Database::open_in_memory().unwrap();
        let bus = EventBus::open(database.clone()).unwrap();
        let first = bus
            .emit("terminal.output", json!({"text":"hello"}))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        drop(bus);
        let reopened = EventBus::open(database).unwrap();
        let second = reopened.emit("system.ready", json!({})).unwrap();
        assert_eq!(first.sequence + 1, second.sequence);
        assert_eq!(reopened.replay(0, 10).len(), 2);
    }

    #[test]
    fn replay_uses_binary_search_for_large_logs() {
        let database = Database::open_in_memory().unwrap();
        let bus = EventBus::open(database).unwrap();
        for index in 0..200 {
            bus.emit("system.test", json!({ "index": index })).unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        let slice = bus.replay(150, 10);
        assert_eq!(slice.len(), 10);
        assert_eq!(slice[0].sequence, 151);
    }
}
