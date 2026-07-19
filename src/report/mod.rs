pub mod markdown;
pub mod pdf;

use std::path::Path;

use crate::config::RedactConfig;
use crate::group::group_items;
use crate::model::{ActivityItem, BlockerKind, BlockerSignal, Confidence, Report};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    Md,
    Pdf,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Md => "md",
            OutputFormat::Pdf => "pdf",
        }
    }
}

/// Explicit --format wins; else a .pdf extension (case-insensitive); else Markdown.
pub fn resolve_format(explicit: Option<OutputFormat>, path: Option<&Path>) -> OutputFormat {
    if let Some(f) = explicit {
        return f;
    }
    match path.and_then(|p| p.extension()).and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("pdf") => OutputFormat::Pdf,
        _ => OutputFormat::Md,
    }
}

/// The single file writer for both CLI and TUI. One redaction boundary:
/// Md redacts the rendered string; Pdf redacts inside collect_lines.
pub fn write_report_file(
    report: &crate::model::Report,
    path: &Path,
    format: OutputFormat,
    redact_cfg: &RedactConfig,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Md => {
            let md = crate::redact::apply(&markdown::render(report, false), redact_cfg);
            std::fs::write(path, md)?;
        }
        OutputFormat::Pdf => {
            let bytes =
                pdf::render(report, redact_cfg).map_err(|e| anyhow::anyhow!("pdf render: {e}"))?;
            std::fs::write(path, bytes)?;
        }
    }
    Ok(())
}

const EXPLICIT_BLOCKER_SIGNALS: &[&str] = &[
    "blocked",
    "failing-check",
    "requested-changes",
    "waiting-review",
];
const NEXT_UP_SIGNALS: &[&str] = &[
    "assigned-active",
    "awaiting-action",
    "high-priority",
    "unmerged",
    "uncommitted-changes",
];

pub fn build(
    items: Vec<ActivityItem>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    warnings: Vec<String>,
) -> Report {
    let worked_on: Vec<String> = items.iter().map(describe_item).collect();

    let mut next_up = Vec::new();
    for item in &items {
        if item
            .signals
            .iter()
            .any(|s| NEXT_UP_SIGNALS.contains(&s.as_str()))
        {
            let explicit = matches!(
                item.status.as_deref(),
                Some("assigned") | Some("in_progress")
            );
            let label = if explicit { "" } else { " (inferred)" };
            next_up.push(format!("{}{}", describe_item(item), label));
        }
    }

    let mut blockers = Vec::new();
    for item in &items {
        for sig in &item.signals {
            if EXPLICIT_BLOCKER_SIGNALS.contains(&sig.as_str()) {
                blockers.push(BlockerSignal {
                    kind: BlockerKind::Explicit,
                    reason: format!("{}: {}", sig, item.title),
                    source_item_url: item.url.clone(),
                    confidence: Confidence::High,
                });
            } else if sig == "stale-branch" {
                blockers.push(BlockerSignal {
                    kind: BlockerKind::Inferred,
                    reason: format!("possibly stalled: {}", item.title),
                    source_item_url: item.url.clone(),
                    confidence: Confidence::Low,
                });
            }
        }
    }

    let source_links: Vec<String> = items.iter().filter_map(|i| i.url.clone()).collect();
    let groups = group_items(items);

    Report {
        window_start,
        window_end,
        groups,
        summary: None,
        worked_on,
        next_up,
        blockers,
        source_links,
        generation_warnings: warnings,
    }
}

fn describe_item(item: &ActivityItem) -> String {
    let repo = item.repo.as_deref().unwrap_or("");
    if repo.is_empty() {
        format!("[{:?}] {}", item.source, item.title)
    } else {
        format!("[{}] {}", repo, item.title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    fn item(title: &str, signals: &[&str]) -> ActivityItem {
        ActivityItem {
            source: Source::Github,
            source_id: title.into(),
            title: title.into(),
            url: Some(format!("https://x/{title}")),
            project_key: None,
            repo: Some("devday".into()),
            activity_type: "pr".into(),
            status: None,
            actor: None,
            timestamp: Utc::now(),
            summary: None,
            signals: signals.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn explicit_blocker_is_classified_high_confidence() {
        let r = build(
            vec![item("PR fails CI", &["failing-check"])],
            Utc::now(),
            Utc::now(),
            vec![],
        );
        assert_eq!(r.blockers.len(), 1);
        assert_eq!(r.blockers[0].kind, BlockerKind::Explicit);
        assert_eq!(r.blockers[0].confidence, Confidence::High);
    }

    #[test]
    fn stale_branch_is_classified_inferred_low_confidence() {
        let r = build(
            vec![item("old feature branch", &["stale-branch"])],
            Utc::now(),
            Utc::now(),
            vec![],
        );
        assert_eq!(r.blockers.len(), 1);
        assert_eq!(r.blockers[0].kind, BlockerKind::Inferred);
        assert_eq!(r.blockers[0].confidence, Confidence::Low);
    }

    #[test]
    fn next_up_labels_inferred_items() {
        let r = build(
            vec![item("branch WIP", &["uncommitted-changes"])],
            Utc::now(),
            Utc::now(),
            vec![],
        );
        assert_eq!(r.next_up.len(), 1);
        assert!(r.next_up[0].contains("(inferred)"));
    }

    #[test]
    fn worked_on_lists_every_item() {
        let r = build(
            vec![item("a", &[]), item("b", &[])],
            Utc::now(),
            Utc::now(),
            vec![],
        );
        assert_eq!(r.worked_on.len(), 2);
    }

    #[test]
    fn resolve_format_table() {
        use std::path::Path;
        assert_eq!(resolve_format(None, None), OutputFormat::Md);
        assert_eq!(
            resolve_format(None, Some(Path::new("r.md"))),
            OutputFormat::Md
        );
        assert_eq!(
            resolve_format(None, Some(Path::new("r.pdf"))),
            OutputFormat::Pdf
        );
        assert_eq!(
            resolve_format(None, Some(Path::new("r.PDF"))),
            OutputFormat::Pdf
        );
        assert_eq!(
            resolve_format(Some(OutputFormat::Md), Some(Path::new("r.pdf"))),
            OutputFormat::Md
        );
        assert_eq!(
            resolve_format(Some(OutputFormat::Pdf), None),
            OutputFormat::Pdf
        );
    }
}
