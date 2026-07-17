use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub sources: SourcesConfig,
    pub github: GithubConfig,
    pub linear: LinearConfig,
    pub git: GitConfig,
    pub ai: AiConfig,
    pub slack: SlackConfig,
    pub state: StateConfig,
    pub redact: RedactConfig,
    pub output: Option<String>,
    pub default_since: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct SourcesConfig {
    pub github: bool,
    pub linear: bool,
    pub git: bool,
}
impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            github: true,
            linear: true,
            git: true,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct GithubConfig {
    pub account: Option<String>,
    pub orgs: Vec<String>,
    pub repos: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct LinearConfig {
    /// Env var name holding the token, e.g. "LINEAR_API_KEY". Never the token itself.
    pub token_env: Option<String>,
    pub teams: Vec<String>,
    pub projects: Vec<String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct GitConfig {
    pub roots: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct AiConfig {
    pub provider: Option<String>, // "claude" | "codex" | none
    pub command: Option<String>,  // override binary path
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct SlackConfig {
    pub channel: Option<String>,
    pub webhook_env: Option<String>,   // env var holding webhook URL
    pub bot_token_env: Option<String>, // env var holding bot token
    pub auto_post: bool,               // false => preview required
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct StateConfig {
    pub path: Option<String>,
    pub dedupe_window: String,
}
impl Default for StateConfig {
    fn default() -> Self {
        Self {
            path: None,
            dedupe_window: "72h".into(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct RedactConfig {
    pub hide_local_paths: bool,
    pub hide_private_repos: bool,
    pub extra_patterns: Vec<String>,
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Config> {
        let resolved = match path {
            Some(p) => Some(p.to_path_buf()),
            None => Self::default_config_path().filter(|p| p.exists()),
        };
        match resolved {
            Some(p) => {
                let text = std::fs::read_to_string(&p)
                    .map_err(|e| anyhow::anyhow!("reading config {}: {e}", p.display()))?;
                let cfg: Config = toml::from_str(&text)
                    .map_err(|e| anyhow::anyhow!("parsing config {}: {e}", p.display()))?;
                Ok(cfg)
            }
            None => Ok(Config::default()),
        }
    }

    fn default_config_path() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join(".config")
                .join("devday")
                .join("config.toml")
        })
    }

    pub fn state_path(&self) -> PathBuf {
        if let Some(p) = &self.state.path {
            return PathBuf::from(p);
        }
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(".local")
            .join("state")
            .join("devday")
            .join("state.json")
    }

    pub fn init_template() -> String {
        r##"# devday configuration
output = "report.md"
default_since = "24h"

[sources]
github = true
linear = true
git = true

[github]
# account = "your-login"
orgs = []
repos = []

[linear]
# token_env names the environment variable holding your Linear API token.
token_env = "LINEAR_API_KEY"
teams = []
projects = []
labels = []

[git]
roots = ["~/code"]
exclude = []

[ai]
# provider = "claude"   # or "codex"; omit to disable AI summarization
args = []

[slack]
# channel = "#eng-status"
# webhook_env = "DEVDAY_SLACK_WEBHOOK"
# bot_token_env = "DEVDAY_SLACK_BOT_TOKEN"
auto_post = false

[state]
# dedupe_window = "72h"  # reserved: not yet enforced

[redact]
hide_local_paths = false
hide_private_repos = false
extra_patterns = []
"##
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_enables_all_sources() {
        let c = Config::default();
        assert!(c.sources.github && c.sources.linear && c.sources.git);
    }

    #[test]
    fn parses_partial_toml_with_defaults() {
        let toml = r#"
[sources]
github = false
"#;
        let c: Config = toml::from_str(toml).unwrap();
        assert!(!c.sources.github);
        assert!(c.sources.linear); // default preserved
    }

    #[test]
    fn init_template_is_valid_toml() {
        let t = Config::init_template();
        let _c: Config = toml::from_str(&t).unwrap();
    }

    #[test]
    fn load_missing_path_returns_default() {
        let c = Config::load(Some(Path::new("/nonexistent/xyz.toml")));
        assert!(c.is_err()); // explicit path that doesn't exist is an error
        let d = Config::load(None).unwrap(); // no HOME config -> default (in most CI)
        let _ = d;
    }
}
