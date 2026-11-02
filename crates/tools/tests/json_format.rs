use saqviro_jobs::{Config, ErrorKind, JobInput, JobManager, JobOutput, JobSnapshot, JobStatus, Params};
use saqviro_tools::developer::json_format::{Indent, format};
use serde_json::json;

fn pretty(src: &str) -> String {
    format(src, Indent::Spaces(2), false).unwrap()
}

#[test]
fn pretty_prints_with_two_spaces() {
    assert_eq!(
        pretty(r#"{"a":1,"b":[true,null]}"#),
        "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    null\n  ]\n}\n"
    );
}

#[test]
fn supports_four_spaces_tabs_and_minify() {
    let src = r#"{ "a" : [ 1 ] }"#;
    assert_eq!(
        format(src, Indent::Spaces(4), false).unwrap(),
        "{\n    \"a\": [\n        1\n    ]\n}\n"
    );
    assert_eq!(
        format(src, Indent::Tab, false).unwrap(),
        "{\n\t\"a\": [\n\t\t1\n\t]\n}\n"
    );
    assert_eq!(format(src, Indent::Minify, false).unwrap(), r#"{"a":[1]}"#);
}

#[test]
fn keeps_key_order_unless_asked_to_sort() {
    let src = r#"{"b":1,"a":{"d":1,"c":2}}"#;
    assert_eq!(
        format(src, Indent::Minify, false).unwrap(),
        r#"{"b":1,"a":{"d":1,"c":2}}"#
    );
    assert_eq!(
        format(src, Indent::Minify, true).unwrap(),
        r#"{"a":{"c":2,"d":1},"b":1}"#
    );
}

#[test]
fn never_changes_numbers() {
    let src = "[12345678901234567890123,1.0,1e10,-0,0.1E-5]";
    assert_eq!(format(src, Indent::Minify, false).unwrap(), src);
}

#[test]
fn never_changes_strings() {
    let src = r#"["\u00e9","é","line\nbreak","\"quoted\"","\/"]"#;
    assert_eq!(format(src, Indent::Minify, false).unwrap(), src);
}

#[test]
fn keeps_duplicate_keys() {
    assert_eq!(
        format(r#"{"a":1,"a":2}"#, Indent::Minify, false).unwrap(),
        r#"{"a":1,"a":2}"#
    );
}

#[test]
fn formats_empty_containers_on_one_line() {
    assert_eq!(pretty(r#"{"a":{},"b":[]}"#), "{\n  \"a\": {},\n  \"b\": []\n}\n");
    assert_eq!(pretty("  42 "), "42\n");
}

#[test]
fn reports_line_and_column_of_errors() {
    let err = format("{\n  \"a\": 1,\n  \"b\": }", Indent::Spaces(2), false).unwrap_err();
    assert!(err.contains("line 3, column 8"), "{err}");
}

#[test]
fn rejects_invalid_json() {
    for bad in [
        "",
        "   ",
        "{",
        "[1,]",
        "{\"a\" 1}",
        "01",
        "1.",
        "-",
        "\"\u{1}\"",
        "'a'",
        "[1] 2",
        "tru",
        "\"\\x\"",
        "NaN",
    ] {
        assert!(format(bad, Indent::Minify, false).is_err(), "accepted {bad:?}");
    }
}

#[test]
fn rejects_deep_nesting_without_overflowing_the_stack() {
    let deep = "[".repeat(100_000) + &"]".repeat(100_000);
    let err = format(&deep, Indent::Minify, false).unwrap_err();
    assert!(err.contains("deep"), "{err}");
}

fn run(input: JobInput, params: serde_json::Value) -> JobSnapshot {
    let root = tempfile::tempdir().unwrap();
    let jobs = JobManager::new(
        Config {
            temp_root: root.path().join("jobs"),
            workers: 1,
        },
        |_: &JobSnapshot| {},
    )
    .unwrap();
    let exec: std::sync::Arc<dyn saqviro_jobs::Executor> =
        std::sync::Arc::new(saqviro_tools::developer::json_format::run);
    let params: Params = params.as_object().unwrap().clone();
    jobs.wait(jobs.submit("developer.json.format", exec, input, params))
        .unwrap()
}

#[test]
fn runs_as_a_job() {
    let done = run(
        JobInput::Text(r#"{"b":1,"a":2}"#.into()),
        json!({ "indent": "minify", "sort_keys": true }),
    );
    assert_eq!(done.status, JobStatus::Completed);
    assert_eq!(done.output, Some(JobOutput::Text(r#"{"a":2,"b":1}"#.into())));
}

#[test]
fn invalid_json_fails_the_job_as_invalid_input() {
    let done = run(JobInput::Text("{".into()), json!({ "indent": "2", "sort_keys": false }));
    assert_eq!(done.status, JobStatus::Failed);
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn file_input_is_rejected() {
    let done = run(
        JobInput::Files(vec!["/tmp/a.json".into()]),
        json!({ "indent": "2", "sort_keys": false }),
    );
    assert_eq!(done.error.unwrap().kind, ErrorKind::InvalidInput);
}

#[test]
fn output_is_the_same_json_in_every_mode() {
    let docs = [
        r#"{"name":"Saqviro","tags":["pdf","image"],"nested":{"deep":[{"a":[[]]},{}]},"n":-12.5e-3,"ok":true,"none":null}"#,
        "[1, 2, 3]",
        r#""just a string with \"quotes\" and \t tabs""#,
        r#"{"emoji":"😀","cjk":"漢字","esc":"😀"}"#,
    ];
    for doc in docs {
        let original: serde_json::Value = serde_json::from_str(doc).unwrap();
        for indent in [Indent::Spaces(2), Indent::Spaces(4), Indent::Tab, Indent::Minify] {
            let out = format(doc, indent, false).unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&out).unwrap(),
                original,
                "{indent:?}: {out}"
            );
        }
    }
}
