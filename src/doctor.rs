use crate::config::Config;

#[derive(Debug, PartialEq)]
pub struct Check {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

fn binary_present(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Build the checklist without printing (testable).
pub fn build_checks(cfg: &Config, env_get: &dyn Fn(&str) -> Option<String>) -> Vec<Check> {
    let mut checks = Vec::new();

    if cfg.sources.github {
        checks.push(Check {
            name: "github: gh CLI".into(),
            ok: binary_present("gh"),
            detail: "requires authenticated `gh`".into(),
        });
    }

    if cfg.sources.linear {
        let has_token = cfg.linear.token_env.as_deref().and_then(env_get).is_some();
        checks.push(Check {
            name: "linear: token".into(),
            ok: has_token,
            detail: match &cfg.linear.token_env {
                Some(e) => format!("expects env {e}"),
                None => "no token_env configured".into(),
            },
        });
    }

    if let Some(provider) = &cfg.ai.provider {
        let cmd = cfg.ai.command.clone().unwrap_or_else(|| provider.clone());
        checks.push(Check {
            name: format!("ai: {provider}"),
            ok: binary_present(&cmd),
            detail: format!("provider binary `{cmd}`"),
        });
    }

    // Slack sanity: if auto_post, must have a credential env configured.
    if cfg.slack.auto_post {
        let has_cred = cfg.slack.webhook_env.is_some() || cfg.slack.bot_token_env.is_some();
        checks.push(Check {
            name: "slack: auto-post credentials".into(),
            ok: has_cred,
            detail: "auto_post requires webhook_env or bot_token_env".into(),
        });
    }

    checks
}

pub fn run(cfg: &Config) {
    let getter = |k: &str| std::env::var(k).ok();
    let checks = build_checks(cfg, &getter);
    for c in &checks {
        let mark = if c.ok { "OK " } else { "FAIL" };
        println!("[{mark}] {} — {}", c.name, c.detail);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_check_fails_without_token_env_value() {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.linear.token_env = Some("SOME_ENV_THAT_IS_UNSET".into());
        let checks = build_checks(&cfg, &|_| None);
        let linear = checks.iter().find(|c| c.name.contains("linear")).unwrap();
        assert!(!linear.ok);
    }

    #[test]
    fn linear_check_passes_when_env_present() {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.linear.token_env = Some("MY_TOKEN".into());
        let checks = build_checks(&cfg, &|k| (k == "MY_TOKEN").then(|| "value".to_string()));
        let linear = checks.iter().find(|c| c.name.contains("linear")).unwrap();
        assert!(linear.ok);
    }

    #[test]
    fn autopost_without_credentials_fails() {
        let mut cfg = Config::default();
        cfg.sources.github = false;
        cfg.sources.linear = false;
        cfg.slack.auto_post = true;
        let checks = build_checks(&cfg, &|_| None);
        assert!(checks.iter().any(|c| c.name.contains("auto-post") && !c.ok));
    }
}
