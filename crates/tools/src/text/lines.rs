//! Tools that work on a text line by line.

use std::{cmp::Ordering, collections::HashSet};

use saqviro_jobs::Params;

use crate::params;

/// Lines joined back with `\n`. A final line break is kept if there was one.
fn rebuild<'a>(text: &str, lines: impl Iterator<Item = &'a str>) -> String {
    let mut out = lines.collect::<Vec<_>>().join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    out
}

pub fn dedupe(text: &str, params: &Params) -> Result<String, String> {
    let ignore_case = params::flag(params, "ignore_case");
    let ignore_spaces = params::flag(params, "ignore_spaces");
    let mut seen = HashSet::new();
    let kept = text.lines().filter(|line| {
        let key = if ignore_spaces { line.trim() } else { line };
        seen.insert(if ignore_case {
            key.to_lowercase()
        } else {
            key.to_owned()
        })
    });
    Ok(rebuild(text, kept))
}

/// Compares runs of digits as numbers, so `file2` comes before `file10`.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a, b);
    loop {
        let (Some(x), Some(y)) = (a.chars().next(), b.chars().next()) else {
            return a.len().cmp(&b.len());
        };
        if x.is_ascii_digit() && y.is_ascii_digit() {
            let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
            let (na, nb) = (digits(a), digits(b));
            let (da, db) = (a[..na].trim_start_matches('0'), b[..nb].trim_start_matches('0'));
            let order = da.len().cmp(&db.len()).then_with(|| da.cmp(db));
            if order != Ordering::Equal {
                return order;
            }
            (a, b) = (&a[na..], &b[nb..]);
        } else if x != y {
            return x.cmp(&y);
        } else {
            (a, b) = (&a[x.len_utf8()..], &b[y.len_utf8()..]);
        }
    }
}

/// The first number on a line. Lines without one sort last.
fn leading_number(line: &str) -> Option<f64> {
    let start = line.find(|c: char| c.is_ascii_digit() || c == '-')?;
    let rest = &line[start..];
    let end = rest
        .char_indices()
        .skip(1)
        .find(|(_, c)| !c.is_ascii_digit() && *c != '.')
        .map_or(rest.len(), |(i, _)| i);
    rest[..end].parse().ok()
}

pub fn sort(text: &str, params: &Params) -> Result<String, String> {
    let fold = |s: &str| {
        if params::flag(params, "ignore_case") {
            s.to_lowercase()
        } else {
            s.to_owned()
        }
    };
    let mut lines: Vec<&str> = text.lines().collect();
    match params::text(params, "by") {
        "length" => lines.sort_by_key(|l| l.chars().count()),
        "number" => lines.sort_by(|a, b| match (leading_number(a), leading_number(b)) {
            (Some(x), Some(y)) => x.total_cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }),
        "text" => lines.sort_by_cached_key(|l| fold(l)),
        _ => lines.sort_by(|a, b| natural(&fold(a), &fold(b))),
    }
    if params::flag(params, "descending") {
        lines.reverse();
    }
    Ok(rebuild(text, lines.into_iter()))
}

pub fn reverse(text: &str, _: &Params) -> Result<String, String> {
    Ok(rebuild(text, text.lines().rev()))
}

pub fn remove_empty(text: &str, _: &Params) -> Result<String, String> {
    Ok(rebuild(text, text.lines().filter(|l| !l.trim().is_empty())))
}

pub fn trim(text: &str, params: &Params) -> Result<String, String> {
    let side = params::text(params, "side");
    Ok(rebuild(
        text,
        text.lines().map(|l| match side {
            "start" => l.trim_start(),
            "end" => l.trim_end(),
            _ => l.trim(),
        }),
    ))
}
