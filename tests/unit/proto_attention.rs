//! Unit tests for the proto attention-state extensions (T069, US6): `WaitingForInput` +
//! `Unknown`, the waiting/confirmation session fields, `PromptConvention`, the richer
//! `Outcome`, and backend/source idle rates + availability reasons (data-model.md,
//! FR-015b/020/021b/028).

use daedalus_proto::{
    Availability, BackendId, BackendKind, Capabilities, EnvironmentBackend, EnvironmentId,
    InvocationSpec, ObjectiveId, Outcome, PromptConvention, Session, SessionId, SessionStatus,
    Source, SourceId, SourceKind, Timestamp, ToolDef, ToolId, WorkItemRef,
};

fn sample_session() -> Session {
    Session {
        id: SessionId::new(),
        tool_id: ToolId::new(),
        objective_id: ObjectiveId::new(),
        environment_id: EnvironmentId::new(),
        source_id: SourceId::new(),
        status: SessionStatus::WaitingForInput,
        created_at: Timestamp::from_millis(1),
        started_at: Some(Timestamp::from_millis(2)),
        ended_at: None,
        terminal_outcome: None,
        accepts_input: true,
        pending_prompt: Some("Continue? (y/n)".to_string()),
        waiting_since: Some(Timestamp::from_millis(3)),
        work_item_ref: Some(WorkItemRef {
            tracker: "github".to_string(),
            reference: "daedalus#42".to_string(),
        }),
        last_known_status: None,
    }
}

#[test]
fn waiting_and_unknown_are_non_terminal_attention_states() {
    for status in [SessionStatus::WaitingForInput, SessionStatus::Unknown] {
        assert!(!status.is_terminal(), "{status:?} must not be terminal");
        assert!(
            status.is_attention(),
            "{status:?} feeds the Needs-you queue"
        );
    }
}

#[test]
fn status_tokens_follow_the_design_terminology_mapping() {
    // data-model.md terminology table: one concept, three vocabularies — keep aligned.
    assert_eq!(SessionStatus::WaitingForInput.as_str(), "awaiting");
    assert_eq!(SessionStatus::AwaitingConfirmation.as_str(), "confirm");
    assert_eq!(SessionStatus::Unknown.as_str(), "unknown");
}

#[test]
fn session_carries_waiting_fields_and_work_item_ref() {
    let session = sample_session();
    let json = serde_json::to_string(&session).unwrap();
    let back: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(back, session);
    assert_eq!(back.pending_prompt.as_deref(), Some("Continue? (y/n)"));
    assert_eq!(back.waiting_since, Some(Timestamp::from_millis(3)));
    assert_eq!(back.work_item_ref.unwrap().tracker, "github");
}

#[test]
fn pre_attention_session_json_defaults_the_new_fields() {
    // A record shaped like the pre-US6 Session (no waiting/attention fields) still loads.
    let session = sample_session();
    let mut value = serde_json::to_value(&session).unwrap();
    let obj = value.as_object_mut().unwrap();
    obj.remove("pending_prompt");
    obj.remove("waiting_since");
    obj.remove("work_item_ref");
    obj.remove("last_known_status");
    let back: Session = serde_json::from_value(value).unwrap();
    assert_eq!(back.pending_prompt, None);
    assert_eq!(back.waiting_since, None);
    assert_eq!(back.work_item_ref, None);
    assert_eq!(back.last_known_status, None);
}

#[test]
fn outcome_carries_exit_code_and_exit_summary_for_awaiting_confirmation() {
    let mut session = sample_session();
    session.status = SessionStatus::AwaitingConfirmation;
    session.terminal_outcome = Some(Outcome {
        reason: None,
        exit_code: Some(0),
        exit_summary: Some("2 of 4 tracked tasks done; stopping for review".to_string()),
    });
    let json = serde_json::to_string(&session).unwrap();
    let back: Session = serde_json::from_str(&json).unwrap();
    let outcome = back.terminal_outcome.unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(
        outcome.exit_summary.as_deref(),
        Some("2 of 4 tracked tasks done; stopping for review")
    );

    // Failed/stalled/stopped keep carrying a plain reason.
    let reason = Outcome::reason("stopped by operator");
    assert_eq!(reason.reason.as_deref(), Some("stopped by operator"));
    assert_eq!(reason.exit_code, None);
    assert_eq!(reason.exit_summary, None);
}

#[test]
fn prompt_convention_rides_on_tool_capabilities() {
    let sdk = Capabilities {
        accepts_interactive_input: true,
        prompt_convention: Some(PromptConvention::SdkSignal),
    };
    let pattern = Capabilities {
        accepts_interactive_input: true,
        prompt_convention: Some(PromptConvention::PromptPattern(r"\(y/n\)\s*$".to_string())),
    };
    for caps in [sdk, pattern] {
        let json = serde_json::to_string(&caps).unwrap();
        let back: Capabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(back, caps);
    }

    // Capabilities persisted before US6 (no convention field) default to none.
    let back: Capabilities = serde_json::from_str(r#"{"accepts_interactive_input":true}"#).unwrap();
    assert_eq!(back.prompt_convention, None);
}

#[tokio::test]
async fn only_interactive_tools_may_declare_a_prompt_convention() {
    let fx = daedalus_tests::Fixture::new();
    let def = |accepts: bool, convention: Option<PromptConvention>| ToolDef {
        name: format!("tool-{accepts}-{}", convention.is_some()),
        invocation: InvocationSpec {
            program: "echo".to_string(),
            args: vec![],
            env: vec![],
        },
        capabilities: Capabilities {
            accepts_interactive_input: accepts,
            prompt_convention: convention,
        },
    };

    // Non-interactive + a convention is an invalid declaration (FR-001a).
    let err = fx
        .core
        .register_tool(def(false, Some(PromptConvention::SdkSignal)))
        .unwrap_err();
    assert!(matches!(err, daedalus_core::CoreError::InvalidTool(_)));

    // Interactive tools may carry one; either capability shape without one is fine.
    fx.core
        .register_tool(def(true, Some(PromptConvention::SdkSignal)))
        .unwrap();
    fx.core.register_tool(def(true, None)).unwrap();
    fx.core.register_tool(def(false, None)).unwrap();
}

#[test]
fn backend_and_source_carry_idle_rate_and_availability_reasons() {
    let backend = EnvironmentBackend {
        id: BackendId::new(),
        kind: BackendKind::Workshop,
        availability: Availability::Degraded,
        availability_reason: Some("high memory pressure".to_string()),
        idle_rate: Some(1.25),
    };
    let json = serde_json::to_string(&backend).unwrap();
    let back: EnvironmentBackend = serde_json::from_str(&json).unwrap();
    assert_eq!(back, backend);

    let source = Source {
        id: SourceId::new(),
        kind: SourceKind::Mdns,
        availability: Availability::Unavailable,
        availability_reason: Some("host stopped advertising".to_string()),
    };
    let json = serde_json::to_string(&source).unwrap();
    let back: Source = serde_json::from_str(&json).unwrap();
    assert_eq!(back, source);

    // No rate configured ⇒ no cost estimate shown (FR-021b) — None must round-trip too.
    let back: EnvironmentBackend = serde_json::from_str(
        &serde_json::to_string(&EnvironmentBackend {
            idle_rate: None,
            availability_reason: None,
            ..backend
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(back.idle_rate, None);
    assert_eq!(back.availability_reason, None);
}
