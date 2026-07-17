//! Cross-platform development-server lifecycle tests.

use std::time::Duration;

use retcon_devserver::{
    DevServer, DevServerCommand, DevServerError, DevServerEvent, Framework, PortAllocator,
    StartOptions,
};

fn options() -> StartOptions {
    StartOptions {
        project_key: "runtime-tests".into(),
        worktree_key: "primary".into(),
        requested_port: None,
        allow_alternate_port: true,
        startup_timeout: Duration::from_secs(5),
        max_output_bytes: 512,
    }
}

#[cfg(windows)]
fn ready_command(root: &std::path::Path, crash: bool, large_output: bool) -> DevServerCommand {
    let prefix = if large_output {
        "[Console]::Out.Write(('x' * 4096));"
    } else {
        ""
    };
    let ending = if crash {
        "Start-Sleep -Milliseconds 250; exit 7"
    } else {
        "Start-Sleep -Seconds 30"
    };
    DevServerCommand::new(
        Framework::Custom,
        "powershell.exe",
        [
            "-NoProfile",
            "-Command",
            &format!(
                "{prefix} Write-Output 'Local: http://localhost:{{port}}'; Write-Output 'Hot reload enabled'; {ending}"
            ),
        ],
        root,
    )
}

#[cfg(not(windows))]
fn ready_command(root: &std::path::Path, crash: bool, large_output: bool) -> DevServerCommand {
    let prefix = if large_output {
        "printf '%04096d' 0;"
    } else {
        ""
    };
    let ending = if crash {
        "sleep 0.25; exit 7"
    } else {
        "exec sleep 30"
    };
    DevServerCommand::new(
        Framework::Custom,
        "/bin/sh",
        [
            "-c",
            &format!(
                "{prefix} printf 'Local: http://localhost:{{port}}\\nHot reload enabled\\n'; {ending}"
            ),
        ],
        root,
    )
}

#[cfg(windows)]
fn immediate_exit_command(root: &std::path::Path) -> DevServerCommand {
    DevServerCommand::new(
        Framework::Custom,
        "powershell.exe",
        ["-NoProfile", "-Command", "exit 9"],
        root,
    )
}

#[cfg(windows)]
fn quiet_command(root: &std::path::Path) -> DevServerCommand {
    DevServerCommand::new(
        Framework::Custom,
        "powershell.exe",
        ["-NoProfile", "-Command", "Start-Sleep -Seconds 30"],
        root,
    )
}

#[cfg(not(windows))]
fn quiet_command(root: &std::path::Path) -> DevServerCommand {
    DevServerCommand::new(Framework::Custom, "/bin/sleep", ["30"], root)
}

#[cfg(not(windows))]
fn immediate_exit_command(root: &std::path::Path) -> DevServerCommand {
    DevServerCommand::new(Framework::Custom, "/bin/sh", ["-c", "exit 9"], root)
}

#[allow(clippy::unwrap_used)]
async fn next_matching(
    events: &mut retcon_devserver::EventStream,
    predicate: impl Fn(&DevServerEvent) -> bool,
) -> DevServerEvent {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            if predicate(&event) {
                return event;
            }
        }
    })
    .await
    .unwrap()
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn ready_hot_reload_restart_and_direct_child_cleanup() {
    let directory = tempfile::tempdir().unwrap();
    let allocator = PortAllocator::new(42_000, 42_020).unwrap();
    let server = DevServer::new(allocator.clone());
    let mut events = server.subscribe();
    let first = server
        .start(ready_command(directory.path(), false, false), options())
        .await
        .unwrap();
    assert!(first.ready.detected_from_output);
    next_matching(&mut events, |event| {
        matches!(event, DevServerEvent::HotReload { run_id, .. } if *run_id == first.run_id)
    })
    .await;

    let second = server.restart().await.unwrap();
    assert_ne!(first.run_id, second.run_id);
    server.stop().await.unwrap();
    next_matching(
        &mut events,
        |event| matches!(event, DevServerEvent::Stopped { run_id } if *run_id == second.run_id),
    )
    .await;
    let reservation = allocator
        .reserve("reuse", "reuse", Some(second.ready.port), false)
        .unwrap();
    assert_eq!(reservation.port(), second.ready.port);
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn nonzero_exit_after_ready_emits_crash() {
    let directory = tempfile::tempdir().unwrap();
    let server = DevServer::default();
    let mut events = server.subscribe();
    let started = server
        .start(ready_command(directory.path(), true, false), options())
        .await
        .unwrap();
    let crashed = next_matching(&mut events, |event| {
        matches!(event, DevServerEvent::Crashed { run_id, .. } if *run_id == started.run_id)
    })
    .await;
    assert!(matches!(
        crashed,
        DevServerEvent::Crashed {
            exit_code: Some(7),
            ..
        }
    ));
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn live_output_is_bounded_and_reports_truncation() {
    let directory = tempfile::tempdir().unwrap();
    let server = DevServer::default();
    let mut events = server.subscribe();
    let mut settings = options();
    settings.max_output_bytes = 64;
    let started = server
        .start(ready_command(directory.path(), false, true), settings)
        .await
        .unwrap();
    let mut retained = 0_usize;
    let mut truncated = false;
    while !truncated {
        let event = next_matching(&mut events, |event| {
            matches!(
                event,
                DevServerEvent::Log { run_id, .. }
                    | DevServerEvent::OutputTruncated { run_id, .. }
                    if *run_id == started.run_id
            )
        })
        .await;
        match event {
            DevServerEvent::Log { text, .. } => retained += text.len(),
            DevServerEvent::OutputTruncated { limit_bytes, .. } => {
                assert_eq!(limit_bytes, 64);
                truncated = true;
            }
            _ => {}
        }
    }
    assert!(retained <= 64);
    server.stop().await.unwrap();
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn early_exit_is_a_startup_failure_and_releases_the_port() {
    let directory = tempfile::tempdir().unwrap();
    let allocator = PortAllocator::new(42_100, 42_100).unwrap();
    let server = DevServer::new(allocator.clone());
    let mut settings = options();
    settings.requested_port = Some(42_100);
    settings.allow_alternate_port = false;
    let error = server
        .start(immediate_exit_command(directory.path()), settings)
        .await
        .unwrap_err();
    assert!(matches!(error, DevServerError::StartupExited(Some(9))));
    assert!(
        allocator
            .reserve("again", "again", Some(42_100), false)
            .is_ok()
    );
}

#[tokio::test]
#[allow(clippy::unwrap_used)]
async fn startup_timeout_kills_the_child_and_releases_the_port() {
    let directory = tempfile::tempdir().unwrap();
    let allocator = PortAllocator::new(42_110, 42_110).unwrap();
    let server = DevServer::new(allocator.clone());
    let mut settings = options();
    settings.requested_port = Some(42_110);
    settings.allow_alternate_port = false;
    settings.startup_timeout = Duration::from_millis(150);
    let error = server
        .start(quiet_command(directory.path()), settings)
        .await
        .unwrap_err();
    assert!(matches!(error, DevServerError::StartupTimeout));
    assert!(
        allocator
            .reserve("again", "again", Some(42_110), false)
            .is_ok()
    );
}
