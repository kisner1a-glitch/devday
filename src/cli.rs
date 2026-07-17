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
    /// Open the interactive terminal UI.
    Tui(TuiArgs),
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
    #[arg(long)]
    pub since: Option<String>,
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
pub struct TuiArgs {
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
    fn report_since_defaults_to_none_when_absent() {
        // clap no longer supplies a default; the effective "24h" default is
        // resolved by main.rs (flag > cfg.default_since > "24h").
        let cli = Cli::try_parse_from(["devday", "report"]).unwrap();
        match cli.command {
            Command::Report(a) => assert_eq!(a.since, None),
            _ => panic!("expected report"),
        }
    }

    #[test]
    fn report_since_flag_is_parsed() {
        let cli = Cli::try_parse_from(["devday", "report", "--since", "48h"]).unwrap();
        match cli.command {
            Command::Report(a) => assert_eq!(a.since, Some("48h".to_string())),
            _ => panic!("expected report"),
        }
    }

    #[test]
    fn tui_command_parses() {
        let cli = Cli::try_parse_from(["devday", "tui"]).unwrap();
        assert!(matches!(cli.command, Command::Tui(_)));
    }

    #[test]
    fn verify_cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
