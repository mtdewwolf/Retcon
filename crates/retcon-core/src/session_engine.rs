//! Typed session and turn lifecycle rules.
//!
//! Persistence intentionally stores status values as strings so migrations stay
//! forward-compatible. This module is the single authority that translates
//! those values into legal state transitions before they are written.

use serde::{Deserialize, Serialize};

/// State of an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// A session has been created but no provider work has started.
    Created,
    /// Retcon is validating the provider and workspace.
    Preparing,
    /// Retcon has asked the provider to start the session.
    Starting,
    /// A provider turn is active.
    Running,
    /// The provider is waiting for an approval decision.
    WaitingForApproval,
    /// The provider is waiting for a user response.
    WaitingForUser,
    /// The user paused the session.
    Paused,
    /// All work in the session completed normally.
    Completed,
    /// The session reached an unrecoverable error.
    Failed,
    /// The user deliberately cancelled the session.
    Cancelled,
    /// Retcon lost its connection to a provider session.
    Disconnected,
    /// Retcon is reconciling durable state with the provider after a restart.
    Recovering,
}

impl SessionState {
    /// Whether the state cannot transition further.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Durable storage representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Preparing => "preparing",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::WaitingForUser => "waiting_for_user",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Disconnected => "disconnected",
            Self::Recovering => "recovering",
        }
    }
}

/// State of one provider turn within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    /// The turn is waiting to be sent to the provider.
    Queued,
    /// Retcon is sending the turn to the provider.
    Sending,
    /// The provider is producing a response.
    Running,
    /// A provider tool is executing.
    ToolExecution,
    /// A provider tool is awaiting approval.
    WaitingForApproval,
    /// The provider is finalizing its response.
    Completing,
    /// The turn completed normally.
    Completed,
    /// The turn failed.
    Failed,
    /// The user deliberately cancelled the turn.
    Cancelled,
}

impl TurnState {
    /// Whether the state cannot transition further.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Durable storage representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Sending => "sending",
            Self::Running => "running",
            Self::ToolExecution => "tool_execution",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::Completing => "completing",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Error returned when a caller attempts an invalid lifecycle transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTransition<S> {
    /// Current state.
    pub from: S,
    /// Requested next state.
    pub to: S,
}

/// In-memory state machine for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionMachine {
    state: SessionState,
}

impl SessionMachine {
    /// Start a newly-created session state machine.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: SessionState::Created,
        }
    }

    /// Rehydrate a state machine from durable state.
    #[must_use]
    pub const fn from_state(state: SessionState) -> Self {
        Self { state }
    }

    /// Current state.
    #[must_use]
    pub const fn state(self) -> SessionState {
        self.state
    }

    /// Apply a legal state transition.
    pub fn transition(&mut self, to: SessionState) -> Result<(), InvalidTransition<SessionState>> {
        if session_transition_allowed(self.state, to) {
            self.state = to;
            Ok(())
        } else {
            Err(InvalidTransition {
                from: self.state,
                to,
            })
        }
    }
}

impl Default for SessionMachine {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory state machine for a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnMachine {
    state: TurnState,
}

impl TurnMachine {
    /// Start a queued turn state machine.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: TurnState::Queued,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(self) -> TurnState {
        self.state
    }

    /// Apply a legal state transition.
    pub fn transition(&mut self, to: TurnState) -> Result<(), InvalidTransition<TurnState>> {
        if turn_transition_allowed(self.state, to) {
            self.state = to;
            Ok(())
        } else {
            Err(InvalidTransition {
                from: self.state,
                to,
            })
        }
    }
}

impl Default for TurnMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnMachine {
    /// Rehydrate a state machine from durable state.
    #[must_use]
    pub const fn from_state(state: TurnState) -> Self {
        Self { state }
    }
}

/// Parse a durable session status string.
#[must_use]
pub fn parse_session_state(value: &str) -> Option<SessionState> {
    Some(match value {
        "created" => SessionState::Created,
        "preparing" => SessionState::Preparing,
        "starting" => SessionState::Starting,
        "running" => SessionState::Running,
        "waiting_for_approval" => SessionState::WaitingForApproval,
        "waiting_for_user" => SessionState::WaitingForUser,
        "paused" => SessionState::Paused,
        "completed" => SessionState::Completed,
        "failed" => SessionState::Failed,
        "cancelled" => SessionState::Cancelled,
        "disconnected" => SessionState::Disconnected,
        "recovering" => SessionState::Recovering,
        _ => return None,
    })
}

