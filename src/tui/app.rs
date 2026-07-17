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
