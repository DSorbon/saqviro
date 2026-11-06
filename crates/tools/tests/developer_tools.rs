use chrono::{TimeZone, Utc};
use saqviro_jobs::Params;
use saqviro_tools::developer::{convert, cron::Schedule, generate, web, xml};
use serde_json::{Value, json};

fn params(value: Value) -> Params {
    value.as_object().unwrap().clone()
}

fn none() -> Params {
    Params::new()
}

#[test]
fn json_and_yaml_turn_into_each_other() {
    let yaml = convert::json_to_yaml(r#"{"name":"a","list":[1,true],"inner":{"x":null}}"#, &none()).unwrap();
    let back = convert::yaml_to_json(&yaml, &none()).unwrap();
    let value: Value = serde_json::from_str(&back).unwrap();
    assert_eq!(value, json!({ "name": "a", "list": [1, true], "inner": { "x": null } }));
    assert!(
        yaml.find("name").unwrap() < yaml.find("list").unwrap(),
        "key order is kept: {yaml}"
    );
}

#[test]
fn bad_json_and_yaml_are_explained() {
    assert!(convert::json_to_yaml("{", &none()).unwrap_err().contains("JSON"));
    assert!(convert::yaml_to_json("a: [", &none()).unwrap_err().contains("YAML"));
}

#[test]
fn reads_csv_with_the_delimiter_it_finds() {
    let p = params(json!({ "delimiter": "auto", "typed_values": true }));
    let out = convert::csv_to_json("name;age;code;ok;none\n\"a;b\";30;007;true;\n", &p).unwrap();
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        value,
        json!([{ "name": "a;b", "age": 30, "code": "007", "ok": true, "none": null }])
    );
}

#[test]
fn csv_values_can_stay_text() {
    let out = convert::csv_to_json("a\n1\n", &params(json!({ "typed_values": false }))).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&out).unwrap(), json!([{ "a": "1" }]));
}

#[test]
fn writes_csv_with_every_column() {
    let out = convert::json_to_csv(
        r#"[{"a":1,"b":"x,y"},{"c":{"d":2}}]"#,
        &params(json!({ "delimiter": "comma" })),
    )
    .unwrap();
    assert_eq!(out, "a,b,c\n1,\"x,y\",\n,,\"{\"\"d\"\":2}\"\n");
    assert!(convert::json_to_csv("[1]", &none()).is_err());
    assert!(convert::json_to_csv("5", &none()).is_err());
}

#[test]
fn writes_types_for_a_sample() {
    let sample = r#"{"id":1,"user_info":{"name":"a"},"tags":["x"],"items":[{"a":1},{"a":2,"b":null}],"my-key":true}"#;
    let out = convert::json_to_ts(sample, &params(json!({ "name": "Order" }))).unwrap();
    assert_eq!(
        out,
        "export interface UserInfo {\n  name: string;\n}\n\n\
         export interface ItemsItem {\n  a: number;\n  b?: null;\n}\n\n\
         export interface Order {\n  id: number;\n  user_info: UserInfo;\n  tags: string[];\n  items: ItemsItem[];\n  \"my-key\": boolean;\n}\n"
    );
}

