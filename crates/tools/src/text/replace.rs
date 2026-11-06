//! Find and Replace.

use regex::{NoExpand, RegexBuilder};
use saqviro_jobs::Params;

use crate::params;

const SIZE_LIMIT: usize = 16 * 1024 * 1024;

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    let find = params::text(params, "find");
    if find.is_empty() {
        return Err("enter the text to find".into());
    }
    let use_regex = params::flag(params, "use_regex");
    let pattern = if use_regex {
        find.to_owned()
    } else {
        regex::escape(find)
    };
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(params::flag(params, "ignore_case"))
        .size_limit(SIZE_LIMIT)
        .build()
        .map_err(|e| e.to_string())?;
    let replacement = params::text(params, "replace_with");
    Ok(if use_regex {
        regex.replace_all(text, replacement).into_owned()
    } else {
        regex.replace_all(text, NoExpand(replacement)).into_owned()
    })
}
