//! Approval categories group related risky operations for rules and UI.

/// High-level grouping for approval requests and permission rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalCategory {
    Agent,
    Session,
    Turn,
    Git,
    Terminal,
    File,
    Browser,
    System,
}

impl ApprovalCategory {
    /// Stable snake_case identifier stored in approval payloads and audit events.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Session => "session",
            Self::Turn => "turn",
            Self::Git => "git",
            Self::Terminal => "terminal",
            Self::File => "file",
            Self::Browser => "browser",
            Self::System => "system",
        }
    }

    /// User-facing label for desktop surfaces.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Session => "Session",
            Self::Turn => "Conversation turn",
            Self::Git => "Git",
            Self::Terminal => "Terminal",
            Self::File => "Filesystem",
            Self::Browser => "Browser",
            Self::System => "System",
        }
    }
}
