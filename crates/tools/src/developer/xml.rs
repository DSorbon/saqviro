//! XML Formatter.

use quick_xml::{Reader, Writer, events::Event};
use saqviro_jobs::Params;

use crate::params;

pub fn format(text: &str, params: &Params) -> Result<String, String> {
    let size = if params::text(params, "indent") == "4" { 4 } else { 2 };
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', size);
    let mut depth = 0i64;
    let mut seen = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("this is not valid XML near byte {}: {e}", reader.error_position()))?;
        match &event {
            Event::Eof => break,
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
        seen |= matches!(event, Event::Start(_) | Event::Empty(_));
        writer.write_event(event).map_err(|e| e.to_string())?;
    }
    if !seen {
        return Err("there is no XML element here".into());
    }
    if depth != 0 {
        return Err("this XML has an element that is never closed".into());
    }
    String::from_utf8(writer.into_inner()).map_err(|e| e.to_string())
}
