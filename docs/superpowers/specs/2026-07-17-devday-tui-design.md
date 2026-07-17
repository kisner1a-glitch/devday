# devday TUI — Technical Design

Status: Approved for planning
Date: 2026-07-17
Builds on: `2026-07-16-devday-design.md` (CLI MVP, merged to master)

## 1. Purpose

Add an interactive terminal UI — `devday tui` — for managing devday day-to-day:
browsing reports, previewing and approving Slack posts, editing configuration,
and checking environment health. The TUI is a front-end over the existing,
test-covered library; it adds no new data sources and changes no CLI behavior.

## 2. Decisions

| Decision | Choice | Rationale |
| --- | --- | --- |
| Delivery | `devday tui` subcommand in the existing crate | Reuses the lib directly (collectors, report builder, Slack delivery, redaction); one binary; no stdout-parsing IPC |
| TUI stack | ratatui + crossterm | Standard, maintained Rust TUI stack; `TestBackend` enables headless render tests |
| App structure | Elm-style: pure `update(state, event)` + separate render | Update logic unit-testable without a terminal |
| Async | Collectors/AI/doctor run as spawned tokio tasks; results over mpsc | UI stays responsive; spinner during collection |
| Shared pipeline | Move `build_report` from `main.rs` into the lib (`src/pipeline.rs`) | CLI and TUI share one report pipeline; no logic changes |
| Link opening | `open` crate | Cross-platform browser open for source links |
| Config editing inputs | Hand-rolled single-line input widget | Fields are short (paths, channel names); avoids dependency sprawl (YAGNI) |
| New dependencies | `ratatui`, `crossterm`, `open` only | Minimal surface |

### Judgment calls (explicit)

1. **Interactive posting bypasses `auto_post`.** The `slack.auto_post` gate
   exists to prevent *non-interactive* surprise posts (cron). The TUI's
   confirm modal *is* the SRS §10 "preview/approval" mode, so posting from the
   TUI requires only the explicit keyboard confirmation — not
   `auto_post = true`. CLI gating (`--post && auto_post`) is unchanged.
2. **Config writes are allowed.** The read-only invariant covers GitHub,
   Linear, and git *sources*. devday's own config file is legitimately
   editable by the TUI, behind an explicit save-confirm, written atomically
   (temp file + rename in the same directory).

## 3. Module layout

```
src/
  pipeline.rs          # build_report moved from main.rs (shared CLI + TUI)
  tui/
    mod.rs             # entry: terminal setup/teardown, panic hook, event loop
    app.rs             # App state: active tab, per-tab state, background-task tracking
    event.rs           # Event enum (key, tick, task results) + pure update()
    task.rs            # spawn collectors/AI/doctor/post as tokio tasks -> mpsc
    render/
      mod.rs           # layout: tab bar, status line, active tab dispatch
      report.rs        # Report tab
      slack.rs         # Slack tab
      config.rs        # Config tab (form editor + input widget)
      doctor.rs        # Doctor/State tab
```

`main.rs` gains a `Command::Tui` arm; `cli.rs` gains the `Tui` variant.
`src/lib.rs` gains `pub mod pipeline; pub mod tui;`.

## 4. Screens

Tab bar keys `1`–`4`; global keys: `q` quit, `?` help overlay, `Esc` closes
modals. A status line shows the active config path, since-window, and
background-task progress.

### Tab 1 — Report
- On entry (and on `r`): runs the shared pipeline as a background task with a
  spinner; results replace the view when ready.
- Left pane: group tree (Linear-first groups from `group_items`, with item
  counts). Right pane: selected group's items with title, source, status,
  signals; plus Summary / Next Up / Blockers sections.
- Keys: `Tab` switch panes, arrows/`j`/`k` navigate, `Enter` open the selected
  item's URL in the browser, `s` cycle since-window (24h → 48h → 7d), `a`
  toggle AI on/off, `w` write the Markdown report to the configured output
  path (feedback in status line).

### Tab 2 — Slack
- Shows the exact redacted digest text that would be posted
  (`redact::apply(format_digest(...))` — same construction as the CLI path).
