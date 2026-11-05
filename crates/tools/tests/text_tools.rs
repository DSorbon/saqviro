use saqviro_jobs::Params;
use saqviro_tools::{
    developer::{
        base64,
        hash::{self, Algorithm, hash_text},
        jwt, regex_test, timestamp, uuid,
    },
    text::{
        case::{convert, split_words},
        words::count,
    },
};
use serde_json::{Value, json};

fn params(value: Value) -> Params {
    value.as_object().unwrap().clone()
}

#[test]
fn counts_words_lines_and_paragraphs() {
    let c = count("Hello, big world! How are you?\nFine.\n\nNext one.");
    assert_eq!((c.words, c.lines, c.paragraphs, c.sentences), (9, 4, 2, 4));
}

#[test]
fn counts_what_people_see_as_characters() {
    let family = "👨‍👩‍👧‍👦";
    assert_eq!(count(family).characters, 1);
    assert_eq!(count("é a").characters, 3);
    assert_eq!(count("é a").without_spaces, 2);
    assert_eq!(count("привет мир").words, 2);
}

#[test]
fn counts_nothing_in_nothing() {
    let c = count("  \n ");
    assert_eq!((c.words, c.sentences, c.paragraphs), (0, 0, 0));
}

#[test]
fn splits_identifiers_into_words() {
    assert_eq!(split_words("helloWorld"), ["hello", "World"]);
    assert_eq!(split_words("HTTPServer2Go"), ["HTTP", "Server2", "Go"]);
    assert_eq!(split_words("snake_case-and kebab"), ["snake", "case", "and", "kebab"]);
    assert_eq!(split_words("  "), Vec::<String>::new());
}

#[test]
fn converts_case() {
    let cases = [
        ("upper", "straße"),
        ("camel", "hello big World"),
        ("pascal", "hello big World"),
        ("snake", "helloBigWorld"),
        ("kebab", "Hello Big World"),
        ("constant", "hello-big-world"),
        ("title", "hELLO  wORLD"),
        ("sentence", "hELLO. wORLD! yES"),
    ];
    let expected = [
        "STRASSE",
        "helloBigWorld",
        "HelloBigWorld",
        "hello_big_world",
        "hello-big-world",
        "HELLO_BIG_WORLD",
        "Hello  World",
        "Hello. World! Yes",
    ];
    for ((case, input), want) in cases.into_iter().zip(expected) {
        assert_eq!(convert(input, case).unwrap(), want, "{case}");
    }
    assert!(convert("x", "nope").is_err());
}

#[test]
fn case_works_beyond_ascii() {
    assert_eq!(convert("привет мир", "camel").unwrap(), "приветМир");
    assert_eq!(convert("привет мир", "title").unwrap(), "Привет Мир");
}

#[test]
fn base64_round_trips() {
    for text in ["", "f", "fo", "foo", "привет 👋"] {
        assert_eq!(base64::decode(&base64::encode(text, false)).unwrap(), text);
        assert_eq!(base64::decode(&base64::encode(text, true)).unwrap(), text);
    }
    assert_eq!(base64::encode("foob", false), "Zm9vYg==");
}

#[test]
fn base64_reads_what_people_paste() {
    assert_eq!(base64::decode("Zm9v\nYg").unwrap(), "foob");
    assert_eq!(base64::encode("\u{ff}\u{fe}", true), "w7_Dvg");
    assert_eq!(base64::decode("w7_Dvg").unwrap(), "\u{ff}\u{fe}");
}

#[test]
fn base64_explains_bad_input() {
    assert!(base64::decode("not base64!").unwrap_err().contains("not valid"));
    assert!(base64::decode("/w==").unwrap_err().contains("binary"));
}

#[test]
fn uuids_are_valid_and_of_the_right_version() {
    let ids = uuid::run("", &params(json!({ "version": "4", "count": 3 }))).unwrap();
    let ids: Vec<&str> = ids.lines().collect();
    assert_eq!(ids.len(), 3);
    assert!(ids[0] != ids[1]);
    assert_eq!(&ids[0][14..15], "4");

    let v7 = uuid::run("", &params(json!({ "version": "7", "uppercase": true }))).unwrap();
    assert_eq!(&v7[14..15], "7");
    assert_eq!(v7, v7.to_uppercase());
}

fn now(seconds: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(seconds, 0).unwrap()
}

const TOKEN: &str =
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjMiLCJpYXQiOjE1MTYyMzkwMjIsImV4cCI6MTUxNjIzOTAzMn0.sig";

#[test]
fn decodes_a_jwt_and_says_it_was_not_verified() {
    let text = jwt::decode(TOKEN, now(0)).unwrap();
    assert!(text.contains("\"alg\": \"HS256\""), "{text}");
    assert!(text.contains("\"sub\": \"123\""), "{text}");
    assert!(text.contains("Issued: 2018-01-18 01:30:22 UTC"), "{text}");
    assert!(text.contains("not checked"), "{text}");
    assert!(!text.contains("expired"), "{text}");
}

#[test]
fn says_when_a_jwt_has_expired() {
    assert!(jwt::decode(TOKEN, now(2_000_000_000)).unwrap().contains("has expired"));
}

#[test]
fn accepts_a_bearer_header() {
    assert!(jwt::decode(&format!("Bearer {TOKEN}\n"), now(0)).is_ok());
}

