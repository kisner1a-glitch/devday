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
