# devday TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `devday tui` subcommand — an interactive terminal UI (ratatui) with four tabs: Report browser, Slack preview/approve, Config editor, Doctor/State panel.

**Architecture:** Elm-style TUI inside the existing lib+bin crate. Pure `update(&mut App, Event) -> Vec<Effect>` state transitions (unit-testable, no terminal) separated from rendering (smoke-tested via ratatui `TestBackend`). Collectors/AI/doctor/post run as background tokio tasks feeding an mpsc channel. A shared `pipeline::build_report` (moved from main.rs) serves both CLI and TUI.

**Tech Stack:** existing devday lib (Rust 2021, tokio) + `ratatui 0.29` (crossterm re-exported via `ratatui::crossterm`), `open 5`.

**Spec:** `docs/superpowers/specs/2026-07-17-devday-tui-design.md`
**Linear:** epic CHA-2392; tasks map 1:1 → CHA-2393, CHA-2394, CHA-2395, CHA-2396, CHA-2397, CHA-2398, CHA-2399. The controller (not implementer subagents) updates ticket status between tasks.

## Global Constraints

- Rust edition 2021; MSRV 1.75+. Work on branch `feat/devday-tui`.
- Every task ends with `cargo test` green, `cargo clippy --all-targets -- -D warnings` clean, and `cargo fmt` run, before commit.
- CLI behavior unchanged: all 65 existing tests must stay green untouched.
- NO SECRETS on screen ever: env-var names only; all report/Slack content passes `redact` before rendering.
- Sources stay read-only; the TUI adds no new source interactions.
- Interactive Slack posting bypasses `auto_post` (the confirm modal IS the approval); CLI gating (`--post && auto_post`) unchanged.
- Config saves: validated via serde round-trip, explicit confirm, atomic write (temp file + rename, same directory).
- Terminal safety: panic must restore the terminal; `devday tui` errors cleanly when stdout is not a TTY.
- New dependencies: `ratatui`, `open` only.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `src/pipeline.rs` | Shared `ReportOptions` + `build_report` (moved from main.rs) |
| `src/tui/mod.rs` | Entry: TTY guard, terminal init/restore, event loop, effect execution |
| `src/tui/app.rs` | `App`, `Tab`, per-tab state structs |
| `src/tui/event.rs` | `Event`, `Effect`, pure `update()` + per-tab update fns |
| `src/tui/task.rs` | Background tokio tasks (report, doctor, slack post) → mpsc |
| `src/tui/render/mod.rs` | Layout: tab bar, status line, help overlay, tab dispatch, `buffer_text` test helper |
| `src/tui/render/report.rs` | Report tab rendering |
| `src/tui/render/slack.rs` | Slack tab rendering |
| `src/tui/render/config.rs` | Config tab rendering + input widget rendering |
| `src/tui/render/doctor.rs` | Doctor/State tab rendering |
| `tests/tui_acceptance.rs` | `--help` lists tui; non-TTY invocation errors cleanly |

---

## Task 1: Shared pipeline (`src/pipeline.rs`) — CHA-2393

**Files:**
- Create: `src/pipeline.rs`
- Modify: `src/lib.rs`, `src/main.rs`
- Test: inline in `src/pipeline.rs`

**Interfaces:**
- Consumes: `config::Config`, `cli::parse_since`, `collect::*`, `group`, `report`, `ai`, `model::Report`.
- Produces: `pipeline::ReportOptions { since: Option<String>, github: bool, linear: bool, git: bool, ai_provider: Option<String>, no_ai: bool }` (`Default` = all false/None) and `pub async fn pipeline::build_report(cfg: &Config, opts: &ReportOptions) -> anyhow::Result<Report>`. Semantics identical to today's `build_report` in main.rs: a bool flag force-enables a source (`opts.git || cfg.sources.git`), `since` precedence flag > `cfg.default_since` > `"24h"`, AI runs when `!no_ai && (ai_provider.is_some() || cfg.ai.provider.is_some())` with `ai_provider` overriding.

- [ ] **Step 1: Write the failing test in `src/pipeline.rs`**

