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
}
#[derive(Deserialize)]
struct GhRepo {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

/// Extra per-PR fields not available from `gh search prs` (that endpoint
/// rejects `reviewDecision`/`statusCheckRollup` with "Unknown JSON field" as
/// of gh 2.96.0) but available from `gh pr view <url>`.
#[derive(Deserialize, Default)]
pub(crate) struct GhPrDetail {
    #[serde(rename = "reviewDecision", default)]
    review_decision: Option<String>,
    #[serde(rename = "statusCheckRollup", default)]
    checks: Vec<GhCheck>,
}
#[derive(Deserialize)]
struct GhCheck {
    #[serde(default)]
    conclusion: Option<String>,
}

/// Pure parser over `gh search prs --json ...` output. Unit-tested with fixtures.
pub fn parse_search_prs(json: &str) -> Result<Vec<ActivityItem>, String> {
    let prs: Vec<GhPr> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(prs.into_iter().map(|pr| pr_to_item(pr, None)).collect())
}

/// Pure parser over `gh pr view --json reviewDecision,statusCheckRollup` output.
pub(crate) fn parse_pr_detail(json: &str) -> Result<GhPrDetail, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

fn detail_to_signals(detail: &GhPrDetail) -> Vec<String> {
    let mut signals = Vec::new();
    if detail
        .checks
        .iter()
        .any(|c| c.conclusion.as_deref() == Some("FAILURE"))
    {
        signals.push("failing-check".to_string());
    }
    match detail.review_decision.as_deref() {
        Some("CHANGES_REQUESTED") => signals.push("requested-changes".to_string()),
        Some("REVIEW_REQUIRED") => signals.push("waiting-review".to_string()),
        _ => {}
    }
    signals
}

fn pr_to_item(pr: GhPr, detail: Option<&GhPrDetail>) -> ActivityItem {
    let signals = detail.map(detail_to_signals).unwrap_or_default();
    let repo = pr.repository.name_with_owner;
    ActivityItem {
        source: Source::Github,
        source_id: pr.url.clone(),
        title: pr.title,
        url: Some(pr.url),
        project_key: None,
        project_name: None,
        repo: Some(repo),
        activity_type: "pr".into(),
        status: Some(pr.state),
        actor: None,
        timestamp: pr.updated_at,
        summary: None,
        signals,
    }
}

/// Fetch review-decision/check-status for one PR. Best-effort: any failure
/// here is recorded as a warning and the PR still appears in the report,
/// just without blocker-signal classification.
fn fetch_detail(url: &str, warnings: &mut Vec<String>) -> Option<GhPrDetail> {
    let output = Command::new("gh")
        .args([
            "pr",
            "view",
            url,
            "--json",
            "reviewDecision,statusCheckRollup",
        ])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            match parse_pr_detail(&text) {
                Ok(detail) => Some(detail),
                Err(e) => {
                    warnings.push(format!("github: pr detail parse error for {url}: {e}"));
                    None
                }
            }
        }
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            warnings.push(format!(
                "github: gh pr view failed for {url}: {}",
                err.trim()
            ));
            None
        }
        Err(e) => {
            warnings.push(format!("github: cannot run gh pr view for {url}: {e}"));
            None
        }
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
            "title,url,updatedAt,repository,state",
            "--limit",
            "50",
        ])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            match serde_json::from_str::<Vec<GhPr>>(&text) {
                Ok(prs) => {
                    result.items = prs
                        .into_iter()
                        .map(|pr| {
                            let detail = fetch_detail(&pr.url, &mut result.warnings);
                            pr_to_item(pr, detail.as_ref())
                        })
                        .collect();
                }
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
        "state": "OPEN"
      }
    ]"#;

    const DETAIL_FIXTURE: &str = r#"{
        "reviewDecision": "CHANGES_REQUESTED",
        "statusCheckRollup": [ { "conclusion": "FAILURE" } ]
    }"#;

    #[test]
    fn search_prs_parses_without_review_or_check_fields() {
        let items = parse_search_prs(FIXTURE).unwrap();
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.repo.as_deref(), Some("o/r"));
        assert!(it.signals.is_empty());
    }

    #[test]
    fn pr_detail_parses_blocker_signals() {
        let detail = parse_pr_detail(DETAIL_FIXTURE).unwrap();
        let signals = detail_to_signals(&detail);
        assert!(signals.contains(&"failing-check".to_string()));
        assert!(signals.contains(&"requested-changes".to_string()));
    }

    #[test]
    fn pr_to_item_applies_detail_signals_when_present() {
        let prs: Vec<GhPr> = serde_json::from_str(FIXTURE).unwrap();
        let detail = parse_pr_detail(DETAIL_FIXTURE).unwrap();
        let item = pr_to_item(prs.into_iter().next().unwrap(), Some(&detail));
        assert!(item.signals.contains(&"failing-check".to_string()));
        assert!(item.signals.contains(&"requested-changes".to_string()));
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
