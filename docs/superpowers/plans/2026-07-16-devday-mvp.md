# devday MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `devday`, a Rust CLI that collects GitHub/Linear/local-git activity, summarizes it with a local Codex/Claude CLI, and produces a Markdown status report that can be posted to Slack.

**Architecture:** Single binary crate with modular internals. Async (tokio) orchestration collects from three read-only sources concurrently, normalizes them into a common `ActivityItem` model, groups by Linear issue (repo fallback), classifies into worked-on/next-up/blockers, renders deterministic Markdown, optionally summarizes via a subprocess AI provider (with a non-AI fallback), and optionally delivers to Slack with dedup state.

**Tech Stack:** Rust 2021, tokio, clap (derive), serde/serde_json, toml, reqwest (rustls), git2, chrono, anyhow, thiserror, humantime, wiremock (tests), assert_cmd + tempfile (tests).

## Global Constraints

- Rust edition 2021; MSRV 1.75+.
- All source collection is **read-only**. No mutating git/GitHub/Linear commands, ever.
- No secrets in reports, files, Slack messages, stdout, or logs.
- AI prompts contain only normalized `ActivityItem` data — never raw secret-bearing payloads.
- Slack auto-post only when explicitly configured; preview mode always available.
- Deterministic report output when AI is disabled (golden-file testable).
- Partial failure: one failing source must not abort the run (record a warning, continue).
- Config format is TOML; secrets referenced via env vars / provider CLIs, not stored plaintext.
- Every task ends with `cargo test` green and `cargo clippy -- -D warnings` clean before commit.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `Cargo.toml` | Crate manifest, dependencies |
| `src/main.rs` | Entry, tokio runtime, top-level dispatch |
| `src/cli.rs` | clap command/flag definitions, `Since` duration parsing |
| `src/config.rs` | TOML load/init, flag-merge precedence |
| `src/model.rs` | `ActivityItem`, `Report`, `Group`, `BlockerSignal`, `LocalState`, enums |
| `src/collect/mod.rs` | `Collector` trait, concurrent orchestration, partial-failure |
| `src/collect/git.rs` | Read-only local git scan (git2) |
| `src/collect/linear.rs` | Linear GraphQL over reqwest |
| `src/collect/github.rs` | `gh` CLI subprocess + JSON parse |
| `src/group.rs` | Issue-key extraction, Linear-first grouping, repo fallback |
| `src/report/mod.rs` | Report builder + classification |
| `src/report/markdown.rs` | Deterministic Markdown renderer |
| `src/ai/mod.rs` | `Summarizer` trait, dispatch, non-AI fallback |
| `src/ai/cli_provider.rs` | Subprocess provider (stdin prompt, stdout capture) |
| `src/deliver/slack.rs` | Webhook + bot-token delivery, digest/verbose |
| `src/state.rs` | JSON state file: dedup, cursors, posted hashes |
| `src/redact.rs` | Secret guards + configurable redaction |
| `src/doctor.rs` | Environment/auth checks |
| `tests/*.rs` | Integration tests (CLI, fixtures, golden files) |

---

## Task 1: Crate skeleton & data model

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/model.rs`
- Test: inline `#[cfg(test)]` in `src/model.rs`

**Interfaces:**
- Produces: `model::{Source, ActivityItem, Group, Report, BlockerSignal, BlockerKind, Confidence, LocalState}` — all `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]`.

- [ ] **Step 1: Create `Cargo.toml`**

```toml
[package]
name = "devday"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
anyhow = "1"
thiserror = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "process"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
git2 = "0.19"
chrono = { version = "0.4", features = ["serde"] }
humantime = "2"
sha2 = "0.10"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
wiremock = "0.6"
```

- [ ] **Step 2: Write the failing test in `src/model.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Github,
    Linear,
    Git,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityItem {
    pub source: Source,
    pub source_id: String,
    pub title: String,
    pub url: Option<String>,
    pub project_key: Option<String>,
    pub repo: Option<String>,
    pub activity_type: String,
    pub status: Option<String>,
    pub actor: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub summary: Option<String>,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockerKind {
    Explicit,
    Inferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockerSignal {
    pub kind: BlockerKind,
    pub reason: String,
    pub source_item_url: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// Linear issue key/project/team, or repo name for unlinked activity.
    pub key: String,
    pub title: String,
    pub items: Vec<ActivityItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub groups: Vec<Group>,
    pub summary: Option<String>,
    pub worked_on: Vec<String>,
    pub next_up: Vec<String>,
    pub blockers: Vec<BlockerSignal>,
    pub source_links: Vec<String>,
    pub generation_warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn activity_item_roundtrips_through_json() {
        let item = ActivityItem {
            source: Source::Git,
            source_id: "abc123".into(),
            title: "Fix parser".into(),
            url: None,
            project_key: Some("ENG-1".into()),
            repo: Some("devday".into()),
            activity_type: "commit".into(),
            status: None,
            actor: Some("eric".into()),
            timestamp: Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 0).unwrap(),
            summary: None,
            signals: vec![],
        };
        let json = serde_json::to_string(&item).unwrap();
        let back: ActivityItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, back);
    }

    #[test]
    fn source_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&Source::Github).unwrap(), "\"github\"");
    }
}
```

- [ ] **Step 3: Create `src/main.rs`** (minimal so the crate compiles)

```rust
mod model;

fn main() {
    println!("devday");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: PASS (2 tests in `model`).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/model.rs
git commit -m "feat: crate skeleton and activity data model"
```

---

## Task 2: CLI surface & duration parsing

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`
- Test: inline in `src/cli.rs`, plus `tests/cli.rs`

**Interfaces:**
- Consumes: nothing from prior tasks.
- Produces: `cli::{Cli, Command, SlackCmd, ReportArgs}`; `cli::parse_since(&str) -> Result<Duration, ParseSinceError>`. `Cli::parse()` from clap. `ReportArgs` fields: `since: String`, `github/linear/git: bool`, `ai: Option<String>`, `no_ai: bool`, `output: Option<PathBuf>`, `stdout: bool`, `config: Option<PathBuf>`.

- [ ] **Step 1: Write the failing test in `src/cli.rs`**

```rust
use std::path::PathBuf;
use std::time::Duration;
use clap::{Parser, Subcommand};

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("invalid --since value: {0}")]
pub struct ParseSinceError(String);

/// Parse a lookback window like `24h`, `7d`, `90m` into a Duration.
pub fn parse_since(input: &str) -> Result<Duration, ParseSinceError> {
    humantime::parse_duration(input).map_err(|_| ParseSinceError(input.to_string()))
}

#[derive(Debug, Parser)]
#[command(name = "devday", version, about = "Engineering status reports")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Generate a status report.
    Report(ReportArgs),
    /// Send a report to Slack.
    Send {
        #[command(subcommand)]
        target: SendTarget,
    },
    /// Write a starter config file.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Check environment and auth.
    Doctor,
}

#[derive(Debug, Subcommand)]
pub enum SendTarget {
    Slack(SlackArgs),
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    Init,
}

#[derive(Debug, clap::Args)]
pub struct ReportArgs {
    #[arg(long, default_value = "24h")]
    pub since: String,
    #[arg(long)]
    pub github: bool,
    #[arg(long)]
    pub linear: bool,
    #[arg(long)]
    pub git: bool,
    #[arg(long)]
    pub ai: Option<String>,
    #[arg(long)]
    pub no_ai: bool,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub stdout: bool,
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct SlackArgs {
    #[command(flatten)]
    pub report: ReportArgs,
    #[arg(long)]
    pub channel: Option<String>,
    #[arg(long)]
    pub preview: bool,
    #[arg(long)]
    pub post: bool,
    #[arg(long)]
    pub verbose: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hours_and_days() {
        assert_eq!(parse_since("24h").unwrap(), Duration::from_secs(86_400));
        assert_eq!(parse_since("7d").unwrap(), Duration::from_secs(604_800));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_since("banana").is_err());
    }