Create the file with the options struct, a **verbatim move** of the body of `build_report` from `src/main.rs` (replace every use of clap's `ReportArgs` field with the matching `ReportOptions` field — same names), and these tests:

```rust
use crate::config::Config;
use crate::model::Report;

#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub since: Option<String>,
    pub github: bool,
    pub linear: bool,
    pub git: bool,
    pub ai_provider: Option<String>,
    pub no_ai: bool,
}

pub async fn build_report(cfg: &Config, opts: &ReportOptions) -> anyhow::Result<Report> {
    // MOVED VERBATIM from src/main.rs::build_report. Only mechanical renames:
    //   args.since -> opts.since, args.github -> opts.github, args.linear -> opts.linear,
    //   args.git -> opts.git, args.ai -> opts.ai_provider, args.no_ai -> opts.no_ai.
    // No logic changes. (The existing body computes now/since, runs the three
    // collectors behind their enable checks, builds the report, then AI.)
    unimplemented!("move body from main.rs in Step 2")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silent_config() -> Config {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.sources.linear = false;
        cfg.sources.git = false;
        cfg
    }

    #[tokio::test]
    async fn builds_empty_report_with_all_sources_disabled() {
        let rep = build_report(&silent_config(), &ReportOptions::default())
            .await
            .unwrap();
        assert!(rep.worked_on.is_empty());
        assert!(rep.window_end > rep.window_start);
    }

    #[tokio::test]
    async fn since_flag_overrides_config_default() {
        let mut cfg = silent_config();
        cfg.default_since = Some("48h".into());
        let opts = ReportOptions { since: Some("2h".into()), ..Default::default() };
        let rep = build_report(&cfg, &opts).await.unwrap();
        let window = rep.window_end - rep.window_start;
        assert_eq!(window.num_hours(), 2);
    }

    #[tokio::test]
    async fn bad_since_is_error() {
        let opts = ReportOptions { since: Some("banana".into()), ..Default::default() };
        assert!(build_report(&silent_config(), &opts).await.is_err());
    }
}
```

- [ ] **Step 2: Move the body**

Cut `build_report` out of `src/main.rs`, paste as the body of `pipeline::build_report`, apply the mechanical renames listed in the comment. Add `pub mod pipeline;` to `src/lib.rs`.

- [ ] **Step 3: Rewire main.rs**

In `src/main.rs`, add a small adapter and use it in `run_report` and `run_send_slack`:

```rust
fn to_options(args: &cli::ReportArgs) -> devday::pipeline::ReportOptions {
    devday::pipeline::ReportOptions {
        since: args.since.clone(),
        github: args.github,
        linear: args.linear,
        git: args.git,
        ai_provider: args.ai.clone(),
        no_ai: args.no_ai,
    }
}
```

Both call sites become `devday::pipeline::build_report(&cfg, &to_options(&args)).await?` (for send-slack: `&to_options(&args.report)`).

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: 65 existing + 3 new, all green. Then `cargo fmt && cargo clippy --all-targets -- -D warnings` clean.

- [ ] **Step 5: Commit**

```bash
git add src/pipeline.rs src/lib.rs src/main.rs
git commit -m "refactor: move build_report into lib as shared pipeline (CHA-2393)"
```

---

## Task 2: TUI scaffold — CHA-2394

**Files:**
- Create: `src/tui/mod.rs`, `src/tui/app.rs`, `src/tui/event.rs`, `src/tui/task.rs`, `src/tui/render/mod.rs`
- Modify: `Cargo.toml`, `src/lib.rs`, `src/cli.rs`, `src/main.rs`
- Test: inline in `src/tui/event.rs` and `src/tui/render/mod.rs`

**Interfaces:**
- Consumes: `config::Config`, `pipeline` (Task 1).
- Produces (later tasks extend these enums/structs — keep names exact):
  - `app::{App, Tab}` — `App::new(cfg: Config, cfg_path: Option<PathBuf>) -> App`; fields `cfg`, `cfg_path`, `tab: Tab`, `status: String`, `show_help: bool`, `spinner: usize`, plus per-tab state structs added in Tasks 3–6.
  - `event::{Event, Effect}` and `pub fn update(app: &mut App, ev: Event) -> Vec<Effect>`.
  - `render::draw(f: &mut Frame, app: &App)` and test helper `render::buffer_text(&Terminal<TestBackend>) -> String`.
  - `tui::run(config_path: Option<&Path>) -> anyhow::Result<()>` (async).

- [ ] **Step 1: Add dependencies to `Cargo.toml`**

```toml
ratatui = "0.29"
open = "5"
```

- [ ] **Step 2: CLI variant**

In `src/cli.rs` add to `Command`:

```rust
    /// Open the interactive terminal UI.
    Tui(TuiArgs),
```

```rust
#[derive(Debug, clap::Args)]
pub struct TuiArgs {
    #[arg(long)]
    pub config: Option<PathBuf>,
}
```

Add a unit test alongside the existing cli tests:

```rust
    #[test]
    fn tui_command_parses() {
        let cli = Cli::try_parse_from(["devday", "tui"]).unwrap();
        assert!(matches!(cli.command, Command::Tui(_)));
    }
```

- [ ] **Step 3: Write `src/tui/app.rs`**

```rust
use std::path::PathBuf;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Report,
    Slack,
    Config,
    Doctor,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Report, Tab::Slack, Tab::Config, Tab::Doctor];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Report => "1 Report",
            Tab::Slack => "2 Slack",
            Tab::Config => "3 Config",
            Tab::Doctor => "4 Doctor",
        }
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

pub struct App {
    pub cfg: Config,
    pub cfg_path: Option<PathBuf>,
    pub tab: Tab,
    pub status: String,
    pub show_help: bool,
    pub spinner: usize,
    // Per-tab state is added by Tasks 3-6:
    // pub report: ReportState, pub slack: SlackState,
    // pub config_tab: ConfigState, pub doctor: DoctorState,
}

impl App {
    pub fn new(cfg: Config, cfg_path: Option<PathBuf>) -> Self {
        Self {
            cfg,
            cfg_path,
            tab: Tab::Report,
            status: String::from("? help  q quit"),
            show_help: false,
            spinner: 0,
        }
    }
}
```

- [ ] **Step 4: Write the failing tests + code in `src/tui/event.rs`**

```rust
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::tui::app::{App, Tab};

#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Tick,
    // Task results are added by Tasks 3-6:
    // ReportReady(Result<crate::model::Report, String>), ...
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    Quit,
    // Added by Tasks 3-6:
    // SpawnReport, OpenUrl(String), WriteReport, PostSlack(String),
    // SaveConfig, ClearState, SpawnDoctor,
}

/// Pure state transition: no I/O, no terminal, no async. Fully unit-testable.
pub fn update(app: &mut App, ev: Event) -> Vec<Effect> {
    match ev {
        Event::Tick => {
            app.spinner = app.spinner.wrapping_add(1);
            vec![]
        }
        Event::Key(key) => handle_key(app, key),
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    // Ctrl-C always quits cleanly.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return vec![Effect::Quit];
    }
    if app.show_help {
        // Any key closes the help overlay.
        app.show_help = false;
        return vec![];
    }
    match key.code {
        KeyCode::Char('q') => vec![Effect::Quit],
        KeyCode::Char('?') => {
            app.show_help = true;
            vec![]
        }
        KeyCode::Char('1') => switch_tab(app, Tab::Report),
        KeyCode::Char('2') => switch_tab(app, Tab::Slack),
        KeyCode::Char('3') => switch_tab(app, Tab::Config),
        KeyCode::Char('4') => switch_tab(app, Tab::Doctor),
        _ => tab_key(app, key),
    }
}

fn switch_tab(app: &mut App, tab: Tab) -> Vec<Effect> {
    app.tab = tab;
    vec![]
}

/// Per-tab key handling; Tasks 3-6 fill in the arms.
fn tab_key(_app: &mut App, _key: KeyEvent) -> Vec<Effect> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn app() -> App {
        App::new(Config::default(), None)
    }
    fn key(c: char) -> Event {
        Event::Key(KeyEvent::from(KeyCode::Char(c)))
    }

    #[test]
    fn q_quits() {
        assert_eq!(update(&mut app(), key('q')), vec![Effect::Quit]);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut a = app();
        let ev = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(update(&mut a, ev), vec![Effect::Quit]);
    }

    #[test]
    fn number_keys_switch_tabs() {
        let mut a = app();
        update(&mut a, key('2'));
        assert_eq!(a.tab, Tab::Slack);
        update(&mut a, key('4'));
        assert_eq!(a.tab, Tab::Doctor);
    }

    #[test]
    fn help_overlay_toggles_and_any_key_closes() {
        let mut a = app();
        update(&mut a, key('?'));
        assert!(a.show_help);
        update(&mut a, key('x'));
        assert!(!a.show_help);
    }

    #[test]
    fn q_inside_help_only_closes_help() {
        let mut a = app();
        update(&mut a, key('?'));
        assert_eq!(update(&mut a, key('q')), vec![]);
        assert!(!a.show_help);
    }
}
```

- [ ] **Step 5: Write `src/tui/render/mod.rs`**

```rust
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs};
use ratatui::Frame;

use crate::tui::app::{App, Tab};

pub fn draw(f: &mut Frame, app: &App) {
    let [tab_bar, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let titles: Vec<Line> = Tab::ALL.iter().map(|t| Line::from(t.title())).collect();
    f.render_widget(
        Tabs::new(titles)
            .select(app.tab.index())
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        tab_bar,
    );

    match app.tab {
        // Tasks 3-6 replace these placeholders with real tab renderers.
        Tab::Report => placeholder(f, body, "Report (coming in Task 3)"),
        Tab::Slack => placeholder(f, body, "Slack (coming in Task 4)"),
        Tab::Config => placeholder(f, body, "Config (coming in Task 5)"),
        Tab::Doctor => placeholder(f, body, "Doctor (coming in Task 6)"),
    }

    f.render_widget(Paragraph::new(app.status.as_str()), status);

    if app.show_help {
        let area = centered(f.area(), 60, 12);
        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(HELP_TEXT).block(Block::default().borders(Borders::ALL).title("Help")),
            area,
        );
    }
}

const HELP_TEXT: &str = "1-4 switch tab   q quit   ? help\n\
Report: r regen  s window  a AI  Tab pane  Enter open  w write\n\
Slack:  v verbose  p post (confirm)\n\
Config: arrows move  Enter edit/toggle  S save\n\
Doctor: r re-run  x clear state";

fn placeholder(f: &mut Frame, area: Rect, text: &str) {
    f.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

/// Test helper: flatten a TestBackend buffer to a String.
#[cfg(test)]
pub fn buffer_text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn draws_tab_bar_and_status_without_panic() {
        let app = App::new(Config::default(), None);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("1 Report"));
        assert!(text.contains("4 Doctor"));
    }

    #[test]
    fn help_overlay_renders() {
        let mut app = App::new(Config::default(), None);
        app.show_help = true;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        assert!(buffer_text(&terminal).contains("switch tab"));
    }
}
```

- [ ] **Step 6: Write `src/tui/task.rs`** (empty shell this task; Tasks 3–6 add spawns)

```rust
//! Background tokio tasks. Each spawn sends its result back as an Event
//! over the channel; the event loop never blocks on collection or posting.
```

- [ ] **Step 7: Write `src/tui/mod.rs`**

```rust
pub mod app;
pub mod event;
pub mod render;
pub mod task;

use std::io::IsTerminal;
use std::path::Path;
use std::time::Duration;

use app::App;
use event::{update, Effect, Event};

pub async fn run(config_path: Option<&Path>) -> anyhow::Result<()> {
    if !std::io::stdout().is_terminal() {
        anyhow::bail!("devday tui requires an interactive terminal (stdout is not a TTY)");
    }
    let cfg = crate::config::Config::load(config_path)?;
    // ratatui::init() enters the alternate screen, enables raw mode, and
    // installs a panic hook that restores the terminal before the panic prints.
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, cfg, config_path.map(|p| p.to_path_buf())).await;
    ratatui::restore();
    result
}

async fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    cfg: crate::config::Config,
    cfg_path: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

    // Blocking input thread: crossterm key events -> channel.
    let input_tx = tx.clone();
    std::thread::spawn(move || loop {
        match ratatui::crossterm::event::read() {
            Ok(ratatui::crossterm::event::Event::Key(k))
                if k.kind == ratatui::crossterm::event::KeyEventKind::Press =>
            {
                if input_tx.send(Event::Key(k)).is_err() {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    });

    let mut app = App::new(cfg, cfg_path);
    let mut tick = tokio::time::interval(Duration::from_millis(200));

    loop {
        terminal.draw(|f| render::draw(f, &app))?;
        let ev = tokio::select! {
            Some(e) = rx.recv() => e,
            _ = tick.tick() => Event::Tick,
        };
        for effect in update(&mut app, ev) {
            if !run_effect(&mut app, effect, &tx) {
                return Ok(());
            }
        }
    }
}

/// Executes one effect. Returns false to quit. Tasks 3-6 add arms.
fn run_effect(
    _app: &mut App,
    effect: Effect,
    _tx: &tokio::sync::mpsc::UnboundedSender<Event>,
) -> bool {
    match effect {
        Effect::Quit => false,
    }
}
```

- [ ] **Step 8: Wire lib + main**

`src/lib.rs`: add `pub mod tui;`. `src/main.rs`: add `Command::Tui(targs) => devday::tui::run(targs.config.as_deref()).await?,` to the match.

- [ ] **Step 9: Run everything**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: all green (5 new event tests + 2 render tests + 1 cli test). Manually verify: `cargo run -- tui < /dev/null | cat` errors with the TTY message (piped stdout), and in a real terminal `cargo run -- tui` opens, `2` switches tab, `?` shows help, `q` exits with shell intact.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml Cargo.lock src/tui src/lib.rs src/cli.rs src/main.rs
git commit -m "feat: TUI scaffold - devday tui subcommand, event loop, Elm-style update (CHA-2394)"
```

---

## Task 3: Report tab — CHA-2395

**Files:**
- Create: `src/tui/render/report.rs`
- Modify: `src/tui/app.rs`, `src/tui/event.rs`, `src/tui/task.rs`, `src/tui/render/mod.rs`, `src/tui/mod.rs`
- Test: inline in `src/tui/event.rs` (update tests) and `src/tui/render/report.rs` (smoke)

**Interfaces:**
- Consumes: `pipeline::{build_report, ReportOptions}`, `model::Report`, `redact`, `report::markdown`.
- Produces:
  - `app::ReportState { loading: bool, error: Option<String>, report: Option<Report>, sections: Vec<String>, pane: Pane, selected_group: usize, selected_item: usize, since_idx: usize, ai_enabled: bool }` with `pub enum Pane { Groups, Detail }` and `pub const SINCE_CHOICES: [&str; 3] = ["24h", "48h", "7d"];` — `App` gains `pub report: ReportState`.
  - `Event::ReportReady(Result<Report, String>)`.
  - `Effect::{SpawnReport, OpenUrl(String), WriteReport}`.
  - `task::spawn_report(tx, cfg: Config, opts: ReportOptions)`.

- [ ] **Step 1: Add state to `src/tui/app.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Groups,
    Detail,
}

