mod ai;
mod cli;
mod collect;
mod config;
mod group;
mod model;
mod report;
// Consumed by Slack delivery in Task 12; unused for now.
#[allow(dead_code)]
mod state;

use chrono::Utc;
use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    match cli.command {
        cli::Command::Config {
            action: cli::ConfigAction::Init,
        } => {
            print!("{}", config::Config::init_template());
        }
        cli::Command::Report(args) => run_report(args).await?,
        cli::Command::Send { .. } => println!("send (not yet implemented)"),
        cli::Command::Doctor => println!("doctor (not yet implemented)"),
    }
    Ok(())
}

async fn run_report(args: cli::ReportArgs) -> anyhow::Result<()> {
    let cfg = config::Config::load(args.config.as_deref())?;
    let now = Utc::now();
    let dur = cli::parse_since(&args.since)?;
    let since = now - chrono::Duration::from_std(dur)?;

    let mut collected = collect::CollectResult::default();
    if args.git || cfg.sources.git {
        collected.merge(collect::git::collect(&cfg.git, since, now));
    }
    if args.github || cfg.sources.github {
        collected.merge(collect::github::collect(&cfg.github, since, now));
    }
    if args.linear || cfg.sources.linear {
        match cfg
            .linear
            .token_env
            .as_deref()
            .and_then(|e| std::env::var(e).ok())
        {
            Some(token) => {
                let base = collect::linear::api_base();
                collected
                    .merge(collect::linear::collect(&cfg.linear, &token, since, now, &base).await);
            }
            None => collected
                .warnings
                .push("linear: no token (set linear.token_env)".into()),
        }
    }

    let mut rep = report::build(collected.items, since, now, collected.warnings);

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

    let md = report::markdown::render(&rep, false);

    if let Some(path) = &args.output {
        std::fs::write(path, &md)?;
    }
    if args.stdout || args.output.is_none() {
        print!("{md}");
    }
    Ok(())
}