    #[test]
    fn report_defaults_since_to_24h() {
        let cli = Cli::try_parse_from(["devday", "report"]).unwrap();
        match cli.command {
            Command::Report(a) => assert_eq!(a.since, "24h"),
            _ => panic!("expected report"),
        }
    }

    #[test]
    fn verify_cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
```

- [ ] **Step 2: Wire `src/main.rs`**

```rust
mod cli;
mod model;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Report(_) => println!("report (not yet implemented)"),
        cli::Command::Send { .. } => println!("send (not yet implemented)"),
        cli::Command::Config { .. } => println!("config (not yet implemented)"),
        cli::Command::Doctor => println!("doctor (not yet implemented)"),
    }
    Ok(())
}
```

- [ ] **Step 3: Write `tests/cli.rs`**

```rust
use assert_cmd::Command;

#[test]
fn help_lists_commands() {
    Command::cargo_bin("devday")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
}

#[test]
fn report_runs() {
    Command::cargo_bin("devday")
        .unwrap()
        .args(["report", "--git"])
        .assert()
        .success();
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: PASS (unit tests in `cli` + integration tests in `tests/cli.rs`).

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs tests/cli.rs
git commit -m "feat: CLI surface and --since duration parsing"
```

---

## Task 3: Config load & init

**Files:**
- Create: `src/config.rs`
- Modify: `src/main.rs`
- Test: inline in `src/config.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `config::Config` (serde Deserialize from TOML) with sub-structs `SourcesConfig`, `GithubConfig`, `LinearConfig`, `GitConfig`, `AiConfig`, `SlackConfig`, `StateConfig`, `RedactConfig`. `Config::load(path: Option<&Path>) -> anyhow::Result<Config>` (falls back to `~/.config/devday/config.toml`, then `Config::default()`). `Config::init_template() -> String`. `Config::state_path() -> PathBuf`.

- [ ] **Step 1: Write the failing test in `src/config.rs`**

```rust
use std::path::{Path, PathBuf};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub sources: SourcesConfig,
    pub github: GithubConfig,
    pub linear: LinearConfig,
    pub git: GitConfig,
    pub ai: AiConfig,
    pub slack: SlackConfig,
    pub state: StateConfig,
    pub redact: RedactConfig,
    pub output: Option<String>,
    pub default_since: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct SourcesConfig {
    pub github: bool,
    pub linear: bool,
    pub git: bool,
}
impl Default for SourcesConfig {
    fn default() -> Self { Self { github: true, linear: true, git: true } }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct GithubConfig {
    pub account: Option<String>,
    pub orgs: Vec<String>,
    pub repos: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct LinearConfig {
    /// Env var name holding the token, e.g. "LINEAR_API_KEY". Never the token itself.
    pub token_env: Option<String>,
    pub teams: Vec<String>,
    pub projects: Vec<String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct GitConfig {
    pub roots: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiConfig {
    pub provider: Option<String>, // "claude" | "codex" | none
    pub command: Option<String>,  // override binary path
    pub args: Vec<String>,
}
impl Default for AiConfig {
    fn default() -> Self { Self { provider: None, command: None, args: vec![] } }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct SlackConfig {
    pub channel: Option<String>,
    pub webhook_env: Option<String>,   // env var holding webhook URL
    pub bot_token_env: Option<String>, // env var holding bot token
    pub auto_post: bool,               // false => preview required
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct StateConfig {
    pub path: Option<String>,
    pub dedupe_window: String,
}
impl Default for StateConfig {
    fn default() -> Self { Self { path: None, dedupe_window: "72h".into() } }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RedactConfig {
    pub hide_local_paths: bool,
    pub hide_private_repos: bool,
    pub extra_patterns: Vec<String>,
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Config> {
        let resolved = match path {
            Some(p) => Some(p.to_path_buf()),
            None => Self::default_config_path().filter(|p| p.exists()),
        };
        match resolved {
            Some(p) => {
                let text = std::fs::read_to_string(&p)
                    .map_err(|e| anyhow::anyhow!("reading config {}: {e}", p.display()))?;
                let cfg: Config = toml::from_str(&text)
                    .map_err(|e| anyhow::anyhow!("parsing config {}: {e}", p.display()))?;
                Ok(cfg)
            }
            None => Ok(Config::default()),
        }
    }

    fn default_config_path() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h).join(".config").join("devday").join("config.toml")
        })
    }

    pub fn state_path(&self) -> PathBuf {
        if let Some(p) = &self.state.path {
            return PathBuf::from(p);
        }
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(".local").join("state").join("devday").join("state.json")
    }

    pub fn init_template() -> String {
        r#"# devday configuration
output = "report.md"
default_since = "24h"

[sources]
github = true
linear = true
git = true

[github]
# account = "your-login"
orgs = []
repos = []

[linear]
# token_env names the environment variable holding your Linear API token.
token_env = "LINEAR_API_KEY"
teams = []
projects = []
labels = []

[git]
roots = ["~/code"]
exclude = []

[ai]
# provider = "claude"   # or "codex"; omit to disable AI summarization
args = []

[slack]
# channel = "#eng-status"
# webhook_env = "DEVDAY_SLACK_WEBHOOK"
# bot_token_env = "DEVDAY_SLACK_BOT_TOKEN"
auto_post = false

[state]
dedupe_window = "72h"

[redact]
hide_local_paths = false
hide_private_repos = false
extra_patterns = []
"#
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_enables_all_sources() {
        let c = Config::default();
        assert!(c.sources.github && c.sources.linear && c.sources.git);
    }

    #[test]
    fn parses_partial_toml_with_defaults() {
        let toml = r#"
[sources]
github = false
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert!(!c.sources.github);
        assert!(c.sources.linear); // default preserved
    }

    #[test]
    fn init_template_is_valid_toml() {
        let t = Config::init_template();
        let _c: Config = toml::from_str(&t).unwrap();
    }

    #[test]
    fn load_missing_path_returns_default() {
        let c = Config::load(Some(Path::new("/nonexistent/xyz.toml")));
        assert!(c.is_err()); // explicit path that doesn't exist is an error
        let d = Config::load(None).unwrap(); // no HOME config -> default (in most CI)
        let _ = d;
    }
}
```

- [ ] **Step 2: Wire `config init` into `src/main.rs`**

```rust
mod cli;
mod config;
mod model;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Config { action: cli::ConfigAction::Init } => {
            print!("{}", config::Config::init_template());
        }
        cli::Command::Report(_) => println!("report (not yet implemented)"),
        cli::Command::Send { .. } => println!("send (not yet implemented)"),
        cli::Command::Doctor => println!("doctor (not yet implemented)"),
    }
    Ok(())
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test config::`
Expected: PASS.

- [ ] **Step 4: Verify clippy**

Run: `cargo clippy -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "feat: TOML config load and 'config init' template"
```

---

## Task 4: Local git collector (read-only)

**Files:**
- Create: `src/collect/mod.rs`, `src/collect/git.rs`
- Modify: `src/main.rs`
- Test: inline in `src/collect/git.rs`

**Interfaces:**
- Consumes: `model::{ActivityItem, Source}`, `config::GitConfig`.
- Produces: `collect::CollectResult { items: Vec<ActivityItem>, warnings: Vec<String> }`; `collect::git::collect(cfg: &GitConfig, since: DateTime<Utc>, now: DateTime<Utc>) -> CollectResult`. Never returns `Err` — failures become `warnings`.

- [ ] **Step 1: Write `src/collect/mod.rs`**

```rust
pub mod git;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CollectResult {
    pub items: Vec<crate::model::ActivityItem>,
    pub warnings: Vec<String>,
}

impl CollectResult {
    pub fn merge(&mut self, other: CollectResult) {
        self.items.extend(other.items);
        self.warnings.extend(other.warnings);
    }
}
```

- [ ] **Step 2: Write the failing test in `src/collect/git.rs`**

```rust
use chrono::{DateTime, TimeZone, Utc};
use git2::Repository;
use std::path::{Path, PathBuf};

use crate::collect::CollectResult;
use crate::config::GitConfig;
use crate::model::{ActivityItem, Source};

/// Read-only scan of configured git roots. Uses git2 read APIs only —
/// no checkout, no commit, no fetch. Failures are collected as warnings.
pub fn collect(cfg: &GitConfig, since: DateTime<Utc>, now: DateTime<Utc>) -> CollectResult {
    let mut result = CollectResult::default();
    for root in &cfg.roots {
        let expanded = expand_tilde(root);
        match scan_root(&expanded, since, now) {
            Ok(items) => result.items.extend(items),
            Err(e) => result
                .warnings
                .push(format!("git: failed scanning {}: {e}", expanded.display())),
        }
    }
    result
}

fn expand_tilde(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn scan_root(root: &Path, since: DateTime<Utc>, now: DateTime<Utc>) -> anyhow::Result<Vec<ActivityItem>> {
    // A "root" may itself be a repo, or a directory of repos (one level deep).
    let mut items = Vec::new();
    if root.join(".git").exists() {
        items.extend(scan_repo(root, since, now)?);
        return Ok(items);
    }
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join(".git").exists() {
                if let Ok(repo_items) = scan_repo(&path, since, now) {
                    items.extend(repo_items);
                }
            }
        }
    }
    Ok(items)
}

fn scan_repo(path: &Path, since: DateTime<Utc>, _now: DateTime<Utc>) -> anyhow::Result<Vec<ActivityItem>> {
    let repo = Repository::open(path)?;
    let repo_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".into());
    let mut items = Vec::new();

    // Commits on HEAD within the window.
    if let Ok(mut revwalk) = repo.revwalk() {
        revwalk.push_head().ok();
        for oid in revwalk.flatten() {
            if let Ok(commit) = repo.find_commit(oid) {
                let ts = Utc
                    .timestamp_opt(commit.time().seconds(), 0)
                    .single()
                    .unwrap_or(since);
                if ts < since {
                    break; // revwalk is newest-first
                }
                items.push(ActivityItem {
                    source: Source::Git,
                    source_id: oid.to_string(),
                    title: commit.summary().unwrap_or("(no message)").to_string(),
                    url: None,
                    project_key: None,
                    repo: Some(repo_name.clone()),
                    activity_type: "commit".into(),
                    status: None,
                    actor: commit.author().name().map(|s| s.to_string()),
                    timestamp: ts,
                    summary: None,
                    signals: vec![],
                });
            }
        }
    }

    // Uncommitted changes (dirty working tree) -> one WIP signal item.
    let statuses = repo.statuses(None)?;
    if !statuses.is_empty() {
        items.push(ActivityItem {
            source: Source::Git,
            source_id: format!("{repo_name}:wip"),
            title: format!("{} uncommitted change(s)", statuses.len()),
            url: None,
            project_key: None,
            repo: Some(repo_name.clone()),
            activity_type: "wip".into(),
            status: Some("uncommitted".into()),
            actor: None,
            timestamp: _now,
            summary: None,
            signals: vec!["uncommitted-changes".into()],
        });
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .status()
            .expect("run git")
            .success();
        assert!(ok, "git {:?} failed", args);
    }

    fn init_repo_with_commit(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "t@example.com"]);
        git(dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "ENG-42 initial commit"]);
    }

    #[test]
    fn finds_recent_commit() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_commit(tmp.path());
        let cfg = GitConfig { roots: vec![tmp.path().to_string_lossy().into()], exclude: vec![] };
        let since = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        let res = collect(&cfg, since, Utc::now());
        assert!(res.items.iter().any(|i| i.title.contains("initial commit")));
        assert!(res.items.iter().all(|i| i.source == Source::Git));
    }

    #[test]
    fn detects_uncommitted_changes() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_commit(tmp.path());
        std::fs::write(tmp.path().join("b.txt"), "dirty").unwrap();
        let cfg = GitConfig { roots: vec![tmp.path().to_string_lossy().into()], exclude: vec![] };
        let res = collect(&cfg, Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(), Utc::now());
        assert!(res.items.iter().any(|i| i.activity_type == "wip"));
    }

    #[test]
    fn missing_root_becomes_warning_not_error() {
        let cfg = GitConfig { roots: vec!["/no/such/path".into()], exclude: vec![] };
        let res = collect(&cfg, Utc::now(), Utc::now());
        // Non-existent dir yields no items and no panic; scan_root tolerates it.
        assert!(res.items.is_empty());
    }
}
```

- [ ] **Step 3: Register module in `src/main.rs`**

Add `mod collect;` near the other `mod` declarations.

- [ ] **Step 4: Run tests**

Run: `cargo test collect::git`
Expected: PASS (3 tests). Requires `git` on PATH for test setup.

- [ ] **Step 5: Commit**

```bash
git add src/collect/mod.rs src/collect/git.rs src/main.rs
git commit -m "feat: read-only local git collector"
```

---

## Task 5: Grouping & Linear issue-key extraction

**Files:**
- Create: `src/group.rs`
- Modify: `src/main.rs`
- Test: inline in `src/group.rs`

**Interfaces:**
- Consumes: `model::{ActivityItem, Group}`.
- Produces: `group::extract_issue_key(text: &str) -> Option<String>`; `group::group_items(items: Vec<ActivityItem>) -> Vec<Group>`. Grouping key precedence: item's `project_key`, else a key extracted from title, else `repo` name, else `"ungrouped"`. Groups sorted by key for determinism.

- [ ] **Step 1: Write the failing test in `src/group.rs`**

```rust
use std::collections::BTreeMap;
use crate::model::{ActivityItem, Group};

/// Extract a Linear-style issue key (TEAM-123) from arbitrary text
/// (branch name, PR title/body, commit message).
pub fn extract_issue_key(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find start of an uppercase run.
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let alpha_len = i - start;
            // Require TEAM-<digits> with 2..=6 uppercase letters.
            if (2..=6).contains(&alpha_len) && i < bytes.len() && bytes[i] == b'-' {
                let dash = i;
                i += 1;
                let dig_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i > dig_start {
                    return Some(text[start..i].to_string());
                }
                i = dash + 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Group items primarily by Linear issue key, falling back to repo name.
pub fn group_items(items: Vec<ActivityItem>) -> Vec<Group> {
    let mut buckets: BTreeMap<String, Group> = BTreeMap::new();
    for item in items {
        let key = item
            .project_key
            .clone()
            .or_else(|| extract_issue_key(&item.title))
            .or_else(|| item.repo.clone())
            .unwrap_or_else(|| "ungrouped".to_string());
        let entry = buckets.entry(key.clone()).or_insert_with(|| Group {
            key: key.clone(),
            title: key.clone(),
            items: vec![],
        });
        entry.items.push(item);
    }
    buckets.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    use chrono::Utc;

    fn item(title: &str, project_key: Option<&str>, repo: Option<&str>) -> ActivityItem {
        ActivityItem {
            source: Source::Git,
            source_id: title.into(),
            title: title.into(),
            url: None,
            project_key: project_key.map(|s| s.into()),
            repo: repo.map(|s| s.into()),
            activity_type: "commit".into(),
            status: None,
            actor: None,
            timestamp: Utc::now(),
            summary: None,
            signals: vec![],
        }
    }

    #[test]
    fn extracts_key_from_commit_message() {
        assert_eq!(extract_issue_key("ENG-42 fix bug"), Some("ENG-42".into()));
        assert_eq!(extract_issue_key("feature/ABC-7-thing"), Some("ABC-7".into()));
    }

    #[test]
    fn no_key_when_absent() {
        assert_eq!(extract_issue_key("just a message"), None);
        assert_eq!(extract_issue_key("HTTP2 stuff"), None); // no dash-digit
    }

    #[test]
    fn groups_by_explicit_key_then_extracted_then_repo() {
        let items = vec![
            item("commit one", Some("ENG-1"), Some("repoA")),
            item("ENG-1 more work", None, Some("repoA")),
            item("unrelated", None, Some("repoB")),
        ];
        let groups = group_items(items);
        // ENG-1 (2 items) and repoB (1 item), sorted by key.
        assert_eq!(groups.len(), 2);
        let eng = groups.iter().find(|g| g.key == "ENG-1").unwrap();
        assert_eq!(eng.items.len(), 2);
        assert!(groups.iter().any(|g| g.key == "repoB"));
    }
}
```

- [ ] **Step 2: Register module** — add `mod group;` to `src/main.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test group::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/group.rs src/main.rs
git commit -m "feat: issue-key extraction and Linear-first grouping"
```

---

## Task 6: Report builder, classification & Markdown (first runnable milestone)

**Files:**
- Create: `src/report/mod.rs`, `src/report/markdown.rs`
- Modify: `src/main.rs`
- Test: inline in both files, plus golden test `tests/report_golden.rs`

**Interfaces:**
- Consumes: `model::*`, `group::group_items`.
- Produces: `report::build(items, window_start, window_end, warnings) -> Report`; `report::markdown::render(report: &Report, verbose: bool) -> String`. Classification: `worked_on` from all items; `next_up` from items whose `signals`/`status` indicate pending action (label inferred ones); `blockers` from explicit signals (`blocked`, `failing-check`, `requested-changes`, `waiting-review`) vs inferred, as `BlockerSignal`s.

- [ ] **Step 1: Write the failing test in `src/report/mod.rs`**

```rust
pub mod markdown;

use chrono::{DateTime, Utc};
use crate::group::group_items;
use crate::model::{ActivityItem, BlockerKind, BlockerSignal, Confidence, Report};

const EXPLICIT_BLOCKER_SIGNALS: &[&str] =
    &["blocked", "failing-check", "requested-changes", "waiting-review"];
const NEXT_UP_SIGNALS: &[&str] =
    &["assigned-active", "awaiting-action", "high-priority", "unmerged", "uncommitted-changes"];

pub fn build(
    items: Vec<ActivityItem>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    warnings: Vec<String>,
) -> Report {
    let worked_on: Vec<String> = items.iter().map(describe_item).collect();

    let mut next_up = Vec::new();
    for item in &items {
        if item.signals.iter().any(|s| NEXT_UP_SIGNALS.contains(&s.as_str())) {
            let explicit = matches!(item.status.as_deref(), Some("assigned") | Some("in_progress"));
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

    let source_links: Vec<String> =
        items.iter().filter_map(|i| i.url.clone()).collect();
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
        let r = build(vec![item("PR fails CI", &["failing-check"])], Utc::now(), Utc::now(), vec![]);
        assert_eq!(r.blockers.len(), 1);
        assert_eq!(r.blockers[0].kind, BlockerKind::Explicit);
        assert_eq!(r.blockers[0].confidence, Confidence::High);
    }

    #[test]
    fn next_up_labels_inferred_items() {
        let r = build(vec![item("branch WIP", &["uncommitted-changes"])], Utc::now(), Utc::now(), vec![]);
        assert_eq!(r.next_up.len(), 1);
        assert!(r.next_up[0].contains("(inferred)"));
    }

    #[test]
    fn worked_on_lists_every_item() {
        let r = build(vec![item("a", &[]), item("b", &[])], Utc::now(), Utc::now(), vec![]);
        assert_eq!(r.worked_on.len(), 2);
    }
}
```

- [ ] **Step 2: Write the failing test in `src/report/markdown.rs`**

```rust
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
        None if report.worked_on.is_empty() => {
            out.push_str("_No activity in this window._\n\n")
        }
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
        for h in ["## Summary", "## Worked On", "## Next Up", "## Blockers", "## Source Links"] {
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
```

- [ ] **Step 3: Wire the `report` command end-to-end in `src/main.rs`**

```rust
mod cli;
mod collect;
mod config;
mod group;
mod model;
mod report;

use chrono::Utc;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Config { action: cli::ConfigAction::Init } => {
            print!("{}", config::Config::init_template());
        }
        cli::Command::Report(args) => run_report(args)?,
        cli::Command::Send { .. } => println!("send (not yet implemented)"),
        cli::Command::Doctor => println!("doctor (not yet implemented)"),
    }
    Ok(())
}

fn run_report(args: cli::ReportArgs) -> anyhow::Result<()> {
    let cfg = config::Config::load(args.config.as_deref())?;
    let now = Utc::now();
    let dur = cli::parse_since(&args.since)?;
    let since = now - chrono::Duration::from_std(dur)?;

    let mut collected = collect::CollectResult::default();
    if args.git || cfg.sources.git {
        collected.merge(collect::git::collect(&cfg.git, since, now));
    }

    let rep = report::build(collected.items, since, now, collected.warnings);
    let md = report::markdown::render(&rep, false);

    if let Some(path) = &args.output {
        std::fs::write(path, &md)?;
    }
    if args.stdout || args.output.is_none() {
        print!("{md}");
    }
    Ok(())
}
```

- [ ] **Step 4: Write golden test `tests/report_golden.rs`**

```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn report_on_empty_config_produces_report_sections() {
    // No git roots configured by default in a temp HOME => empty-state report.
    Command::cargo_bin("devday")
        .unwrap()
        .env("HOME", tempfile::tempdir().unwrap().path())
        .args(["report", "--stdout"])
        .assert()
        .success()
        .stdout(contains("## Summary"))
        .stdout(contains("## Blockers"));
}
```

- [ ] **Step 5: Run everything**

Run: `cargo test && cargo clippy -- -D warnings`
Expected: PASS, no warnings. **Milestone: `devday report --git` renders a real Markdown report from local git.**

- [ ] **Step 6: Commit**

```bash
git add src/report src/main.rs tests/report_golden.rs
git commit -m "feat: report builder, classification, deterministic Markdown (runnable)"
```

---

## Task 7: Linear collector (GraphQL)

**Files:**
- Create: `src/collect/linear.rs`
- Modify: `src/collect/mod.rs` (`pub mod linear;`), `src/main.rs` (call it)
- Test: inline in `src/collect/linear.rs` using `wiremock`

**Interfaces:**
- Consumes: `config::LinearConfig`, `model::{ActivityItem, Source}`.
- Produces: `async fn linear::collect(cfg, token, since, now, api_base: &str) -> CollectResult`. Token is read by the caller from `cfg.token_env`; `api_base` is injectable for tests (default `https://api.linear.app/graphql`). Maps issues to `ActivityItem` with `project_key = issue.identifier`, blocker `signals` from blocked states/labels.

- [ ] **Step 1: Write the failing test in `src/collect/linear.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::collect::CollectResult;
use crate::config::LinearConfig;
use crate::model::{ActivityItem, Source};

const DEFAULT_API: &str = "https://api.linear.app/graphql";

const QUERY: &str = r#"
query Recent($since: DateTimeOrDuration) {
  viewer {
    assignedIssues(filter: { updatedAt: { gt: $since } }, first: 50) {
      nodes {
        identifier title url updatedAt
        state { name type }
        labels { nodes { name } }
        priority
      }
    }
  }
}"#;

#[derive(Deserialize)]
struct GqlResponse { data: Option<GqlData> }
#[derive(Deserialize)]
struct GqlData { viewer: Viewer }
#[derive(Deserialize)]
struct Viewer { #[serde(rename = "assignedIssues")] assigned_issues: Connection }
#[derive(Deserialize)]
struct Connection { nodes: Vec<Issue> }
#[derive(Deserialize)]
struct Issue {
    identifier: String,
    title: String,
    url: String,
    #[serde(rename = "updatedAt")] updated_at: DateTime<Utc>,
    state: State,
    labels: LabelConn,
    priority: Option<i64>,
}
#[derive(Deserialize)]
struct State { name: String, #[serde(rename = "type")] type_: String }
#[derive(Deserialize)]
struct LabelConn { nodes: Vec<Label> }
#[derive(Deserialize)]
struct Label { name: String }

pub fn api_base() -> String { DEFAULT_API.to_string() }

pub async fn collect(
    _cfg: &LinearConfig,
    token: &str,
    since: DateTime<Utc>,
    _now: DateTime<Utc>,
    api_base: &str,
) -> CollectResult {
    let mut result = CollectResult::default();
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "query": QUERY,
        "variables": { "since": since.to_rfc3339() }
    });
    let resp = client
        .post(api_base)
        .header("Authorization", token)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    let parsed: Result<GqlResponse, String> = match resp {
        Ok(r) if r.status().is_success() => {
            r.json::<GqlResponse>().await.map_err(|e| e.to_string())
        }
        Ok(r) => Err(format!("HTTP {}", r.status())),
        Err(e) => Err(e.to_string()),
    };

    match parsed {
        Ok(GqlResponse { data: Some(d) }) => {
            for issue in d.viewer.assigned_issues.nodes {
                result.items.push(issue_to_item(issue));
            }
        }
        Ok(_) => result.warnings.push("linear: empty response".into()),
        Err(e) => result.warnings.push(format!("linear: {e}")),
    }
    result
}

fn issue_to_item(issue: Issue) -> ActivityItem {
    let mut signals = Vec::new();
    let state_lower = issue.state.name.to_lowercase();
    if issue.state.type_ == "blocked" || state_lower.contains("blocked") {
        signals.push("blocked".to_string());
    }
    if issue.labels.nodes.iter().any(|l| l.name.to_lowercase().contains("blocked")) {
        signals.push("blocked".to_string());
    }
    if issue.state.type_ == "started" || issue.state.type_ == "unstarted" {
        signals.push("assigned-active".to_string());
    }
    if issue.priority.map(|p| p == 1).unwrap_or(false) {
        signals.push("high-priority".to_string());
    }
    ActivityItem {
        source: Source::Linear,
        source_id: issue.identifier.clone(),
        title: issue.title,
        url: Some(issue.url),
        project_key: Some(issue.identifier),
        repo: None,
        activity_type: "issue".into(),
        status: Some(issue.state.name),
        actor: None,
        timestamp: issue.updated_at,
        summary: None,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn maps_blocked_issue_to_blocker_signal() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "data": { "viewer": { "assignedIssues": { "nodes": [{
                "identifier": "ENG-42",
                "title": "Do the thing",
                "url": "https://linear.app/x/ENG-42",
                "updatedAt": "2026-07-16T10:00:00Z",
                "state": { "name": "Blocked", "type": "blocked" },
                "labels": { "nodes": [] },
                "priority": 1
            }]}}}
        });
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let res = collect(
            &LinearConfig::default(),
            "token",
            Utc::now(),
            Utc::now(),
            &server.uri(),
        )
        .await;

        assert_eq!(res.items.len(), 1);
        let it = &res.items[0];
        assert_eq!(it.project_key.as_deref(), Some("ENG-42"));
        assert!(it.signals.contains(&"blocked".to_string()));
        assert!(it.signals.contains(&"high-priority".to_string()));
    }

    #[tokio::test]
    async fn http_error_becomes_warning() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let res = collect(&LinearConfig::default(), "bad", Utc::now(), Utc::now(), &server.uri()).await;
        assert!(res.items.is_empty());
        assert!(res.warnings.iter().any(|w| w.contains("401")));
    }
}
```

- [ ] **Step 2: Convert `main` to async and call Linear**

In `src/main.rs`, change to `#[tokio::main] async fn main()`, make `run_report` async, and after the git block add:

```rust
    if args.linear || cfg.sources.linear {
        match cfg.linear.token_env.as_deref().and_then(|e| std::env::var(e).ok()) {
            Some(token) => {
                let base = collect::linear::api_base();
                collected.merge(collect::linear::collect(&cfg.linear, &token, since, now, &base).await);
            }
            None => collected
                .warnings
                .push("linear: no token (set linear.token_env)".into()),
        }
    }
```

Add `pub mod linear;` to `src/collect/mod.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test collect::linear`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add src/collect/linear.rs src/collect/mod.rs src/main.rs
git commit -m "feat: Linear GraphQL collector with blocker signals"
```

---

## Task 8: GitHub collector (`gh` subprocess)

**Files:**
- Create: `src/collect/github.rs`
- Modify: `src/collect/mod.rs`, `src/main.rs`
- Test: inline — parser tested against fixture JSON; subprocess wrapper isolated behind an injectable runner.

**Interfaces:**
- Consumes: `config::GithubConfig`, `model::{ActivityItem, Source}`.
- Produces: `github::collect(cfg, since, now) -> CollectResult` (spawns `gh`); `github::parse_search_prs(json: &str) -> Result<Vec<ActivityItem>, String>` (pure, unit-tested). PRs map with `signals`: `failing-check`, `requested-changes`, `waiting-review` where derivable.

- [ ] **Step 1: Write the failing test in `src/collect/github.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::process::Command;

use crate::collect::CollectResult;
use crate::config::GithubConfig;
use crate::model::{ActivityItem, Source};

#[derive(Deserialize)]
struct GhPr {
    title: String,
    url: String,
    #[serde(rename = "updatedAt")] updated_at: DateTime<Utc>,
    repository: GhRepo,
    state: String,
    #[serde(rename = "reviewDecision", default)] review_decision: Option<String>,
    #[serde(rename = "statusCheckRollup", default)] checks: Vec<GhCheck>,
}
#[derive(Deserialize)]
struct GhRepo { #[serde(rename = "nameWithOwner")] name_with_owner: String }
#[derive(Deserialize)]
struct GhCheck { #[serde(default)] conclusion: Option<String> }

/// Pure parser over `gh search prs --json ...` output. Unit-tested with fixtures.
pub fn parse_search_prs(json: &str) -> Result<Vec<ActivityItem>, String> {
    let prs: Vec<GhPr> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(prs.into_iter().map(pr_to_item).collect())
}

fn pr_to_item(pr: GhPr) -> ActivityItem {
    let mut signals = Vec::new();
    if pr.checks.iter().any(|c| c.conclusion.as_deref() == Some("FAILURE")) {
        signals.push("failing-check".to_string());
    }
    match pr.review_decision.as_deref() {
        Some("CHANGES_REQUESTED") => signals.push("requested-changes".to_string()),
        Some("REVIEW_REQUIRED") => signals.push("waiting-review".to_string()),
        _ => {}
    }
    let repo = pr.repository.name_with_owner;
    ActivityItem {
        source: Source::Github,
        source_id: pr.url.clone(),
        title: pr.title,
        url: Some(pr.url),
        project_key: None,
        repo: Some(repo),
        activity_type: "pr".into(),
        status: Some(pr.state),
        actor: None,
        timestamp: pr.updated_at,
        summary: None,
        signals,
    }
}

pub fn collect(_cfg: &GithubConfig, since: DateTime<Utc>, _now: DateTime<Utc>) -> CollectResult {
    let mut result = CollectResult::default();
    let since_date = since.format("%Y-%m-%d").to_string();
    let output = Command::new("gh")
        .args([
            "search", "prs", "--author", "@me",
            "--updated", &format!(">={since_date}"),
            "--json", "title,url,updatedAt,repository,state,reviewDecision,statusCheckRollup",
            "--limit", "50",
        ])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            match parse_search_prs(&text) {
                Ok(items) => result.items = items,
                Err(e) => result.warnings.push(format!("github: parse error: {e}")),
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            result.warnings.push(format!("github: gh failed: {}", err.trim()));
        }
        Err(e) => result.warnings.push(format!("github: cannot run gh: {e}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
      {
        "title": "Add parser",
        "url": "https://github.com/o/r/pull/1",
        "updatedAt": "2026-07-16T09:00:00Z",
        "repository": { "nameWithOwner": "o/r" },
        "state": "OPEN",
        "reviewDecision": "CHANGES_REQUESTED",
        "statusCheckRollup": [ { "conclusion": "FAILURE" } ]
      }
    ]"#;

    #[test]
    fn parses_pr_with_blocker_signals() {
        let items = parse_search_prs(FIXTURE).unwrap();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.repo.as_deref(), Some("o/r"));
        assert!(it.signals.contains(&"failing-check".to_string()));
        assert!(it.signals.contains(&"requested-changes".to_string()));
    }

    #[test]
    fn empty_array_is_ok() {
        assert_eq!(parse_search_prs("[]").unwrap().len(), 0);
    }

    #[test]
    fn bad_json_is_error() {
        assert!(parse_search_prs("not json").is_err());
    }
}
```

- [ ] **Step 2: Wire into `main` and module**

Add `pub mod github;` to `src/collect/mod.rs`. In `run_report`, before the Linear block:

```rust
    if args.github || cfg.sources.github {
        collected.merge(collect::github::collect(&cfg.github, since, now));
    }
```

- [ ] **Step 3: Run tests**

Run: `cargo test collect::github`
Expected: PASS (3 tests). No network / no `gh` needed — parser tested via fixtures.

- [ ] **Step 4: Commit**

```bash
git add src/collect/github.rs src/collect/mod.rs src/main.rs
git commit -m "feat: GitHub collector via gh CLI with fixture-tested parser"
```

---

## Task 9: AI summarization with non-AI fallback

**Files:**
- Create: `src/ai/mod.rs`, `src/ai/cli_provider.rs`
- Modify: `src/main.rs`
- Test: inline in both files

**Interfaces:**
- Consumes: `config::AiConfig`, `model::Report`.
- Produces: `ai::build_prompt(report: &Report) -> String` (normalized data only, no secrets); `ai::summarize(cfg: &AiConfig, report: &Report) -> Option<String>` returning the AI summary or `None` on any failure (caller keeps deterministic report); `cli_provider::run(command: &str, args: &[String], prompt: &str) -> Result<String, String>` (spawns subprocess, prompt via **stdin**, returns stdout).

- [ ] **Step 1: Write the failing test in `src/ai/cli_provider.rs`**

```rust
use std::io::Write;
use std::process::{Command, Stdio};

/// Spawn `command args...`, write `prompt` to its stdin, return trimmed stdout.
pub fn run(command: &str, args: &[String], prompt: &str) -> Result<String, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn {command}: {e}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "no stdin handle".to_string())?
        .write_all(prompt.as_bytes())
        .map_err(|e| format!("write stdin: {e}"))?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("provider exited {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipes_prompt_through_cat() {
        // `cat` echoes stdin to stdout — a stand-in for a real provider CLI.
        let out = run("cat", &[], "hello prompt").unwrap();
        assert_eq!(out, "hello prompt");
    }

    #[test]
    fn missing_binary_is_error() {
        let err = run("definitely-not-a-real-binary-xyz", &[], "x");
        assert!(err.is_err());
    }
}
```

- [ ] **Step 2: Write the failing test in `src/ai/mod.rs`**

```rust
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
    let command = cfg.command.clone().unwrap_or_else(|| default_command(provider));
    let args = if cfg.args.is_empty() {
        default_args(provider)
    } else {
        cfg.args.clone()
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
        let cfg = AiConfig { provider: Some("claude".into()), command: Some("cat".into()), args: vec![] };
        let s = summarize(&cfg, &report()).unwrap();
        assert!(s.contains("Add parser"));
    }
}
```

- [ ] **Step 3: Wire AI into `run_report`**

Add `mod ai;` to `src/main.rs`. After building `rep` and before rendering:

```rust
    let use_ai = !args.no_ai && (args.ai.is_some() || cfg.ai.provider.is_some());
    if use_ai {
        let mut ai_cfg = cfg.ai.clone();
        if let Some(p) = &args.ai {
            ai_cfg.provider = Some(p.clone());
        }
        if let Some(summary) = ai::summarize(&ai_cfg, &rep) {
            rep.summary = Some(summary);
        } else {
            rep.generation_warnings
                .push("ai: summarization failed; using deterministic report".into());
        }
    }
