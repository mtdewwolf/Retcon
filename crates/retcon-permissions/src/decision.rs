//! User and rule decisions for the approval engine.

/// Outcome when a user resolves a pending approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approve,
    Deny,
}

impl ApprovalDecision {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "approve",
            Self::Deny => "deny",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "approve" | "approved" => Some(Self::Approve),
            "deny" | "denied" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// Whether a permission rule grants or blocks matching operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEffect {
    Allow,
    Deny,
}

impl RuleEffect {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(Self::Allow),
            "deny" => Some(Self::Deny),
            _ => None,
        }
    }
}

/// How long an approved decision should be remembered as a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RememberScope {
    Once,
    Session,
    Always,
}

impl RememberScope {
    pub fn parse(value: Option<&str>) -> Option<Self> {
        match value {
            None | Some("once") => Some(Self::Once),
            Some("session") => Some(Self::Session),
            Some("always") => Some(Self::Always),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::Session => "session",
            Self::Always => "always",
        }
    }
}
