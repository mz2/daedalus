//! Secret redaction applied to captured output **before** it is streamed or persisted.
//!
//! Daedalus never prompts for, stores, or displays secrets (FR-031, research R9). The
//! operator pre-provisions secrets in the environment; this pass keeps any that surface in
//! terminal output out of the persisted record and the event stream by construction
//! (contracts C-A3, C-T4).
//!
//! The redactor is deliberately conservative and dependency-free (no regex): it targets the
//! shapes secrets most commonly take in terminal output. It can over-redact (safe) but is
//! designed not to silently leak the patterns below.

use bytes::Bytes;

const PLACEHOLDER: &str = "[REDACTED]";

/// Key fragments whose `KEY=VALUE` / `KEY: VALUE` value is always redacted.
const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "api-key",
    "access_key",
    "secret_key",
    "private_key",
    "client_secret",
    "auth",
    "credential",
];

/// Redact secret-shaped content from a UTF-8 string.
#[must_use]
pub fn redact_str(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_private_key_block = false;

    for line in input.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);

        // PEM / private-key blocks: redact the whole body between BEGIN/END markers.
        if trimmed.contains("-----BEGIN") && trimmed.contains("PRIVATE KEY-----") {
            in_private_key_block = true;
            out.push_str(line);
            continue;
        }
        if in_private_key_block {
            if trimmed.contains("-----END") && trimmed.contains("PRIVATE KEY-----") {
                in_private_key_block = false;
                out.push_str(line);
            } else {
                push_redacted_line(&mut out, line, trimmed);
            }
            continue;
        }

        out.push_str(&redact_line(trimmed));
        // Preserve the original line terminator.
        if let Some(term) = line.strip_prefix(trimmed) {
            out.push_str(term);
        }
    }

    out
}

fn push_redacted_line(out: &mut String, original: &str, trimmed: &str) {
    out.push_str(PLACEHOLDER);
    if let Some(term) = original.strip_prefix(trimmed) {
        out.push_str(term);
    }
}

fn redact_line(line: &str) -> String {
    // 1) KEY=VALUE / KEY: VALUE where KEY looks sensitive → redact the value.
    if let Some(redacted) = redact_key_value(line) {
        return redacted;
    }

    // 2) High-entropy / long token-shaped words → redact the token only.
    line.split(' ')
        .map(|word| {
            if looks_like_secret_token(word) {
                PLACEHOLDER.to_string()
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_key_value(line: &str) -> Option<String> {
    for sep in ['=', ':'] {
        if let Some(idx) = line.find(sep) {
            let (key, rest) = line.split_at(idx);
            let key_l = key.trim().to_ascii_lowercase();
            if SENSITIVE_KEYS
                .iter()
                .any(|k| key_l.ends_with(k) || key_l == *k)
            {
                let value = &rest[1..];
                if value.trim().is_empty() {
                    return None;
                }
                let lead_ws: String = value.chars().take_while(|c| c.is_whitespace()).collect();
                return Some(format!("{key}{sep}{lead_ws}{PLACEHOLDER}"));
            }
        }
    }
    None
}

/// Heuristic: a "secret token" is a long word with mixed character classes, or a recognised
/// credential prefix (AWS, GitHub, Slack, bearer tokens, etc.).
fn looks_like_secret_token(word: &str) -> bool {
    let w = word.trim_matches(|c: char| c == '"' || c == '\'' || c == ',' || c == ';');
    if w.len() < 20 {
        // Short tokens are too noisy to redact, except known short-prefixed ones below.
        return matches!(
            w.get(0..4),
            Some("AKIA") | Some("ghp_") | Some("xox") | Some("ghs_")
        );
    }
    let known_prefix = [
        "AKIA", "ASIA", "ghp_", "gho_", "ghs_", "xoxb", "xoxp", "sk-", "AIza",
    ]
    .iter()
    .any(|p| w.starts_with(p));
    if known_prefix {
        return true;
    }
    // Mixed-class, long, no spaces ⇒ likely a key.
    let has_lower = w.chars().any(|c| c.is_ascii_lowercase());
    let has_upper = w.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = w.chars().any(|c| c.is_ascii_digit());
    let alnum_like = w
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_-+/=.".contains(c));
    alnum_like && has_lower && has_upper && has_digit && w.len() >= 24
}

/// Redact secret-shaped content from a byte chunk (lossy-UTF8 then redact).
#[must_use]
pub fn redact_bytes(input: &[u8]) -> Bytes {
    let text = String::from_utf8_lossy(input);
    Bytes::from(redact_str(&text).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_key_value_secrets() {
        assert_eq!(redact_str("password=hunter2"), "password=[REDACTED]");
        assert_eq!(
            redact_str("AWS_SECRET_ACCESS_KEY: abcd/efgh"),
            "AWS_SECRET_ACCESS_KEY: [REDACTED]"
        );
        assert_eq!(
            redact_str("export API_KEY=foo"),
            "export API_KEY=[REDACTED]"
        );
    }

    #[test]
    fn redacts_known_credential_prefixes() {
        let line = "token AKIAIOSFODNN7EXAMPLE used";
        assert!(redact_str(line).contains(PLACEHOLDER));
        assert!(!redact_str(line).contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn redacts_private_key_block() {
        let pem = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\nsecretline\n-----END RSA PRIVATE KEY-----\n";
        let out = redact_str(pem);
        assert!(out.contains("-----BEGIN RSA PRIVATE KEY-----"));
        assert!(out.contains("-----END RSA PRIVATE KEY-----"));
        assert!(!out.contains("MIIEowIBAAKCAQEA"));
        assert!(!out.contains("secretline"));
    }

    #[test]
    fn leaves_ordinary_output_untouched() {
        let ordinary = "Compiling daedalus-core v0.1.0\n  Finished in 3.2s\n";
        assert_eq!(redact_str(ordinary), ordinary);
    }

    #[test]
    fn preserves_newlines() {
        assert_eq!(redact_str("a\nb\n"), "a\nb\n");
    }
}
