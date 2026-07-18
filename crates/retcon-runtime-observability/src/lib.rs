//! Installable, content-free runtime metric seam.
//!
//! Collection is disabled until the owning process installs a recorder and
//! explicitly enables it from the durable privacy setting. This crate performs
//! no I/O and has no networking dependencies.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

/// Bounded unit attached to a runtime metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricUnit {
    /// A discrete occurrence.
    Count,
    /// Elapsed wall-clock time.
    Milliseconds,
}

/// Closed, content-free metric passed to the process recorder.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeMetric {
    /// Stable component identifier.
    pub component: &'static str,
    /// Stable metric identifier.
    pub name: &'static str,
    /// Stable operation dimension.
    pub operation: &'static str,
    /// Stable outcome dimension.
    pub outcome: &'static str,
    /// Non-negative, bounded numeric value.
    pub value: f64,
    /// Metric unit.
    pub unit: MetricUnit,
}

/// Process-owned destination for runtime metrics.
pub trait RuntimeRecorder: Send + Sync {
    /// Record one already-validated, content-free metric.
    fn record(&self, metric: RuntimeMetric);
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static RECORDER: OnceLock<RwLock<Option<Arc<dyn RuntimeRecorder>>>> = OnceLock::new();

fn recorder() -> &'static RwLock<Option<Arc<dyn RuntimeRecorder>>> {
    RECORDER.get_or_init(|| RwLock::new(None))
}

/// Install or remove the process recorder and atomically set collection state.
///
/// The durable Core privacy setting must be the source of `enabled`. Installing
/// `None` always disables collection. Recorder failures must be swallowed by an
/// adapter before returning from [`RuntimeRecorder::record`].
pub fn configure(recorder_value: Option<Arc<dyn RuntimeRecorder>>, enabled: bool) {
    let available = recorder_value.is_some();
    if !enabled || !available {
        ENABLED.store(false, Ordering::Release);
    }
    if let Ok(mut slot) = recorder().write() {
        *slot = recorder_value;
    }
    if enabled && available {
        ENABLED.store(true, Ordering::Release);
    }
}

/// Whether collection is explicitly enabled and a recorder is installed.
#[must_use]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// Record a bounded duration metric. Invalid vocabulary is dropped.
pub fn record_duration(
    component: &'static str,
    name: &'static str,
    operation: &'static str,
    outcome: &'static str,
    duration: Duration,
) {
    record(RuntimeMetric {
        component,
        name,
        operation,
        outcome,
        value: duration.as_millis().min(86_400_000) as f64,
        unit: MetricUnit::Milliseconds,
    });
}

/// Record a single lifecycle occurrence. Invalid vocabulary is dropped.
pub fn record_count(
    component: &'static str,
    name: &'static str,
    operation: &'static str,
    outcome: &'static str,
) {
    record(RuntimeMetric {
        component,
        name,
        operation,
        outcome,
        value: 1.0,
        unit: MetricUnit::Count,
    });
}

fn record(metric: RuntimeMetric) {
    if !is_enabled() || !valid(&metric) {
        return;
    }
    let destination = recorder().read().ok().and_then(|slot| slot.clone());
    if let Some(destination) = destination {
        destination.record(metric);
    }
}

fn valid(metric: &RuntimeMetric) -> bool {
    matches!(
        metric.component,
        "agents" | "approvals" | "browser" | "devserver" | "filesystem" | "git" | "terminal"
    ) && matches!(
        metric.name,
        "approval.lifecycle.count"
            | "browser.operation.duration"
            | "devserver.operation.duration"
            | "filesystem.operation.duration"
            | "git.operation.duration"
            | "model.response.count"
            | "provider.lifecycle.count"
            | "terminal.lifecycle.count"
            | "tool.execution.count"
    ) && matches!(
        metric.operation,
        "action"
            | "branch"
            | "checkout"
            | "close"
            | "commit"
            | "complete"
            | "decide"
            | "diff"
            | "exit"
            | "init"
            | "kill"
            | "launch"
            | "list"
            | "metadata"
            | "navigation"
            | "observation"
            | "other"
            | "push"
            | "read"
            | "request"
            | "response"
            | "restart"
            | "service_start"
            | "session"
            | "stage"
            | "start"
            | "status"
            | "stop"
            | "takeover"
            | "trace"
            | "verification"
            | "worktree"
            | "write"
    ) && matches!(
        metric.outcome,
        "approved" | "cancelled" | "denied" | "error" | "ok" | "pending"
    ) && metric.value.is_finite()
        && metric.value >= 0.0
        && metric.value <= 86_400_000.0
        && matches!(
            (metric.name.ends_with(".count"), metric.unit),
            (true, MetricUnit::Count) | (false, MetricUnit::Milliseconds)
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Default)]
    struct Capture(Mutex<Vec<RuntimeMetric>>);

    impl RuntimeRecorder for Capture {
        fn record(&self, metric: RuntimeMetric) {
            if let Ok(mut metrics) = self.0.lock() {
                metrics.push(metric);
            }
        }
    }

    #[test]
    fn disabled_default_and_explicit_opt_out_record_nothing() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let capture = Arc::new(Capture::default());
        configure(Some(capture.clone()), false);
        record_count("git", "git.operation.duration", "status", "ok");
        assert!(
            capture
                .0
                .lock()
                .map(|items| items.is_empty())
                .unwrap_or(false)
        );
        configure(None, false);
    }

    #[test]
    fn enabled_recorder_accepts_only_closed_bounded_dimensions() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let capture = Arc::new(Capture::default());
        configure(Some(capture.clone()), true);
        record_duration(
            "browser",
            "browser.operation.duration",
            "verification",
            "error",
            Duration::from_secs(100_000),
        );
        record_count(
            "browser",
            "browser.operation.duration",
            "https://user:pass@example.invalid",
            "ok",
        );
        let metrics = capture.0.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].value, 86_400_000.0);
        assert_eq!(metrics[0].operation, "verification");
        drop(metrics);
        configure(None, false);
    }
}
