//! XML Formatter.

use quick_xml::{
    Reader, Writer, XmlVersion,
    escape::escape,
    events::{BytesStart, Event},
};
use saqviro_jobs::Params;
use serde_json::{Map, Value};

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

fn read_attributes(start: &BytesStart<'_>, node: &mut Map<String, Value>) -> Result<(), String> {
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|e| e.to_string())?;
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?;
        node.insert(
            format!("@{}", attribute.key.into_inner()),
            Value::String(value.into_owned()),
        );
    }
    Ok(())
}

/// What `&name;` stands for. Entities a document defines itself stay as they
/// are: expanding them is how XML bombs work.
fn entity_text(name: &str) -> String {
    let number = name.strip_prefix('#').and_then(|n| match n.strip_prefix('x') {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => n.parse().ok(),
    });
    match (name, number.and_then(char::from_u32)) {
        (_, Some(c)) => c.to_string(),
        ("amp", _) => "&".into(),
        ("lt", _) => "<".into(),
        ("gt", _) => ">".into(),
        ("quot", _) => "\"".into(),
        ("apos", _) => "'".into(),
        _ => format!("&{name};"),
    }
}

/// An element as JSON: attributes as `@name`, text as `#text`, and repeated
/// children as arrays. An element with text alone is just that text.
fn element(start: &BytesStart<'_>, reader: &mut Reader<&[u8]>, depth: usize) -> Result<Value, String> {
    if depth > 256 {
        return Err("this XML is nested too deeply".into());
    }
    let mut node = Map::new();
    read_attributes(start, &mut node)?;
    let mut text = String::new();
    loop {
        match reader.read_event().map_err(|e| format!("this is not valid XML: {e}"))? {
            Event::Start(child) => {
                let child = child.into_owned();
                let value = element(&child, reader, depth + 1)?;
                add_child(&mut node, &child, value);
            }
            Event::Empty(child) => {
                let child = child.into_owned();
                let value = attributes_only(&child)?;
                add_child(&mut node, &child, value);
            }
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::CData(t) => text.push_str(&t.xml10_content()),
            Event::GeneralRef(entity) => text.push_str(&entity_text(&entity.xml10_content())),
            Event::End(_) => break,
            Event::Eof => return Err("this XML has an element that is never closed".into()),
            _ => {}
        }
    }
    let text = text.trim().to_owned();
    if node.is_empty() {
        return Ok(if text.is_empty() {
            Value::Null
        } else {
            Value::String(text)
        });
    }
    if !text.is_empty() {
        node.insert("#text".into(), Value::String(text));
    }
    Ok(Value::Object(node))
}

fn attributes_only(start: &BytesStart<'_>) -> Result<Value, String> {
    let mut node = Map::new();
    read_attributes(start, &mut node)?;
    Ok(if node.is_empty() {
        Value::Null
    } else {
        Value::Object(node)
    })
}

fn add_child(node: &mut Map<String, Value>, start: &BytesStart<'_>, value: Value) {
    let name = start.name().into_inner().to_owned();
    match node.get_mut(&name) {
        Some(Value::Array(items)) => items.push(value),
        Some(first) => *first = Value::Array(vec![first.take(), value]),
        None => {
            node.insert(name, value);
        }
    }
}

pub fn to_json(text: &str, _: &Params) -> Result<String, String> {
    let mut reader = Reader::from_str(text);
    loop {
        let root = match reader.read_event().map_err(|e| format!("this is not valid XML: {e}"))? {
            Event::Start(start) => {
                let start = start.into_owned();
                (start.clone(), element(&start, &mut reader, 0)?)
            }
            Event::Empty(start) => {
                let start = start.into_owned();
                (start.clone(), attributes_only(&start)?)
            }
            Event::Eof => return Err("there is no XML element here".into()),
            _ => continue,
        };
        let mut out = Map::new();
        add_child(&mut out, &root.0, root.1);
        return serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string());
    }
}

fn is_name(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))
}

fn write_value(name: &str, value: &Value, level: usize, out: &mut String) -> Result<(), String> {
    if !is_name(name) {
        return Err(format!("`{name}` cannot be an XML element name"));
    }
    let pad = "  ".repeat(level);
    match value {
        Value::Array(items) => {
            for item in items {
                write_value(name, item, level, out)?;
            }
        }
        Value::Object(members) => {
            let mut open = format!("{pad}<{name}");
            let mut text = None;
            let mut children = Vec::new();
            for (key, member) in members {
                if let Some(attribute) = key.strip_prefix('@') {
                    let shown = member.as_str().map_or_else(|| member.to_string(), str::to_owned);
                    open.push_str(&format!(" {attribute}=\"{}\"", escape(shown.as_str())));
                } else if key == "#text" {
                    text = Some(member);
                } else {
                    children.push((key, member));
                }
            }
            if children.is_empty() && text.is_none() {
                out.push_str(&format!("{open}/>\n"));
                return Ok(());
            }
            out.push_str(&format!("{open}>"));
            if let Some(text) = text {
                let shown = text.as_str().map_or_else(|| text.to_string(), str::to_owned);
                out.push_str(&escape(shown.as_str()));
            }
            if !children.is_empty() {
                out.push('\n');
                for (key, member) in children {
                    write_value(key, member, level + 1, out)?;
                }
                out.push_str(&pad);
            }
            out.push_str(&format!("</{name}>\n"));
        }
        Value::Null => out.push_str(&format!("{pad}<{name}/>\n")),
        Value::String(text) => out.push_str(&format!("{pad}<{name}>{}</{name}>\n", escape(text.as_str()))),
        other => out.push_str(&format!("{pad}<{name}>{other}</{name}>\n")),
    }
    Ok(())
}

pub fn from_json(text: &str, params: &Params) -> Result<String, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("this is not valid JSON: {e}"))?;
    let mut out = String::new();
    let root = params::text(params, "root");
    let root = if root.is_empty() { "root" } else { root };
    match &value {
        // One key at the top is the root already, as XML to JSON writes it.
        Value::Object(members) if members.len() == 1 && !members.values().all(Value::is_array) => {
            let (name, member) = members.iter().next().ok_or("the JSON is empty")?;
            write_value(name, member, 0, &mut out)?;
        }
        Value::Array(_) => {
            out.push_str(&format!("<{root}>\n"));
            write_value("item", &value, 1, &mut out)?;
            out.push_str(&format!("</{root}>\n"));
        }
        other => write_value(root, other, 0, &mut out)?,
    }
    Ok(out)
}
