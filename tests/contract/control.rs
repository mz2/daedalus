//! Contract tests for control: stop/teardown idempotency (C-B5) and send-input rejection
//! when the tool does not accept input (C-A2).

use bytes::Bytes;
use daedalus_proto::{Capabilities, InvocationSpec, ToolDef};
use daedalus_tests::Fixture;

fn no_input_tool(fx: &Fixture, name: &str) -> daedalus_proto::ToolId {
    fx.core
        .register_tool(ToolDef {
            name: name.into(),
            invocation: InvocationSpec {
                program: "batch-agent".into(),
                args: vec![],
                env: vec![],
            },
            capabilities: Capabilities {
                accepts_interactive_input: false,
            },
        })
        .unwrap()
}

#[tokio::test]
async fn c_a2_send_input_rejected_when_unsupported() {
    let fx = Fixture::new();
    let tool = no_input_tool(&fx, "batch");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    let err = fx
        .core
        .send_input(id, Bytes::from_static(b"hi"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("does not accept input"));
}

#[tokio::test]
async fn stop_and_cleanup_are_idempotent() {
    let fx = Fixture::new();
    let tool = fx.register_sample_tool("claude");
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    fx.core.stop_session(id).await.unwrap();
    fx.core.stop_session(id).await.unwrap(); // idempotent
    fx.core.clean_up(id).await.unwrap();
    fx.core.clean_up(id).await.unwrap(); // idempotent
}