/// Parse a durable turn status string.
#[must_use]
pub fn parse_turn_state(value: &str) -> Option<TurnState> {
    Some(match value {
        "queued" => TurnState::Queued,
        "sending" => TurnState::Sending,
        "running" => TurnState::Running,
        "tool_execution" => TurnState::ToolExecution,
        "waiting_for_approval" => TurnState::WaitingForApproval,
        "completing" => TurnState::Completing,
        "completed" => TurnState::Completed,
        "failed" => TurnState::Failed,
        "cancelled" => TurnState::Cancelled,
        _ => return None,
    })
}

const fn session_transition_allowed(from: SessionState, to: SessionState) -> bool {
    matches!(
        (from, to),
        (
            SessionState::Created,
            SessionState::Preparing | SessionState::Cancelled
        ) | (
            SessionState::Preparing,
            SessionState::Starting | SessionState::Failed | SessionState::Cancelled
        ) | (
            SessionState::Starting,
            SessionState::Running
                | SessionState::Disconnected
                | SessionState::Failed
                | SessionState::Cancelled
        ) | (
            SessionState::Running,
            SessionState::WaitingForApproval
                | SessionState::WaitingForUser
                | SessionState::Paused
                | SessionState::Completed
                | SessionState::Failed
                | SessionState::Cancelled
                | SessionState::Disconnected
        ) | (
            SessionState::WaitingForApproval,
            SessionState::Running
                | SessionState::Paused
                | SessionState::Cancelled
                | SessionState::Disconnected
        ) | (
            SessionState::WaitingForUser,
            SessionState::Running
                | SessionState::Paused
                | SessionState::Cancelled
                | SessionState::Disconnected
        ) | (
            SessionState::Paused,
            SessionState::Preparing | SessionState::Cancelled
        ) | (
            SessionState::Disconnected,
            SessionState::Recovering | SessionState::Failed | SessionState::Cancelled
        ) | (
            SessionState::Recovering,
            SessionState::Running
                | SessionState::WaitingForApproval
                | SessionState::WaitingForUser
                | SessionState::Paused
                | SessionState::Failed
                | SessionState::Disconnected
                | SessionState::Cancelled
        )
    )
}

const fn turn_transition_allowed(from: TurnState, to: TurnState) -> bool {
    matches!(
        (from, to),
        (TurnState::Queued, TurnState::Sending | TurnState::Failed | TurnState::Cancelled)
            | (
                TurnState::Sending,
                TurnState::Running | TurnState::Failed | TurnState::Cancelled
            )
            | (
                TurnState::Running,
                TurnState::ToolExecution
                    | TurnState::Completing
                    | TurnState::Failed
                    | TurnState::Cancelled
            )
            | (
                TurnState::ToolExecution,
                TurnState::Running
                    | TurnState::WaitingForApproval
                    | TurnState::Failed
                    | TurnState::Cancelled
            )
            | (
                TurnState::WaitingForApproval,
                TurnState::ToolExecution
                    | TurnState::Running
                    | TurnState::Cancelled
                    | TurnState::Failed
            )
            | (
                TurnState::Completing,
                TurnState::Completed | TurnState::Failed | TurnState::Cancelled
            )
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn session_allows_pause_and_resume() {
        let mut session = SessionMachine::new();
        for state in [
            SessionState::Preparing,
            SessionState::Starting,
            SessionState::Running,
            SessionState::Paused,
            SessionState::Preparing,
        ] {
            session.transition(state).unwrap();
        }
        assert_eq!(session.state(), SessionState::Preparing);
    }

    #[test]
    fn session_rejects_skipping_provider_start() {
        let mut session = SessionMachine::new();
        assert_eq!(
            session.transition(SessionState::Running),
            Err(InvalidTransition {
                from: SessionState::Created,
                to: SessionState::Running
            })
        );
    }

    #[test]
    fn turn_tracks_approval_and_completion() {
        let mut turn = TurnMachine::new();
        for state in [
            TurnState::Sending,
            TurnState::Running,
            TurnState::ToolExecution,
            TurnState::WaitingForApproval,
            TurnState::ToolExecution,
            TurnState::Running,
            TurnState::Completing,
            TurnState::Completed,
        ] {
            turn.transition(state).unwrap();
        }
        assert!(turn.state().is_terminal());
    }

    #[test]
    fn turn_allows_queued_to_failed_for_process_recovery() {
        let mut turn = TurnMachine::new();
        turn.transition(TurnState::Failed).unwrap();
        assert!(turn.state().is_terminal());
    }
}
