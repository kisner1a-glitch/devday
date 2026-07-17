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
    // Per-tab state is added by Tasks 5-6:
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
            started: false,
            report: ReportState::default(),
            slack: SlackState::default(),
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
}
