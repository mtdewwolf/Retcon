//! Wave 3 integration checkpoint: session lifecycle, event replay, terminal cleanup.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use retcon_integration::harness::{RpcClient, TestCore, assert_ok, default_shell};
use serde_json::Value;
use serde_json::json;
use serial_test::serial;

#[tokio::test]
#[serial]
async fn session_lifecycle_create_start_cancel_with_permissions_bypass() {
    let core = TestCore::start().await;
    let endpoint = core.endpoint().clone();
    let token = core.discovery.token.clone();
    let project_dir = core.data_dir.clone();
    let mut client = RpcClient::connect(&endpoint, &token).await;

    let project_response = client
        .call(1, "project.open", json!({"path": project_dir}))
        .await;
    let project = assert_ok(&project_response);
    let project_id = project["id"].as_str().expect("project id");

    let created_response = client
        .call(
            2,
            "session.create",
            json!({"projectId": project_id, "title": "Integration session"}),
        )
        .await;
    let created = assert_ok(&created_response);
    let session_id = created["sessionId"]
        .as_str()
        .expect("session id")
        .to_owned();

    let started_response = client
        .call(
            3,
            "session.start",
            json!({"sessionId": session_id, "cwd": project_dir}),
        )
        .await;
    let started = assert_ok(&started_response);
    assert_eq!(started["status"], "running");

    let cancelled_response = client
        .call(4, "session.cancel", json!({"sessionId": session_id}))
        .await;
    let cancelled = assert_ok(&cancelled_response);
    assert_eq!(cancelled["status"], "cancelled");

    let listed_response = client
        .call(5, "session.list", json!({"projectId": project_id}))
        .await;
    let listed = assert_ok(&listed_response);
    let sessions = listed["sessions"].as_array().expect("sessions array");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["sessionId"], session_id);
    assert_eq!(sessions[0]["status"], "cancelled");

    core.shutdown().await;
}

#[tokio::test]
#[serial]
async fn event_replay_after_client_reconnect() {
    let core = TestCore::start().await;
    let endpoint = core.endpoint().clone();
    let token = core.discovery.token.clone();
    let mut first = RpcClient::connect(&endpoint, &token).await;

    let emitted_response = first
        .call(
            1,
            "events.emit",
            json!({
                "kind": "system.integrationTest",
                "payload": {"marker": "wave3-replay"}
            }),
        )
        .await;
    let emitted = assert_ok(&emitted_response);
    let sequence = emitted["sequence"].as_u64().expect("event sequence");

    drop(first);

    let mut second = RpcClient::connect(&endpoint, &token).await;
    let replay_response = second
        .call(
            2,
            "events.replay",
            json!({"afterSequence": sequence.saturating_sub(1), "limit": 10}),
        )
        .await;
    let replay = assert_ok(&replay_response);
    let events = replay["events"].as_array().expect("events array");
    assert!(
        events.iter().any(|event| {
            event["kind"] == "system.integrationTest"
                && event["payload"]["marker"] == "wave3-replay"
        }),
        "replay missing emitted event: {replay}"
    );
    assert!(
        replay["latestSequence"].as_u64().unwrap_or(0) >= sequence,
        "latestSequence should cover emitted event"
    );

    core.shutdown().await;
}

#[tokio::test]
#[serial]
async fn terminal_cleanup_on_core_shutdown() {
    let core = TestCore::start().await;
    let endpoint = core.endpoint().clone();
    let token = core.discovery.token.clone();
    let mut client = RpcClient::connect(&endpoint, &token).await;

    let started = client
        .call(
            1,
            "terminal.start",
            json!({"shell": default_shell(), "cwd": core.data_dir}),
        )
        .await;
    let result = assert_ok(&started);
    let terminal_id = result["terminalId"].as_u64().expect("terminal id");
    let session_id = result["sessionId"].as_str().expect("session id");

    let live_response = client
        .call(2, "terminal.scrollback", json!({"sessionId": session_id}))
        .await;
    let live = assert_ok(&live_response);
    assert_eq!(live["live"], true);

    drop(client);

    core.shutdown().await;

    let restarted = TestCore::start().await;
    let mut client = RpcClient::connect(restarted.endpoint(), &restarted.discovery.token).await;
    let killed = client
        .call(3, "terminal.kill", json!({"id": terminal_id}))
        .await;
    assert_rpc_error(&killed);

    restarted.shutdown().await;
}

fn assert_rpc_error(response: &Value) {
    assert!(
        response.get("error").is_some(),
        "expected RPC error, got: {response}"
    );
}