#[test]
fn types_a_top_level_array_and_mixed_values() {
    let out = convert::json_to_ts(r#"[{"a":1},{"a":"x"}]"#, &params(json!({ "name": "Row" }))).unwrap();
    assert!(out.contains("a: number | string;"), "{out}");
    assert!(out.contains("export type Row = RowItem[];"), "{out}");
    let out = convert::json_to_ts("[1,\"x\"]", &none()).unwrap();
    assert!(out.contains("export type Root = (number | string)[];"), "{out}");
}

#[test]
fn json_diff_ignores_order_and_layout() {
    let same = convert::json_diff(
        r#"{"a":1,"b":2}"#,
        &params(json!({ "changed_json": "{ \"b\": 2,\n\"a\": 1 }" })),
    );
    assert_eq!(same.unwrap(), "The texts are the same");
    let out = convert::json_diff(r#"{"a":1}"#, &params(json!({ "changed_json": r#"{"a":2}"# }))).unwrap();
    assert!(out.contains("-   \"a\": 1") && out.contains("+   \"a\": 2"), "{out}");
    let bad = convert::json_diff("{", &params(json!({ "changed_json": "1" }))).unwrap_err();
    assert!(bad.contains("first JSON"), "{bad}");
}

#[test]
fn indents_xml() {
    let out = xml::format("<a x=\"1\"><b>text</b><c/></a>", &params(json!({ "indent": "2" }))).unwrap();
    assert_eq!(out, "<a x=\"1\">\n  <b>text</b>\n  <c/>\n</a>");
    assert!(xml::format("<a><b></a>", &none()).is_err());
    assert!(xml::format("<a><b>", &none()).is_err());
    assert!(xml::format("plain", &none()).is_err());
}

#[test]
fn encodes_and_decodes_urls() {
    let encode = |t, p| web::url_encode(t, &params(p)).unwrap();
    assert_eq!(encode("a b&c=д", json!({ "mode": "encode" })), "a%20b%26c%3D%D0%B4");
    assert_eq!(
        encode("https://x.org/a b?q=1", json!({ "keep_url_characters": true })),
        "https://x.org/a%20b?q=1"
    );
    assert_eq!(encode("a%20b%D0%B4", json!({ "mode": "decode" })), "a bд");
    assert!(web::url_encode("%FF", &params(json!({ "mode": "decode" }))).is_err());
}

#[test]
fn splits_a_url_into_parts() {
    let out = web::url_parse("https://me@example.org/a/b?x=1&y=two%20words#top", &none()).unwrap();
    assert_eq!(
        out,
        "Scheme: https\nUser: me\nHost: example.org\nPort: 443\nPath: /a/b\nFragment: top\n\nQuery\nx = 1\ny = two words\n"
    );
    assert!(web::url_parse("not a url", &none()).is_err());
}

#[test]
fn escapes_html() {
    let run = |t, mode: &str| web::html_entities(t, &params(json!({ "mode": mode }))).unwrap();
    assert_eq!(run("<a href=\"x\">&", "encode"), "&lt;a href=&quot;x&quot;&gt;&amp;");
    assert_eq!(run("&lt;&amp;&eacute;&#65;", "decode"), "<&éA");
}

#[test]
fn finds_http_statuses() {
    assert!(web::http_status("404", &none()).unwrap().starts_with("404 Not Found"));
    assert!(web::http_status("teapot", &none()).unwrap().starts_with("418"));
    assert_eq!(web::http_status("3xx", &none()).unwrap().matches("\n\n").count(), 6);
    assert!(web::http_status("999", &none()).is_err());
}

#[test]
fn finds_mime_types_both_ways() {
    assert_eq!(web::mime("report.pdf", &none()).unwrap(), "application/pdf");
    assert!(web::mime(".PNG", &none()).unwrap().contains("image/png"));
    assert!(web::mime("image/jpeg", &none()).unwrap().contains("jpg"));
    assert!(web::mime("nope", &none()).is_err());
    assert!(web::mime("x/y", &none()).is_err());
}

#[test]
fn passwords_have_the_length_and_kinds_asked_for() {
    let p = params(json!({ "length": 12, "count": 200, "uppercase": true, "digits": true, "symbols": false }));
    let out = generate::passwords("", &p).unwrap();
    let all: Vec<&str> = out.lines().collect();
    assert_eq!(all.len(), 200);
    for password in &all {
        assert_eq!(password.len(), 12);
        assert!(password.chars().all(char::is_alphanumeric));
        assert!(password.chars().any(|c| c.is_ascii_digit()), "{password}");
        assert!(password.chars().any(|c| c.is_ascii_uppercase()), "{password}");
        assert!(password.chars().any(|c| c.is_ascii_lowercase()), "{password}");
    }
    assert_ne!(all[0], all[1]);
}

#[test]
fn passwords_can_avoid_similar_characters() {
    let p = params(json!({ "length": 200, "uppercase": true, "digits": true, "avoid_similar": true }));
    let out = generate::passwords("", &p).unwrap();
    assert!(!out.contains(['I', 'l', '1', 'O', '0', 'o']), "{out}");
}

#[test]
fn encodes_ulids() {
    assert_eq!(generate::encode_ulid(0, 0), "0".repeat(26));
    assert_eq!(
        generate::encode_ulid((1 << 48) - 1, u128::MAX),
        format!("7{}", "Z".repeat(25))
    );
    assert!(generate::encode_ulid(1, 0) < generate::encode_ulid(2, 0));
    let made = generate::ulids("", &params(json!({ "count": 3 }))).unwrap();
    assert_eq!(made.lines().count(), 3);
    assert!(made.lines().all(|l| l.len() == 26));
}

#[test]
fn signs_with_known_results() {
    let text = "The quick brown fox jumps over the lazy dog";
    let sign = |p| generate::hmac(text, &params(p));
    assert_eq!(
        sign(json!({ "key": "key" })).unwrap(),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
    );
    assert_eq!(
        sign(json!({ "key": "key", "algorithm": "sha1" })).unwrap(),
        "de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9"
    );
    assert_eq!(
        sign(json!({ "key": "key", "output": "base64" })).unwrap(),
        "97yD9DBThCSxMpjmqm+xQ+9NWaFJRhdZl0edvC0aPNg="
    );
    assert!(sign(json!({})).is_err());
}

#[test]
fn converts_number_bases() {
    let huge = "f".repeat(33);
    let run = |t, p| generate::number_base(t, &params(p));
    let all = "Binary: 11111111\nOctal: 377\nDecimal: 255\nHex: FF";
    assert_eq!(run("255", json!({})).unwrap(), all);
    assert_eq!(run("0xff", json!({})).unwrap(), all);
    assert_eq!(run("0b1111_1111", json!({})).unwrap(), all);
    assert_eq!(run("ff", json!({ "base": "16" })).unwrap(), all);
    assert_eq!(run("377", json!({ "base": "8" })).unwrap(), all);
    assert!(run("12", json!({ "base": "2" })).unwrap_err().contains("base 2"));
    assert!(run(&huge, json!({ "base": "16" })).unwrap_err().contains("too large"));
    assert!(run("-1", json!({})).is_err());
}

#[test]
fn escapes_for_regular_expressions() {
    assert_eq!(generate::regex_escape("a.b*c", &none()).unwrap(), r"a\.b\*c");
}

fn next(expression: &str, count: usize) -> Vec<String> {
    let from = Utc.with_ymd_and_hms(2026, 11, 6, 10, 0, 0).unwrap();
    let schedule = Schedule::parse(expression).unwrap();
    schedule
        .next_runs(from, count)
        .iter()
        .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
        .collect()
}

#[test]
fn finds_the_next_cron_runs() {
    assert_eq!(next("30 9 * * 1-5", 2), ["2026-11-09 09:30", "2026-11-10 09:30"]);
    assert_eq!(next("*/20 * * * *", 2), ["2026-11-06 10:20", "2026-11-06 10:40"]);
    assert_eq!(next("0 10 * * *", 1), ["2026-11-07 10:00"]);
    assert_eq!(next("0 0 1 jan *", 1), ["2027-01-01 00:00"]);
    assert_eq!(next("0 0 * * sun", 1), ["2026-11-08 00:00"]);
    assert_eq!(next("0 0 * * 7", 1), ["2026-11-08 00:00"]);
    assert_eq!(next("0 0 29 2 *", 1), ["2028-02-29 00:00"]);
    assert_eq!(next("0 0 31 2 *", 1), Vec::<String>::new());
}

#[test]
fn either_day_field_can_match() {
    // The 13th, or any Friday.
    assert_eq!(
        next("0 12 13 * 5", 3),
        ["2026-11-06 12:00", "2026-11-13 12:00", "2026-11-20 12:00"]
    );
}

#[test]
fn explains_a_bad_cron_expression() {
    let wrong = |e| Schedule::parse(e).err().unwrap();
    assert!(wrong("* * *").contains("5 fields"));
    assert!(wrong("60 * * * *").contains("minute"));
    assert!(wrong("* 25 * * *").contains("hour"));
    assert!(wrong("* * 0 * *").contains("day of month"));
    assert!(wrong("*/0 * * * *").contains("minute"));
    assert!(wrong("a * * * *").contains("minute"));
    assert!(wrong("5-1 * * * *").contains("minute"));
}

#[test]
fn xml_turns_into_json() {
    let out = xml::to_json(r#"<a x="1"><b>one &amp; two</b><b>2</b><c/></a>"#, &none()).unwrap();
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value, json!({ "a": { "@x": "1", "b": ["one & two", "2"], "c": null } }));
    assert!(xml::to_json("plain", &none()).is_err());
    assert!(xml::to_json("<a><b></a>", &none()).is_err());
}

#[test]
fn xml_keeps_unknown_entities_unexpanded() {
    let doc = r#"<!DOCTYPE a [<!ENTITY lol "lol">]><a>&lol;</a>"#;
    let value: Value = serde_json::from_str(&xml::to_json(doc, &none()).unwrap()).unwrap();
    assert_eq!(value, json!({ "a": "&lol;" }));
}

#[test]
fn json_turns_into_xml() {
    let out = xml::from_json(r#"{"a":{"@x":"1","b":["o<e",2],"c":null}}"#, &none()).unwrap();
    assert_eq!(out, "<a x=\"1\">\n  <b>o&lt;e</b>\n  <b>2</b>\n  <c/>\n</a>\n");
    let out = xml::from_json("[1,2]", &params(json!({ "root": "list" }))).unwrap();
    assert_eq!(out, "<list>\n  <item>1</item>\n  <item>2</item>\n</list>\n");
    assert!(xml::from_json(r#"{"a b":1,"c":2}"#, &none()).is_err());
}

#[test]
fn xml_and_json_survive_a_round_trip() {
    let source = r#"<a x="1"><b>t</b><b>u</b></a>"#;
    let json = xml::to_json(source, &none()).unwrap();
    let back = xml::from_json(&json, &none()).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&xml::to_json(&back, &none()).unwrap()).unwrap(),
        serde_json::from_str::<Value>(&json).unwrap()
    );
}

#[test]
fn formats_sql() {
    let p = params(json!({ "uppercase": true, "indent": "2" }));
    let out = generate::sql_format("select a, b from t where x=1", &p).unwrap();
    assert!(out.starts_with("SELECT\n  a,\n  b\nFROM\n  t\nWHERE"), "{out}");
}

#[test]
fn converts_colors() {
    use saqviro_tools::developer::color;
    let all = "Hex: #ff8000\nRGB: rgb(255, 128, 0)\nHSL: hsl(30, 100%, 50%)";
    assert_eq!(color::run("#ff8000", &none()).unwrap(), all);
    assert_eq!(color::run("rgb(255, 128, 0)", &none()).unwrap(), all);
    assert_eq!(color::run("255,128,0", &none()).unwrap(), all);
    assert!(color::run("hsl(30, 100%, 50%)", &none()).unwrap().contains("#ff8000"));
    assert!(color::run("#000", &none()).unwrap().contains("hsl(0, 0%, 0%)"));
    assert!(color::run("rgb(300, 0, 0)", &none()).is_err());
    assert!(color::run("blue-ish", &none()).is_err());
}