pub const SINCE_CHOICES: [&str; 3] = ["24h", "48h", "7d"];

#[derive(Default)]
pub struct ReportState {
    pub loading: bool,
    pub error: Option<String>,
    pub report: Option<crate::model::Report>,
    /// Redacted rendered markdown lines for the sections view.
    pub sections: Vec<String>,
    pub pane: Pane,
    pub selected_group: usize,
    pub selected_item: usize,
    pub since_idx: usize,
    pub ai_enabled: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Pane::Groups
    }
}
```

Add `pub report: ReportState` to `App` and `report: ReportState::default()` in `App::new`. Also add a helper on `App`:

```rust
    pub fn report_options(&self) -> crate::pipeline::ReportOptions {
        crate::pipeline::ReportOptions {
            since: Some(SINCE_CHOICES[self.report.since_idx].to_string()),
            no_ai: !self.report.ai_enabled,
            ..Default::default()
        }
    }
```

- [ ] **Step 2: Extend `src/tui/event.rs`**

Add `Event::ReportReady(Result<crate::model::Report, String>)` and `Effect::{SpawnReport, OpenUrl(String), WriteReport}`. In `update`, handle `ReportReady`:

```rust
        Event::ReportReady(res) => {
            app.report.loading = false;
            match res {
                Ok(rep) => {
                    let md = crate::redact::apply(
                        &crate::report::markdown::render(&rep, true),
                        &app.cfg.redact,
                    );
                    app.report.sections = md.lines().map(String::from).collect();
                    app.report.report = Some(rep);
                    app.report.error = None;
                    app.report.selected_group = 0;
                    app.report.selected_item = 0;
                    app.status = "report ready".into();
                }
                Err(e) => {
                    app.report.error = Some(e);
                    app.status = "report failed".into();
                }
            }
            vec![]
        }
```

In `tab_key`, add the `Tab::Report` arm:

```rust
fn tab_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    match app.tab {
        Tab::Report => report_key(app, key),
        _ => vec![],
    }
}

fn report_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    let r = &mut app.report;
    match key.code {
        KeyCode::Char('r') => {
            if r.loading {
                app.status = "generation already in flight".into();
                vec![]
            } else {
                r.loading = true;
                app.status = "generating...".into();
                vec![Effect::SpawnReport]
            }
        }
        KeyCode::Char('s') => {
            r.since_idx = (r.since_idx + 1) % crate::tui::app::SINCE_CHOICES.len();
            app.status = format!("window: {} (r to regenerate)", crate::tui::app::SINCE_CHOICES[r.since_idx]);
            vec![]
        }
        KeyCode::Char('a') => {
            r.ai_enabled = !r.ai_enabled;
            app.status = format!("AI: {} (r to regenerate)", if r.ai_enabled { "on" } else { "off" });
            vec![]
        }
        KeyCode::Char('w') => vec![Effect::WriteReport],
        KeyCode::Tab => {
            r.pane = match r.pane {
                Pane::Groups => Pane::Detail,
                Pane::Detail => Pane::Groups,
            };
            vec![]
        }
        KeyCode::Down | KeyCode::Char('j') => {
            move_selection(r, 1);
            vec![]
        }
        KeyCode::Up | KeyCode::Char('k') => {
            move_selection(r, -1);
            vec![]
        }
        KeyCode::Enter => {
            if let Some(url) = selected_url(r) {
                return vec![Effect::OpenUrl(url)];
            }
            vec![]
        }
        _ => vec![],
    }
}

