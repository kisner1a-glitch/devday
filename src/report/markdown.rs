use crate::model::Report;

/// Render a Report to Markdown with the 5 required sections.
/// Deterministic: no timestamps of "now", stable ordering from the Report.
pub fn render(report: &Report, verbose: bool) -> String {
    let mut out = String::new();
    out.push_str("# Status Report\n\n");
    out.push_str(&format!(
        "_Window: {} → {}_\n\n",
        report.window_start.format("%Y-%m-%d %H:%M UTC"),
        report.window_end.format("%Y-%m-%d %H:%M UTC")
    ));

    out.push_str("## Summary\n\n");
    match &report.summary {
        Some(s) => out.push_str(&format!("{s}\n\n")),
        None if report.worked_on.is_empty() => out.push_str("_No activity in this window._\n\n"),
        None => out.push_str(&format!(
            "{} item(s) across {} group(s).\n\n",
            report.worked_on.len(),
            report.groups.len()
        )),
    }

    section(&mut out, "Worked On", &report.worked_on);

    section(&mut out, "Next Up", &report.next_up);

    out.push_str("## Blockers\n\n");
    if report.blockers.is_empty() {
        out.push_str("_None detected._\n\n");
    } else {
        for b in &report.blockers {
            let tag = match b.kind {
                crate::model::BlockerKind::Explicit => "explicit",
                crate::model::BlockerKind::Inferred => "inferred",
            };
            out.push_str(&format!("- [{tag}] {}\n", b.reason));
        }
        out.push('\n');
    }

    out.push_str("## Source Links\n\n");
    if report.source_links.is_empty() {
        out.push_str("_None._\n");
    } else {
        for link in &report.source_links {
            out.push_str(&format!("- {link}\n"));
        }
    }

    if verbose && !report.generation_warnings.is_empty() {
        out.push_str("\n## Warnings\n\n");
        for w in &report.generation_warnings {
            out.push_str(&format!("- {w}\n"));
        }
    }

    out
}

fn section(out: &mut String, title: &str, lines: &[String]) {
    out.push_str(&format!("## {title}\n\n"));
    if lines.is_empty() {
        out.push_str("_Nothing._\n\n");
    } else {
        for l in lines {
            out.push_str(&format!("- {l}\n"));
        }
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn empty_report() -> Report {
        Report {
            window_start: Utc.with_ymd_and_hms(2026, 7, 15, 0, 0, 0).unwrap(),
            window_end: Utc.with_ymd_and_hms(2026, 7, 16, 0, 0, 0).unwrap(),
            groups: vec![],
            summary: None,
            worked_on: vec![],
            next_up: vec![],
            blockers: vec![],
            source_links: vec![],
            generation_warnings: vec![],
        }
    }

    #[test]
    fn empty_window_has_all_sections() {
        let md = render(&empty_report(), false);
        for h in [
            "## Summary",
            "## Worked On",
            "## Next Up",
            "## Blockers",
            "## Source Links",
        ] {
            assert!(md.contains(h), "missing {h}");
        }
        assert!(md.contains("No activity"));
    }

    #[test]
    fn render_is_deterministic() {
        let r = empty_report();
        assert_eq!(render(&r, false), render(&r, false));
    }
}
