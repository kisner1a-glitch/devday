use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn report_on_empty_config_produces_report_sections() {
    // No git roots configured by default in a temp HOME => empty-state report.
    Command::cargo_bin("devday")
        .unwrap()
        .env("HOME", tempfile::tempdir().unwrap().path())
        .args(["report", "--stdout"])
        .assert()
        .success()
        .stdout(contains("## Summary"))
        .stdout(contains("## Blockers"));
}

#[test]
fn send_slack_post_without_auto_post_falls_back_to_preview() {
    // No config in a temp HOME => cfg.slack.auto_post defaults to false, so
    // `--post` alone must NOT trigger a real Slack post (which would fail /
    // hang without network access here) — it must print the preview digest
    // and exit successfully instead.
    Command::cargo_bin("devday")
        .unwrap()
        .env("HOME", tempfile::tempdir().unwrap().path())
        .args(["send", "slack", "--post"])
        .assert()
        .success()
        .stdout(contains("*devday status*"));
}