fn move_selection(r: &mut crate::tui::app::ReportState, delta: isize) {
    let Some(rep) = &r.report else { return };
    match r.pane {
        Pane::Groups => {
            let len = rep.groups.len();
            if len > 0 {
                r.selected_group =
                    (r.selected_group as isize + delta).rem_euclid(len as isize) as usize;
                r.selected_item = 0;
            }
        }
        Pane::Detail => {
            let len = rep.groups.get(r.selected_group).map_or(0, |g| g.items.len());
            if len > 0 {
                r.selected_item =
                    (r.selected_item as isize + delta).rem_euclid(len as isize) as usize;
            }
        }
    }
}

fn selected_url(r: &crate::tui::app::ReportState) -> Option<String> {
    r.report
        .as_ref()?
        .groups
        .get(r.selected_group)?
        .items
        .get(r.selected_item)?
        .url
        .clone()
}
```

Also: entering the Report tab with no report yet should auto-generate — in `switch_tab`, after setting `app.tab`, when `tab == Tab::Report && app.report.report.is_none() && !app.report.loading`, set `loading = true` and return `vec![Effect::SpawnReport]`. Apply the same on the FIRST `Event::Tick` after startup (add a `started: bool` field to `App`, false initially; on first Tick set it true and, if on Report tab with no report, kick off `SpawnReport`).

- [ ] **Step 3: `src/tui/task.rs` spawn**

```rust
use tokio::sync::mpsc::UnboundedSender;

use crate::config::Config;
use crate::pipeline::{build_report, ReportOptions};
use crate::tui::event::Event;

pub fn spawn_report(tx: UnboundedSender<Event>, cfg: Config, opts: ReportOptions) {
    tokio::spawn(async move {
        let res = build_report(&cfg, &opts).await.map_err(|e| e.to_string());
        let _ = tx.send(Event::ReportReady(res));
    });
}
```

- [ ] **Step 4: Effect execution in `src/tui/mod.rs`**

Extend `run_effect`:

```rust
        Effect::SpawnReport => {
            task::spawn_report(tx.clone(), app.cfg.clone(), app.report_options());
            true
        }
        Effect::OpenUrl(url) => {
            app.status = match open::that(&url) {
                Ok(()) => format!("opened {url}"),
                Err(e) => format!("open failed: {e}"),
            };
            true
        }
        Effect::WriteReport => {
            app.status = write_report(app);
            true
        }
```

with:

```rust
fn write_report(app: &mut App) -> String {
    let Some(rep) = &app.report.report else {
        return "no report to write".into();
    };
    let md = crate::redact::apply(
        &crate::report::markdown::render(rep, false),
        &app.cfg.redact,
    );
    let path = app.cfg.output.clone().unwrap_or_else(|| "report.md".into());
    match std::fs::write(&path, md) {
        Ok(()) => format!("wrote {path}"),
        Err(e) => format!("write failed: {e}"),
    }
}
```

- [ ] **Step 5: Write `src/tui/render/report.rs`** and dispatch from `render/mod.rs` (`Tab::Report => report::draw(f, body, app)`)

```rust
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::redact::scrub_secrets;
use crate::tui::app::{App, Pane, SINCE_CHOICES};

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let r = &app.report;
    if r.loading {
        let spin = ["|", "/", "-", "\\"][app.spinner % 4];
        f.render_widget(
            Paragraph::new(format!("{spin} generating report ({})...", SINCE_CHOICES[r.since_idx]))
                .block(Block::default().borders(Borders::ALL).title("Report")),
            area,
        );
        return;
    }
    if let Some(err) = &r.error {
        f.render_widget(
            Paragraph::new(format!("error: {err}\n\nr to retry"))
                .block(Block::default().borders(Borders::ALL).title("Report")),
            area,
        );
        return;
    }
    let Some(rep) = &r.report else {
        f.render_widget(
            Paragraph::new("press r to generate")
                .block(Block::default().borders(Borders::ALL).title("Report")),
            area,
        );
        return;
    };

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)]).areas(area);

    // Left: group tree.
    let mut lines: Vec<Line> = Vec::new();
    for (i, g) in rep.groups.iter().enumerate() {
        let text = format!("{} ({})", scrub_secrets(&g.title), g.items.len());
        let style = if i == r.selected_group {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        lines.push(Line::styled(text, style));
    }
    let left_border = if r.pane == Pane::Groups { "Groups*" } else { "Groups" };
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(left_border)),
        left,
    );

    // Right: selected group's items, then the redacted sections text.
    let mut detail: Vec<Line> = Vec::new();
    if let Some(g) = rep.groups.get(r.selected_group) {
        for (i, item) in g.items.iter().enumerate() {
            let text = format!(
                "[{}] {} {}",
                item.activity_type,
                scrub_secrets(&item.title),
                item.status.as_deref().unwrap_or("")
            );
            let style = if r.pane == Pane::Detail && i == r.selected_item {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            detail.push(Line::styled(text, style));
        }
        detail.push(Line::from(""));
    }
    for l in &r.sections {
        detail.push(Line::from(l.as_str()));
    }
    let right_border = if r.pane == Pane::Detail { "Detail*" } else { "Detail" };
    f.render_widget(
        Paragraph::new(detail).block(Block::default().borders(Borders::ALL).title(right_border)),
        right,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::model::{ActivityItem, Group, Report, Source};
    use crate::tui::render::{buffer_text, draw as draw_root};
    use chrono::Utc;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn fixture_report() -> Report {
        let item = ActivityItem {
            source: Source::Git,
            source_id: "1".into(),
            title: "token ghp_SECRET123 in title".into(),
            url: Some("https://example.com/x".into()),
            project_key: None,
            repo: Some("devday".into()),
            activity_type: "commit".into(),
            status: None,
            actor: None,
            timestamp: Utc::now(),
            summary: None,
            signals: vec![],
        };
        Report {
            window_start: Utc::now(),
            window_end: Utc::now(),
            groups: vec![Group { key: "devday".into(), title: "devday".into(), items: vec![item] }],
            summary: None,
            worked_on: vec!["x".into()],
            next_up: vec![],
            blockers: vec![],
            source_links: vec![],
            generation_warnings: vec![],
        }
    }

    #[test]
    fn report_tab_renders_and_scrubs_secrets() {
        let mut app = App::new(Config::default(), None);
        app.report.report = Some(fixture_report());
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("devday (1)"));
        assert!(text.contains("[REDACTED]"));
        assert!(!text.contains("ghp_SECRET123"));
    }
}
```

Note: `buffer_text` must become `#[cfg(test)] pub` visible to sibling tests — it already is (`pub` + `#[cfg(test)]` in `render/mod.rs`).

- [ ] **Step 6: Update tests in `src/tui/event.rs`**

```rust
    #[test]
    fn r_spawns_report_once() {
        let mut a = app();
        assert_eq!(update(&mut a, key('r')), vec![Effect::SpawnReport]);
        assert!(a.report.loading);
        // Second r while loading is ignored.
        assert_eq!(update(&mut a, key('r')), vec![]);
    }

    #[test]
    fn report_ready_stores_redacted_sections() {
        let mut a = app();
        a.report.loading = true;
        let rep = crate::model::Report {
            window_start: chrono::Utc::now(),
            window_end: chrono::Utc::now(),
            groups: vec![],
            summary: Some("has ghp_LEAKME99 inside".into()),
            worked_on: vec![],
            next_up: vec![],
            blockers: vec![],
            source_links: vec![],
            generation_warnings: vec![],
        };
        update(&mut a, Event::ReportReady(Ok(rep)));
        assert!(!a.report.loading);
        let joined = a.report.sections.join("\n");
        assert!(joined.contains("[REDACTED]"));
        assert!(!joined.contains("ghp_LEAKME99"));
    }

    #[test]
    fn s_cycles_since_window() {
        let mut a = app();
        update(&mut a, key('s'));
        assert_eq!(a.report.since_idx, 1);
        update(&mut a, key('s'));
        update(&mut a, key('s'));
        assert_eq!(a.report.since_idx, 0);
    }

    #[test]
    fn enter_opens_selected_url() {
        let mut a = app();
        // reuse the render fixture shape: one group, one item with a url
        a.report.report = Some(crate::tui::render::report::tests_fixture());
        let fx = update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(fx, vec![Effect::OpenUrl("https://example.com/x".into())]);
    }
```

