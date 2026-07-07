//! Unit tests for secret redaction-on-capture (T015, FR-031).

use daedalus_core::redact::{redact_bytes, redact_str, Redactor};

/// Drive a sequence of byte chunks through a single [`Redactor`] and return the
/// concatenation of every emitted `push` plus the final `flush` remainder.
fn drive(chunks: &[&[u8]]) -> Vec<u8> {
    let mut r = Redactor::new();
    let mut out = Vec::new();
    for chunk in chunks {
        out.extend_from_slice(&r.push(chunk));
    }
    out.extend_from_slice(&r.flush());
    out
}

#[test]
fn redactor_secret_split_across_chunk_boundary() {
    // `export API_KEY=secret` split mid-key: the second chunk alone parses the key as
    // "KEY" (not sensitive) under the stateless path and leaks. Stateful framing must
    // buffer the partial line and redact the whole `API_KEY=` assignment.
    let out = drive(&[b"export API_", b"KEY=secret\n"]);
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("[REDACTED]"), "got: {text:?}");
    assert!(!text.contains("secret"), "secret leaked: {text:?}");
}

#[test]
fn redactor_pem_block_split_across_chunks() {
    // A private-key block whose BEGIN/body/END arrive in separate chunks must stay
    // fully redacted — the in-block state has to carry across `push` calls.
    let out = drive(&[
        b"-----BEGIN RSA PRIVATE KEY-----\nMIIEow",
        b"IBAAKCAQEA\nsecretline\n",
        b"-----END RSA PRIVATE KEY-----\n",
    ]);
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("-----BEGIN RSA PRIVATE KEY-----"));
    assert!(text.contains("-----END RSA PRIVATE KEY-----"));
    assert!(
        !text.contains("MIIEowIBAAKCAQEA"),
        "PEM body leaked: {text:?}"
    );
    assert!(!text.contains("secretline"), "PEM body leaked: {text:?}");
}

#[test]
fn redactor_multibyte_codepoint_split_across_boundary() {
    // "café\n" where the two bytes of 'é' (0xC3 0xA9) land in different chunks must be
    // reassembled intact — the stateless per-chunk lossy decode inserts U+FFFD instead.
    let bytes = "café\n".as_bytes(); // c a f 0xC3 0xA9 \n
    let split = bytes.len() - 2; // between 0xC3 and 0xA9 of 'é'
    let out = drive(&[&bytes[..split], &bytes[split..]]);
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("café"), "codepoint corrupted: {text:?}");
    assert!(
        !text.contains('\u{FFFD}'),
        "replacement char present: {text:?}"
    );
}

#[test]
fn redactor_per_line_redaction_within_one_chunk_unchanged() {
    // A complete secret line in a single chunk is still redacted exactly as before.
    let out = drive(&[b"password=hunter2\n"]);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text, "password=[REDACTED]\n");
}

#[test]
fn redacts_secret_key_values() {
    assert_eq!(redact_str("password=hunter2"), "password=[REDACTED]");
    assert!(!redact_str("AWS_SECRET_ACCESS_KEY: wJalrXUtnFEMI").contains("wJalrXUtnFEMI"));
}

#[test]
fn redacts_credential_prefixed_tokens() {
    let out = redact_str("using ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 now");
    assert!(out.contains("[REDACTED]"));
    assert!(!out.contains("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"));
}

#[test]
fn bytes_helper_matches_string_helper() {
    let input = b"token=supersecretvalue123\n";
    let redacted = redact_bytes(input);
    assert!(!redacted.iter().eq(input.iter()));
    assert!(String::from_utf8_lossy(&redacted).contains("[REDACTED]"));
}

#[test]
fn ordinary_build_output_is_untouched() {
    let ordinary = "Compiling daedalus-core v0.1.0\nFinished in 3.2s\n";
    assert_eq!(redact_str(ordinary), ordinary);
}