- `v` toggles verbose; the preview re-renders.
- `p` opens a confirm modal ("Post to <channel> via <webhook|bot>?"). Confirm
  posts using the existing `deliver::slack` functions, then `mark_posted` +
  state save, then shows the result (success or the non-secret error string).
- If the digest's hash is already in `posted_report_hashes`, the modal warns
  "identical report already posted" before allowing confirm.
- If no webhook/bot credentials are configured, `p` shows the doctor-style
  explanation instead of a modal.

### Tab 3 — Config
- Form over the loaded `Config`: source toggles (github/linear/git), git
  roots as an add/remove list, Slack channel + webhook_env/bot_token_env +
  auto_post toggle, AI provider picker (none/claude/codex) + command override,
  output path, default_since.
- Field editing via a hand-rolled single-line input widget (cursor, insert,
  delete, Esc cancel, Enter accept).
- **Secret values are never displayed** — env-var *names* only, exactly like
  `doctor`.
- `S` (save) validates by serializing and re-parsing through the real `Config`
  serde round-trip, then asks for confirm, then writes atomically to the
  active config path. Validation failure shows the parse error; nothing is
  written.
- Note: `Config` currently only derives `Deserialize`; saving requires adding
  `Serialize` to the config structs (a additive derive change).

### Tab 4 — Doctor / State
- Doctor pane: runs `doctor::build_checks` (via the real env getter) as a
  task; renders each check green (OK) / red (FAIL) with its detail; `r`
  re-runs.
- State pane: loads `LocalState` from `Config::state_path()`; shows last
  successful run, posted-report count, and the most recent posted hashes
  (truncated). `x` clears state behind a typed-confirm modal
  (deletes/rewrites the state file only — never any source data).

## 5. Event & data flow

```
crossterm events ─┐
tick timer ───────┼─> Event enum ─> update(&mut App, Event) ─> render(&App)
task results ─────┘        (pure state transitions;      (ratatui frame,
  (mpsc from tokio          side effects requested         no logic)
   background tasks)        via returned Effect list)
```

- `update` returns `Vec<Effect>` (SpawnReportTask, PostSlack, SaveConfig,
  OpenUrl, WriteReport, ClearState, Quit); the event loop executes effects.
  This keeps `update` pure and unit-testable.
- One background task per kind at a time (e.g. regenerating while a
  generation is in flight is ignored with a status-line notice).

## 6. Terminal safety

- Alternate screen + raw mode on entry; both restored on every exit path.
- A panic hook restores the terminal *before* printing the panic, so a crash
  never leaves the user's shell raw.
- Ctrl-C is handled as a quit event (clean teardown), not a signal kill.
- `devday tui` refuses to start when stdout is not a TTY (clear error), so a
  misconfigured cron line can never hang on a hidden TUI.

## 7. Safety invariants (carried over from the CLI, tested)

- Sources remain read-only; the TUI adds no new source interactions.
- Everything rendered from report/Slack content passes through the existing
  `redact` boundary before hitting the screen.
- No secret values on screen, ever: config editor and doctor show env-var
  names and presence booleans only.
- Posting always requires the explicit confirm modal; dedup warning on
  identical hash; state saved after successful post exactly like the CLI.
- Config saves are explicit, validated, and atomic.

## 8. Testing strategy

- **Pure update tests** per tab: feed key events into `update`, assert state
  transitions and emitted `Effect`s (no terminal, no async).
- **Render smoke tests** with ratatui `TestBackend`: each tab renders without
  panic at a couple of terminal sizes; secret-bearing fixtures assert env-var
  names (not values) appear in the buffer.
- **Config round-trip test**: edited `Config` serializes, re-parses, and
  compares equal; invalid edits surface errors without writing.
- **No-secrets render test**: a `Report`/digest fixture containing
  token-shaped strings renders with `[REDACTED]` in the terminal buffer.
- Existing 65 tests untouched; acceptance additions: `devday tui` appears in
  `--help`; `devday tui` errors cleanly when not a TTY.

## 9. Out of scope

- Mouse support, themes, and layout customization.
- Editing Linear/GitHub filters beyond what the config structs already hold.
- Live/streaming activity updates (the TUI regenerates on demand).
- A daemon or watch mode; the SRS's exclusions (web UI, scheduler, source
  mutation) all still hold.
