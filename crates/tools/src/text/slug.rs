//! Slug Generator. Other scripts are spelled out in Latin letters first, so
//! `Привет, мир!` becomes `privet-mir`.

use saqviro_jobs::Params;

use crate::params;

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    let separator = if params::text(params, "separator") == "_" {
        "_"
    } else {
        "-"
    };
    let latin = deunicode::deunicode(text).to_lowercase();
    let words = latin
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty());
    let mut slug = String::new();
    let limit = params::number(params, "max_length", u32::MAX) as usize;
    for word in words {
        let extra = if slug.is_empty() { 0 } else { separator.len() };
        if slug.len() + extra + word.len() > limit {
            if slug.is_empty() {
                slug.push_str(&word[..limit]);
            }
            break;
        }
        if extra > 0 {
            slug.push_str(separator);
        }
        slug.push_str(word);
    }
    if slug.is_empty() {
        return Err("there is nothing to make a slug from".into());
    }
    Ok(slug)
}
