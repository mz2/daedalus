//! Integration test for waiting-for-input detection (T070, US6, FR-015b): a blocked agent
//! is surfaced per the tool's declared prompt convention — an SDK waiting signal or a
//! declared prompt pattern matched in terminal output. Tools with no declaration never
//! enter the state, and answering returns the session to Running and clears the prompt.

use bytes::Bytes;
use daedalus_core::AgentSignal;
use daedalus_proto::{AppEvent, PromptConvention, SessionId, SessionStatus};
use daedalus_sdk::WaitingState;
use daedalus_tests::Fixture;

const PROMPT: &str = "Continue? (y/n)";

/// Stream the seeded output chunks through the capture pipeline, so any declared prompt
/// pattern is observed the same way the live terminal is.
async fn stream_output(fx: &Fixture, id: SessionId, chunks: Vec<&'static [u8]>) {
    let n = chunks.len();
    fx.terminal
        .seed(id, chunks.into_iter().map(Bytes::from_static).collect());
    let mut channel = fx.core.attach_and_capture(id).await.unwrap();
    for _ in 0..n {
        channel.output.recv().await.unwrap();
    }
}

#[tokio::test]
async fn prompt_pattern_match_enters_waiting_and_answering_returns_to_running() {
    let fx = Fixture::new();
    let tool = fx.register_tool_with(
        "claude",
        true,
        Some(PromptConvention::PromptPattern(
            r"Continue\? \(y/n\)".to_string(),
        )),
    );
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    let mut rx = fx.app.subscribe();

    stream_output(&fx, id, vec![b"working...\n", b"Continue? (y/n)\n"]).await;

    // The matched question is captured, with the waiting duration anchor (FR-015b).
    let session = fx.core.session_detail(id).unwrap().session;
    assert_eq!(session.status, SessionStatus::WaitingForInput);
    assert_eq!(session.pending_prompt.as_deref(), Some(PROMPT));
    assert!(session.waiting_since.is_some());

    // The status change is pushed to the surfaces like any other.
    let saw_waiting = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
        matches!(
            e,
            AppEvent::SessionStatusChanged {
                status: SessionStatus::WaitingForInput,
                ..
            }
        )
    });
    assert!(
        saw_waiting,
        "WaitingForInput is announced as a status change"
    );

    // Answering through the existing send-input path returns to Running and clears.
    fx.core
        .send_input(id, Bytes::from_static(b"y\n"))
        .await
        .unwrap();
    let session = fx.core.session_detail(id).unwrap().session;
    assert_eq!(session.status, SessionStatus::Running);
    assert_eq!(session.pending_prompt, None);
    assert_eq!(session.waiting_since, None);
}

#[tokio::test]
async fn sdk_signal_surfaces_the_waiting_state_and_question() {
    let fx = Fixture::new();
    let tool = fx.register_tool_with("speckit", true, Some(PromptConvention::SdkSignal));
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();

    // The in-Workshop SDK signals a waiting state with the pending question.
    fx.core
        .apply_sdk_waiting(
            id,
            Some(WaitingState {
                question: "Which database should the service use?".to_string(),
            }),
        )
        .unwrap();
    let session = fx.core.session_detail(id).unwrap().session;
    assert_eq!(session.status, SessionStatus::WaitingForInput);
    assert_eq!(
        session.pending_prompt.as_deref(),
        Some("Which database should the service use?")
    );
    assert!(session.waiting_since.is_some());

    // The SDK clearing its signal resumes the session too (agent unblocked itself).
    fx.core.apply_sdk_waiting(id, None).unwrap();
    assert_eq!(
        fx.core.session_detail(id).unwrap().session.status,
        SessionStatus::Running
    );

    // Re-enter, then answer through send-input: back to Running, prompt cleared.
    fx.core
        .apply_sdk_waiting(
            id,
            Some(WaitingState {
                question: "Apply the migration now?".to_string(),
            }),
        )
        .unwrap();
    fx.core
        .send_input(id, Bytes::from_static(b"yes\n"))
        .await
        .unwrap();
    let session = fx.core.session_detail(id).unwrap().session;
    assert_eq!(session.status, SessionStatus::Running);
    assert_eq!(session.pending_prompt, None);
    assert_eq!(session.waiting_since, None);
}

#[tokio::test]
async fn tools_without_a_declaration_never_enter_waiting() {
    let fx = Fixture::new();
    // Interactive but with no declared prompt convention (FR-015b: never enters), and a
    // non-interactive tool (FR-023: cannot enter by definition).
    let undeclared = fx.register_tool_with("undeclared", true, None);
    let batch = fx.register_tool_with("batch", false, None);

    for tool in [undeclared, batch] {
        let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
        stream_output(&fx, id, vec![b"working...\n", b"Continue? (y/n)\n"]).await;
        let session = fx.core.session_detail(id).unwrap().session;
        assert_eq!(
            session.status,
            SessionStatus::Running,
            "prompt-looking output must not flip an undeclared tool to waiting"
        );
        assert_eq!(session.pending_prompt, None);
        assert_eq!(session.waiting_since, None);
    }
}

#[tokio::test]
async fn a_waiting_session_is_not_marked_stalled_past_the_stall_interval() {
    let fx = Fixture::new();
    let tool = fx.register_tool_with(
        "claude",
        true,
        Some(PromptConvention::PromptPattern(
            r"Continue\? \(y/n\)".to_string(),
        )),
    );
    let id = fx.core.start_session(fx.fresh_request(tool)).await.unwrap();
    stream_output(&fx, id, vec![b"Continue? (y/n)\n"]).await;
    assert_eq!(
        fx.core.session_detail(id).unwrap().session.status,
        SessionStatus::WaitingForInput
    );

    // Any stall interval has long elapsed; the detector must skip a waiting session.
    fx.core.set_stall_interval(0).unwrap();
    let mut rx = fx.app.subscribe();
    let status = fx
        .core
        .apply_signal(id, AgentSignal::NoProgress)
        .await
        .unwrap();
    assert_eq!(status, SessionStatus::WaitingForInput);
    assert_eq!(
        fx.core.session_detail(id).unwrap().session.status,
        SessionStatus::WaitingForInput
    );
    let stalled = std::iter::from_fn(|| rx.try_recv().ok()).any(|e| {
        matches!(
            e,
            AppEvent::SessionStatusChanged {
                status: SessionStatus::Stalled,
                ..
            }
        )
    });
    assert!(
        !stalled,
        "a waiting session is never reported stalled (FR-015b)"
    );
}
