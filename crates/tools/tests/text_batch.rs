use saqviro_jobs::Params;
use saqviro_tools::text::{clean, diff, lines, lorem, replace, slug, unicode};
use serde_json::{Value, json};

fn params(value: Value) -> Params {
    value.as_object().unwrap().clone()
}

fn none() -> Params {
    Params::new()
}

#[test]
fn keeps_the_first_of_repeated_lines() {
    assert_eq!(lines::dedupe("b\na\nb\na\n", &none()).unwrap(), "b\na\n");
    assert_eq!(lines::dedupe("A\na", &none()).unwrap(), "A\na");
    assert_eq!(
        lines::dedupe("A\na", &params(json!({ "ignore_case": true }))).unwrap(),
        "A"
    );
    assert_eq!(
        lines::dedupe("x\n x ", &params(json!({ "ignore_spaces": true }))).unwrap(),
        "x"
    );
}

#[test]
fn sorts_like_people_expect() {
    let sort = |text, p| lines::sort(text, &params(p)).unwrap();
    assert_eq!(
        sort("file10\nfile2\nfile1", json!({ "by": "natural" })),
        "file1\nfile2\nfile10"
    );
    assert_eq!(sort("file10\nfile2", json!({ "by": "text" })), "file10\nfile2");
    assert_eq!(sort("b\nA\nc", json!({ "by": "text", "ignore_case": true })), "A\nb\nc");
    assert_eq!(sort("b\nA\nc", json!({ "by": "text" })), "A\nb\nc");
    assert_eq!(
        sort("ccc\na\nbb", json!({ "by": "length", "descending": true })),
        "ccc\nbb\na"
    );
    assert_eq!(
        sort("x 10\nnone\ny -2\nz 3.5", json!({ "by": "number" })),
        "y -2\nz 3.5\nx 10\nnone"
    );
    assert_eq!(sort("b\na\n", json!({})), "a\nb\n");
}

#[test]
fn natural_order_survives_leading_zeros_and_huge_numbers() {
    use std::cmp::Ordering::*;
    assert_eq!(lines::natural("a007", "a7"), Equal);
    assert_eq!(
        lines::natural("99999999999999999999998", "99999999999999999999999"),
        Less
    );
    assert_eq!(lines::natural("а2", "а10"), Less);
    assert_eq!(lines::natural("a", "a1"), Less);
}

#[test]
fn reverses_cleans_and_trims_lines() {
    assert_eq!(lines::reverse("1\n2\n3\n", &none()).unwrap(), "3\n2\n1\n");
    assert_eq!(lines::remove_empty("a\n\n  \nb", &none()).unwrap(), "a\nb");
    assert_eq!(
        lines::trim("  a \n b", &params(json!({ "side": "both" }))).unwrap(),
        "a\nb"
    );
    assert_eq!(lines::trim("  a ", &params(json!({ "side": "start" }))).unwrap(), "a ");
    assert_eq!(lines::trim("  a ", &params(json!({ "side": "end" }))).unwrap(), "  a");
}

#[test]
fn normalizes_whitespace() {
    let keep = params(json!({ "keep_line_breaks": true }));
    assert_eq!(
        clean::whitespace("a \t b\u{00A0}c\n\n\n\nd\n\n", &keep).unwrap(),
        "a b c\n\nd"
    );
    assert_eq!(clean::whitespace("\n\na", &keep).unwrap(), "a");
    assert_eq!(clean::whitespace("a\n\n b ", &none()).unwrap(), "a b");
}

#[test]
fn removes_what_cannot_be_seen_and_nothing_else() {
    let run = |t| clean::invisible(t, &none()).unwrap();
    assert_eq!(run("a\u{200B}b\u{FEFF}c\u{00AD}d\u{202E}e\u{0007}"), "abcde");
    assert_eq!(run("line\n\tnext\r\n"), "line\n\tnext\r\n");
    assert_eq!(run("привет"), "привет");
    // Emoji held together by a joiner stay one emoji, but a stray joiner goes.
    assert_eq!(run("👩\u{200D}💻"), "👩\u{200D}💻");
    assert_eq!(run("❤\u{FE0F}\u{200D}🔥"), "❤\u{FE0F}\u{200D}🔥");
    assert_eq!(run("a\u{200D}b"), "ab");
}

