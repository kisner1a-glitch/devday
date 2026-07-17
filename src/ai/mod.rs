pub mod cli_provider;

use crate::config::AiConfig;
use crate::model::Report;

/// Build a prompt from normalized report data only. Never includes tokens,
/// headers, or raw payloads — only titles, group keys, and signals.
pub fn build_prompt(report: &Report) -> String {
    let mut p = String::new();
    p.push_str("Summarize this engineering status in 3-5 sentences. ");
    p.push_str("Answer: what was worked on, what's next, what's blocked.\n\n");
    p.push_str("Worked on:\n");
    for w in &report.worked_on {
        p.push_str(&format!("- {w}\n"));
    }
    p.push_str("\nNext up:\n");
    for n in &report.next_up {
        p.push_str(&format!("- {n}\n"));
    }
    p.push_str("\nBlockers:\n");
    for b in &report.blockers {
        p.push_str(&format!("- {}\n", b.reason));
    }
    p
}

/// Returns Some(summary) if AI succeeds, None on any failure or when disabled.
pub fn summarize(cfg: &AiConfig, report: &Report) -> Option<String> {
    let provider = cfg.provider.as_deref()?;
    let command = cfg
        .command
        .clone()
        .unwrap_or_else(|| default_command(provider));
    let args = if !cfg.args.is_empty() {
        // Explicit args always win.
        cfg.args.clone()
    } else if cfg.command.is_none() {
        // No override at all: use the provider's paired default command+args.
        default_args(provider)
    } else {
        // Command was overridden (e.g. to a stand-in binary in tests) but no
        // args given: don't inject the *other* provider's default flags.
        vec![]
    };
    let prompt = build_prompt(report);
    cli_provider::run(&command, &args, &prompt).ok()
}

fn default_command(provider: &str) -> String {
    match provider {
        "codex" => "codex".into(),
        _ => "claude".into(),
    }
}

fn default_args(provider: &str) -> Vec<String> {
    match provider {
        "codex" => vec!["exec".into(), "-".into()],
        _ => vec!["-p".into()], // claude reads the prompt from stdin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockerKind, BlockerSignal, Confidence, Report};
    use chrono::Utc;

    fn report() -> Report {
        Report {
            window_start: Utc::now(),
            window_end: Utc::now(),
            groups: vec![],
            summary: None,
            worked_on: vec!["[o/r] Add parser".into()],
            next_up: vec![],
            blockers: vec![BlockerSignal {
                kind: BlockerKind::Explicit,
                reason: "failing-check: CI".into(),
                source_item_url: None,
                confidence: Confidence::High,
            }],
            source_links: vec![],
            generation_warnings: vec![],
        }
    }

    #[test]
    fn prompt_contains_activity_not_secrets() {
        let p = build_prompt(&report());
        assert!(p.contains("Add parser"));
        assert!(p.contains("failing-check"));
        assert!(!p.to_lowercase().contains("authorization"));
        assert!(!p.contains("token"));
    }

    #[test]
    fn summarize_disabled_when_no_provider() {
        let cfg = AiConfig::default();
        assert!(summarize(&cfg, &report()).is_none());
    }

    #[test]
    fn summarize_uses_cat_stand_in() {
        // Point the "provider" at `cat` so stdout == prompt; proves the wiring.
        let cfg = AiConfig {
            provider: Some("claude".into()),
            command: Some("cat".into()),
            args: vec![],
        };
        let s = summarize(&cfg, &report()).unwrap();
        assert!(s.contains("Add parser"));
    }
}
