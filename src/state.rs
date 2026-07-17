use std::path::Path;

use sha2::{Digest, Sha256};

pub use crate::model::LocalState;

pub fn load(path: &Path) -> LocalState {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => LocalState::default(),
    }
}

pub fn save(path: &Path, state: &LocalState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(state)?;
    std::fs::write(path, text)?;
    Ok(())
}

pub fn report_hash(markdown: &str) -> String {
    let mut h = Sha256::new();
    h.update(markdown.as_bytes());
    format!("{:x}", h.finalize())
}

impl LocalState {
    pub fn already_posted(&self, hash: &str) -> bool {
        self.posted_report_hashes.iter().any(|h| h == hash)
    }
    pub fn mark_posted(
        &mut self,
        hash: String,
        item_ids: Vec<String>,
        when: chrono::DateTime<chrono::Utc>,
    ) {
        if !self.already_posted(&hash) {
            self.posted_report_hashes.push(hash);
        }
        for id in item_ids {
            if !self.posted_item_ids.contains(&id) {
                self.posted_item_ids.push(id);
            }
        }
        self.last_successful_run_at = Some(when);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrips_via_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut s = LocalState::default();
        s.mark_posted(report_hash("hello"), vec!["id1".into()], chrono::Utc::now());
        save(&path, &s).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.posted_item_ids, vec!["id1".to_string()]);
    }

    #[test]
    fn detects_already_posted_hash() {
        let mut s = LocalState::default();
        let h = report_hash("same content");
        assert!(!s.already_posted(&h));
        s.mark_posted(h.clone(), vec![], chrono::Utc::now());
        assert!(s.already_posted(&h));
    }

    #[test]
    fn missing_file_loads_default() {
        assert_eq!(
            load(Path::new("/no/such/state.json")),
            LocalState::default()
        );
    }
}
