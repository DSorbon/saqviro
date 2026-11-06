//! Runs the real program.

use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn saqviro(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_saqviro"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    assert!(!output.status.success(), "should have failed");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn with_input(args: &[&str], text: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_saqviro"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    stdout(&child.wait_with_output().unwrap())
}

#[test]
fn lists_tools_by_category() {
    let all = stdout(&saqviro(&["list"]));
    assert!(all.lines().count() > 100);
    let text = stdout(&saqviro(&["list", "text"]));
    assert!(text.contains("text.case") && !text.contains("pdf.merge"), "{text}");
}

#[test]
fn explains_a_tool() {
    let out = stdout(&saqviro(&["help", "text.lines.sort"]));
    assert!(out.starts_with("Sort Lines\n"), "{out}");
    assert!(
        out.contains("--by") && out.contains("natural | text | number | length"),
        "{out}"
    );
    assert!(out.contains("--ignore-case"), "{out}");
}

#[test]
fn runs_a_text_tool_on_standard_input() {
    assert_eq!(with_input(&["run", "text.case", "--case", "upper"], "hello"), "HELLO\n");
}

#[test]
fn runs_a_text_tool_on_an_argument() {
    let out = stdout(&saqviro(&[
        "run",
        "text.slug",
        "--text",
        "Hello World",
        "--separator=_",
    ]));
    assert_eq!(out, "hello_world\n");
}

#[test]
fn takes_yes_and_no_settings() {
    let args = [
        "run",
        "text.lines.sort",
        "--by",
        "text",
        "--no-ignore-case",
        "--descending",
        "--text",
        "a\nB",
    ];
    assert_eq!(stdout(&saqviro(&args)), "a\nB\n");
}

#[test]
fn runs_a_tool_that_takes_nothing() {
    let out = stdout(&saqviro(&["run", "developer.uuid", "--count", "2"]));
    assert_eq!(out.lines().count(), 2);
}

#[test]
fn writes_results_where_asked() {
    let dir = tempfile::tempdir().unwrap();
    let out = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "hi").unwrap();
    let archive = stdout(&saqviro(&[
        "run",
        "files.zip.create",
        "--out",
        out.path().to_str().unwrap(),
        file.to_str().unwrap(),
    ]));
    let archive = Path::new(archive.trim());
    assert_eq!(archive.parent().unwrap(), out.path());
    assert!(archive.exists());
}

#[test]
fn runs_a_workflow_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("w.json");
    let workflow = r#"{ "schema_version": 1, "id": "w", "name": "W", "steps": [
        { "tool": "text.lines.dedupe" }, { "tool": "text.case", "params": { "case": "upper" } } ] }"#;
    fs::write(&path, workflow).unwrap();
    let out = stdout(&saqviro(&["workflow", path.to_str().unwrap(), "--text", "a\na"]));
    assert_eq!(out, "A\n");
}

#[test]
fn explains_what_went_wrong() {
    assert!(stderr(&saqviro(&["run", "nope.nope"])).contains("no tool `nope.nope`"));
    assert!(stderr(&saqviro(&["run", "text.case", "--wat", "1"])).contains("unknown option --wat"));
    assert!(stderr(&saqviro(&["run", "text.case", "--case"])).contains("needs a value"));
    assert!(stderr(&saqviro(&["run", "text.case", "--case", "wobbly", "--text", "a"])).contains("case"));
    assert!(stderr(&saqviro(&["run", "developer.uuid", "--count", "many"])).contains("--count"));
    assert!(stderr(&saqviro(&["run", "files.sha256"])).contains("at least one file"));
    assert!(stderr(&saqviro(&["dance"])).contains("unknown command"));
}

#[test]
fn prints_its_version_and_usage() {
    assert!(stdout(&saqviro(&["--version"])).starts_with("saqviro "));
    assert!(stdout(&saqviro(&[])).contains("Usage:"));
}

#[test]
fn a_generated_picture_goes_where_asked_and_replaces_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().to_str().unwrap();
    let first = stdout(&saqviro(&["run", "qr.generate", "--text", "a", "--out", out]));
    let second = stdout(&saqviro(&["run", "qr.generate", "--text", "b", "--out", out]));
    assert_ne!(first, second);
    assert!(Path::new(first.trim()).starts_with(dir.path()));
    assert!(Path::new(second.trim()).exists());
}
