//! Execution seam for the separately packaged verification runner.

use retcon_storage::VerificationRunDetails;

/// Receives durable lifecycle transitions after they have been committed.
///
/// The default implementation is intentionally inert. A runner crate can inject an
/// implementation without making core persistence depend on process execution code.
pub trait VerificationRunner: Send + Sync {
    /// Begin executing the gates in a run whose durable status is already `running`.
    fn start(&self, run: &VerificationRunDetails) -> Result<(), String>;

    /// Stop external execution after the durable run has been cancelled.
    fn cancel(&self, run: &VerificationRunDetails) -> Result<(), String>;
}

/// Default runner used when the separately packaged executor is not installed.
#[derive(Default)]
pub struct NoopVerificationRunner;

impl VerificationRunner for NoopVerificationRunner {
    fn start(&self, _run: &VerificationRunDetails) -> Result<(), String> {
        Ok(())
    }

    fn cancel(&self, _run: &VerificationRunDetails) -> Result<(), String> {
        Ok(())
    }
}