#[test]
fn replaces_plain_text_literally() {
    let run = |t, p| replace::run(t, &params(p));
    assert_eq!(
        run("a.b a.b", json!({ "find": "a.b", "replace_with": "$1" })).unwrap(),
        "$1 $1"
    );
    assert_eq!(
        run("axb", json!({ "find": "a.b", "replace_with": "z" })).unwrap(),
        "axb"
    );
    assert_eq!(run("Hi hi", json!({ "find": "hi", "ignore_case": true })).unwrap(), " ");
}

#[test]
fn replaces_with_a_regular_expression() {
    let p = json!({ "find": r"(\w+)@(\w+)", "replace_with": "$2 at $1", "use_regex": true });
    assert_eq!(replace::run("me@home", &params(p)).unwrap(), "home at me");
}

#[test]
fn explains_a_bad_search() {
    assert!(replace::run("a", &none()).unwrap_err().contains("find"));
    let p = json!({ "find": "(", "use_regex": true });
    assert!(replace::run("a", &params(p)).is_err());
}

#[test]
fn makes_slugs_from_any_script() {
    let run = |t, p| slug::run(t, &params(p));
    assert_eq!(run("  Hello, big World!  ", json!({})).unwrap(), "hello-big-world");
    assert_eq!(run("Привет, мир!", json!({})).unwrap(), "privet-mir");
    assert_eq!(
        run("Crème Brûlée", json!({ "separator": "_" })).unwrap(),
        "creme_brulee"
    );
    assert_eq!(run("one two three", json!({ "max_length": 8 })).unwrap(), "one-two");
    assert_eq!(run("extraordinary", json!({ "max_length": 5 })).unwrap(), "extra");
    assert!(run("!!!", json!({})).is_err());
}

#[test]
fn lorem_ipsum_is_repeatable_and_the_right_size() {
    let p = params(json!({ "paragraphs": 3, "sentences": 2 }));
    let text = lorem::run("", &p).unwrap();
    assert_eq!(text, lorem::run("", &p).unwrap());
    assert!(text.starts_with("Lorem ipsum dolor sit amet"));
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    assert_eq!(paragraphs.len(), 3);
    assert!(paragraphs.iter().all(|p| p.matches('.').count() == 2), "{text}");
}

#[test]
fn shows_code_points_and_bytes() {
    let out = unicode::run("A€\u{200B}", &none()).unwrap();
    let rows: Vec<&str> = out.lines().collect();
    assert_eq!(rows, ["U+0041\tA\t41", "U+20AC\t€\tE2 82 AC", "U+200B\t·\tE2 80 8B"]);
    let emoji = unicode::run("😀", &none()).unwrap();
    assert_eq!(emoji.trim_end(), "U+1F600\t😀\tF0 9F 98 80");
}

#[test]
fn diffs_lines() {
    let p = json!({ "changed_text": "a\nB\nc\nd\n" });
    assert_eq!(
        diff::run("a\nb\nc\n", &params(p)).unwrap(),
        "2 added, 1 removed\n\n  a\n- b\n+ B\n  c\n+ d\n"
    );
    let p = json!({ "changed_text": "a\nB\n", "only_changes": true });
    assert_eq!(
        diff::run("a\nb\n", &params(p)).unwrap(),
        "1 added, 1 removed\n\n- b\n+ B\n"
    );
    assert_eq!(
        diff::run("a\n", &params(json!({ "changed_text": "a\n" }))).unwrap(),
        "The texts are the same"
    );
    let p = json!({ "changed_text": "a  b \n", "ignore_spaces": true });
    assert_eq!(diff::run(" a b\n", &params(p)).unwrap(), "The texts are the same");
}
