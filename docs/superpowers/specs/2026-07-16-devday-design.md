# devday — Technical Design

Status: Approved for planning
Date: 2026-07-16
Source spec: `devday-srs.md`

## 1. Purpose

`devday` is an open-source Rust CLI that generates shareable engineering status
reports from GitHub, Linear, and local git activity, summarizes them with a
configured local AI provider (Codex or Claude), and optionally posts a
Slack-ready digest. It is a private-first personal operating report, not a
surveillance dashboard. See `devday-srs.md` for full requirements.

This document resolves the SRS's open planning decisions (§19) into a concrete,
buildable architecture and is the basis for the implementation plan.

## 2. Resolved decisions

These were open in the SRS and are now fixed:

| Decision | Choice | Rationale |
| --- | --- | --- |
| Plan shape | One comprehensive MVP plan, pipeline-first ordering | Covers all SRS requirements; ordered so an end-to-end path runs early |
| Crate layout | Single binary crate, modular internals | Simplest for a solo MVP; can split into the SRS's 5-crate workspace later without changing module boundaries |
| Config format | TOML | Idiomatic for Rust CLIs, strong comment support, easy hand-editing |
| AI CLI contract | Subprocess, prompt via stdin, stdout captured | Avoids arg-length limits and keeps prompts out of process listings |
| Async runtime | tokio | Needed for Linear + Slack HTTP and concurrent source collection |
| HTTP client | reqwest | Linear GraphQL, Slack webhook/bot delivery |
| GitHub access | `gh` CLI subprocess, JSON output | SRS FR-2 prefers authenticated local `gh` |
| Local git backend | `git2` crate (read-only), with subprocess fallback where `git2` is awkward (e.g. stashes) | Read-only guarantee is structural, not just conventional |
| State storage | JSON file | Simple, human-inspectable, easy to version |
| Error handling | `thiserror` in modules, `anyhow` at CLI boundary | Typed library errors, ergonomic top-level handling |
| Testing | TDD; unit + fixture-based integration + golden-file Markdown | Deterministic output is directly assertable |

## 3. Module layout

Single binary crate `devday`:

```
src/
  main.rs              # entry, async runtime, top-level dispatch
  cli.rs               # clap command/flag definitions
  config.rs            # TOML load/init; merge precedence: CLI flags > config > defaults
  model.rs             # ActivityItem, Report, BlockerSignal, LocalState (SRS §17)
  collect/
    mod.rs             # Collector trait, concurrent orchestration, partial-failure handling
    github.rs          # `gh` CLI subprocess + JSON parse
    linear.rs          # Linear GraphQL over reqwest, pagination
    git.rs             # read-only local git scan
  group.rs             # Linear-first grouping, issue-key extraction, repo fallback
  report/
    mod.rs             # Report builder: worked_on / next_up / blockers classification
    markdown.rs        # deterministic Markdown renderer (5 sections)
  ai/
    mod.rs             # Summarizer trait, provider dispatch, non-AI fallback
    cli_provider.rs    # subprocess (claude/codex), prompt via stdin, capture stdout
  deliver/
    slack.rs           # webhook + bot-token delivery, digest vs verbose formatting
  state.rs             # JSON state file: dedup, cursors, posted hashes
  redact.rs            # secret guards + configurable coworker-facing redaction
  doctor.rs            # `devday doctor` environment/auth checks
```

Each module has one clear purpose and a small interface. Collectors implement a
common `Collector` trait so orchestration is uniform and each source is
independently testable. The AI layer sits behind a `Summarizer` trait so a
non-AI fallback is a drop-in implementation.

## 4. Data model (SRS §17)

- **ActivityItem**: `source` (github|linear|git), `source_id`, `title`, `url`,
  `project_key`, `repo`, `activity_type`, `status`, `actor`, `timestamp`,
  `summary`, `signals`.
- **Report**: `window_start`, `window_end`, `groups`, `summary`, `worked_on`,
  `next_up`, `blockers`, `source_links`, `generation_warnings`.
- **LocalState**: `last_successful_run_at`, `posted_item_ids`,
  `posted_report_hashes`, `source_cursor_by_integration`, `dedupe_window`.
- **BlockerSignal**: `kind` (explicit|inferred), `reason`, `source_item_url`,
  `confidence` (high|medium|low).

All serialized via `serde`.

## 5. Data flow

