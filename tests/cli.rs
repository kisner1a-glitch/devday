use assert_cmd::Command;

#[test]
fn help_lists_commands() {
    Command::cargo_bin("devday")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
}

#[test]
fn report_runs() {
    Command::cargo_bin("devday")
        .unwrap()
        .args(["report", "--git"])
        .assert()
        .success();
}
