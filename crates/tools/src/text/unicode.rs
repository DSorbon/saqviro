//! Unicode Inspector.

use std::fmt::Write;

use saqviro_jobs::Params;

const MAX_SHOWN: usize = 5000;

fn shown(c: char) -> String {
    if c.is_control()
        || c.is_whitespace()
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}')
    {
        "·".into()
    } else {
        c.to_string()
    }
}

pub fn run(text: &str, _: &Params) -> Result<String, String> {
    let mut out = String::new();
    let total = text.chars().count();
    for c in text.chars().take(MAX_SHOWN) {
        let mut utf8 = [0; 4];
        let bytes: Vec<String> = c.encode_utf8(&mut utf8).bytes().map(|b| format!("{b:02X}")).collect();
        let _ = writeln!(out, "U+{:04X}\t{}\t{}", c as u32, shown(c), bytes.join(" "));
    }
    if total > MAX_SHOWN {
        let _ = writeln!(out, "… and {} more", total - MAX_SHOWN);
    }
    Ok(out)
}
