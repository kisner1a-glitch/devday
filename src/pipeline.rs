use crate::config::Config;
use crate::model::Report;

#[derive(Debug, Clone, Default)]
pub struct ReportOptions {
    pub since: Option<String>,
    pub github: bool,
    pub linear: bool,
    pub git: bool,
    pub ai_provider: Option<String>,
    pub no_ai: bool,
}

pub async fn build_report(cfg: &Config, opts: &ReportOptions) -> anyhow::Result<Report> {
    let now = chrono::Utc::now();
    let since_str = opts
        .since
        .clone()
        .or_else(|| cfg.default_since.clone())
        .unwrap_or_else(|| "24h".to_string());
    let dur = crate::cli::parse_since(&since_str)?;
    let since = now - chrono::Duration::from_std(dur)?;

    let mut collected = crate::collect::CollectResult::default();
    if opts.git || cfg.sources.git {
        collected.merge(crate::collect::git::collect(&cfg.git, since, now));
    }
    if opts.github || cfg.sources.github {
        collected.merge(crate::collect::github::collect(&cfg.github, since, now));
    }
    if opts.linear || cfg.sources.linear {
        match cfg
            .linear
            .token_env
            .as_deref()
            .and_then(|e| std::env::var(e).ok())
        {
            Some(token) => {
                let base = crate::collect::linear::api_base();
                collected.merge(
                    crate::collect::linear::collect(&cfg.linear, &token, since, now, &base).await,
                );
            }
            None => collected
                .warnings
                .push("linear: no token (set linear.token_env)".into()),
        }
    }

    let mut rep = crate::report::build(collected.items, since, now, collected.warnings);

    let use_ai = !opts.no_ai && (opts.ai_provider.is_some() || cfg.ai.provider.is_some());
    if use_ai {
        let mut ai_cfg = cfg.ai.clone();
        if let Some(p) = &opts.ai_provider {
            ai_cfg.provider = Some(p.clone());
        }
        if let Some(summary) = crate::ai::summarize(&ai_cfg, &rep) {
            rep.summary = Some(summary);
        } else {
            rep.generation_warnings
                .push("ai: summarization failed; using deterministic report".into());
        }
    }

    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silent_config() -> Config {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.sources.linear = false;
        cfg.sources.git = false;
        cfg
    }

    #[tokio::test]
    async fn builds_empty_report_with_all_sources_disabled() {
        let rep = build_report(&silent_config(), &ReportOptions::default())
            .await
            .unwrap();
        assert!(rep.worked_on.is_empty());
        assert!(rep.window_end > rep.window_start);
    }

    #[tokio::test]
    async fn since_flag_overrides_config_default() {
        let mut cfg = silent_config();
        cfg.default_since = Some("48h".into());
        let opts = ReportOptions {
            since: Some("2h".into()),
            ..Default::default()
        };
        let rep = build_report(&cfg, &opts).await.unwrap();
        let window = rep.window_end - rep.window_start;
        assert_eq!(window.num_hours(), 2);
    }

    #[tokio::test]
    async fn bad_since_is_error() {
        let opts = ReportOptions {
            since: Some("banana".into()),
            ..Default::default()
        };
        assert!(build_report(&silent_config(), &opts).await.is_err());
    }
}