#[test]
fn warns_about_unsigned_tokens() {
    let header = base64::encode(r#"{"alg":"none"}"#, true);
    let payload = base64::encode("{}", true);
    let text = jwt::decode(&format!("{header}.{payload}."), now(0)).unwrap();
    assert!(text.contains("no signature"), "{text}");
}

#[test]
fn rejects_what_is_not_a_jwt() {
    assert!(jwt::decode("abc", now(0)).unwrap_err().contains("three parts"));
    assert!(jwt::decode("a.b.c", now(0)).is_err());
    let not_json = base64::encode("hi", true);
    assert!(
        jwt::decode(&format!("{not_json}.{not_json}.x"), now(0))
            .unwrap_err()
            .contains("JSON")
    );
}

#[test]
fn hashes_match_published_values() {
    let cases = [
        (Algorithm::Md5, "900150983cd24fb0d6963f7d28e17f72"),
        (Algorithm::Sha1, "a9993e364706816aba3e25717850c26c9cd0d89d"),
        (
            Algorithm::Sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            Algorithm::Blake3,
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85",
        ),
    ];
    for (algorithm, expected) in cases {
        assert_eq!(hash_text(algorithm, "abc"), expected, "{algorithm:?}");
    }
    assert!(hash_text(Algorithm::Sha512, "abc").starts_with("ddaf35a193617aba"));
}

#[test]
fn hash_generator_gives_one_or_all() {
    let one = hash::run("abc", &params(json!({ "algorithm": "sha256" }))).unwrap();
    assert_eq!(one.len(), 64);
    let all = hash::run("abc", &params(json!({ "algorithm": "all" }))).unwrap();
    assert_eq!(all.lines().count(), 5);
    assert!(all.contains("SHA-1: a9993e"), "{all}");
}

#[test]
fn reads_seconds_milliseconds_and_dates() {
    let want = 1_730_000_000;
    assert_eq!(timestamp::parse("1730000000").unwrap().timestamp(), want);
    assert_eq!(timestamp::parse("1730000000000").unwrap().timestamp(), want);
    assert_eq!(timestamp::parse("2024-10-27T03:33:20Z").unwrap().timestamp(), want);
    assert_eq!(timestamp::parse("2024-10-27T06:33:20+03:00").unwrap().timestamp(), want);
    assert_eq!(timestamp::parse("2024-10-27 03:33:20").unwrap().timestamp(), want);
    assert_eq!(timestamp::parse("2024-10-27").unwrap().timestamp(), 1_729_987_200);
    assert_eq!(timestamp::parse("-1").unwrap().timestamp(), -1);
}

#[test]
fn rejects_what_is_not_a_time() {
    for text in ["soon", "2024-13-40", "99999999999999999999999"] {
        assert!(timestamp::parse(text).is_err(), "{text}");
    }
}

#[test]
fn describes_a_moment_and_empty_input_means_now() {
    let text = timestamp::describe(now(1_730_000_000));
    assert!(text.contains("Unix milliseconds: 1730000000000"), "{text}");
    assert!(text.contains("UTC: 2024-10-27T03:33:20.000Z"), "{text}");
    let current = timestamp::run("  ", &Params::new()).unwrap();
    assert!(current.starts_with("Unix seconds: 1"), "{current}");
}

#[test]
fn lists_matches_with_positions_and_groups() {
    let p = params(json!({ "pattern": r"(?P<key>\w+)=(\d+)?" }));
    let text = regex_test::run("a=1\n  привет=", &p).unwrap();
    assert!(text.starts_with("2 matches"), "{text}");
    assert!(text.contains("1. line 1, column 1: a=1"), "{text}");
    assert!(text.contains("group key: a"), "{text}");
    assert!(text.contains("2. line 2, column 3: привет="), "{text}");
    assert!(text.contains("group 2: (no match)"), "{text}");
}

#[test]
fn regex_flags_work() {
    let plain = params(json!({ "pattern": "HELLO" }));
    assert_eq!(regex_test::run("hello", &plain).unwrap(), "No matches");
    let ignore = params(json!({ "pattern": "HELLO", "ignore_case": true }));
    assert!(regex_test::run("hello", &ignore).unwrap().starts_with("1 match"));
}

#[test]
fn replaces_with_groups() {
    let p = params(json!({ "pattern": r"(\w+) (\w+)", "replacing": true, "replacement": "$2 $1" }));
    assert_eq!(regex_test::run("hello world", &p).unwrap(), "world hello");
}

#[test]
fn explains_a_bad_pattern() {
    assert!(
        regex_test::run("x", &params(json!({ "pattern": "(" })))
            .unwrap_err()
            .contains("unclosed")
    );
    assert!(
        regex_test::run("x", &params(json!({})))
            .unwrap_err()
            .contains("enter a pattern")
    );
}

#[test]
fn a_pattern_that_breaks_other_engines_finishes_at_once() {
    let text = "a".repeat(50_000) + "!";
    let started = std::time::Instant::now();
    regex_test::run(&text, &params(json!({ "pattern": "(a+)+$" }))).unwrap();
    assert!(started.elapsed().as_secs() < 5, "took {:?}", started.elapsed());
}

#[test]
fn long_match_lists_are_cut_off() {
    let text = "a ".repeat(1500);
    let out = regex_test::run(&text, &params(json!({ "pattern": "a" }))).unwrap();
    assert!(
        out.starts_with("1500 matches") && out.ends_with("and 500 more"),
        "{}",
        &out[out.len() - 40..]
    );
}
