use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Github,
    Linear,
    Git,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityItem {
    pub source: Source,
    pub source_id: String,
    pub title: String,
    pub url: Option<String>,
    pub project_key: Option<String>,
    /// Human-readable project/board name (e.g. a Linear project like
    /// "PromptPantry"), distinct from `project_key` which is the specific
    /// issue/ticket key used to correlate activity across sources.
    pub project_name: Option<String>,
    pub repo: Option<String>,
    pub activity_type: String,
    pub status: Option<String>,
    pub actor: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub summary: Option<String>,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockerKind {
    Explicit,
    Inferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockerSignal {
    pub kind: BlockerKind,
    pub reason: String,
    pub source_item_url: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Group {
    /// Linear issue key/project/team, or repo name for unlinked activity.
    pub key: String,
    pub title: String,
    pub items: Vec<ActivityItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub groups: Vec<Group>,
    pub summary: Option<String>,
    pub worked_on: Vec<String>,
    pub next_up: Vec<String>,
    pub blockers: Vec<BlockerSignal>,
    pub source_links: Vec<String>,
    pub generation_warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LocalState {
    pub last_successful_run_at: Option<DateTime<Utc>>,
    pub posted_item_ids: Vec<String>,
    pub posted_report_hashes: Vec<String>,
    pub source_cursor_by_integration: std::collections::BTreeMap<String, String>,
    pub dedupe_window: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn activity_item_roundtrips_through_json() {
        let item = ActivityItem {
            source: Source::Git,
            source_id: "abc123".into(),
            title: "Fix parser".into(),
            url: None,
            project_key: Some("ENG-1".into()),
            project_name: Some("Engineering".into()),
            repo: Some("devday".into()),
            activity_type: "commit".into(),
            status: None,
            actor: Some("eric".into()),
            timestamp: Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 0).unwrap(),
            summary: None,
            signals: vec![],
        };
        let json = serde_json::to_string(&item).unwrap();
        let back: ActivityItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, back);
    }

    #[test]
    fn source_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&Source::Github).unwrap(),
            "\"github\""
        );
    }
}
