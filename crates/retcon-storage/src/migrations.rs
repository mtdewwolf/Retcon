//! Ordered, append-only database migrations.

pub(crate) struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub(crate) const LATEST_VERSION: u32 = 3;

pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial durable state schema",
        sql: include_str!("migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        name: "durable core events and supervised jobs",
        sql: include_str!("migrations/0002_durable_core_runtime.sql"),
    },
    Migration {
        version: 3,
        name: "recovery and artifact hash indexes",
        sql: include_str!("migrations/0003_recovery_and_artifact_indexes.sql"),
    },
];
