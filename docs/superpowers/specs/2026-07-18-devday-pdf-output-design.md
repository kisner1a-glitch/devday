# devday PDF Report Output — Technical Design

Status: Approved for planning
Date: 2026-07-18
Builds on: CLI MVP + TUI (both merged to master; 104 tests)

## 1. Purpose

Add PDF as a report output format: `devday report --output report.pdf` (or the
config `output` path, or the TUI `w` key) writes a PDF instead of Markdown.
Markdown remains the default and the only stdout format. No new data, no new
sections — the PDF renders the same `Report` the Markdown renderer does.

## 2. Decisions

| Decision | Choice | Rationale |
| --- | --- | --- |
| PDF engine | Pure Rust, `printpdf` crate, built-in Helvetica fonts | No external tools or font files; keeps the single-binary, cron-friendly promise. Simple clean output, not fancy typography |
| Format selection | Extension inference (`.pdf`, case-insensitive) PLUS explicit `--format md\|pdf` override | `--output report.pdf` "just works"; `--format` covers edge cases and wins over the extension |
| Render source | The `Report` struct directly | Same data as Markdown renderer; no Markdown parsing |
| Redaction | Applied to every text run BEFORE drawing | PDF bytes cannot be scrubbed after the fact; the redaction boundary moves in front of layout |
| New dependency | `printpdf` only | Minimal surface |

## 3. Module design

### `src/report/pdf.rs` (new)

Two-layer split so the text content is testable without parsing PDF bytes:

1. `collect_lines(report: &Report, redact: &RedactConfig) -> Vec<PdfLine>` —
   pure. Emits every text run in order: title, window line, then the five
   sections (Summary, Worked On, Next Up, Blockers, Source Links) and
   generation warnings, mirroring the Markdown renderer's content and
   empty-state text. Every string passes `redact::apply` here. `PdfLine`
   carries the text plus a style tag (`Title`, `Heading`, `Body`).
2. `draw(lines: &[PdfLine]) -> Vec<u8>` — printpdf layout: A4 portrait,
   15 mm margins, built-in Helvetica (body 10 pt) / Helvetica-Bold (headings
   14 pt, title 20 pt), word-based wrap (~90 chars), automatic page breaks
   when the cursor reaches the bottom margin.

`pub fn render(report, redact) -> anyhow::Result<Vec<u8>>` composes the two.

### `src/report/mod.rs` (extended)

- `pub enum OutputFormat { Md, Pdf }` (+ clap `ValueEnum` in cli.rs).
- `pub fn resolve_format(explicit: Option<OutputFormat>, path: Option<&Path>) -> OutputFormat`
  — explicit wins; else `.pdf` extension (case-insensitive) → `Pdf`; else `Md`.
- `pub fn write_report_file(report: &Report, path: &Path, format: OutputFormat, redact: &RedactConfig) -> anyhow::Result<()>`
  — single writer used by the CLI and the TUI: `Md` renders markdown then
  `redact::apply` then writes text; `Pdf` calls `pdf::render` and writes bytes.

## 4. CLI surface

- New flag on `report`: `--format <md|pdf>` (optional; also reachable via
  `send slack`'s flattened report args but only meaningful with an output path).
- Resolution per output path: `--output` flag, else config `output`, using
  `resolve_format`.
- Rules:
  - `--stdout` always prints Markdown (never PDF bytes to a terminal).
  - `--format pdf` with no output path (flag or config) → error:
    `pdf output requires --output <path> (or output in config)`.
  - `--format md --output x.pdf` writes Markdown into `x.pdf` (explicit wins).

## 5. TUI

The `w` key routes through the same `resolve_format(None, cfg.output)` +
`write_report_file` helper. Status line reports the written path and format
(e.g. `wrote report.pdf (pdf)`). No new keys, no new state.

## 6. Invariants (carried over, tested)

- No secrets in PDF output: every text run is redacted in `collect_lines`
  before layout; a unit test feeds a token-shaped string and a configured
  `extra_patterns` word through a fixture report and asserts neither survives
  in the collected lines.
- Deterministic content: same Report → same lines (PDF byte-level determinism
  is not asserted — document metadata may vary — content lines are the
  contract).
- Slack, AI, collectors, and all existing CLI behavior untouched; Markdown
  output byte-identical to today.

## 7. Testing

- Unit (`pdf.rs`): section presence + ordering in collected lines; empty-state
  report produces the "No activity" line; redaction test (token +
  extra_patterns never in lines); wrap helper splits long lines at word
  boundaries.
- Unit (`report/mod.rs`): `resolve_format` table (explicit-wins, `.pdf`/`.PDF`,
  non-pdf, no path).
- Smoke: `draw`/`render` bytes start with `%PDF-`; a many-item fixture yields
  more than one page object.
- Integration (`tests/`): `report --output x.pdf` writes a `%PDF-` file;
  `--format pdf --stdout`-only errors non-zero; `--format md --output x.pdf`
  writes Markdown text.
- Docs: README output-formats subsection; ACCEPTANCE.md rows for the new
  tests.

## 8. Out of scope

Custom fonts/themes/logos, landscape/paper-size options, PDF to stdout,
Slack PDF attachments, HTML output.
