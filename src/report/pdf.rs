//! PDF rendering of a Report. Three layers:
//!   collect_lines (redacted text runs) -> layout (wrap + paginate, pure) -> draw (printpdf).
//! Redaction happens in collect_lines, BEFORE any drawing — PDF bytes are
//! never post-scrubbed.

use printpdf::{BuiltinFont, Mm, PdfDocument};

use crate::config::RedactConfig;
use crate::model::{BlockerKind, Report};
use crate::redact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Title,
    Heading,
    Body,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfLine {
    pub style: LineStyle,
    pub text: String,
}

fn line(style: LineStyle, text: impl Into<String>) -> PdfLine {
    PdfLine {
        style,
        text: text.into(),
    }
}

/// Every string emitted here passes redact::apply — this is the redaction
/// boundary for PDF output.
pub fn collect_lines(report: &Report, cfg: &RedactConfig) -> Vec<PdfLine> {
    let r = |s: &str| redact::apply(s, cfg);
    let mut out = Vec::new();
    out.push(line(LineStyle::Title, "Status Report"));
    out.push(line(
        LineStyle::Body,
        format!(
            "Window: {} - {}",
            report.window_start.format("%Y-%m-%d %H:%M UTC"),
            report.window_end.format("%Y-%m-%d %H:%M UTC")
        ),
    ));

    out.push(line(LineStyle::Heading, "Summary"));
    match &report.summary {
        Some(s) => out.push(line(LineStyle::Body, r(s))),
        None if report.worked_on.is_empty() => {
            out.push(line(LineStyle::Body, "No activity in this window."))
        }
        None => out.push(line(
            LineStyle::Body,
            format!(
                "{} item(s) across {} group(s).",
                report.worked_on.len(),
                report.groups.len()
            ),
        )),
    }

    section(&mut out, "Worked On", &report.worked_on, &r);
    section(&mut out, "Next Up", &report.next_up, &r);

    out.push(line(LineStyle::Heading, "Blockers"));
    if report.blockers.is_empty() {
        out.push(line(LineStyle::Body, "None detected."));
    } else {
        for b in &report.blockers {
            let tag = match b.kind {
                BlockerKind::Explicit => "explicit",
                BlockerKind::Inferred => "inferred",
            };
            out.push(line(LineStyle::Body, format!("- [{tag}] {}", r(&b.reason))));
        }
    }

    out.push(line(LineStyle::Heading, "Source Links"));
    if report.source_links.is_empty() {
        out.push(line(LineStyle::Body, "None."));
    } else {
        for l in &report.source_links {
            out.push(line(LineStyle::Body, format!("- {}", r(l))));
        }
    }

    if !report.generation_warnings.is_empty() {
        out.push(line(LineStyle::Heading, "Warnings"));
        for w in &report.generation_warnings {
            out.push(line(LineStyle::Body, format!("- {}", r(w))));
        }
    }
    out
}

fn section(out: &mut Vec<PdfLine>, title: &str, items: &[String], r: &impl Fn(&str) -> String) {
    out.push(line(LineStyle::Heading, title));
    if items.is_empty() {
        out.push(line(LineStyle::Body, "Nothing."));
    } else {
        for item in items {
            out.push(line(LineStyle::Body, format!("- {}", r(item))));
        }
    }
}

