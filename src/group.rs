use crate::model::{ActivityItem, Group};
use std::collections::BTreeMap;

/// Extract a Linear-style issue key (TEAM-123) from arbitrary text
/// (branch name, PR title/body, commit message).
pub fn extract_issue_key(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find start of an uppercase run.
        if bytes[i].is_ascii_uppercase() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_uppercase() {
                i += 1;
            }
            let alpha_len = i - start;
            // Require TEAM-<digits> with 2..=6 uppercase letters.
            if (2..=6).contains(&alpha_len) && i < bytes.len() && bytes[i] == b'-' {
                let dash = i;
                i += 1;
                let dig_start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i > dig_start {
                    return Some(text[start..i].to_string());
                }
                i = dash + 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Group items primarily by Linear issue key, falling back to repo name.
pub fn group_items(items: Vec<ActivityItem>) -> Vec<Group> {
    let mut buckets: BTreeMap<String, Group> = BTreeMap::new();
    for item in items {
        let key = item
            .project_key
            .clone()
            .or_else(|| extract_issue_key(&item.title))
            .or_else(|| item.repo.clone())
            .unwrap_or_else(|| "ungrouped".to_string());
        let entry = buckets.entry(key.clone()).or_insert_with(|| Group {
            key: key.clone(),
            title: key.clone(),
            items: vec![],
        });
        entry.items.push(item);
    }
    buckets.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    use chrono::Utc;

    fn item(title: &str, project_key: Option<&str>, repo: Option<&str>) -> ActivityItem {
        ActivityItem {
            source: Source::Git,
            source_id: title.into(),
            title: title.into(),
            url: None,
            project_key: project_key.map(|s| s.into()),
            project_name: None,
            repo: repo.map(|s| s.into()),
            activity_type: "commit".into(),
            status: None,
            actor: None,
            timestamp: Utc::now(),
            summary: None,
            signals: vec![],
        }
    }

    #[test]
    fn extracts_key_from_commit_message() {
        assert_eq!(extract_issue_key("ENG-42 fix bug"), Some("ENG-42".into()));
        assert_eq!(
            extract_issue_key("feature/ABC-7-thing"),
            Some("ABC-7".into())
        );
    }

    #[test]
    fn no_key_when_absent() {
        assert_eq!(extract_issue_key("just a message"), None);
        assert_eq!(extract_issue_key("HTTP2 stuff"), None); // no dash-digit
    }

    #[test]
    fn groups_by_explicit_key_then_extracted_then_repo() {
        let items = vec![
            item("commit one", Some("ENG-1"), Some("repoA")),
            item("ENG-1 more work", None, Some("repoA")),
            item("unrelated", None, Some("repoB")),
        ];
        let groups = group_items(items);
        // ENG-1 (2 items) and repoB (1 item), sorted by key.
        assert_eq!(groups.len(), 2);
        let eng = groups.iter().find(|g| g.key == "ENG-1").unwrap();
        assert_eq!(eng.items.len(), 2);
        assert!(groups.iter().any(|g| g.key == "repoB"));
    }
}
