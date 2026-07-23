use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::collect::CollectResult;
use crate::config::LinearConfig;
use crate::model::{ActivityItem, Source};

const DEFAULT_API: &str = "https://api.linear.app/graphql";

const QUERY: &str = r#"
query Recent($since: DateTimeOrDuration) {
  viewer {
    assignedIssues(filter: { updatedAt: { gt: $since } }, first: 50) {
      nodes {
        identifier title url updatedAt
        state { name type }
        labels { nodes { name } }
        priority
        project { name }
      }
    }
  }
}"#;

#[derive(Deserialize)]
struct GqlResponse {
    data: Option<GqlData>,
}
#[derive(Deserialize)]
struct GqlData {
    viewer: Viewer,
}
#[derive(Deserialize)]
struct Viewer {
    #[serde(rename = "assignedIssues")]
    assigned_issues: Connection,
}
#[derive(Deserialize)]
struct Connection {
    nodes: Vec<Issue>,
}
#[derive(Deserialize)]
struct Issue {
    identifier: String,
    title: String,
    url: String,
    #[serde(rename = "updatedAt")]
    updated_at: DateTime<Utc>,
    state: State,
    labels: LabelConn,
    priority: Option<i64>,
    #[serde(default)]
    project: Option<Project>,
}
#[derive(Deserialize)]
struct Project {
    name: String,
}
#[derive(Deserialize)]
struct State {
    name: String,
    #[serde(rename = "type")]
    type_: String,
}
#[derive(Deserialize)]
struct LabelConn {
    nodes: Vec<Label>,
}
#[derive(Deserialize)]
struct Label {
    name: String,
}

pub fn api_base() -> String {
    DEFAULT_API.to_string()
}

/// Read-only Linear GraphQL collector. Issues a single `query` (never a
/// mutation) for the viewer's assigned issues updated since `since`. Never
/// returns Err: HTTP/parse failures are folded into `CollectResult::warnings`.
pub async fn collect(
    _cfg: &LinearConfig,
    token: &str,
    since: DateTime<Utc>,
    _now: DateTime<Utc>,
    api_base: &str,
) -> CollectResult {
    let mut result = CollectResult::default();
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "query": QUERY,
        "variables": { "since": since.to_rfc3339() }
    });
    let resp = client
        .post(api_base)
        .header("Authorization", token)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    let parsed: Result<GqlResponse, String> = match resp {
        Ok(r) if r.status().is_success() => {
            r.json::<GqlResponse>().await.map_err(|e| e.to_string())
        }
        Ok(r) => Err(format!("HTTP {}", r.status())),
        Err(e) => Err(e.to_string()),
    };

    match parsed {
        Ok(GqlResponse { data: Some(d) }) => {
            for issue in d.viewer.assigned_issues.nodes {
                result.items.push(issue_to_item(issue));
            }
        }
        Ok(_) => result.warnings.push("linear: empty response".into()),
        Err(e) => result.warnings.push(format!("linear: {e}")),
    }
    result
}

fn issue_to_item(issue: Issue) -> ActivityItem {
    let mut signals = Vec::new();
    let state_lower = issue.state.name.to_lowercase();
    if issue.state.type_ == "blocked" || state_lower.contains("blocked") {
        signals.push("blocked".to_string());
    }
    if issue
        .labels
        .nodes
        .iter()
        .any(|l| l.name.to_lowercase().contains("blocked"))
    {
        signals.push("blocked".to_string());
    }
    if issue.state.type_ == "started" || issue.state.type_ == "unstarted" {
        signals.push("assigned-active".to_string());
    }
    if issue.priority.map(|p| p == 1).unwrap_or(false) {
        signals.push("high-priority".to_string());
    }
    ActivityItem {
        source: Source::Linear,
        source_id: issue.identifier.clone(),
        title: issue.title,
        url: Some(issue.url),
        project_key: Some(issue.identifier),
        project_name: issue.project.map(|p| p.name),
        repo: None,
        activity_type: "issue".into(),
        status: Some(issue.state.name),
        actor: None,
        timestamp: issue.updated_at,
        summary: None,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn maps_blocked_issue_to_blocker_signal() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "data": { "viewer": { "assignedIssues": { "nodes": [{
                "identifier": "ENG-42",
                "title": "Do the thing",
                "url": "https://linear.app/x/ENG-42",
                "updatedAt": "2026-07-16T10:00:00Z",
                "state": { "name": "Blocked", "type": "blocked" },
                "labels": { "nodes": [] },
                "priority": 1
            }]}}}
        });
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let res = collect(
            &LinearConfig::default(),
            "token",
            Utc::now(),
            Utc::now(),
            &server.uri(),
        )
        .await;

        assert_eq!(res.items.len(), 1);
        let it = &res.items[0];
        assert_eq!(it.project_key.as_deref(), Some("ENG-42"));
        assert!(it.signals.contains(&"blocked".to_string()));
        assert!(it.signals.contains(&"high-priority".to_string()));
    }

    #[tokio::test]
    async fn maps_project_name_for_grouping() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "data": { "viewer": { "assignedIssues": { "nodes": [{
                "identifier": "ENG-1",
                "title": "Do the other thing",
                "url": "https://linear.app/x/ENG-1",
                "updatedAt": "2026-07-16T10:00:00Z",
                "state": { "name": "Todo", "type": "unstarted" },
                "labels": { "nodes": [] },
                "priority": null,
                "project": { "name": "Engineering" }
            }]}}}
        });
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let res = collect(
            &LinearConfig::default(),
            "token",
            Utc::now(),
            Utc::now(),
            &server.uri(),
        )
        .await;

        assert_eq!(res.items[0].project_name.as_deref(), Some("Engineering"));
    }

    #[tokio::test]
    async fn missing_project_field_defaults_to_none() {
        // Older/minimal fixtures that omit `project` entirely should still parse.
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "data": { "viewer": { "assignedIssues": { "nodes": [{
                "identifier": "ENG-2",
                "title": "No project assigned",
                "url": "https://linear.app/x/ENG-2",
                "updatedAt": "2026-07-16T10:00:00Z",
                "state": { "name": "Todo", "type": "unstarted" },
                "labels": { "nodes": [] },
                "priority": null
            }]}}}
        });
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let res = collect(
            &LinearConfig::default(),
            "token",
            Utc::now(),
            Utc::now(),
            &server.uri(),
        )
        .await;

        assert_eq!(res.items[0].project_name, None);
    }

    #[tokio::test]
    async fn http_error_becomes_warning() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let res = collect(
            &LinearConfig::default(),
            "bad",
            Utc::now(),
            Utc::now(),
            &server.uri(),
        )
        .await;
        assert!(res.items.is_empty());
        assert!(res.warnings.iter().any(|w| w.contains("401")));
    }
}