For the last test, export the fixture from `render/report.rs` as `#[cfg(test)] pub fn tests_fixture() -> Report` (move `fixture_report`'s body there and have the render test call it too).

- [ ] **Step 7: Run everything**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: green. Manual check in a real terminal: `cargo run -- tui` auto-generates, groups navigate, `Enter` opens a link, `w` writes the file.

- [ ] **Step 8: Commit**

```bash
git add src/tui
git commit -m "feat: TUI report tab - generate and browse interactively (CHA-2395)"
```

---

## Task 4: Slack tab — CHA-2396

**Files:**
- Modify: `src/tui/app.rs`, `src/tui/event.rs`, `src/tui/task.rs`, `src/tui/mod.rs`
- Create: `src/tui/render/slack.rs` (dispatch from `render/mod.rs`)
- Test: inline update tests + render smoke

**Interfaces:**
- Consumes: `deliver::slack::{format_digest, post_webhook, post_bot}`, `state`, `redact`.
- Produces:
  - `app::SlackState { verbose: bool, digest: Option<String>, already_posted: bool, modal: bool, posting: bool, result: Option<String> }` — `App` gains `pub slack: SlackState`.
  - `Event::PostResult(Result<(), String>)`.
  - `Effect::PostSlack(String)`.
  - `task::spawn_post(tx, cfg: Config, text: String)` — chooses webhook (if `webhook_env` set) else bot token (needs `bot_token_env` + channel), reads env vars, posts, on success `mark_posted` + save state; sends `PostResult`. **Bypasses `auto_post` by design (interactive confirm IS the approval). Never include credentials in the error string** — reuse the fixed-constant errors from `deliver::slack` and env-var lookup errors of the form `"env {name} unset"` (name only, never value).

- [ ] **Step 1: State + digest refresh in `src/tui/app.rs`**

```rust
#[derive(Default)]
pub struct SlackState {
    pub verbose: bool,
    pub digest: Option<String>,
    pub already_posted: bool,
    pub modal: bool,
    pub posting: bool,
    pub result: Option<String>,
}
```

Add `pub slack: SlackState` to `App` plus:

```rust
    /// Recompute the redacted digest from the current report (if any).
    pub fn refresh_digest(&mut self) {
        self.slack.digest = self.report.report.as_ref().map(|rep| {
            crate::redact::apply(
                &crate::deliver::slack::format_digest(rep, self.slack.verbose),
                &self.cfg.redact,
            )
        });
        self.slack.already_posted = match &self.slack.digest {
            Some(text) => {
                let st = crate::state::load(&self.cfg.state_path());
                st.already_posted(&crate::state::report_hash(text))
            }
            None => false,
        };
    }

    pub fn slack_creds_configured(&self) -> bool {
        self.cfg.slack.webhook_env.is_some()
            || (self.cfg.slack.bot_token_env.is_some() && self.cfg.slack.channel.is_some())
    }
```

- [ ] **Step 2: Keys + events in `src/tui/event.rs`**

Add `Event::PostResult(Result<(), String>)`, `Effect::PostSlack(String)`. `switch_tab` to `Tab::Slack` calls `app.refresh_digest()`. `ReportReady(Ok)` also calls `app.refresh_digest()` after storing. Add the `Tab::Slack` arm:

```rust
fn slack_key(app: &mut App, key: KeyCode_holder) -> Vec<Effect> { /* signature matches report_key */ }
```

Concretely:

```rust
fn slack_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    if app.slack.modal {
        return match key.code {
            KeyCode::Enter | KeyCode::Char('y') => {
                app.slack.modal = false;
                if let Some(text) = app.slack.digest.clone() {
                    app.slack.posting = true;
                    app.status = "posting...".into();
                    vec![Effect::PostSlack(text)]
                } else {
                    vec![]
                }
            }
            _ => {
                app.slack.modal = false;
                app.status = "post canceled".into();
                vec![]
            }
        };
    }
    match key.code {
        KeyCode::Char('v') => {
            app.slack.verbose = !app.slack.verbose;
            app.refresh_digest();
            vec![]
        }
        KeyCode::Char('p') => {
            if app.slack.posting {
                app.status = "post already in flight".into();
            } else if app.slack.digest.is_none() {
                app.status = "no report yet - generate on the Report tab (1, then r)".into();
            } else if !app.slack_creds_configured() {
                app.status =
                    "no Slack credentials configured (set slack.webhook_env or bot_token_env + channel)"
                        .into();
            } else {
                app.slack.modal = true;
            }
            vec![]
        }
        _ => vec![],
    }
}
```

Handle `Event::PostResult(res)`:

```rust
        Event::PostResult(res) => {
            app.slack.posting = false;
            app.slack.result = Some(match res {
                Ok(()) => "posted successfully".into(),
                Err(e) => format!("post failed: {e}"),
            });
            app.refresh_digest(); // picks up the new posted-hash state
            vec![]
        }
```

- [ ] **Step 3: `task::spawn_post`**

```rust
pub fn spawn_post(tx: UnboundedSender<Event>, cfg: Config, text: String) {
    tokio::spawn(async move {
        let res = post(&cfg, &text).await;
        let _ = tx.send(Event::PostResult(res));
    });
}

async fn post(cfg: &Config, text: &str) -> Result<(), String> {
    use crate::deliver::slack;
    let outcome = if let Some(env) = &cfg.slack.webhook_env {
        let url = std::env::var(env).map_err(|_| format!("env {env} unset"))?;
        slack::post_webhook(&url, text).await
    } else if let (Some(env), Some(channel)) = (&cfg.slack.bot_token_env, &cfg.slack.channel) {
        let token = std::env::var(env).map_err(|_| format!("env {env} unset"))?;
        slack::post_bot("https://slack.com/api", &token, channel, text).await
    } else {
        Err("no Slack webhook or bot token configured".to_string())
    };
    if outcome.is_ok() {
        let path = cfg.state_path();
        let mut st = crate::state::load(&path);
        st.mark_posted(crate::state::report_hash(text), vec![], chrono::Utc::now());
        let _ = crate::state::save(&path, &st);
    }
    outcome
}
```

Effect arm in `run_effect`: `Effect::PostSlack(text) => { task::spawn_post(tx.clone(), app.cfg.clone(), text); true }`.

- [ ] **Step 4: `src/tui/render/slack.rs`**

```rust
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::app::App;

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let s = &app.slack;
    let mut lines: Vec<Line> = Vec::new();
    match &s.digest {
        None => lines.push(Line::from("no report yet - generate on the Report tab (1, then r)")),
        Some(text) => {
            if s.already_posted {
                lines.push(Line::from("!! identical report already posted (dedup) !!"));
                lines.push(Line::from(""));
            }
            for l in text.lines() {
                lines.push(Line::from(l.to_string()));
            }
        }
    }
    if let Some(result) = &s.result {
        lines.push(Line::from(""));
        lines.push(Line::from(result.as_str()));
    }
    let title = format!("Slack preview ({}) - v verbose, p post", if s.verbose { "verbose" } else { "digest" });
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );

    if s.modal {
        let modal = super::centered(area, 50, 6);
        f.render_widget(Clear, modal);
        let channel = app.cfg.slack.channel.as_deref().unwrap_or("(webhook)");
        f.render_widget(
            Paragraph::new(format!("Post to {channel}?\n\nEnter/y confirm - any other key cancels"))
                .block(Block::default().borders(Borders::ALL).title("Confirm post")),
            modal,
        );
    }
}
```

(Make `centered` in `render/mod.rs` `pub(crate)` so tab renderers can use it.)

- [ ] **Step 5: Tests**

Update tests in `event.rs`:

```rust
    #[test]
    fn p_without_creds_sets_status_not_modal() {
        let mut a = app();
        a.tab = Tab::Slack;
        a.slack.digest = Some("hi".into());
        update(&mut a, key('p'));
        assert!(!a.slack.modal);
        assert!(a.status.contains("credentials"));
    }

    #[test]
    fn p_with_creds_opens_modal_and_confirm_posts() {
        let mut a = app();
        a.tab = Tab::Slack;
        a.cfg.slack.webhook_env = Some("HOOK".into());
        a.slack.digest = Some("digest text".into());
        update(&mut a, key('p'));
        assert!(a.slack.modal);
        let fx = update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(fx, vec![Effect::PostSlack("digest text".into())]);
        assert!(a.slack.posting);
    }

    #[test]
    fn modal_any_other_key_cancels() {
        let mut a = app();
        a.tab = Tab::Slack;
        a.cfg.slack.webhook_env = Some("HOOK".into());
        a.slack.digest = Some("x".into());
        update(&mut a, key('p'));
        assert_eq!(update(&mut a, key('n')), vec![]);
        assert!(!a.slack.modal);
        assert!(!a.slack.posting);
    }

    #[test]
    fn post_result_recorded() {
        let mut a = app();
        a.slack.posting = true;
        update(&mut a, Event::PostResult(Err("slack webhook: network error".into())));
        assert!(!a.slack.posting);
        assert!(a.slack.result.as_deref().unwrap().contains("network error"));
    }
```

Render smoke test in `slack.rs` mirroring the report one: app with `digest = Some("*devday status*\nxoxb-SECRET should never appear")` — wait: the digest is ALREADY redacted by `refresh_digest`; the render test instead asserts the modal renders and the dedup banner shows when `already_posted = true`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::Tab;
    use crate::tui::render::{buffer_text, draw as draw_root};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn dedup_banner_and_modal_render() {
        let mut app = App::new(Config::default(), None);
        app.tab = Tab::Slack;
        app.slack.digest = Some("*devday status*".into());
        app.slack.already_posted = true;
        app.slack.modal = true;
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| draw_root(f, &app)).unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("already posted"));
        assert!(text.contains("Confirm post"));
    }
}
```

- [ ] **Step 6: Run everything**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: green.

- [ ] **Step 7: Commit**

```bash
git add src/tui
git commit -m "feat: TUI slack tab - preview, confirm-to-post, dedup warning (CHA-2396)"
```

---

## Task 5: Config tab — CHA-2397

**Files:**
- Modify: `src/config.rs` (add `Serialize`), `src/tui/app.rs`, `src/tui/event.rs`, `src/tui/mod.rs`
- Create: `src/tui/render/config.rs` (dispatch from `render/mod.rs`)
- Test: inline (config round-trip, update tests, render smoke, atomic-save unit test)

**Interfaces:**
- Consumes: `config::Config` (now `Serialize + Deserialize`), `toml`.
- Produces:
  - `Serialize` derive added to `Config` and all its sub-structs (additive; existing tests untouched).
  - `app::{ConfigState, Field}` — a fixed, ordered field list; `ConfigState { selected: usize, editing: Option<String>, confirm_save: bool, error: Option<String>, dirty: bool }`.
  - `Effect::SaveConfig`.
  - `tui::save_config(cfg: &Config, path: &Path) -> Result<(), String>` — serialize → re-parse validate → write `path.tmp` → rename.

- [ ] **Step 1: Add `Serialize` to every config struct in `src/config.rs`**

Change each `#[derive(Debug, Clone, ..., Deserialize, PartialEq)]` to include `Serialize` (import `serde::Serialize`). Add test:

