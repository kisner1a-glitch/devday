use clap::Parser;
use devday::{cli, config, deliver, doctor, redact, report, state};

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
        cli::Command::Tui(targs) => devday::tui::run(targs.config.as_deref()).await?,
    }
    Ok(())
}

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

async fn run_report(args: cli::ReportArgs) -> anyhow::Result<()> {
    let cfg = config::Config::load(args.config.as_deref())?;
    let rep = devday::pipeline::build_report(&cfg, &to_options(&args)).await?;

    let output_path = args
        .output
        .clone()
        .or_else(|| cfg.output.clone().map(std::path::PathBuf::from));
    let format = report::resolve_format(args.format, output_path.as_deref());
    if format == report::OutputFormat::Pdf && output_path.is_none() {
        anyhow::bail!("pdf output requires --output <path> (or output in config)");
    }
    if let Some(path) = &output_path {
        report::write_report_file(&rep, path, format, &cfg.redact)?;
    }
    if args.stdout || output_path.is_none() {
        let md = redact::apply(&report::markdown::render(&rep, false), &cfg.redact);
        print!("{md}");
    }
    Ok(())
}

async fn run_send_slack(args: cli::SlackArgs) -> anyhow::Result<()> {
    let cfg = config::Config::load(args.report.config.as_deref())?;
    let rep = devday::pipeline::build_report(&cfg, &to_options(&args.report)).await?;
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