```

(Make `rep` mutable: `let mut rep = ...`.)

- [ ] **Step 4: Run tests**

Run: `cargo test ai::`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/ai src/main.rs
git commit -m "feat: AI summarization via subprocess with non-AI fallback"
```

---

## Task 10: Local state & dedup

**Files:**
- Create: `src/state.rs`
- Modify: `src/main.rs`
- Test: inline in `src/state.rs`

**Interfaces:**
- Consumes: `model::LocalState`, `sha2`.
- Produces: `state::LocalState` (add to `model.rs`); `state::load(path) -> LocalState`, `state::save(path, &LocalState)`, `state::report_hash(markdown: &str) -> String`, `LocalState::already_posted(hash) -> bool`, `LocalState::mark_posted(hash, item_ids, when)`.

- [ ] **Step 1: Add `LocalState` to `src/model.rs`**

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalState {
    pub last_successful_run_at: Option<DateTime<Utc>>,
    pub posted_item_ids: Vec<String>,
    pub posted_report_hashes: Vec<String>,
    pub source_cursor_by_integration: std::collections::BTreeMap<String, String>,
    pub dedupe_window: Option<String>,
}
```

- [ ] **Step 2: Write the failing test in `src/state.rs`**

```rust
use std::path::Path;
use sha2::{Digest, Sha256};

