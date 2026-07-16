//! Durable, sequence-numbered core event bus.

#![allow(missing_docs)]

use std::collections::{HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{CoreError, ErrorCode, ErrorSource};

const DEFAULT_MAX_EVENTS: usize = 10_000;
const DEFAULT_MAX_AGE_DAYS: i64 = 7;

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
    events: VecDeque<EventEnvelope>,
    ids: HashSet<Uuid>,
    next_sequence: u64,
    file: File,
    path: PathBuf,
}

pub struct EventBus {
    inner: Mutex<EventLog>,
    sender: broadcast::Sender<EventEnvelope>,
    max_events: usize,
    max_age: Duration,
}

impl EventBus {
    pub fn open(data_dir: &Path) -> Result<Self, CoreError> {
        std::fs::create_dir_all(data_dir)
            .map_err(|error| CoreError::io("create event data directory", error))?;
        let path = data_dir.join("events.jsonl");
        let mut events = VecDeque::new();
        let mut ids = HashSet::new();
        let cutoff = Utc::now() - Duration::days(DEFAULT_MAX_AGE_DAYS);
        if path.exists() {
            let reader = BufReader::new(
                File::open(&path).map_err(|error| CoreError::io("open event log", error))?,
            );
            for line in reader.lines() {
                let line = line.map_err(|error| CoreError::io("read event log", error))?;
                match serde_json::from_str::<EventEnvelope>(&line) {
                    Ok(event) if event.timestamp >= cutoff && ids.insert(event.id) => {
                        events.push_back(event);
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%error, "ignoring malformed persisted event"),
                }
            }
        }
        while events.len() > DEFAULT_MAX_EVENTS {
            if let Some(old) = events.pop_front() {
                ids.remove(&old.id);
            }
        }
        let next_sequence = events.back().map_or(1, |event| event.sequence + 1);
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| CoreError::io("open event log for append", error))?;
        let (sender, _) = broadcast::channel(1024);
        Ok(Self {
            inner: Mutex::new(EventLog {
                events,
                ids,
                next_sequence,
                file,
                path,
            }),
            sender,
            max_events: DEFAULT_MAX_EVENTS,
            max_age: Duration::days(DEFAULT_MAX_AGE_DAYS),
        })
    }

    pub fn emit(
        &self,
        kind: impl Into<String>,
        payload: Value,
    ) -> Result<EventEnvelope, CoreError> {
        let kind = kind.into();
        let category = category_for(&kind);
        let mut inner = self.inner.lock().map_err(|_| poisoned())?;
        let event = EventEnvelope {
            id: Uuid::new_v4(),
            sequence: inner.next_sequence,
            timestamp: Utc::now(),
            category,
            kind,
            payload,
        };
        inner.next_sequence += 1;
        let encoded = serde_json::to_vec(&event).map_err(internal)?;
        inner
            .file
            .write_all(&encoded)
            .and_then(|_| inner.file.write_all(b"\n"))
            .and_then(|_| inner.file.flush())
            .map_err(|error| CoreError::io("persist event", error))?;
        inner.ids.insert(event.id);
        inner.events.push_back(event.clone());
        let needs_compaction = {
            let EventLog { events, ids, .. } = &mut *inner;
            retain(events, ids, self.max_events, self.max_age)
        };
        if needs_compaction {
            compact(&mut inner)?;
        }
        drop(inner);
        let _ = self.sender.send(event.clone());
        Ok(event)
    }

    pub fn replay(&self, after_sequence: u64, limit: usize) -> Vec<EventEnvelope> {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .events
                    .iter()
                    .filter(|e| e.sequence > after_sequence)
                    .take(limit.min(1000))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope> {
        self.sender.subscribe()
    }

    pub fn latest_sequence(&self) -> u64 {
        self.inner
            .lock()
            .ok()
            .and_then(|i| i.events.back().map(|e| e.sequence))
            .unwrap_or(0)
    }
}

fn retain(
    events: &mut VecDeque<EventEnvelope>,
    ids: &mut HashSet<Uuid>,
    max: usize,
    age: Duration,
) -> bool {
    let cutoff = Utc::now() - age;
    let mut changed = false;
    while events.len() > max || events.front().is_some_and(|e| e.timestamp < cutoff) {
        if let Some(old) = events.pop_front() {
            ids.remove(&old.id);
            changed = true;
        }
    }
    changed
}

fn compact(inner: &mut EventLog) -> Result<(), CoreError> {
    let temp = inner.path.with_extension("jsonl.tmp");
    let mut file =
        File::create(&temp).map_err(|error| CoreError::io("compact event log", error))?;
    for event in &inner.events {
        serde_json::to_writer(&mut file, event).map_err(internal)?;
        file.write_all(b"\n")
            .map_err(|error| CoreError::io("compact event log", error))?;
    }
    file.sync_all()
        .map_err(|error| CoreError::io("sync event log", error))?;
    std::fs::rename(&temp, &inner.path)
        .map_err(|error| CoreError::io("publish compacted event log", error))?;
    inner.file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&inner.path)
        .map_err(|error| CoreError::io("reopen event log", error))?;
    Ok(())
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
        ErrorSource::System,
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn events_persist_replay_and_continue_sequences() {
        let dir = tempfile::tempdir().unwrap();
        let bus = EventBus::open(dir.path()).unwrap();
        let first = bus
            .emit("terminal.output", json!({"text":"hello"}))
            .unwrap();
        drop(bus);
        let reopened = EventBus::open(dir.path()).unwrap();
        let second = reopened.emit("system.ready", json!({})).unwrap();
        assert_eq!(first.sequence + 1, second.sequence);
        assert_eq!(reopened.replay(0, 10).len(), 2);
        assert_eq!(reopened.replay(first.sequence, 10)[0].id, second.id);
    }
}
