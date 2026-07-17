//! Background tokio tasks. Each spawn sends its result back as an Event
//! over the channel; the event loop never blocks on collection or posting.

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
