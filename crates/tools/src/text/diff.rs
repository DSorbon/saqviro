//! Text Diff.

use std::fmt::Write;

use saqviro_jobs::Params;
use similar::{ChangeTag, TextDiff};

use crate::params;

/// Lines of two texts marked `-` for removed, `+` for added.
pub fn compare(old: &str, new: &str, only_changes: bool) -> String {
    let diff = TextDiff::from_lines(old, new);
    let (mut added, mut removed) = (0, 0);
    let mut body = String::new();
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => {
                removed += 1;
                '-'
            }
            ChangeTag::Insert => {
                added += 1;
                '+'
            }
            ChangeTag::Equal if only_changes => continue,
            ChangeTag::Equal => ' ',
        };
        let _ = writeln!(body, "{sign} {}", change.value().trim_end_matches(['\r', '\n']));
    }
    if added == 0 && removed == 0 {
        return "The texts are the same".into();
    }
    format!("{added} added, {removed} removed\n\n{body}")
}

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    let other = params::text(params, "changed_text");
    let only_changes = params::flag(params, "only_changes");
    if params::flag(params, "ignore_spaces") {
        let squeeze = |s: &str| {
            let lines = s.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "));
            lines.collect::<Vec<_>>().join("\n") + "\n"
        };
        return Ok(compare(&squeeze(text), &squeeze(other), only_changes));
    }
    Ok(compare(text, other, only_changes))
}
