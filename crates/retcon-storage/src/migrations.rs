//! Ordered, append-only database migrations.

pub(crate) struct Migration {
    pub version: u32,
    pub name: &'static str,
    pub sql: &'static str,
}

pub(crate) const LATEST_VERSION: u32 = 10;

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
    Migration {
        version: 4,
        name: "compact UUID storage",
        sql: include_str!("migrations/0004_uuid_blobs.sql"),
    },
    Migration {
        version: 5,
        name: "task planning and acceptance gates",
        sql: include_str!("migrations/0005_task_planning.sql"),
    },
    Migration {
        version: 6,
        name: "immutable task acceptance audit history",
        sql: include_str!("migrations/0006_task_acceptance_audit.sql"),
    },
    Migration {
        version: 7,
        name: "durable verification runs and completion evidence",
        sql: include_str!("migrations/0007_durable_verification.sql"),
    },
    Migration {
        version: 8,
        name: "durable development servers and port leases",
        sql: include_str!("migrations/0008_durable_dev_servers.sql"),
    },
    Migration {
        version: 9,
        name: "durable managed browser sessions and evidence",
        sql: include_str!("migrations/0009_durable_browser.sql"),
    },
    Migration {
        version: 10,
        name: "durable browser verification and visual evidence",
        sql: include_str!("migrations/0010_browser_verification.sql"),
    },
];
