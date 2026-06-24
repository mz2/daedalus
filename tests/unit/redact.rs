//! Unit tests for secret redaction-on-capture (T015, FR-031).

use daedalus_core::redact::{redact_bytes, redact_str};

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
