use chrono::{DateTime, TimeZone, Utc};
use git2::Repository;
use std::path::{Path, PathBuf};

use crate::collect::CollectResult;
use crate::config::GitConfig;
use crate::model::{ActivityItem, Source};

/// Read-only scan of configured git roots. Uses git2 read APIs only —
/// no checkout, no commit, no fetch. Failures are collected as warnings.
pub fn collect(cfg: &GitConfig, since: DateTime<Utc>, now: DateTime<Utc>) -> CollectResult {
    let mut result = CollectResult::default();
    for root in &cfg.roots {
        let expanded = expand_tilde(root);
        match scan_root(&expanded, since, now) {
            Ok((items, warnings)) => {
                result.items.extend(items);
                result.warnings.extend(warnings);
            }
            Err(e) => result
                .warnings
                .push(format!("git: failed scanning {}: {e}", expanded.display())),
        }
    }
    result
}

fn expand_tilde(p: &str) -> PathBuf {
    // Bare "~" (no trailing slash) previously fell through to being treated
    // as a literal path named "~", which never exists — the scan silently
    // found nothing and no warning surfaced it. Handle it the same as "~/".
    if p == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(p)
}

fn scan_root(
    root: &Path,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> anyhow::Result<(Vec<ActivityItem>, Vec<String>)> {
    // A "root" may itself be a repo, or a directory of repos (one level deep).
    let mut items = Vec::new();
    let mut warnings = Vec::new();
    if root.join(".git").exists() {
        items.extend(scan_repo(root, since, now)?);
        return Ok((items, warnings));
    }
    let entries = std::fs::read_dir(root)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join(".git").exists() {
            match scan_repo(&path, since, now) {
                Ok(repo_items) => items.extend(repo_items),
                Err(e) => warnings.push(format!("git: failed scanning {}: {e}", path.display())),
            }
        }
    }
    Ok((items, warnings))
}

fn scan_repo(
    path: &Path,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> anyhow::Result<Vec<ActivityItem>> {
    let repo = Repository::open(path)?;
    let repo_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".into());
    let mut items = Vec::new();

    // Commits on HEAD within the window.
    if let Ok(mut revwalk) = repo.revwalk() {
        revwalk.set_sorting(git2::Sort::TIME).ok();
        revwalk.push_head().ok();
        for oid in revwalk.flatten() {
            if let Ok(commit) = repo.find_commit(oid) {
                let ts = Utc
                    .timestamp_opt(commit.time().seconds(), 0)
                    .single()
                    .unwrap_or(since);
                if ts < since {
                    break; // revwalk is newest-first
                }
                items.push(ActivityItem {
                    source: Source::Git,
                    source_id: oid.to_string(),
                    title: commit.summary().unwrap_or("(no message)").to_string(),
                    url: None,
                    project_key: None,
                    repo: Some(repo_name.clone()),
                    activity_type: "commit".into(),
                    status: None,
                    actor: commit.author().name().map(|s| s.to_string()),
                    timestamp: ts,
                    summary: None,
                    signals: vec![],
                });
            }
        }
    }

    // Uncommitted changes (dirty working tree) -> one WIP signal item.
    let statuses = repo.statuses(None)?;
    if !statuses.is_empty() {
        items.push(ActivityItem {
            source: Source::Git,
            source_id: format!("{repo_name}:wip"),
            title: format!("{} uncommitted change(s)", statuses.len()),
            url: None,
            project_key: None,
            repo: Some(repo_name.clone()),
            activity_type: "wip".into(),
            status: Some("uncommitted".into()),
            actor: None,
            timestamp: now,
            summary: None,
            signals: vec!["uncommitted-changes".into()],
        });
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn expand_tilde_handles_bare_tilde() {
        std::env::set_var("HOME", "/tmp/fakehome");
        assert_eq!(expand_tilde("~"), PathBuf::from("/tmp/fakehome"));
        assert_eq!(
            expand_tilde("~/code"),
            PathBuf::from("/tmp/fakehome/code")
        );
    }

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args(args)
            .status()
            .expect("run git")
            .success();
        assert!(ok, "git {:?} failed", args);
    }

    fn init_repo_with_commit(dir: &Path) {
        git(dir, &["init", "-q"]);
        git(dir, &["config", "user.email", "t@example.com"]);
        git(dir, &["config", "user.name", "Test"]);
        std::fs::write(dir.join("a.txt"), "hi").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-q", "-m", "ENG-42 initial commit"]);
    }

    #[test]
    fn finds_recent_commit() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_commit(tmp.path());
        let cfg = GitConfig {
            roots: vec![tmp.path().to_string_lossy().into()],
            exclude: vec![],
        };
        let since = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        let res = collect(&cfg, since, Utc::now());
        assert!(res.items.iter().any(|i| i.title.contains("initial commit")));
        assert!(res.items.iter().all(|i| i.source == Source::Git));
    }

    #[test]
    fn detects_uncommitted_changes() {
        let tmp = TempDir::new().unwrap();
        init_repo_with_commit(tmp.path());
        std::fs::write(tmp.path().join("b.txt"), "dirty").unwrap();
        let cfg = GitConfig {
            roots: vec![tmp.path().to_string_lossy().into()],
            exclude: vec![],
        };
        let res = collect(
            &cfg,
            Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
            Utc::now(),
        );
        assert!(res.items.iter().any(|i| i.activity_type == "wip"));
    }

    #[test]
    fn missing_root_becomes_warning_not_error() {
        let cfg = GitConfig {
            roots: vec!["/no/such/path".into()],
            exclude: vec![],
        };
        let res = collect(&cfg, Utc::now(), Utc::now());
        // Non-existent dir yields no items and no panic; the failure is
        // surfaced as a warning rather than swallowed or propagated as Err.
        assert!(res.items.is_empty());
        assert!(!res.warnings.is_empty());
        assert!(res.warnings.iter().any(|w| w.contains("/no/such/path")));
    }

    #[test]
    fn finds_commits_from_directory_of_repos() {
        let tmp = TempDir::new().unwrap();
        let repo_a = tmp.path().join("repo-a");
        let repo_b = tmp.path().join("repo-b");
        std::fs::create_dir(&repo_a).unwrap();
        std::fs::create_dir(&repo_b).unwrap();
        std::fs::write(repo_a.join("a.txt"), "a").unwrap();
        std::fs::write(repo_b.join("b.txt"), "b").unwrap();
        git(&repo_a, &["init", "-q"]);
        git(&repo_a, &["config", "user.email", "t@example.com"]);
        git(&repo_a, &["config", "user.name", "Test"]);
        git(&repo_a, &["add", "."]);
        git(&repo_a, &["commit", "-q", "-m", "repo-a commit"]);
        git(&repo_b, &["init", "-q"]);
        git(&repo_b, &["config", "user.email", "t@example.com"]);
        git(&repo_b, &["config", "user.name", "Test"]);
        git(&repo_b, &["add", "."]);
        git(&repo_b, &["commit", "-q", "-m", "repo-b commit"]);

        let cfg = GitConfig {
            roots: vec![tmp.path().to_string_lossy().into()],
            exclude: vec![],
        };
        let since = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
        let res = collect(&cfg, since, Utc::now());
        assert!(res.items.iter().any(|i| i.title.contains("repo-a commit")));
        assert!(res.items.iter().any(|i| i.title.contains("repo-b commit")));
    }
}