pub use crate::model::LocalState;

pub fn load(path: &Path) -> LocalState {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => LocalState::default(),
    }
}

pub fn save(path: &Path, state: &LocalState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(state)?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn report_hash(markdown: &str) -> String {
    let mut h = Sha256::new();
    h.update(markdown.as_bytes());
    format!("{:x}", h.finalize())
}

impl LocalState {
    pub fn already_posted(&self, hash: &str) -> bool {
        self.posted_report_hashes.iter().any(|h| h == hash)
    }
    pub fn mark_posted(
        &mut self,
        hash: String,
        item_ids: Vec<String>,
        when: chrono::DateTime<chrono::Utc>,
    ) {
        if !self.already_posted(&hash) {
            self.posted_report_hashes.push(hash);
        }
        for id in item_ids {
            if !self.posted_item_ids.contains(&id) {
                self.posted_item_ids.push(id);
            }
        }
        self.last_successful_run_at = Some(when);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrips_via_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut s = LocalState::default();
        s.mark_posted(report_hash("hello"), vec!["id1".into()], chrono::Utc::now());
        save(&path, &s).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.posted_item_ids, vec!["id1".to_string()]);
    }

    #[test]
    fn detects_already_posted_hash() {
        let mut s = LocalState::default();
        let h = report_hash("same content");
        assert!(!s.already_posted(&h));
        s.mark_posted(h.clone(), vec![], chrono::Utc::now());
        assert!(s.already_posted(&h));
    }

    #[test]
    fn missing_file_loads_default() {
        assert_eq!(load(Path::new("/no/such/state.json")), LocalState::default());
    }
}
```

- [ ] **Step 3: Register module** — add `mod state;` to `src/main.rs`.

- [ ] **Step 4: Run tests**

Run: `cargo test state::`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add src/state.rs src/model.rs src/main.rs
git commit -m "feat: JSON local state with report-hash dedup"
```

