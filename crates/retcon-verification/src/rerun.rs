//! Framework-aware commands for rerunning failed tests.

use crate::{CommandSpec, ParserKind, TestCaseResult, VerificationStatus};

/// Construct the smallest practical commands that rerun the supplied failures.
#[must_use]
pub fn build_rerun_failed(
    parser: ParserKind,
    base: &CommandSpec,
    tests: &[TestCaseResult],
) -> Vec<CommandSpec> {
    let failed: Vec<_> = tests
        .iter()
        .filter(|test| test.status == VerificationStatus::Failed)
        .collect();
    if failed.is_empty() {
        return Vec::new();
    }
    match parser {
        ParserKind::CargoTest => failed
            .into_iter()
            .map(|test| {
                let mut command = base.clone();
                command
                    .args
                    .push(test.source_id.as_deref().unwrap_or(&test.name).to_owned());
                command.args.extend(["--".into(), "--exact".into()]);
                command
            })
            .collect(),
        ParserKind::CargoNextest => failed
            .into_iter()
            .map(|test| {
                let mut command = base.clone();
                let name = test.source_id.as_deref().unwrap_or(&test.name);
                command.args.extend(["-E".into(), format!("test(={name})")]);
                command
            })
            .collect(),
        ParserKind::FlutterTest => failed
            .into_iter()
            .map(|test| {
                let mut command = base.clone();
                if let Some(location) = test.locations.first()
                    && !command
                        .args
                        .iter()
                        .any(|arg| arg == location.file.to_string_lossy().as_ref())
                {
                    command
                        .args
                        .push(location.file.to_string_lossy().into_owned());
                }
                command
                    .args
                    .extend(["--plain-name".into(), test.name.clone()]);
                command
            })
            .collect(),
        ParserKind::JestJson | ParserKind::VitestJson => failed
            .into_iter()
            .map(|test| {
                let mut command = base.clone();
                if let Some(location) = test.locations.first() {
                    command
                        .args
                        .push(location.file.to_string_lossy().into_owned());
                }
                command
                    .args
                    .extend(["--testNamePattern".into(), test.name.clone()]);
                command
            })
            .collect(),
        ParserKind::Pytest => {
            let mut command = base.clone();
            command.args.extend(
                failed
                    .into_iter()
                    .map(|test| test.source_id.as_deref().unwrap_or(&test.name).to_owned()),
            );
            vec![command]
        }
        ParserKind::None | ParserKind::Auto | ParserKind::Junit => Vec::new(),
    }
}
