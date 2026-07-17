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

pub struct App {
    pub cfg: Config,
    pub cfg_path: Option<PathBuf>,
    pub tab: Tab,
    pub status: String,
    pub show_help: bool,
    pub spinner: usize,
    pub started: bool,
    pub report: ReportState,
    // Per-tab state is added by Tasks 4-6:
    // pub slack: SlackState,
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
        }
    }

    pub fn report_options(&self) -> crate::pipeline::ReportOptions {
        crate::pipeline::ReportOptions {
            since: Some(SINCE_CHOICES[self.report.since_idx].to_string()),
            no_ai: !self.report.ai_enabled,
            ..Default::default()
        }
    }
}
