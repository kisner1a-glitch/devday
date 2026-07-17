use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::process::Command;

use crate::collect::CollectResult;
use crate::config::GithubConfig;
use crate::model::{ActivityItem, Source};

#[derive(Deserialize)]
struct GhPr {
    title: String,
    url: String,
    #[serde(rename = "updatedAt")]
    updated_at: DateTime<Utc>,
    repository: GhRepo,
    state: String,
    #[serde(rename = "reviewDecision", default)]
    review_decision: Option<String>,
    #[serde(rename = "statusCheckRollup", default)]
    checks: Vec<GhCheck>,
}
#[derive(Deserialize)]
struct GhRepo {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}
#[derive(Deserialize)]
struct GhCheck {
    #[serde(default)]
    conclusion: Option<String>,
}

/// Pure parser over `gh search prs --json ...` output. Unit-tested with fixtures.
pub fn parse_search_prs(json: &str) -> Result<Vec<ActivityItem>, String> {
    let prs: Vec<GhPr> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(prs.into_iter().map(pr_to_item).collect())
}

fn pr_to_item(pr: GhPr) -> ActivityItem {
    let mut signals = Vec::new();
    if pr
        .checks
        .iter()
        .any(|c| c.conclusion.as_deref() == Some("FAILURE"))
    {
        signals.push("failing-check".to_string());
    }
    match pr.review_decision.as_deref() {
        Some("CHANGES_REQUESTED") => signals.push("requested-changes".to_string()),
        Some("REVIEW_REQUIRED") => signals.push("waiting-review".to_string()),
        _ => {}
    }
    let repo = pr.repository.name_with_owner;
    ActivityItem {
        source: Source::Github,
        source_id: pr.url.clone(),
        title: pr.title,
        url: Some(pr.url),
        project_key: None,
        repo: Some(repo),
        activity_type: "pr".into(),
        status: Some(pr.state),
        actor: None,
        timestamp: pr.updated_at,
        summary: None,
        signals,
    }
}

pub fn collect(_cfg: &GithubConfig, since: DateTime<Utc>, _now: DateTime<Utc>) -> CollectResult {
    let mut result = CollectResult::default();
    let since_date = since.format("%Y-%m-%d").to_string();
    let output = Command::new("gh")
        .args([
            "search",
            "prs",
            "--author",
            "@me",
            "--updated",
            &format!(">={since_date}"),
            "--json",
            "title,url,updatedAt,repository,state,reviewDecision,statusCheckRollup",
            "--limit",
            "50",
        ])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            match parse_search_prs(&text) {
                Ok(items) => result.items = items,
                Err(e) => result.warnings.push(format!("github: parse error: {e}")),
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            result
                .warnings
                .push(format!("github: gh failed: {}", err.trim()));
        }
        Err(e) => result.warnings.push(format!("github: cannot run gh: {e}")),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
      {
        "title": "Add parser",
        "url": "https://github.com/o/r/pull/1",
        "updatedAt": "2026-07-16T09:00:00Z",
        "repository": { "nameWithOwner": "o/r" },
        "state": "OPEN",
        "reviewDecision": "CHANGES_REQUESTED",
        "statusCheckRollup": [ { "conclusion": "FAILURE" } ]
      }
    ]"#;

    #[test]
    fn parses_pr_with_blocker_signals() {
        let items = parse_search_prs(FIXTURE).unwrap();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.repo.as_deref(), Some("o/r"));
        assert!(it.signals.contains(&"failing-check".to_string()));
        assert!(it.signals.contains(&"requested-changes".to_string()));
    }

    #[test]
    fn empty_array_is_ok() {
        assert_eq!(parse_search_prs("[]").unwrap().len(), 0);
    }

    #[test]
    fn bad_json_is_error() {
        assert!(parse_search_prs("not json").is_err());
    }
}