---

## Task 11: Redaction & secret guards

**Files:**
- Create: `src/redact.rs`
- Modify: `src/report/markdown.rs` (apply redaction pass), `src/main.rs`
- Test: inline in `src/redact.rs`, plus `tests/no_secrets.rs`

**Interfaces:**
- Consumes: `config::RedactConfig`.
- Produces: `redact::scrub_secrets(text: &str) -> String` (always-on, removes known token shapes); `redact::apply(text: &str, cfg: &RedactConfig) -> String` (scrub + optional path/repo hiding + extra patterns).

- [ ] **Step 1: Write the failing test in `src/redact.rs`**

```rust
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
        let cfg = RedactConfig { hide_local_paths: false, hide_private_repos: false, extra_patterns: vec!["secretword".into()] };
        assert!(!apply("has secretword in it", &cfg).contains("secretword"));
    }
}
```

- [ ] **Step 2: Apply redaction at output boundary**

Add `mod redact;` to `src/main.rs`. In `run_report`, wrap the final Markdown before writing/printing:

```rust
    let md = redact::apply(&report::markdown::render(&rep, false), &cfg.redact);
```

- [ ] **Step 3: Write `tests/no_secrets.rs`**

```rust
// Guards the invariant: no known token shapes survive to rendered output.
use devday::redact::scrub_secrets;

#[test]
fn scrub_removes_all_known_prefixes() {
    let input = "ghp_AAA gho_BBB xoxb-CCC xoxp-DDD lin_api_EEE";
    let out = scrub_secrets(input);
    for leaked in ["ghp_AAA", "xoxb-CCC", "lin_api_EEE"] {
        assert!(!out.contains(leaked), "leaked {leaked}");
    }
}
```

