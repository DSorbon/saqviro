//! Markdown to HTML.

use pulldown_cmark::{Options, Parser, html};
use saqviro_jobs::Params;

pub fn run(text: &str, _: &Params) -> Result<String, String> {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(text, options));
    Ok(out)
}