```rust
    #[test]
    fn config_round_trips_through_toml() {
        let mut c = Config::default();
        c.git.roots = vec!["~/code".into()];
        c.slack.channel = Some("#eng".into());
        let text = toml::to_string(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(c, back);
    }
```

- [ ] **Step 2: Field model in `src/tui/app.rs`**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    SourceGithub,
    SourceLinear,
    SourceGit,
    GitRootAdd,        // Enter opens input; appends to cfg.git.roots
    GitRootRemoveLast, // Enter pops the last root
    SlackChannel,
    SlackWebhookEnv,
    SlackBotTokenEnv,
    SlackAutoPost,
    AiProvider,        // Enter cycles none -> claude -> codex
    AiCommand,
    Output,
    DefaultSince,
}

pub const FIELDS: [Field; 13] = [
    Field::SourceGithub,
    Field::SourceLinear,
    Field::SourceGit,
    Field::GitRootAdd,
    Field::GitRootRemoveLast,
    Field::SlackChannel,
    Field::SlackWebhookEnv,
    Field::SlackBotTokenEnv,
    Field::SlackAutoPost,
    Field::AiProvider,
    Field::AiCommand,
    Field::Output,
    Field::DefaultSince,
];

#[derive(Default)]
pub struct ConfigState {
    pub selected: usize,
    /// Some(buffer) while a text field is being edited.
    pub editing: Option<String>,
    pub confirm_save: bool,
    pub error: Option<String>,
    pub dirty: bool,
}
```

Add `pub config_tab: ConfigState` to `App`. Add display + edit helpers on `App`:

```rust
    pub fn field_label(&self, field: Field) -> String {
        let c = &self.cfg;
        match field {
            Field::SourceGithub => format!("[{}] source: github", tick(c.sources.github)),
            Field::SourceLinear => format!("[{}] source: linear", tick(c.sources.linear)),
            Field::SourceGit => format!("[{}] source: git", tick(c.sources.git)),
            Field::GitRootAdd => format!("git roots: {:?}  (Enter: add)", c.git.roots),
            Field::GitRootRemoveLast => "git roots: remove last (Enter)".into(),
            Field::SlackChannel => format!("slack channel: {}", c.slack.channel.as_deref().unwrap_or("-")),
            // env-var NAMES only - never values:
            Field::SlackWebhookEnv => format!("slack webhook_env: {}", c.slack.webhook_env.as_deref().unwrap_or("-")),
            Field::SlackBotTokenEnv => format!("slack bot_token_env: {}", c.slack.bot_token_env.as_deref().unwrap_or("-")),
            Field::SlackAutoPost => format!("[{}] slack auto_post", tick(c.slack.auto_post)),
            Field::AiProvider => format!("ai provider: {} (Enter cycles)", c.ai.provider.as_deref().unwrap_or("none")),
            Field::AiCommand => format!("ai command override: {}", c.ai.command.as_deref().unwrap_or("-")),
            Field::Output => format!("output path: {}", c.output.as_deref().unwrap_or("-")),
            Field::DefaultSince => format!("default_since: {}", c.default_since.as_deref().unwrap_or("-")),
        }
    }
```

with `fn tick(b: bool) -> &'static str { if b { "x" } else { " " } }` as a free fn in app.rs. Text-editable fields start editing with the current value; `apply_field_edit(&mut self, field: Field, value: String)` writes the buffer back (empty string → `None` for the Option fields; for `GitRootAdd` pushes non-empty value).

- [ ] **Step 3: Keys in `src/tui/event.rs`** (`Tab::Config` arm)

