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

pub fn spawn_doctor(tx: UnboundedSender<Event>, cfg: Config) {
    tokio::spawn(async move {
        let checks = tokio::task::spawn_blocking(move || {
            let getter = |k: &str| std::env::var(k).ok();
            crate::doctor::build_checks(&cfg, &getter)
                .into_iter()
                .map(|c| (c.name, c.ok, c.detail))
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        let _ = tx.send(Event::DoctorReady(checks));
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
    outcome?;
    let path = cfg.state_path();
    let mut st = crate::state::load(&path);
    st.mark_posted(crate::state::report_hash(text), vec![], chrono::Utc::now());
    crate::state::save(&path, &st)
        .map_err(|e| format!("posted, but saving dedup state failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// A successful webhook post whose dedup-state save also succeeds must
    /// return Ok(()).
    #[tokio::test]
    async fn post_success_with_working_state_save_returns_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let env_name = "DEVDAY_TASK_TEST_WEBHOOK_OK";
        std::env::set_var(env_name, server.uri());

        let mut cfg = Config::default();
        cfg.slack.webhook_env = Some(env_name.to_string());
        let dir = tempfile::tempdir().unwrap();
        cfg.state.path = Some(dir.path().join("state.json").display().to_string());

        let result = post(&cfg, "hello").await;
        std::env::remove_var(env_name);

        assert!(result.is_ok());
    }

    /// A successful webhook post whose dedup-state save fails must surface
    /// that failure in the returned Err, not silently discard it (a
    /// silently-discarded save failure would let a duplicate slip past
    /// dedup on the next run). Regression test for the earlier `let _ =
    /// crate::state::save(...)` bug.
    #[tokio::test]
    async fn post_success_surfaces_state_save_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let env_name = "DEVDAY_TASK_TEST_WEBHOOK_SAVE_FAIL";
        std::env::set_var(env_name, server.uri());

        let mut cfg = Config::default();
        cfg.slack.webhook_env = Some(env_name.to_string());
        // Force state::save to fail: /dev/null is a file, not a directory,
        // so create_dir_all on a path nested under it errors.
        cfg.state.path = Some("/dev/null/unwritable/state.json".to_string());

        let result = post(&cfg, "hello").await;
        std::env::remove_var(env_name);

        let err = result.expect_err("save failure must surface as Err, not be swallowed");
        assert!(err.contains("posted, but saving dedup state failed"));
    }
}
