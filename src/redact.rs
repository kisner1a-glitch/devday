use crate::config::RedactConfig;

const TOKEN_PREFIXES: &[&str] = &["ghp_", "gho_", "xoxb-", "xoxp-", "lin_api_"];

/// Always-on: replace any token-shaped substring with a redaction marker.
pub fn scrub_secrets(text: &str) -> String {
    let mut out = text.to_string();
    for prefix in TOKEN_PREFIXES {
        out = redact_prefix(&out, prefix);
    }
    out
}

fn redact_prefix(text: &str, prefix: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(prefix) {
        result.push_str(&rest[..pos]);
        let after = &rest[pos + prefix.len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(after.len());
        result.push_str("[REDACTED]");
        rest = &after[end..];
    }
    result.push_str(rest);
    result
}

pub fn apply(text: &str, cfg: &RedactConfig) -> String {
    let mut out = scrub_secrets(text);
    if cfg.hide_local_paths {
        if let Some(home) = std::env::var_os("HOME") {
            out = out.replace(&home.to_string_lossy().to_string(), "~");
        }
    }
    for pat in &cfg.extra_patterns {
        out = out.replace(pat, "[REDACTED]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_github_token() {
        let s = scrub_secrets("token is ghp_abc123DEF456 ok");
        assert!(s.contains("[REDACTED]"));
        assert!(!s.contains("ghp_abc123"));
    }

    #[test]
    fn redacts_slack_bot_token() {
        let s = scrub_secrets("xoxb-111-222-abcXYZ trailing");
        assert!(!s.contains("xoxb-111"));
        assert!(s.contains("trailing"));
    }

    #[test]
    fn leaves_clean_text_untouched() {
        assert_eq!(scrub_secrets("nothing to see here"), "nothing to see here");
    }

    #[test]
    fn extra_patterns_and_paths() {
        let cfg = RedactConfig {
            hide_local_paths: false,
            hide_private_repos: false,
            extra_patterns: vec!["secretword".into()],
        };
        assert!(!apply("has secretword in it", &cfg).contains("secretword"));
    }
}
