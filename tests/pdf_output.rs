use assert_cmd::Command;
use predicates::str::contains;

fn devday(home: &std::path::Path) -> Command {
    let mut c = Command::cargo_bin("devday").unwrap();
    c.env("HOME", home);
    c
}

#[test]
fn output_pdf_extension_writes_pdf() {
    let home = tempfile::tempdir().unwrap();
    let out = home.path().join("report.pdf");
    devday(home.path())
        .args(["report", "--output", out.to_str().unwrap()])
        .assert()
        .success();
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"%PDF-"), "expected PDF header");
}

#[test]
fn format_pdf_without_output_errors() {
    let home = tempfile::tempdir().unwrap();
    devday(home.path())
        .args(["report", "--format", "pdf"])
        .assert()
        .failure()
        .stderr(contains("requires --output"));
}

#[test]
fn explicit_md_beats_pdf_extension() {
    let home = tempfile::tempdir().unwrap();
    let out = home.path().join("report.pdf");
    devday(home.path())
        .args([
            "report",
            "--format",
            "md",
            "--output",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("# Status Report"));
}