```rust
fn config_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    use crate::tui::app::{Field, FIELDS};
    let field = FIELDS[app.config_tab.selected];

    // Text-input mode.
    if let Some(buf) = &mut app.config_tab.editing {
        match key.code {
            KeyCode::Enter => {
                let value = app.config_tab.editing.take().unwrap();
                app.apply_field_edit(field, value);
                app.config_tab.dirty = true;
            }
            KeyCode::Esc => app.config_tab.editing = None,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(ch) => buf.push(ch),
            _ => {}
        }
        return vec![];
    }
    // Save-confirm modal.
    if app.config_tab.confirm_save {
        app.config_tab.confirm_save = false;
        return match key.code {
            KeyCode::Enter | KeyCode::Char('y') => vec![Effect::SaveConfig],
            _ => {
                app.status = "save canceled".into();
                vec![]
            }
        };
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.config_tab.selected = (app.config_tab.selected + 1) % FIELDS.len();
            vec![]
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.config_tab.selected =
                (app.config_tab.selected + FIELDS.len() - 1) % FIELDS.len();
            vec![]
        }
        KeyCode::Enter => {
            match field {
                Field::SourceGithub => app.cfg.sources.github = !app.cfg.sources.github,
                Field::SourceLinear => app.cfg.sources.linear = !app.cfg.sources.linear,
                Field::SourceGit => app.cfg.sources.git = !app.cfg.sources.git,
                Field::SlackAutoPost => app.cfg.slack.auto_post = !app.cfg.slack.auto_post,
                Field::GitRootRemoveLast => {
                    app.cfg.git.roots.pop();
                }
                Field::AiProvider => {
                    app.cfg.ai.provider = match app.cfg.ai.provider.as_deref() {
                        None => Some("claude".into()),
                        Some("claude") => Some("codex".into()),
                        _ => None,
                    };
                }
                Field::GitRootAdd => {
                    app.config_tab.editing = Some(String::new());
                    return vec![];
                }
                _ => {
                    app.config_tab.editing = Some(app.field_current_text(field));
                    return vec![];
                }
            }
            app.config_tab.dirty = true;
            vec![]
        }
        KeyCode::Char('S') => {
            app.config_tab.confirm_save = true;
            vec![]
        }
        _ => vec![],
    }
}
```

(`field_current_text` returns the current string value of a text field, empty for None.)

- [ ] **Step 4: Save path in `src/tui/mod.rs`**

```rust
pub fn save_config(cfg: &crate::config::Config, path: &std::path::Path) -> Result<(), String> {
    let text = toml::to_string_pretty(cfg).map_err(|e| format!("serialize: {e}"))?;
    // Validate: what we write must parse back into an identical Config.
    let _check: crate::config::Config =
        toml::from_str(&text).map_err(|e| format!("round-trip validation failed: {e}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &text).map_err(|e| format!("write: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("rename: {e}"))?;
    Ok(())
}
```

Effect arm:

```rust
        Effect::SaveConfig => {
            let path = app.cfg_path.clone().unwrap_or_else(default_config_path);
            match save_config(&app.cfg, &path) {
                Ok(()) => {
                    app.config_tab.dirty = false;
                    app.config_tab.error = None;
                    app.status = format!("saved {}", path.display());
                }
                Err(e) => {
                    app.config_tab.error = Some(e);
                    app.status = "save failed".into();
                }
            }
            true
        }
```

with `fn default_config_path() -> PathBuf` returning `$HOME/.config/devday/config.toml` (mirror the private helper in config.rs).

- [ ] **Step 5: `src/tui/render/config.rs`**

Renders the field list (selected line REVERSED), a `[modified]` marker when dirty, the input box when `editing` is Some (bordered single-line at the bottom showing the buffer + cursor `_`), the confirm modal when `confirm_save`, and `error` if set. Same Paragraph/Line pattern as the other tabs — follow `render/slack.rs` structurally; no new widget concepts.

- [ ] **Step 6: Tests**

In `event.rs`:

```rust
    #[test]
    fn toggle_source_marks_dirty() {
        let mut a = app();
        a.tab = Tab::Config;
        let before = a.cfg.sources.github;
        update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(a.cfg.sources.github, !before);
        assert!(a.config_tab.dirty);
    }

    #[test]
    fn edit_channel_roundtrip() {
        let mut a = app();
        a.tab = Tab::Config;
        a.config_tab.selected = 5; // SlackChannel
        update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert!(a.config_tab.editing.is_some());
        for ch in "#eng".chars() {
            update(&mut a, key(ch));
        }
        update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(a.cfg.slack.channel.as_deref(), Some("#eng"));
    }

    #[test]
    fn capital_s_then_enter_emits_save() {
        let mut a = app();
        a.tab = Tab::Config;
        update(&mut a, key('S'));
        assert!(a.config_tab.confirm_save);
        let fx = update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(fx, vec![Effect::SaveConfig]);
    }
```

In `tui/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_config_is_atomic_and_reloadable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let mut cfg = crate::config::Config::default();
        cfg.slack.channel = Some("#eng".into());
        save_config(&cfg, &path).unwrap();
        let loaded = crate::config::Config::load(Some(&path)).unwrap();
        assert_eq!(loaded.slack.channel.as_deref(), Some("#eng"));
        assert!(!path.with_extension("toml.tmp").exists());
    }
}
```

Render smoke test in `render/config.rs`: buffer contains `webhook_env` label text and (with `cfg.slack.webhook_env = Some("MY_HOOK_ENV")`) contains `MY_HOOK_ENV` but the test also sets env-style value `std::env::set_var` — NO: keep it simple and pure — assert the buffer shows the env NAME and never calls env at all (the render path has no env access by construction).

- [ ] **Step 7: Run everything**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: green (including the 65 CLI tests — the `Serialize` derive is additive).

- [ ] **Step 8: Commit**

```bash
git add src/config.rs src/tui
git commit -m "feat: TUI config tab - form editor, validation, atomic save (CHA-2397)"
```

---

## Task 6: Doctor/State tab — CHA-2398

**Files:**
- Modify: `src/tui/app.rs`, `src/tui/event.rs`, `src/tui/task.rs`, `src/tui/mod.rs`
- Create: `src/tui/render/doctor.rs` (dispatch from `render/mod.rs`)
- Test: inline update tests + render smoke + clear-state unit test

