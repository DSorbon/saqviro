//! Tools that clean up what copying from elsewhere leaves in a text.

use saqviro_jobs::Params;

use crate::params;

pub fn whitespace(text: &str, params: &Params) -> Result<String, String> {
    let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    if !params::flag(params, "keep_line_breaks") {
        return Ok(collapse(text));
    }
    let mut out: Vec<String> = Vec::new();
    for line in text.lines().map(collapse) {
        // One blank line can separate paragraphs; more say nothing.
        if !line.is_empty() || out.last().is_some_and(|l| !l.is_empty()) {
            out.push(line);
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    Ok(out.join("\n"))
}

fn is_invisible(c: char) -> bool {
    matches!(c,
        '\u{00AD}' | '\u{034F}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{206F}' | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}' | '\u{E0000}'..='\u{E007F}')
        || (c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

/// Emoji such as 👩‍💻 are held together by a zero-width joiner.
fn joins_emoji(before: Option<char>, after: Option<char>) -> bool {
    let pictograph = |c: Option<char>| c.is_some_and(|c| c >= '\u{2190}' && !is_invisible(c));
    pictograph(before) && pictograph(after)
}

pub fn invisible(text: &str, _: &Params) -> Result<String, String> {
    let chars: Vec<char> = text.chars().collect();
    let kept = chars.iter().enumerate().filter(|&(i, &c)| {
        let before = i.checked_sub(1).and_then(|i| chars.get(i)).copied();
        let after = chars.get(i + 1).copied();
        // A variation selector may stand between the emoji and the joiner.
        let before = before
            .filter(|c| *c != '\u{FE0F}')
            .or(i.checked_sub(2).and_then(|i| chars.get(i)).copied());
        !is_invisible(c) || (c == '\u{200D}' && joins_emoji(before, after))
    });
    Ok(kept.map(|(_, c)| *c).collect())
}
