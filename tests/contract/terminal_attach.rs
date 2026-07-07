//! Contract tests for embedded terminal attach (C-T1 latency, C-T2 reason, C-T3 excessive
//! output, C-T4 redaction).

use std::time::Duration;

use bytes::Bytes;
use daedalus_proto::{EventPayload, SessionId};
use daedalus_tests::Fixture;

#[tokio::test]
async fn c_t1_output_appears_quickly() {
    let fx = Fixture::new();
    let session = SessionId::new();
    fx.terminal
        .seed(session, vec![Bytes::from_static(b"first line\n")]);

    let mut channel = fx.core.attach_and_capture(session).await.unwrap();
    let chunk = tokio::time::timeout(Duration::from_secs(5), channel.output.recv())
        .await
        .expect("output within 5s (SC-003)")
        .expect("a chunk");
    assert_eq!(&chunk[..], b"first line\n");
}

#[tokio::test]
async fn c_t2_unattachable_session_yields_clear_reason() {
    let fx = Fixture::new();
    // Nothing seeded for this session ⇒ the terminal reports it is not attachable.
    let err = match fx.core.attach_and_capture(SessionId::new()).await {
        Err(e) => e,
        Ok(_) => panic!("expected an attach error"),
    };
    assert!(err.to_string().contains("not attachable"), "reason: {err}");
}

#[tokio::test]
async fn c_t3_excessive_output_is_fully_captured() {
    let fx = Fixture::new();
    let session = SessionId::new();
    let chunks: Vec<Bytes> = (0..2000)
        .map(|i| Bytes::from(format!("line {i}\n")))
        .collect();
    let expected_len: usize = chunks.iter().map(|c| c.len()).sum();
    fx.terminal.seed(session, chunks);

    let mut channel = fx.core.attach_and_capture(session).await.unwrap();
    let mut received = 0usize;
    while let Ok(Some(chunk)) =
        tokio::time::timeout(Duration::from_secs(5), channel.output.recv()).await
    {
        received += chunk.len();
    }
    assert_eq!(
        received, expected_len,
        "view stays responsive and loses nothing"
    );

    // The persisted capture file holds the full stream too (no record loss).
    let events = fx.core.store().list_events(session).unwrap();
    let total: u64 = events
        .iter()
        .filter_map(|e| match &e.payload {
            EventPayload::Output { len, .. } => Some(*len),
            _ => None,
        })
        .sum();
    assert_eq!(total as usize, expected_len);
}

// --- ProcessTerminal: subprocess-bridged attach (the real-backend path, issue #15) ------
// A session's terminal is reached by spawning a bridge process (e.g. `openshell sandbox
// exec --tty … -- zellij attach <s>`) whose stdio IS the terminal. Hermetic here via
// `cat` (echoes stdin→stdout, proving both directions); the OpenShell command shape is
// contract-tested in `contract_openshell_backend`.

#[tokio::test]
async fn process_terminal_bridges_duplex_io() {
    use daedalus_zellij::{ProcessTerminal, TerminalAttach};
    let terminal = ProcessTerminal::new(Box::new(|_| Ok(vec!["cat".into()])));
    let mut ch = terminal.attach(SessionId::new()).await.unwrap();

    ch.input
        .send(Bytes::from_static(b"hello bridge\n"))
        .await
        .unwrap();
    // The bridge runs on a real PTY (TTY-mode CLIs need a controlling terminal), so the
    // line discipline may echo the input and translate \n → \r\n; assert the round-trip,
    // not exact bytes.
    let mut got = Vec::new();
    while !String::from_utf8_lossy(&got).contains("hello bridge") {
        let chunk = tokio::time::timeout(Duration::from_secs(5), ch.output.recv())
            .await
            .expect("echo within 5s (C-T1)")
            .expect("bridge stays open");
        got.extend_from_slice(&chunk);
    }
}

// Surface-driven resize reaches the bridge child as a real winsize change + SIGWINCH on
// its controlling PTY (issue #15 follow-up): the child traps WINCH and reports its size.
#[tokio::test]
async fn process_terminal_resizes_the_bridge_pty() {
    use daedalus_zellij::{ProcessTerminal, TerminalAttach, TerminalSize};
    let terminal = ProcessTerminal::new(Box::new(|_| {
        Ok(vec![
            "sh".into(),
            "-c".into(),
            // Print the initial size, then re-print on every WINCH.
            "trap 'stty size' WINCH; stty size; while :; do sleep 0.2; done".into(),
        ])
    }));
    let mut ch = terminal.attach(SessionId::new()).await.unwrap();

    // Initial geometry: the fixed default (120x40 → "40 120").
    let mut seen = String::new();
    while !seen.contains("40 120") {
        let chunk = tokio::time::timeout(Duration::from_secs(5), ch.output.recv())
            .await
            .expect("initial size within 5s")
            .expect("bridge open");
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }

    ch.resize
        .send(TerminalSize {
            cols: 200,
            rows: 50,
        })
        .await
        .expect("resize accepted");

    while !seen.contains("50 200") {
        let chunk = tokio::time::timeout(Duration::from_secs(5), ch.output.recv())
            .await
            .expect("WINCH-reported size within 5s of resize")
            .expect("bridge open");
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }
}

// C-T2: a bridge that cannot spawn yields a clear reason, never a silent blank view.
#[tokio::test]
async fn process_terminal_reports_spawn_failure_with_reason() {
    use daedalus_zellij::{AttachError, ProcessTerminal, TerminalAttach};
    let terminal =
        ProcessTerminal::new(Box::new(|_| Ok(vec!["/nonexistent/attach-bridge".into()])));
    let err = terminal.attach(SessionId::new()).await.unwrap_err();
    assert!(matches!(err, AttachError::ZellijUnavailable(_)), "{err}");
}

// C-T2: an unresolvable session (unknown / no environment) propagates its reason.
#[tokio::test]
async fn process_terminal_propagates_resolver_error() {
    use daedalus_zellij::{AttachError, ProcessTerminal, TerminalAttach};
    let terminal = ProcessTerminal::new(Box::new(|_| Err(AttachError::NotFound)));
    assert!(matches!(
        terminal.attach(SessionId::new()).await.unwrap_err(),
        AttachError::NotFound
    ));
}

#[tokio::test]
async fn c_t4_captured_stream_is_redacted() {
    let fx = Fixture::new();
    let session = SessionId::new();
    fx.terminal.seed(
        session,
        vec![Bytes::from_static(b"password=hunter2 done\n")],
    );

    let mut channel = fx.core.attach_and_capture(session).await.unwrap();
    let chunk = channel.output.recv().await.unwrap();
    let text = String::from_utf8_lossy(&chunk);
    assert!(text.contains("[REDACTED]"));
    assert!(
        !text.contains("hunter2"),
        "secret never reaches the view or persistence"
    );
}
