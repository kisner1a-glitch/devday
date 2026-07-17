use chrono::Utc;
use clap::Parser;
use devday::model::Report;
use devday::{ai, cli, collect, config, deliver, doctor, redact, report, state};

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
        cli::Command::Send {
            target: cli::SendTarget::Slack(sargs),
        } => run_send_slack(sargs).await?,
        cli::Command::Doctor => {
            let cfg = config::Config::load(None)?;
            doctor::run(&cfg);
        }
    }
    Ok(())
}

/// Collect activity from configured sources and build the (AI-summarized,
/// un-redacted) report. Shared by `report` and `send slack` so both commands
/// build reports identically.
async fn build_report(cfg: &config::Config, args: &cli::ReportArgs) -> anyhow::Result<Report> {
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

    Ok(rep)
}

async fn run_report(args: cli::ReportArgs) -> anyhow::Result<()> {
    let cfg = config::Config::load(args.config.as_deref())?;
    let rep = build_report(&cfg, &args).await?;

    let md = redact::apply(&report::markdown::render(&rep, false), &cfg.redact);

    if let Some(path) = &args.output {
        std::fs::write(path, &md)?;
    }
    if args.stdout || args.output.is_none() {
        print!("{md}");
    }
    Ok(())
}

async fn run_send_slack(args: cli::SlackArgs) -> anyhow::Result<()> {
    let cfg = config::Config::load(args.report.config.as_deref())?;
    let rep = build_report(&cfg, &args.report).await?;
    let text = redact::apply(
        &deliver::slack::format_digest(&rep, args.verbose),
        &cfg.redact,
    );

    // Posting requires BOTH --post and cfg.slack.auto_post; --preview always
    // forces preview regardless of the other flags.
    let want_post = args.post && cfg.slack.auto_post;
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
