//! Durable, sequence-numbered core event bus backed by SQLite.

#![allow(missing_docs)]

use std::collections::VecDeque;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{CoreError, ErrorCode, ErrorSource};
use retcon_storage::Database;

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
    events: VecDeque<EventEnvelope>,
    next_sequence: u64,
}

pub struct EventBus {
    inner: Mutex<EventLog>,
    sender: broadcast::Sender<EventEnvelope>,
    database: Database,
}

impl EventBus {
    pub fn open(database: Database) -> Result<Self, CoreError> {
        let cutoff = (Utc::now() - Duration::days(DEFAULT_MAX_AGE_DAYS)).timestamp_millis();
        database.execute("DELETE FROM agent_events WHERE created_at < ?1 OR id NOT IN (SELECT id FROM agent_events ORDER BY id DESC LIMIT ?2)", &[&cutoff, &(DEFAULT_MAX_EVENTS as i64)])?;
        let mut events = database.read(|db| {
            let mut statement = db.prepare("SELECT id,event_id,category,kind,payload_json,created_at FROM agent_events WHERE event_id IS NOT NULL ORDER BY id")?;
            statement.query_map([], |row| {
                let sequence: i64 = row.get(0)?;
                let id_text: String = row.get(1)?;
                let kind: String = row.get(3)?;
                let payload_text: String = row.get(4)?;
                let timestamp_ms: i64 = row.get(5)?;
                let id = Uuid::parse_str(&id_text).map_err(|e| rusqlite::Error::FromSqlConversionFailure(36, rusqlite::types::Type::Text, Box::new(e)))?;
                let payload = serde_json::from_str(&payload_text).map_err(|e| rusqlite::Error::FromSqlConversionFailure(payload_text.len(), rusqlite::types::Type::Text, Box::new(e)))?;
                let timestamp = DateTime::from_timestamp_millis(timestamp_ms).ok_or_else(|| rusqlite::Error::IntegralValueOutOfRange(5, timestamp_ms))?;
                Ok(EventEnvelope { id, sequence: sequence as u64, timestamp, category: category_for(&kind), kind, payload })
            })?.collect::<rusqlite::Result<VecDeque<_>>>()
        })?;
        let next_sequence = database.read(|db| {
            db.query_row(
                "SELECT coalesce(max(id), 0) + 1 FROM agent_events",
                [],
                |row| row.get::<_, u64>(0),
            )
        })?;
        let (sender, _) = broadcast::channel(1024);
        Ok(Self {
            inner: Mutex::new(EventLog {
                events: std::mem::take(&mut events),
                next_sequence,
            }),
            sender,
            database,
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
        let encoded = serde_json::to_string(&event.payload).map_err(internal)?;
        self.database.execute("INSERT INTO agent_events (id,event_id,category,kind,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6)", &[&(event.sequence as i64), &event.id.to_string(), &event.category.as_str(), &event.kind, &encoded, &event.timestamp.timestamp_millis()])?;
        inner.next_sequence = inner.next_sequence.saturating_add(1);
        inner.events.push_back(event.clone());
        while inner.events.len() > DEFAULT_MAX_EVENTS {
            inner.events.pop_front();
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
        drop(bus);
        let reopened = EventBus::open(database).unwrap();
        let second = reopened.emit("system.ready", json!({})).unwrap();
        assert_eq!(first.sequence + 1, second.sequence);
        assert_eq!(reopened.replay(0, 10).len(), 2);
    }
}
