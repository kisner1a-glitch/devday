# devday PDF Report Output Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `devday report --output report.pdf` (and the TUI `w` key with a `.pdf` config output) writes a redacted PDF; `--format md|pdf` overrides extension inference; Markdown behavior stays byte-identical.

**Architecture:** New pure-Rust renderer `src/report/pdf.rs` with three testable layers — `collect_lines` (redacted text runs from the `Report`), `layout` (wrap + paginate, pure), `draw` (printpdf, built-in Helvetica). A shared `OutputFormat`/`resolve_format`/`write_report_file` in `src/report/mod.rs` serves CLI and TUI through one writer and one redaction boundary.

**Tech Stack:** existing devday lib + `printpdf = "0.6"` (built-in fonts, `save_to_bytes`).

**Spec:** `docs/superpowers/specs/2026-07-18-devday-pdf-output-design.md`
**Linear:** CHA-2453 (single ticket; controller updates status).

## Global Constraints

- Rust 2021; MSRV 1.75+. Branch `feat/devday-pdf`.
- Every task ends with `cargo test` green, `cargo clippy --all-targets -- -D warnings` clean, `cargo fmt` run, before commit.
- Markdown output and all existing behavior byte-identical: the 104 existing tests stay green untouched.
- NO SECRETS IN PDF BYTES: every text run passes `redact::apply` inside `collect_lines`, BEFORE layout/draw. PDF bytes are never post-scrubbed.
- `--stdout` always prints Markdown; `--format pdf` without an output path (flag or config) is a clear error; explicit `--format` beats extension inference.
- New dependency: `printpdf` only.
- printpdf 0.6 API note: if the resolved 0.6.x differs in small ways (f32 vs f64 sizes, `save_to_bytes` availability), adapt mechanically without changing the three-layer architecture; if `save_to_bytes` is absent, use `doc.save(&mut std::io::BufWriter::new(&mut bytes))`.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/report/pdf.rs` | `PdfLine`/`LineStyle`, `collect_lines`, `wrap_words`, `layout`, `draw`, `render` |
| `src/report/mod.rs` | + `OutputFormat`, `resolve_format`, `write_report_file` |
| `src/cli.rs` | + `--format` flag on `ReportArgs` |
| `src/main.rs` | `run_report` uses resolve + shared writer |
| `src/tui/mod.rs` | `write_report` effect uses resolve + shared writer |
| `tests/pdf_output.rs` | CLI integration: pdf write, error case, md override |
| `README.md`, `docs/ACCEPTANCE.md` | Output-formats docs + acceptance rows |

---

## Task 1: PDF renderer (`src/report/pdf.rs`) — CHA-2453

**Files:**
- Create: `src/report/pdf.rs`
- Modify: `Cargo.toml` (add `printpdf = "0.6"`), `src/report/mod.rs` (add `pub mod pdf;`)
- Test: inline in `src/report/pdf.rs`

**Interfaces:**
- Consumes: `model::Report`, `config::RedactConfig`, `redact::apply`.
- Produces: `pdf::render(report: &Report, redact: &RedactConfig) -> Result<Vec<u8>, String>`; internals `collect_lines(report, redact) -> Vec<PdfLine>`, `wrap_words(text, max) -> Vec<String>`, `layout(lines) -> Vec<Vec<(f64, PdfLine)>>` all pub(crate)-or-private but unit-tested in-module. `PdfLine { style: LineStyle, text: String }`, `LineStyle { Title, Heading, Body }`.

- [ ] **Step 1: Add dependency**

```toml
printpdf = "0.6"
```

- [ ] **Step 2: Write `src/report/pdf.rs`** (code + tests together; tests drive the pure layers)

```rust
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
    PdfLine { style, text: text.into() }
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
            pages
                .last_mut()
                .expect("pages never empty")
                .push((y, PdfLine { style: l.style, text: piece }));
        }
    }
    pages
}

