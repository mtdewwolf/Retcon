//! Project verification command detection, safe execution, and result parsing.

#![allow(missing_docs)] // Public wire fields are described by their owning domain types.

mod detect;
mod domain;
mod execute;
mod parser;
mod rerun;

pub use detect::{DetectionError, detect_gates};
pub use domain::{
    CommandSpec, DetectionSource, FailureLocation, GateDefinition, GateKind, GateOverride,
    GateResult, OutputCapture, ParserKind, TestCaseResult, VerificationStatus,
};
pub use execute::{CancellationHandle, ExecutionError, ExecutionOptions, execute};
pub use parser::{ParseError, parse_test_output};
pub use rerun::build_rerun_failed;
