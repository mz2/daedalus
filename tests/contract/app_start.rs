//! Contract test for `RegisterTool` + `StartSession` (C-A1: failure leaves no record).

use daedalus_app::{AppQuery, Command, CommandResult};
use daedalus_tests::Fixture;

#[tokio::test]
async fn register_tool_then_start_session_succeeds() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let res = fx
        .app
        .execute(Command::StartSession(fx.fresh_request(tool)))
        .await
        .unwrap();
    assert!(matches!(res, CommandResult::SessionStarted(_)));
    assert_eq!(fx.app.fleet().len(), 1);
}

#[tokio::test]
async fn c_a1_unprovisionable_environment_creates_no_session_record() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    // Force acquire to fail: no environment can be provisioned.
    fx.backend.fail_next_acquire("no capacity");

    let err = fx
        .app
        .execute(Command::StartSession(fx.fresh_request(tool)))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("provision"),
        "stated reason: {err}"
    );

    // No session record and no orphaned environment.
    assert!(
        fx.app.fleet().is_empty(),
        "failure must leave no session record"
    );
    assert_eq!(fx.backend.live_environment_count(), 0);
}

#[tokio::test]
async fn duplicate_tool_name_is_rejected() {
    let fx = Fixture::new();
    fx.register_sample_tool("claude");
    let err = fx
        .app
        .execute(Command::RegisterTool(daedalus_proto::ToolDef {
            name: "claude".into(),
            invocation: daedalus_proto::InvocationSpec {
                program: "claude".into(),
                args: vec![],
                env: vec![],
            },
            capabilities: daedalus_proto::Capabilities::default(),
        }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already registered"));
}