/// Word-based wrap; a single word longer than max is hard-split.
pub fn wrap_words(text: &str, max: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.chars().count() + 1 + word.chars().count() <= max {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
        // Hard-split any oversized single word.
        while current.chars().count() > max {
            let head: String = current.chars().take(max).collect();
            let tail: String = current.chars().skip(max).collect();
            lines.push(head);
            current = tail;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

const PAGE_W: f64 = 210.0;
const PAGE_H: f64 = 297.0;
const MARGIN: f64 = 15.0;
const WRAP_CHARS: usize = 90;

fn style_metrics(style: LineStyle) -> (f64, f64, f64) {
    // (font size pt, line height mm, pre-gap mm)
    match style {
        LineStyle::Title => (20.0, 10.0, 0.0),
        LineStyle::Heading => (14.0, 8.0, 4.0),
        LineStyle::Body => (10.0, 5.0, 0.0),
    }
}

/// Pure pagination: wraps body lines and assigns y positions per page.
pub fn layout(lines: &[PdfLine]) -> Vec<Vec<(f64, PdfLine)>> {
    let mut pages: Vec<Vec<(f64, PdfLine)>> = vec![Vec::new()];
    let mut y = PAGE_H - MARGIN;
    for l in lines {
        let (_, lh, gap) = style_metrics(l.style);
        let wrapped = wrap_words(&l.text, WRAP_CHARS);
        for (i, piece) in wrapped.into_iter().enumerate() {
            let need = lh + if i == 0 { gap } else { 0.0 };
            if y - need < MARGIN {
                pages.push(Vec::new());
                y = PAGE_H - MARGIN;
            }
            y -= need;
            pages.last_mut().expect("pages never empty").push((
                y,
                PdfLine {
                    style: l.style,
                    text: piece,
                },
            ));
        }
    }
    pages
}

/// printpdf drawing over the laid-out pages.
fn draw(pages: &[Vec<(f64, PdfLine)>]) -> Result<Vec<u8>, String> {
    // printpdf 0.6's Mm/font-size API takes f32; layout math above stays in
    // f64, and we narrow at this drawing boundary only.
    let (doc, page1, layer1) = PdfDocument::new(
        "devday status report",
        Mm(PAGE_W as f32),
        Mm(PAGE_H as f32),
        "Layer 1",
    );
    let regular = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("font: {e}"))?;
    let bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("font: {e}"))?;

    for (i, page_lines) in pages.iter().enumerate() {
        let layer = if i == 0 {
            doc.get_page(page1).get_layer(layer1)
        } else {
            let (p, l) = doc.add_page(Mm(PAGE_W as f32), Mm(PAGE_H as f32), "Layer 1");
            doc.get_page(p).get_layer(l)
        };
        for (y, l) in page_lines {
            let (size, _, _) = style_metrics(l.style);
            let font = match l.style {
                LineStyle::Body => &regular,
                _ => &bold,
            };
            layer.use_text(
                l.text.clone(),
                size as f32,
                Mm(MARGIN as f32),
                Mm(*y as f32),
                font,
            );
        }
    }
    doc.save_to_bytes().map_err(|e| format!("pdf save: {e}"))
}

pub fn render(report: &Report, cfg: &RedactConfig) -> Result<Vec<u8>, String> {
    draw(&layout(&collect_lines(report, cfg)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockerSignal, Confidence};
    use chrono::{TimeZone, Utc};

    fn report() -> Report {
        Report {
            window_start: Utc.with_ymd_and_hms(2026, 7, 18, 0, 0, 0).unwrap(),
            window_end: Utc.with_ymd_and_hms(2026, 7, 19, 0, 0, 0).unwrap(),
            groups: vec![],
            summary: Some("summary with token ghp_PDFSECRET1 and SECRETWORD".into()),
            worked_on: vec!["did the thing".into()],
            next_up: vec![],
            blockers: vec![BlockerSignal {
                kind: BlockerKind::Explicit,
                reason: "failing-check: CI".into(),
                source_item_url: None,
                confidence: Confidence::High,
            }],
            source_links: vec!["https://example.com/pr/1".into()],
            generation_warnings: vec![],
        }
    }

    #[test]
    fn collects_all_sections_in_order() {
        let lines = collect_lines(&report(), &RedactConfig::default());
        let headings: Vec<&str> = lines
            .iter()
            .filter(|l| l.style == LineStyle::Heading)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(
            headings,
            [
                "Summary",
                "Worked On",
                "Next Up",
                "Blockers",
                "Source Links"
            ]
        );
        assert_eq!(lines[0].style, LineStyle::Title);
    }

    #[test]
    fn empty_report_has_empty_state_line() {
        let mut r = report();
        r.summary = None;
        r.worked_on.clear();
        let lines = collect_lines(&r, &RedactConfig::default());
        assert!(lines.iter().any(|l| l.text.contains("No activity")));
    }

    #[test]
    fn redaction_applies_before_layout() {
        let cfg = RedactConfig {
            hide_local_paths: false,
            hide_private_repos: false,
            extra_patterns: vec!["SECRETWORD".into()],
        };
        let joined: String = collect_lines(&report(), &cfg)
            .iter()
            .map(|l| l.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!joined.contains("ghp_PDFSECRET1"), "token leaked");
        assert!(!joined.contains("SECRETWORD"), "extra pattern leaked");
        assert!(joined.contains("[REDACTED]"));
    }

    #[test]
    fn wrap_words_wraps_and_hard_splits() {
        assert_eq!(wrap_words("a b c", 3), vec!["a b", "c"]);
        let long = "x".repeat(200);
        let pieces = wrap_words(&long, 90);
        assert_eq!(pieces.len(), 3);
        assert!(pieces.iter().all(|p| p.chars().count() <= 90));
    }

    #[test]
    fn layout_paginates_long_reports() {
        let mut r = report();
        r.worked_on = (0..200).map(|i| format!("item number {i}")).collect();
        let pages = layout(&collect_lines(&r, &RedactConfig::default()));
        assert!(
            pages.len() > 1,
            "expected multiple pages, got {}",
            pages.len()
        );
        // y positions decrease within a page and stay inside margins.
        for page in &pages {
            for pair in page.windows(2) {
                assert!(pair[1].0 < pair[0].0);
            }
            for (y, _) in page {
                assert!(*y >= MARGIN && *y <= PAGE_H - MARGIN);
            }
        }
    }

    #[test]
    fn render_produces_pdf_bytes() {
        let bytes = render(&report(), &RedactConfig::default()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "not a PDF header");
        assert!(bytes.len() > 500);
    }
}