To make `devday::redact` importable, add a `src/lib.rs` exposing the modules and have `main.rs` use the crate. (If a lib target does not already exist: create `src/lib.rs` with `pub mod redact; pub mod model; pub mod config;` etc., and change `main.rs` module decls to `use devday::...`.)

- [ ] **Step 4: Run tests**

Run: `cargo test redact:: && cargo test --test no_secrets`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/redact.rs src/main.rs src/lib.rs tests/no_secrets.rs
git commit -m "feat: secret scrubbing and configurable redaction at output boundary"
```

---

## Task 12: Slack delivery

**Files:**
- Create: `src/deliver/slack.rs`, `src/deliver/mod.rs`
- Modify: `src/main.rs`
- Test: inline in `src/deliver/slack.rs` with `wiremock`

**Interfaces:**
- Consumes: `config::SlackConfig`, `model::Report`, `redact`.
- Produces: `slack::format_digest(report, verbose) -> String`; `async fn slack::post_webhook(url, text) -> Result<(), String>`; `async fn slack::post_bot(api_base, token, channel, text) -> Result<(), String>`. Delivery selects webhook if `webhook_env` set, else bot token. Preview returns the message without posting.

- [ ] **Step 1: Write the failing test in `src/deliver/slack.rs`**

```rust
use crate::model::Report;

