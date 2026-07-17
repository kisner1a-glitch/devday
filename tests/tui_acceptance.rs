// TUI acceptance tests, exercised against the real `devday` binary. See
// docs/ACCEPTANCE.md for the TUI coverage mapping (TTY guard, no-secrets
// rendering, preview-never-posts, clear-state guard, and the manual
// real-terminal walkthrough this suite can't automate).
use assert_cmd::Command;
use predicates::str::contains;

/// `devday --help` lists the `tui` subcommand alongside the existing
/// non-interactive commands.
#[test]
fn help_lists_tui_command() {
    Command::cargo_bin("devday")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("tui"));
}

/// `devday tui` refuses to start when stdout is not an interactive
/// terminal, rather than attempting to enter the alternate screen. This is
/// the TTY guard in `src/tui/mod.rs::run` (`stdout().is_terminal()`
/// check); `main` returns the resulting `anyhow::Error` up through `?`, so
/// anyhow's top-level handler prints "Error: devday tui requires an
/// interactive terminal ..." to stderr and exits non-zero.
#[test]
fn tui_refuses_non_tty_stdout() {
    // assert_cmd pipes stdout, so is_terminal() is false inside the child.
    Command::cargo_bin("devday")
        .unwrap()
        .env("HOME", tempfile::tempdir().unwrap().path())
        .arg("tui")
        .assert()
        .failure()
        .stderr(contains("interactive terminal"));
}
