// SRS §18 CLI-observable acceptance criteria, exercised against the real
// `devday` binary. Every test runs with HOME pointed at a fresh temp
// directory so the developer's real ~/.config/devday/config.toml (and real
// `gh`/Linear/Slack credentials) never leak into or influence the run — see
// docs/ACCEPTANCE.md for the full SRS §18 -> test/manual-step mapping.
use assert_cmd::Command;
use predicates::str::contains;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn devday(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("devday").unwrap();
    c.env("HOME", home);
    c
}

/// AC1: `devday config init` emits a valid, complete starter TOML config.
#[test]
fn ac1_config_init_emits_valid_toml() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["config", "init"])
        .assert()
        .success()
        .stdout(contains("[sources]"))
        .stdout(contains("[github]"))
        .stdout(contains("[linear]"))
        .stdout(contains("[git]"))
        .stdout(contains("[ai]"))
        .stdout(contains("[slack]"))
        .stdout(contains("[state]"))
        .stdout(contains("[redact]"));
}

/// AC2/AC3/FR-8: `report --stdout` renders all five required Markdown
/// sections (Summary is asserted implicitly via the empty-state test below).
#[test]
fn ac2_report_generates_all_sections() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["report", "--stdout"])
        .assert()
        .success()
        .stdout(contains("## Worked On"))
        .stdout(contains("## Next Up"))
        .stdout(contains("## Blockers"))
        .stdout(contains("## Source Links"));
}

/// AC7/NFR-7: an empty activity window (fresh HOME, no config, no git
/// roots, no gh auth, no Linear token) produces a useful empty-state report
/// rather than an error or a blank page.
#[test]
fn ac7_empty_window_is_useful_empty_state() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["report", "--stdout"])
        .assert()
        .success()
        .stdout(contains("No activity"));
}

/// AC12/AC13: Slack preview mode always previews and never posts, even if
/// the caller also passes --post (posting additionally requires
/// slack.auto_post = true in config, which this fresh HOME does not have).
#[test]
fn ac12_slack_preview_does_not_post() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["send", "slack", "--preview"])
        .assert()
        .success()
        .stdout(contains("devday status"));
}

/// AC13 (continued): --post alone, without slack.auto_post = true in
/// config, must not post either -- it should fall back to preview output
/// and still succeed, because no Slack credentials are configured to post
/// with in a fresh HOME.
#[test]
fn ac13_post_without_auto_post_config_falls_back_to_preview() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["send", "slack", "--post"])
        .assert()
        .success()
        .stdout(contains("devday status"));
}

/// AC10/FR-10: when an AI provider is configured but its binary can't run,
/// summarization fails and the command falls back to the deterministic
/// report rather than erroring out.
#[test]
fn ac10_ai_failure_falls_back_to_deterministic_report() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args([
            "report",
            "--ai",
            "definitely-not-a-real-binary-xyz",
            "--stdout",
        ])
        .assert()
        .success()
        .stdout(contains("## Summary"))
        .stdout(contains("No activity"));
}

/// AC11/AC13/AC16/FR-12/FR-16: with a config that enables auto_post and
/// points a webhook env var at a mock Slack endpoint, `--post` actually
/// posts (proving the webhook delivery path end-to-end through the real
/// binary, not just the `post_webhook` unit), and a second identical run
/// is deduplicated via local state and does not post again.
// Needs a real multi-thread runtime: the test body makes *blocking*
// subprocess calls (assert_cmd::Command::assert), and the mock HTTP server
// the subprocess talks to needs its own executor thread to keep answering
// requests while that blocking call is in flight.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac11_ac16_webhook_post_and_dedup_via_local_state() {
    let home = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1) // only one POST across both runs below: the second is deduped.
        .mount(&server)
        .await;

    let config_path = home.path().join("devday.toml");
    std::fs::write(
        &config_path,
        "[sources]\ngithub = false\nlinear = false\ngit = false\n\n[slack]\nwebhook_env = \"DEVDAY_TEST_WEBHOOK\"\nauto_post = true\n",
    )
    .unwrap();

    let run = || {
        let mut c = devday(home.path());
        c.env("DEVDAY_TEST_WEBHOOK", server.uri())
            .args(["send", "slack", "--config"])
            .arg(&config_path)
            .arg("--post");
        c
    };

    // First run: no prior state, so it posts.
    run().assert().success();
    // Second run: identical report content -> deduped, still exits 0, no
    // second POST (enforced by `.expect(1)` on the mock above).
    run().assert().success();

    server.verify().await;
}

/// AC15/NFR-2: the CLI runs to completion non-interactively (no TTY, no
/// prompts) under cron-like conditions -- assert_cmd never attaches a TTY,
/// so a clean exit here demonstrates the property.
#[test]
fn ac15_runs_noninteractively() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path()).args(["report"]).assert().success();
}

/// AC19/FR-18: --help output (which echoes back the parsed CLI surface)
/// never contains anything token-shaped. A light, cheap complement to the
/// dedicated tests/no_secrets.rs guard.
#[test]
fn ac19_help_output_has_no_secrets() {
    let home = tempfile::tempdir().unwrap();
    let out = devday(home.path()).arg("--help").assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    for prefix in ["ghp_", "gho_", "xoxb-", "xoxp-", "lin_api_"] {
        assert!(
            !stdout.contains(prefix),
            "unexpected token prefix {prefix} in --help output"
        );
    }
}