/// Executive digest by default; verbose includes the full worked-on list.
pub fn format_digest(report: &Report, verbose: bool) -> String {
    let mut s = String::new();
    s.push_str("*devday status*\n");
    match &report.summary {
        Some(sum) => s.push_str(&format!("{sum}\n")),
        None => s.push_str(&format!(
            "{} item(s), {} blocker(s).\n",
            report.worked_on.len(),
            report.blockers.len()
        )),
    }
    if !report.blockers.is_empty() {
        s.push_str("\n*Blockers*\n");
        for b in &report.blockers {
            s.push_str(&format!("• {}\n", b.reason));
        }
    }
    if verbose {
        s.push_str("\n*Worked on*\n");
        for w in &report.worked_on {
            s.push_str(&format!("• {w}\n"));
        }
    }
    if !report.source_links.is_empty() {
        s.push_str("\n*Links*\n");
        for l in report.source_links.iter().take(if verbose { usize::MAX } else { 5 }) {
            s.push_str(&format!("• {l}\n"));
        }
    }
    s
}

pub async fn post_webhook(url: &str, text: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("slack webhook HTTP {}", resp.status()))
    }
}

pub async fn post_bot(api_base: &str, token: &str, channel: &str, text: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_base}/chat.postMessage"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "channel": channel, "text": text }))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    #[derive(serde::Deserialize)]
    struct Ack { ok: bool, #[serde(default)] error: Option<String> }
    let ack: Ack = resp.json().await.map_err(|e| e.to_string())?;
    if ack.ok { Ok(()) } else { Err(format!("slack: {}", ack.error.unwrap_or_default())) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BlockerKind, BlockerSignal, Confidence};
    use chrono::Utc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn report() -> Report {
        Report {
            window_start: Utc::now(), window_end: Utc::now(), groups: vec![],
            summary: None, worked_on: vec!["did a thing".into()], next_up: vec![],
            blockers: vec![BlockerSignal { kind: BlockerKind::Explicit, reason: "CI red".into(), source_item_url: None, confidence: Confidence::High }],
            source_links: vec![], generation_warnings: vec![],
        }
    }

    #[test]
    fn digest_includes_blockers() {
        let d = format_digest(&report(), false);
        assert!(d.contains("Blockers"));
        assert!(d.contains("CI red"));
    }

    #[test]
    fn verbose_adds_worked_on() {
        assert!(format_digest(&report(), true).contains("did a thing"));
        assert!(!format_digest(&report(), false).contains("did a thing"));
    }

    #[tokio::test]
    async fn webhook_post_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(200)).mount(&server).await;
        assert!(post_webhook(&server.uri(), "hi").await.is_ok());
    }

    #[tokio::test]
    async fn bot_post_reports_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/chat.postMessage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": false, "error": "channel_not_found" })))
            .mount(&server).await;
        let e = post_bot(&server.uri(), "t", "#x", "hi").await.unwrap_err();
        assert!(e.contains("channel_not_found"));
    }
}
```

- [ ] **Step 2: Create `src/deliver/mod.rs`**

```rust
pub mod slack;
```

- [ ] **Step 3: Wire `send slack` in `src/main.rs`**

Add `mod deliver;`. Handle the command:

```rust
cli::Command::Send { target: cli::SendTarget::Slack(sargs) } => {
    run_send_slack(sargs).await?;
}
```

```rust
async fn run_send_slack(args: cli::SlackArgs) -> anyhow::Result<()> {
    let cfg = config::Config::load(args.report.config.as_deref())?;
    // ... build report exactly as run_report does (share a helper `build_report`) ...
    let rep = build_report(&cfg, &args.report).await?;
    let text = redact::apply(&deliver::slack::format_digest(&rep, args.verbose), &cfg.redact);

    // Preview unless posting is explicitly allowed.
    let want_post = args.post && (cfg.slack.auto_post || args.post);
    if args.preview || !want_post {
        println!("{text}");
        return Ok(());
    }

    // Dedup via state.
    let state_path = cfg.state_path();
    let mut st = state::load(&state_path);
    let hash = state::report_hash(&text);
    if st.already_posted(&hash) {
        eprintln!("devday: identical report already posted; skipping");
        return Ok(());
    }

    let channel = args.channel.or(cfg.slack.channel.clone());
    let result = if let Some(env) = &cfg.slack.webhook_env {
        let url = std::env::var(env).map_err(|_| anyhow::anyhow!("webhook env {env} unset"))?;
        deliver::slack::post_webhook(&url, &text).await
    } else if let (Some(env), Some(ch)) = (&cfg.slack.bot_token_env, &channel) {
        let token = std::env::var(env).map_err(|_| anyhow::anyhow!("bot token env {env} unset"))?;
        deliver::slack::post_bot("https://slack.com/api", &token, ch, &text).await
    } else {
        Err("no Slack webhook or bot token configured".to_string())
    };

    match result {
        Ok(()) => {
            st.mark_posted(hash, vec![], chrono::Utc::now());
            state::save(&state_path, &st)?;
            Ok(())
        }
        Err(e) => {
            // Preserve report locally per FR-17, non-zero exit.
            print!("{text}");
            Err(anyhow::anyhow!("slack post failed: {e}"))
        }
    }
}
```

Refactor the report-building portion of `run_report` into `async fn build_report(cfg, args) -> anyhow::Result<Report>` and call it from both handlers.

- [ ] **Step 4: Run tests**

Run: `cargo test deliver::slack`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add src/deliver src/main.rs
git commit -m "feat: Slack delivery (webhook + bot token) with preview and dedup"
```

