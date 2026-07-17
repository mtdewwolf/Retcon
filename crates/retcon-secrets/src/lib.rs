//! Secret detection, redaction, and content scanning for Retcon.
//!
//! Used by the core service before `turn.send`, diagnostic bundles, protocol
//! trace logs, and the Git pre-commit hook (shared with future commit RPC).

#![allow(missing_docs)] // Phase 20 API; public documentation lands with the generated protocol.

mod detect;
mod redact;

pub use detect::{FindingKind, ScanResult, SecretFinding, scan_text, scan_texts};
pub use redact::{redact_text, scrub_json, scrub_string_fields};
