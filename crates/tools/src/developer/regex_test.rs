//! Regex Tester. The `regex` crate runs in linear time, so no pattern can
//! hang the app, but it has no lookaround or backreferences.

use regex::{Regex, RegexBuilder};
use saqviro_jobs::Params;

use crate::params;

const MAX_SHOWN: usize = 1000;
const SIZE_LIMIT: usize = 16 * 1024 * 1024;

fn compile(params: &Params) -> Result<Regex, String> {
    let pattern = params::text(params, "pattern");
    if pattern.is_empty() {
        return Err("enter a pattern".into());
    }
    RegexBuilder::new(pattern)
        .case_insensitive(params::flag(params, "ignore_case"))
        .multi_line(params::flag(params, "multi_line"))
        .dot_matches_new_line(params::flag(params, "dot_all"))
        .size_limit(SIZE_LIMIT)
        .build()
        .map_err(|e| e.to_string())
}

/// Line and column of a byte offset, both from 1.
fn position(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |i| i + 1);
    (line, before[start..].chars().count() + 1)
}

fn find_all(regex: &Regex, text: &str) -> String {
    let total = regex.find_iter(text).count();
    if total == 0 {
        return "No matches".into();
    }
    let mut lines = vec![if total == 1 {
        "1 match".to_owned()
    } else {
        format!("{total} matches")
    }];
    for (i, captures) in regex.captures_iter(text).take(MAX_SHOWN).enumerate() {
        let whole = captures.get(0).expect("group 0 is the whole match");
        let (line, column) = position(text, whole.start());
        lines.push(format!("\n{}. line {line}, column {column}: {}", i + 1, whole.as_str()));
        for (n, name) in regex.capture_names().enumerate().skip(1) {
            let label = name.map_or_else(|| n.to_string(), str::to_owned);
            match captures.get(n) {
                Some(group) => lines.push(format!("   group {label}: {}", group.as_str())),
                None => lines.push(format!("   group {label}: (no match)")),
            }
        }
    }
    if total > MAX_SHOWN {
        lines.push(format!("\nand {} more", total - MAX_SHOWN));
    }
    lines.join("\n")
}

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    let regex = compile(params)?;
    Ok(if params::flag(params, "replacing") {
        regex
            .replace_all(text, params::text(params, "replacement"))
            .into_owned()
    } else {
        find_all(&regex, text)
    })
}