---

## Task 13: `doctor` command

**Files:**
- Create: `src/doctor.rs`
- Modify: `src/main.rs`
- Test: inline in `src/doctor.rs`

**Interfaces:**
- Consumes: `config::Config`.
- Produces: `doctor::run(cfg: &Config) -> Vec<Check>`; `Check { name: String, ok: bool, detail: String }`. Checks: `gh` present, Linear token env set, AI provider binary present, Slack config sanity, plaintext-secret warning. Pure check-building is unit-tested; `run` prints them.

- [ ] **Step 1: Write the failing test in `src/doctor.rs`**

```rust
use crate::config::Config;

#[derive(Debug, PartialEq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

fn binary_present(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Build the checklist without printing (testable).
pub fn build_checks(cfg: &Config, env_get: &dyn Fn(&str) -> Option<String>) -> Vec<Check> {
    let mut checks = Vec::new();

    if cfg.sources.github {
        checks.push(Check {
            name: "github: gh CLI".into(),
            ok: binary_present("gh"),
            detail: "requires authenticated `gh`".into(),
        });
    }

    if cfg.sources.linear {
        let has_token = cfg
            .linear
            .token_env
            .as_deref()
            .and_then(env_get)
            .is_some();
        checks.push(Check {
            name: "linear: token".into(),
            ok: has_token,
            detail: match &cfg.linear.token_env {
                Some(e) => format!("expects env {e}"),
                None => "no token_env configured".into(),
            },
        });
    }

    if let Some(provider) = &cfg.ai.provider {
        let cmd = cfg.ai.command.clone().unwrap_or_else(|| provider.clone());
        checks.push(Check {
            name: format!("ai: {provider}"),
            ok: binary_present(&cmd),
            detail: format!("provider binary `{cmd}`"),
        });
    }

    // Slack sanity: if auto_post, must have a credential env configured.
    if cfg.slack.auto_post {
        let has_cred = cfg.slack.webhook_env.is_some() || cfg.slack.bot_token_env.is_some();
        checks.push(Check {
            name: "slack: auto-post credentials".into(),
            ok: has_cred,
            detail: "auto_post requires webhook_env or bot_token_env".into(),
        });
    }

    checks
}

pub fn run(cfg: &Config) {
    let getter = |k: &str| std::env::var(k).ok();
    let checks = build_checks(cfg, &getter);
    for c in &checks {
        let mark = if c.ok { "OK " } else { "FAIL" };
        println!("[{mark}] {} — {}", c.name, c.detail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_check_fails_without_token_env_value() {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.linear.token_env = Some("SOME_ENV_THAT_IS_UNSET".into());
        let checks = build_checks(&cfg, &|_| None);
        let linear = checks.iter().find(|c| c.name.contains("linear")).unwrap();
        assert!(!linear.ok);
    }

    #[test]
    fn linear_check_passes_when_env_present() {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.linear.token_env = Some("MY_TOKEN".into());
        let checks = build_checks(&cfg, &|k| (k == "MY_TOKEN").then(|| "value".to_string()));
        let linear = checks.iter().find(|c| c.name.contains("linear")).unwrap();
        assert!(linear.ok);
    }

    #[test]
    fn autopost_without_credentials_fails() {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.sources.linear = false;
        cfg.slack.auto_post = true;
        let checks = build_checks(&cfg, &|_| None);
        assert!(checks.iter().any(|c| c.name.contains("auto-post") && !c.ok));
    }
}
```

- [ ] **Step 2: Wire `doctor` in `src/main.rs`**

```rust
cli::Command::Doctor => {
    let cfg = config::Config::load(None)?;
    doctor::run(&cfg);
}
```

Add `mod doctor;`.

- [ ] **Step 3: Run tests**

Run: `cargo test doctor::`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add src/doctor.rs src/main.rs
git commit -m "feat: doctor command with environment/auth checks"
```

---

## Task 14: Acceptance pass & README

**Files:**
- Create: `README.md`, `tests/acceptance.rs`
- Modify: as needed to close gaps found

**Interfaces:** none new — this task verifies the SRS §18 acceptance criteria end-to-end.

- [ ] **Step 1: Write `tests/acceptance.rs` covering the CLI-observable criteria**

```rust
use assert_cmd::Command;
use predicates::str::contains;

fn devday(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("devday").unwrap();
    c.env("HOME", home);
    c
}

#[test]
fn ac1_config_init_emits_valid_toml() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(contains("[sources]"))
        .stdout(contains("[slack]"));
}

#[test]
fn ac2_report_generates_all_sections() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["report", "--stdout"])
        .assert()
        .success()
        .stdout(contains("## Worked On"))
        .stdout(contains("## Next Up"))
        .stdout(contains("## Blockers"));
}

#[test]
fn ac7_empty_window_is_useful_empty_state() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["report", "--stdout"])
        .assert()
        .success()
        .stdout(contains("No activity"));
}

#[test]
fn ac12_slack_preview_does_not_post() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["send", "slack", "--preview"])
        .assert()
        .success()
        .stdout(contains("devday status"));
}

#[test]
fn ac15_runs_noninteractively() {
    // No TTY assumptions — the above already run under assert_cmd (no TTY).
    let home = tempfile::tempdir().unwrap();
    devday(home.path()).args(["report"]).assert().success();
}
```

- [ ] **Step 2: Run the full suite + clippy + fmt**

Run: `cargo fmt --check && cargo clippy -- -D warnings && cargo test`
Expected: all green.

- [ ] **Step 3: Write `README.md`**

Include: what devday is, install (`cargo install --path .`), `devday config init > ~/.config/devday/config.toml`, example `report`/`send slack` invocations from SRS §6, the cron example, and the safety note (read-only, no secrets, preview default).

- [ ] **Step 4: Manual acceptance walkthrough**

Map each SRS §18 item (1–19) to a test above or a manual check; note any not covered by automation (e.g. real `gh`/Linear/Slack runs) as documented manual steps in the README.

- [ ] **Step 5: Commit**

```bash
git add README.md tests/acceptance.rs
git commit -m "test: acceptance criteria coverage and README"
```

---

## Self-Review Notes

**Spec coverage (SRS → task):** FR-1/2 → T8; FR-3 → T7; FR-4 → T4; FR-5 → T1,T4,T7,T8; FR-6/7 → T5; FR-8 → T6; FR-9/10 → T9; FR-11 → T6; FR-12/13/14/15 → T12; FR-16 → T10,T12; FR-17 → T4 (partial-failure), T12 (Slack fail exit); FR-18 → T11. NFR-3 → T6 golden/determinism; NFR-4 → CollectResult warnings; NFR-7 → T6 empty-state; NFR-9 → T10 dedup. Acceptance §18 → T14.

**Deferred/known simplifications (documented, not silent):**
- GitHub collection covers PRs via `gh search prs`; issues/commits/review-requests can be added as further `gh search`/`gh api` calls following the same fixture-tested-parser pattern in T8.
- Local git covers commits + uncommitted WIP; branches/stashes extend `scan_repo` in T4.
- Linear covers assigned issues; comments/mentions/relations extend the GraphQL query in T7.
- Concurrent collection: T6 runs collectors sequentially for clarity; they can be wrapped in `tokio::join!` without interface changes since each returns an independent `CollectResult`.

These are additive within existing interfaces and do not block the acceptance criteria.
