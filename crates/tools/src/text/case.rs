//! Case Converter.

use saqviro_jobs::Params;

use crate::params;

/// Splits an identifier or phrase into words: at anything that is not a
/// letter or digit, at `aB`, and at the end of an acronym as in `HTTPServer`.
pub fn split_words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut word = String::new();
    for (i, c) in chars.iter().copied().enumerate() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            continue;
        }
        let previous = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1).copied();
        let after_lower = previous.is_some_and(|p| p.is_lowercase() || p.is_numeric()) && c.is_uppercase();
        let ends_acronym =
            previous.is_some_and(char::is_uppercase) && c.is_uppercase() && next.is_some_and(char::is_lowercase);
        if (after_lower || ends_acronym) && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        word.push(c);
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// Keeps spacing and punctuation; capitalizes each word.
fn title(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_start = true;
    for c in text.chars() {
        if at_start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        at_start = c.is_whitespace();
    }
    out
}

/// Keeps spacing and punctuation; capitalizes each sentence.
fn sentence(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_start = true;
    for c in text.chars() {
        if at_start && c.is_alphanumeric() {
            out.extend(c.to_uppercase());
            at_start = false;
        } else {
            out.extend(c.to_lowercase());
        }
        if matches!(c, '.' | '!' | '?' | '…') {
            at_start = true;
        }
    }
    out
}

fn joined(text: &str, separator: &str, change: impl Fn(&str) -> String) -> String {
    split_words(text)
        .iter()
        .map(|w| change(w))
        .collect::<Vec<_>>()
        .join(separator)
}

pub fn convert(text: &str, case: &str) -> Result<String, String> {
    Ok(match case {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        "title" => title(text),
        "sentence" => sentence(text),
        "camel" => split_words(text)
            .iter()
            .enumerate()
            .map(|(i, w)| if i == 0 { w.to_lowercase() } else { capitalized(w) })
            .collect(),
        "pascal" => joined(text, "", capitalized),
        "snake" => joined(text, "_", str::to_lowercase),
        "kebab" => joined(text, "-", str::to_lowercase),
        "constant" => joined(text, "_", str::to_uppercase),
        other => return Err(format!("unknown case `{other}`")),
    })
}

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    convert(text, params::text(params, "case"))
}
