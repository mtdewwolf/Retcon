//! Checkpoint kind labels persisted in SQLite.

use std::fmt;

/// Why a checkpoint was created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointKind {
    /// User explicitly requested a checkpoint.
    Manual,
    /// Captured before a shell command runs.
    PreCommand,
    /// Captured immediately before a file write.
    PreFileWrite,
    /// Captured before a mutating Git operation.
    PreGit,
    /// Captured when an agent turn starts.
    TurnStart,
}

impl CheckpointKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::PreCommand => "pre-command",
            Self::PreFileWrite => "pre-file-write",
            Self::PreGit => "pre-git",
            Self::TurnStart => "turn-start",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "pre-command" => Some(Self::PreCommand),
            "pre-file-write" => Some(Self::PreFileWrite),
            "pre-git" => Some(Self::PreGit),
            "turn-start" => Some(Self::TurnStart),
            _ => None,
        }
    }
}

impl fmt::Display for CheckpointKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
