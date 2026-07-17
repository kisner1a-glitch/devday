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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
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

#[derive(Default)]
pub struct SlackState {
    pub verbose: bool,
    pub digest: Option<String>,
    pub already_posted: bool,
    pub modal: bool,
    pub posting: bool,
    pub result: Option<String>,
}

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
    AiProvider, // Enter cycles none -> claude -> codex
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

#[derive(Default)]
pub struct DoctorState {
    pub running: bool,
    /// `(name, ok, detail)` tuples - `doctor::Check` isn't Clone, so the
    /// background task sends plain tuples instead.
    pub checks: Option<Vec<(String, bool, String)>>,
    pub state_summary: Option<crate::model::LocalState>,
    /// Some(buffer) while the typed "clear" confirmation is active.
    pub confirm_clear: Option<String>,
}

fn tick(b: bool) -> &'static str {
    if b {
        "x"
    } else {
        " "
    }
}

pub struct App {
    pub cfg: Config,
    pub cfg_path: Option<PathBuf>,
    pub tab: Tab,
    pub status: String,
    pub show_help: bool,
    pub spinner: usize,
    pub started: bool,
    pub report: ReportState,
    pub slack: SlackState,
    pub config_tab: ConfigState,
    pub doctor: DoctorState,
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
            started: false,
            report: ReportState::default(),
            slack: SlackState::default(),
            config_tab: ConfigState::default(),
            doctor: DoctorState::default(),
        }
    }

    pub fn report_options(&self) -> crate::pipeline::ReportOptions {
        crate::pipeline::ReportOptions {
            since: Some(SINCE_CHOICES[self.report.since_idx].to_string()),
            no_ai: !self.report.ai_enabled,
            ..Default::default()
        }
    }

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

    pub fn field_label(&self, field: Field) -> String {
        let c = &self.cfg;
        match field {
            Field::SourceGithub => format!("[{}] source: github", tick(c.sources.github)),
            Field::SourceLinear => format!("[{}] source: linear", tick(c.sources.linear)),
            Field::SourceGit => format!("[{}] source: git", tick(c.sources.git)),
            Field::GitRootAdd => format!("git roots: {:?}  (Enter: add)", c.git.roots),
            Field::GitRootRemoveLast => "git roots: remove last (Enter)".into(),
            Field::SlackChannel => {
                format!(
                    "slack channel: {}",
                    c.slack.channel.as_deref().unwrap_or("-")
                )
            }
            // env-var NAMES only - never values:
            Field::SlackWebhookEnv => format!(
                "slack webhook_env: {}",
                c.slack.webhook_env.as_deref().unwrap_or("-")
            ),
            Field::SlackBotTokenEnv => format!(
                "slack bot_token_env: {}",
                c.slack.bot_token_env.as_deref().unwrap_or("-")
            ),
            Field::SlackAutoPost => format!("[{}] slack auto_post", tick(c.slack.auto_post)),
            Field::AiProvider => format!(
                "ai provider: {} (Enter cycles)",
                c.ai.provider.as_deref().unwrap_or("none")
            ),
            Field::AiCommand => {
                format!(
                    "ai command override: {}",
                    c.ai.command.as_deref().unwrap_or("-")
                )
            }
            Field::Output => format!("output path: {}", c.output.as_deref().unwrap_or("-")),
            Field::DefaultSince => {
                format!(
                    "default_since: {}",
                    c.default_since.as_deref().unwrap_or("-")
                )
            }
        }
    }

    /// Writes an edited text buffer back into the config. Empty string clears
    /// Option fields; `GitRootAdd` appends a non-empty value to the roots list.
    pub fn apply_field_edit(&mut self, field: Field, value: String) {
        let opt = |s: String| if s.is_empty() { None } else { Some(s) };
        match field {
            Field::GitRootAdd => {
                if !value.is_empty() {
                    self.cfg.git.roots.push(value);
                }
            }
            Field::SlackChannel => self.cfg.slack.channel = opt(value),
            Field::SlackWebhookEnv => self.cfg.slack.webhook_env = opt(value),
            Field::SlackBotTokenEnv => self.cfg.slack.bot_token_env = opt(value),
            Field::AiCommand => self.cfg.ai.command = opt(value),
            Field::Output => self.cfg.output = opt(value),
            Field::DefaultSince => self.cfg.default_since = opt(value),
            // Toggled/cycled directly in config_key; never reached via text edit.
            Field::SourceGithub
            | Field::SourceLinear
            | Field::SourceGit
            | Field::GitRootRemoveLast
            | Field::SlackAutoPost
            | Field::AiProvider => {}
        }
    }

    /// Current string value of a text field, used to seed the edit buffer.
    /// Empty for unset Option fields.
    pub fn field_current_text(&self, field: Field) -> String {
        match field {
            Field::GitRootAdd => String::new(),
            Field::SlackChannel => self.cfg.slack.channel.clone().unwrap_or_default(),
            Field::SlackWebhookEnv => self.cfg.slack.webhook_env.clone().unwrap_or_default(),
            Field::SlackBotTokenEnv => self.cfg.slack.bot_token_env.clone().unwrap_or_default(),
            Field::AiCommand => self.cfg.ai.command.clone().unwrap_or_default(),
            Field::Output => self.cfg.output.clone().unwrap_or_default(),
            Field::DefaultSince => self.cfg.default_since.clone().unwrap_or_default(),
            Field::SourceGithub
            | Field::SourceLinear
            | Field::SourceGit
            | Field::GitRootRemoveLast
            | Field::SlackAutoPost
            | Field::AiProvider => String::new(),
        }
    }
}