**Interfaces:**
- Consumes: `doctor::{build_checks, Check}`, `state`, `model::LocalState`.
- Produces:
  - `app::DoctorState { running: bool, checks: Option<Vec<(String, bool, String)>>, state_summary: Option<crate::model::LocalState>, confirm_clear: Option<String> }` — `App` gains `pub doctor: DoctorState`. (`Check` isn't Clone; tasks send plain tuples.)
  - `Event::DoctorReady(Vec<(String, bool, String)>)`.
  - `Effect::{SpawnDoctor, ClearState}`.
  - `task::spawn_doctor(tx, cfg: Config)` — runs `build_checks` with the real env getter inside `tokio::task::spawn_blocking` (binary checks spawn processes).

- [ ] **Step 1: State in app.rs** (as above), plus load the state summary whenever the tab is entered: in `switch_tab` for `Tab::Doctor`, set `app.doctor.state_summary = Some(crate::state::load(&app.cfg.state_path()))` and, if `checks.is_none() && !running`, set running and return `vec![Effect::SpawnDoctor]`.

- [ ] **Step 2: `task::spawn_doctor`**

```rust
pub fn spawn_doctor(tx: UnboundedSender<Event>, cfg: Config) {
    tokio::spawn(async move {
        let checks = tokio::task::spawn_blocking(move || {
            let getter = |k: &str| std::env::var(k).ok();
            crate::doctor::build_checks(&cfg, &getter)
                .into_iter()
                .map(|c| (c.name, c.ok, c.detail))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        let _ = tx.send(Event::DoctorReady(checks));
    });
}
```

- [ ] **Step 3: Keys in event.rs** (`Tab::Doctor` arm)

```rust
fn doctor_key(app: &mut App, key: KeyEvent) -> Vec<Effect> {
    // Typed-confirm modal for clearing state.
    if let Some(buf) = &mut app.doctor.confirm_clear {
        match key.code {
            KeyCode::Enter => {
                let typed = app.doctor.confirm_clear.take().unwrap();
                if typed == "clear" {
                    return vec![Effect::ClearState];
                }
                app.status = "type 'clear' to confirm".into();
            }
            KeyCode::Esc => app.doctor.confirm_clear = None,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(ch) => buf.push(ch),
            _ => {}
        }
        return vec![];
    }
    match key.code {
        KeyCode::Char('r') => {
            if !app.doctor.running {
                app.doctor.running = true;
                vec![Effect::SpawnDoctor]
            } else {
                vec![]
            }
        }
        KeyCode::Char('x') => {
            app.doctor.confirm_clear = Some(String::new());
            vec![]
        }
        _ => vec![],
    }
}
```

`Event::DoctorReady(checks)` sets `running = false; checks = Some(checks)`.

- [ ] **Step 4: ClearState effect in `tui/mod.rs`**

```rust
        Effect::ClearState => {
            let path = app.cfg.state_path();
            app.status = match crate::state::save(&path, &crate::model::LocalState::default()) {
                Ok(()) => "state cleared".into(),
                Err(e) => format!("clear failed: {e}"),
            };
            app.doctor.state_summary = Some(crate::state::load(&path));
            true
        }
        Effect::SpawnDoctor => {
            task::spawn_doctor(tx.clone(), app.cfg.clone());
            true
        }
```

- [ ] **Step 5: `render/doctor.rs`** — two stacked panes: checks (`OK`/`FAIL` prefix, FAIL styled `Style::default().add_modifier(Modifier::REVERSED)`), state summary (last run, posted count, last 5 hashes truncated to 12 chars), the typed-confirm modal when active ("Type 'clear' + Enter to wipe local state: <buffer>_"). Same Paragraph/Line pattern as other tabs.

- [ ] **Step 6: Tests**

```rust
    #[test]
    fn doctor_ready_stores_checks() {
        let mut a = app();
        a.doctor.running = true;
        update(&mut a, Event::DoctorReady(vec![("gh".into(), true, "ok".into())]));
        assert!(!a.doctor.running);
        assert_eq!(a.doctor.checks.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn clear_state_requires_typed_confirm() {
        let mut a = app();
        a.tab = Tab::Doctor;
        update(&mut a, key('x'));
        assert!(a.doctor.confirm_clear.is_some());
        // Wrong word does nothing.
        for ch in "nope".chars() {
            update(&mut a, key(ch));
        }
        assert_eq!(update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter))), vec![]);
        // Correct word emits ClearState.
        update(&mut a, key('x'));
        for ch in "clear".chars() {
            update(&mut a, key(ch));
        }
        assert_eq!(
            update(&mut a, Event::Key(KeyEvent::from(KeyCode::Enter))),
            vec![Effect::ClearState]
        );
    }
```

Render smoke test: checks `[("linear: token".into(), false, "expects env LINEAR_API_KEY".into())]` renders `FAIL` and the env NAME.

- [ ] **Step 7: Run everything**

Run: `cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: green.

- [ ] **Step 8: Commit**

```bash
git add src/tui
git commit -m "feat: TUI doctor/state tab - live checks, state summary, guarded clear (CHA-2398)"
```

---

## Task 7: Test & acceptance pass + README — CHA-2399

**Files:**
- Create: `tests/tui_acceptance.rs`
- Modify: `README.md`, `docs/ACCEPTANCE.md`, `src/tui/render/mod.rs` (multi-size smoke test)

**Interfaces:** none new — closes out the feature.

- [ ] **Step 1: Write `tests/tui_acceptance.rs`**

```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn help_lists_tui_command() {
    Command::cargo_bin("devday")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("tui"));
}

#[test]
fn tui_refuses_non_tty_stdout() {
    // assert_cmd pipes stdout, so is_terminal() is false inside the child.
    Command::cargo_bin("devday")
        .unwrap()
        .env("HOME", tempfile::tempdir().unwrap().path())
        .arg("tui")
        .assert()
        .failure()
        .stderr(contains("interactive terminal"));
}
```

- [ ] **Step 2: Multi-size render smoke test in `src/tui/render/mod.rs`**

```rust
    #[test]
    fn all_tabs_render_at_multiple_sizes() {
        use crate::tui::app::Tab;
        for (w, h) in [(80u16, 24u16), (120, 40), (40, 12)] {
            for tab in Tab::ALL {
                let mut app = App::new(Config::default(), None);
                app.tab = tab;
                let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
                terminal.draw(|f| draw(f, &app)).unwrap(); // must not panic
            }
        }
    }
```

- [ ] **Step 3: README TUI section**

Add after the CLI usage section: what `devday tui` is, the four tabs, the key map (from `HELP_TEXT`), the TTY requirement, and — explicitly — the posting semantics: *"Posting from the TUI requires only the interactive confirmation; the `slack.auto_post` config gate applies to the non-interactive `devday send slack --post` path only."* Update `docs/ACCEPTANCE.md` with a TUI subsection mapping: TTY guard → `tui_refuses_non_tty_stdout`; no-secrets rendering → `report_tab_renders_and_scrubs_secrets`; preview-never-posts → `modal_any_other_key_cancels` + `p_without_creds_sets_status_not_modal`; clear-state guard → `clear_state_requires_typed_confirm`; manual: real-terminal interaction pass (open, navigate all tabs, panic-hook check via `RUST_BACKTRACE=1` and a forced resize).

- [ ] **Step 4: Final gate**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: everything green (65 original + all TUI tests). Manual: full walkthrough in a real terminal — all four tabs, generate, preview, save config to a scratch path, doctor run, clear state, quit; shell intact afterward.

- [ ] **Step 5: Commit**

```bash
git add tests/tui_acceptance.rs README.md docs/ACCEPTANCE.md src/tui/render/mod.rs
git commit -m "test: TUI acceptance coverage and README section (CHA-2399)"
```

---

## Self-Review Notes

**Spec coverage:** §2 decisions → Tasks 1–2 (pipeline, stack, Elm split, deps); §3 module layout → Tasks 2–6; §4 Tab 1 → T3, Tab 2 → T4, Tab 3 → T5, Tab 4 → T6; §5 event/effect flow → T2 (+ per-task effects); §6 terminal safety → T2 (init/restore/panic hook/Ctrl-C/TTY guard) + T7 (non-TTY test); §7 invariants → redaction at render boundaries (T3/T4), env-names-only (T5/T6), confirm-gated post (T4), typed-confirm clear (T6); §8 testing → per-task update/render tests + T7.

**Known simplifications (documented, not silent):**
- Config tab edits a fixed field list, not arbitrary TOML — matches spec scope; org/team filter fields stay config-file-only.
- Report tab's detail pane shows items + the redacted markdown sections rather than a per-section navigator — satisfies "sections as navigable panes" at MVP fidelity; deeper navigation is additive.
- `open::that` result is reported in the status line, not tested (external side effect).
- ratatui API details (exact method names on `Layout`/`Tabs`) may need minor adaptation to the resolved 0.29.x version — implementers should fix compile errors mechanically without changing the architecture.

**Type consistency:** `Effect`/`Event` variants introduced in T3–T6 match their `run_effect`/`update` arms; `ReportState`/`SlackState`/`ConfigState`/`DoctorState` field names used in render modules match app.rs definitions; `pipeline::ReportOptions` field names match the T1 adapter and T3 `report_options()`.