/// printpdf drawing over the laid-out pages.
fn draw(pages: &[Vec<(f64, PdfLine)>]) -> Result<Vec<u8>, String> {
    let (doc, page1, layer1) =
        PdfDocument::new("devday status report", Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
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
            let (p, l) = doc.add_page(Mm(PAGE_W), Mm(PAGE_H), "Layer 1");
            doc.get_page(p).get_layer(l)
        };
        for (y, l) in page_lines {
            let (size, _, _) = style_metrics(l.style);
            let font = match l.style {
                LineStyle::Body => &regular,
                _ => &bold,
            };
            layer.use_text(l.text.clone(), size, Mm(MARGIN), Mm(*y), font);
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
        assert_eq!(headings, ["Summary", "Worked On", "Next Up", "Blockers", "Source Links"]);
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
        assert!(pages.len() > 1, "expected multiple pages, got {}", pages.len());
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
```

- [ ] **Step 3: Register module** — add `pub mod pdf;` to `src/report/mod.rs`.

- [ ] **Step 4: Run gates**

Run: `cargo test report::pdf` then full `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: 6 new tests + 104 existing, all green.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/report/pdf.rs src/report/mod.rs
git commit -m "feat: pure-Rust PDF renderer with redact-before-draw (CHA-2453)"
```

---

## Task 2: Format resolution, shared writer, CLI wiring — CHA-2453

**Files:**
- Modify: `src/report/mod.rs`, `src/cli.rs`, `src/main.rs`
- Create: `tests/pdf_output.rs`

**Interfaces:**
- Consumes: `pdf::render` (Task 1), `markdown::render`, `redact::apply`.
- Produces: `report::OutputFormat { Md, Pdf }` (derives `Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum` + `as_str(&self) -> &'static str`); `report::resolve_format(explicit: Option<OutputFormat>, path: Option<&Path>) -> OutputFormat`; `report::write_report_file(report: &Report, path: &Path, format: OutputFormat, redact: &RedactConfig) -> anyhow::Result<()>`; `cli::ReportArgs.format: Option<report::OutputFormat>` (`--format`, value_enum).

- [ ] **Step 1: Add to `src/report/mod.rs`**

```rust
use std::path::Path;

use crate::config::RedactConfig;

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
            let bytes = pdf::render(report, redact_cfg)
                .map_err(|e| anyhow::anyhow!("pdf render: {e}"))?;
            std::fs::write(path, bytes)?;
        }
    }
    Ok(())
}
```

With tests in `report/mod.rs`'s test module:

```rust
    #[test]
    fn resolve_format_table() {
        use std::path::Path;
        assert_eq!(resolve_format(None, None), OutputFormat::Md);
        assert_eq!(resolve_format(None, Some(Path::new("r.md"))), OutputFormat::Md);
        assert_eq!(resolve_format(None, Some(Path::new("r.pdf"))), OutputFormat::Pdf);
        assert_eq!(resolve_format(None, Some(Path::new("r.PDF"))), OutputFormat::Pdf);
        assert_eq!(
            resolve_format(Some(OutputFormat::Md), Some(Path::new("r.pdf"))),
            OutputFormat::Md
        );
        assert_eq!(resolve_format(Some(OutputFormat::Pdf), None), OutputFormat::Pdf);
    }
```

- [ ] **Step 2: CLI flag in `src/cli.rs`**

Add to `ReportArgs`:

```rust
    /// Output format for --output (default: inferred from extension).
    #[arg(long, value_enum)]
    pub format: Option<crate::report::OutputFormat>,
```

Add test:

```rust
    #[test]
    fn format_flag_parses() {
        let cli = Cli::try_parse_from(["devday", "report", "--format", "pdf"]).unwrap();
        match cli.command {
            Command::Report(a) => assert_eq!(a.format, Some(crate::report::OutputFormat::Pdf)),
            _ => panic!("expected report"),
        }
    }
```

- [ ] **Step 3: `src/main.rs` `run_report` output section**

Replace the current output-path/write/stdout block with:

```rust
    let output_path = args
        .output
        .clone()
        .or_else(|| cfg.output.clone().map(std::path::PathBuf::from));
    let format = report::resolve_format(args.format, output_path.as_deref());
    if format == report::OutputFormat::Pdf && output_path.is_none() {
        anyhow::bail!("pdf output requires --output <path> (or output in config)");
    }
    if let Some(path) = &output_path {
        report::write_report_file(&rep, path, format, &cfg.redact)?;
    }
    if args.stdout || output_path.is_none() {
        let md = redact::apply(&report::markdown::render(&rep, false), &cfg.redact);
        print!("{md}");
    }
```

(Markdown-to-stdout behavior unchanged; the file write now goes through the shared writer.)

- [ ] **Step 4: Write `tests/pdf_output.rs`**

```rust
use assert_cmd::Command;
use predicates::str::contains;

fn devday(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("devday").unwrap();
    c.env("HOME", home);
    c
}

#[test]
fn output_pdf_extension_writes_pdf() {
    let home = tempfile::tempdir().unwrap();
    let out = home.path().join("report.pdf");
    devday(home.path())
        .args(["report", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"%PDF-"), "expected PDF header");
}

#[test]
fn format_pdf_without_output_errors() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["report", "--format", "pdf"])
        .assert()
        .failure()
        .stderr(contains("requires --output"));
}

#[test]
fn explicit_md_beats_pdf_extension() {
    let home = tempfile::tempdir().unwrap();
    let out = home.path().join("report.pdf");
    devday(home.path())
        .args(["report", "--format", "md", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("# Status Report"));
}
```

- [ ] **Step 5: Run gates**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: all green (Task 1 tests + resolve table + cli test + 3 integration).

- [ ] **Step 6: Commit**

```bash
git add src/report/mod.rs src/cli.rs src/main.rs tests/pdf_output.rs
git commit -m "feat: --format flag, .pdf inference, shared report writer (CHA-2453)"
```

---

## Task 3: TUI `w` support, docs, final gate — CHA-2453

**Files:**
- Modify: `src/tui/mod.rs` (write_report effect), `README.md`, `docs/ACCEPTANCE.md`
- Test: extend TUI tests only if an existing one asserts the old status string.

**Interfaces:**
- Consumes: `report::{resolve_format, write_report_file, OutputFormat}`.
- Produces: no new interfaces — behavior change to the existing `Effect::WriteReport` handler.

- [ ] **Step 1: Rewire `write_report` in `src/tui/mod.rs`**

Replace the body so it uses the shared writer with extension inference:

```rust
fn write_report(app: &mut App) -> String {
    let Some(rep) = &app.report.report else {
        return "no report to write".into();
    };
    let path = std::path::PathBuf::from(
        app.cfg.output.clone().unwrap_or_else(|| "report.md".into()),
    );
    let format = crate::report::resolve_format(None, Some(&path));
    match crate::report::write_report_file(rep, &path, format, &app.cfg.redact) {
        Ok(()) => format!("wrote {} ({})", path.display(), format.as_str()),
        Err(e) => format!("write failed: {e}"),
    }
}
```

Fix any existing TUI test that asserted the old `wrote {path}` status text (search `src/tui` for `"wrote "` assertions and update to the new format string).

- [ ] **Step 2: README**

In the output/usage section add an "Output formats" note: Markdown default; `--output report.pdf` infers PDF; `--format md|pdf` overrides; `--stdout` is always Markdown; PDF is pure-Rust (no external tools) with the same five sections and full redaction. Mention the TUI `w` key follows the config `output` extension. Keep claims exactly matching behavior.

- [ ] **Step 3: ACCEPTANCE.md**

Add rows: pdf write → `output_pdf_extension_writes_pdf`; pdf-without-output error → `format_pdf_without_output_errors`; explicit override → `explicit_md_beats_pdf_extension`; redaction-in-pdf → `report::pdf::tests::redaction_applies_before_layout`; manual: open the generated PDF in a viewer once.

- [ ] **Step 4: Final gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: everything green (target ≈ 114 tests). Also run a real end-to-end: `cargo run -- report --output /tmp/devday-test.pdf` and confirm the file opens as a PDF (`file /tmp/devday-test.pdf` says "PDF document"), then delete it.

- [ ] **Step 5: Commit**

```bash
git add src/tui/mod.rs README.md docs/ACCEPTANCE.md
git commit -m "feat: TUI w writes pdf via shared writer; document output formats (CHA-2453)"
```

---

## Self-Review Notes

**Spec coverage:** §3 renderer layers → T1; §3 mod.rs helpers + §4 CLI rules → T2; §5 TUI → T3; §6 invariants → T1 redaction test + T2 stdout/error rules; §7 tests → T1 unit, T2 table + integration, T3 docs/manual. Out-of-scope items untouched.

**Type consistency:** `OutputFormat`/`resolve_format`/`write_report_file` names match across T2 producers and T2/T3 consumers; `pdf::render(report, cfg) -> Result<Vec<u8>, String>` matches `write_report_file`'s `map_err` usage; `PdfLine`/`LineStyle` internal to T1.

**Known simplifications (documented):** links render as plain text (no clickable annotations); no byte-level golden test (document metadata varies — content-line tests are the contract); `send slack` gains a harmless unused `--format` via flattened args (documented spec §4).