```
config + CLI flags
  -> collect (github, linear, git concurrently; read-only; partial-failure tolerant)
  -> normalize to Vec<ActivityItem>
  -> group (extract Linear issue keys from branch / PR title / PR body / commit msg;
            unlinked activity falls back to repo name)
  -> report builder
       worked_on : source activity showing user involvement in window
       next_up   : assigned active Linear issues, PRs awaiting action, failing
                   checks/requested changes, recent unmerged/uncommitted branches,
                   high-priority Linear; inferred items labeled "inferred"
       blockers  : explicit (Linear blocked status/labels/relations, failing checks,
                   requested changes, PRs waiting for review) vs inferred, distinguished
  -> AI summary (or deterministic fallback) fills Summary
  -> Markdown render (Summary, Worked On, Next Up, Blockers, Source Links)
  -> output: stdout and/or file
  -> optional Slack: digest (default) or verbose; preview vs auto-post; dedup via state
```

Empty windows produce a useful empty-state report (NFR-7).

## 6. CLI surface (SRS §11)

Commands: `devday report`, `devday send slack`, `devday config init`,
`devday doctor`.

Report flags: `--since <duration>` (default `24h`), `--github`, `--linear`,
`--git`, `--ai <provider>`, `--no-ai`, `--output <path>`, `--stdout`,
`--config <path>`.

Slack flags: `--channel <channel>`, `--preview`, `--post`, `--verbose`.

Runnable non-interactively for cron (NFR-2); `--since` accepts duration strings
(e.g. `24h`, `7d`).

## 7. Safety invariants (enforced and tested)

- All source collection is read-only; no mutating commands are ever issued
  (git backend is read-only by construction).
- No secrets in reports, files, Slack messages, stdout, or logs. A redaction
  layer scrubs output and a test greps rendered output for known token shapes
  (`ghp_`, `xoxb-`, `lin_api_`, bearer headers, etc.).
- AI prompts receive only normalized `ActivityItem` data, never raw
  secret-bearing payloads.
- Slack auto-post occurs only when explicitly configured; preview mode is always
  available.
- Coworker-facing reports may include local paths / private repo details by
  default, but redaction is configurable per SRS §14.

## 8. Error handling & resilience

- Per-source failures degrade gracefully: a failed collector is recorded in
  `generation_warnings` and the run continues with available sources (NFR-4).
- AI failure falls back to the deterministic non-AI report (FR-10) — never a
  total failure.
- Slack post failure returns a non-zero exit code while preserving the generated
  report locally / in stdout (FR-17, SRS §10).
- `thiserror` typed errors per module; `anyhow` context at the CLI boundary.

## 9. Configuration (SRS §12)

TOML config defines: enabled sources; GitHub account/orgs/repos/filters; Linear
workspace/teams/projects/labels/filters; local git roots and include/exclude
rules; AI provider and settings; Slack channel and delivery mode; Slack
webhook/bot-token config; preview-vs-auto-post behavior; project grouping rules;
state path and dedupe behavior; redaction rules; default output path and window.

Secrets are read from environment variables, provider CLIs, or explicitly
configured secret references. Plaintext secrets in config are discouraged and
`doctor` warns when found.

## 10. Testing strategy

- **Unit tests** per module (grouping/issue-key extraction, classification,
  duration parsing, config merge precedence, redaction).
- **Integration tests** with fixtures: recorded `gh` JSON, Linear GraphQL
  responses, Slack request capture (mock server), and temp-dir git repos built
  in-test for the git collector.
- **Golden-file tests** for deterministic Markdown output (NFR-3).
- **Safety test** asserting no known secret shapes appear in any rendered output.
- Acceptance-criteria pass mapping each SRS §18 item to a test or manual check.

## 11. Plan ordering (pipeline-first)

One comprehensive plan, ordered so an end-to-end path runs as early as possible:

1. Data model + config/CLI skeleton (`clap`, TOML load/init).
2. Local git collector (read-only).
3. Grouping + Linear issue-key extraction, repo fallback.
4. Deterministic Markdown report + stdout/file output. **← first runnable milestone.**
5. Linear collector (GraphQL, pagination).
6. GitHub collector (`gh` subprocess).
7. Report classification refinement (next_up / explicit vs inferred blockers).
8. AI summarization layer + non-AI fallback.
9. Slack delivery (webhook + bot token, digest/verbose, preview/post).
10. Local state + scheduled-run dedup.
11. `doctor` command.
12. Redaction hardening + secret-leak tests.
13. Acceptance-criteria pass against SRS §18.

## 12. Out of scope (SRS §5)

No built-in scheduler/daemon; no mutation of GitHub/Linear/git; no surveillance
dashboards or manager analytics; no multi-user admin; no web UI; no remote host
scanning; no calendar integration; no email/Telegram delivery.
