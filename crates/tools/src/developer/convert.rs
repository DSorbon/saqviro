//! Conversions between data formats.

use std::collections::{BTreeSet, HashMap, HashSet};

use saqviro_jobs::Params;
use serde_json::{Map, Value};

use crate::{params, text::diff};

fn parse_json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("this is not valid JSON: {e}"))
}

fn pretty(value: &Value) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| e.to_string())
}

pub fn json_to_yaml(text: &str, _: &Params) -> Result<String, String> {
    serde_saphyr::to_string(&parse_json(text)?).map_err(|e| e.to_string())
}

pub fn yaml_to_json(text: &str, _: &Params) -> Result<String, String> {
    let value: Value = serde_saphyr::from_str(text).map_err(|e| format!("this is not valid YAML: {e}"))?;
    pretty(&value)
}

fn guess_delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or_default();
    let count = |d: char| first.matches(d).count();
    b",;\t"
        .iter()
        .copied()
        .max_by_key(|d| count(*d as char))
        .unwrap_or(b',')
}

fn delimiter(params: &Params, text: &str) -> u8 {
    match params::text(params, "delimiter") {
        "semicolon" => b';',
        "tab" => b'\t',
        "comma" => b',',
        _ => guess_delimiter(text),
    }
}

/// Numbers with a leading zero stay text: `007` is a code, not seven.
fn typed(cell: &str) -> Value {
    let looks_padded = cell.len() > 1 && cell.starts_with('0') && !cell.starts_with("0.");
    match cell {
        "" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ if looks_padded => Value::String(cell.into()),
        // A number JSON cannot hold exactly stays as written.
        _ => match serde_json::from_str::<serde_json::Number>(cell) {
            Ok(number) if number.to_string() == cell => Value::Number(number),
            _ => Value::String(cell.into()),
        },
    }
}

/// Headers with repeated names told apart, so no column overwrites another.
fn column_names(headers: &csv::StringRecord) -> Vec<String> {
    let mut seen = HashSet::new();
    headers
        .iter()
        .map(|name| {
            let mut unique = name.to_owned();
            let mut n = 2;
            while !seen.insert(unique.clone()) {
                unique = format!("{name}_{n}");
                n += 1;
            }
            unique
        })
        .collect()
}

pub fn csv_to_json(text: &str, params: &Params) -> Result<String, String> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter(params, text))
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers = column_names(reader.headers().map_err(|e| e.to_string())?);
    let convert = params::flag(params, "typed_values");
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|e| e.to_string())?;
        let row: Map<String, Value> = record
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let value = if convert {
                    typed(cell)
                } else {
                    Value::String(cell.into())
                };
                // A row longer than the header keeps its extra cells.
                let key = headers.get(i).cloned().unwrap_or_else(|| format!("column_{}", i + 1));
                (key, value)
            })
            .collect();
        rows.push(Value::Object(row));
    }
    pretty(&Value::Array(rows))
}

fn cell(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

pub fn json_to_csv(text: &str, params: &Params) -> Result<String, String> {
    let value = parse_json(text)?;
    let rows = match &value {
        Value::Array(rows) => rows.as_slice(),
        Value::Object(_) => std::slice::from_ref(&value),
        _ => return Err("give an array of objects".into()),
    };
    let mut columns: Vec<&str> = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let Value::Object(row) = row else {
            return Err("every item of the array must be an object".into());
        };
        for key in row.keys() {
            if seen.insert(key.as_str()) {
                columns.push(key);
            }
        }
    }
    let mut writer = csv::WriterBuilder::new()
        .delimiter(delimiter(params, ","))
        .from_writer(Vec::new());
    writer.write_record(&columns).map_err(|e| e.to_string())?;
    for row in rows {
        writer
            .write_record(columns.iter().map(|c| cell(row.get(c))))
            .map_err(|e| e.to_string())?;
    }
    let bytes = writer.into_inner().map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// `user_name` → `UserName`.
fn type_name(key: &str) -> String {
    let words = key.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty());
    let name: String = words
        .map(|w| {
            let mut chars = w.chars();
            let first = chars.next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
            first + chars.as_str()
        })
        .collect();
    match name.chars().next() {
        Some(c) if !c.is_ascii_digit() => name,
        _ => format!("Type{name}"),
    }
}

fn property(key: &str) -> String {
    let plain = key
        .chars()
        .enumerate()
        .all(|(i, c)| c == '_' || c == '$' || c.is_alphabetic() || (i > 0 && c.is_numeric()));
    if plain && !key.is_empty() {
        key.into()
    } else {
        Value::String(key.into()).to_string()
    }
}

#[derive(Default)]
struct Types {
    /// Finished declarations; nested types come first.
    done: Vec<String>,
    names: HashSet<String>,
    /// The last number given to each wanted name.
    used: HashMap<String, usize>,
}

impl Types {
    fn free_name(&mut self, wanted: &str) -> String {
        let mut name = wanted.to_owned();
        let n = self.used.entry(wanted.to_owned()).or_insert(1);
        while !self.names.insert(name.clone()) {
            *n += 1;
            name = format!("{wanted}{n}");
        }
        name
    }

    /// One interface for objects that are the same kind of thing: a key
    /// missing from some of them is optional.
    fn interface(&mut self, wanted: &str, objects: &[&Map<String, Value>]) -> String {
        let name = self.free_name(wanted);
        let mut keys: Vec<&String> = Vec::new();
        let mut seen = HashSet::new();
        for object in objects {
            for key in object.keys() {
                if seen.insert(key) {
                    keys.push(key);
                }
            }
        }
        let mut body = String::new();
        for key in keys {
            let values: Vec<&Value> = objects.iter().filter_map(|o| o.get(key)).collect();
            let optional = if values.len() < objects.len() { "?" } else { "" };
            let shape = self.merged(&type_name(key), &values);
            body.push_str(&format!("  {}{optional}: {shape};\n", property(key)));
        }
        self.done.push(format!("export interface {name} {{\n{body}}}\n"));
        name
    }

    fn merged(&mut self, name: &str, values: &[&Value]) -> String {
        let mut shapes = BTreeSet::new();
        let objects: Vec<_> = values.iter().filter_map(|v| v.as_object()).collect();
        if !objects.is_empty() {
            shapes.insert(self.interface(name, &objects));
        }
        let items: Vec<&Value> = values.iter().filter_map(|v| v.as_array()).flatten().collect();
        if values.iter().any(|v| v.is_array()) {
            let inner = if items.is_empty() {
                "unknown".to_owned()
            } else {
                self.merged(&format!("{name}Item"), &items)
            };
            shapes.insert(if inner.contains(" | ") {
                format!("({inner})[]")
            } else {
                format!("{inner}[]")
            });
        }
        for value in values {
            match value {
                Value::Null => shapes.insert("null".into()),
                Value::Bool(_) => shapes.insert("boolean".into()),
                Value::Number(_) => shapes.insert("number".into()),
                Value::String(_) => shapes.insert("string".into()),
                _ => false,
            };
        }
        shapes.into_iter().collect::<Vec<_>>().join(" | ")
    }
}

pub fn json_to_ts(text: &str, params: &Params) -> Result<String, String> {
    let value = parse_json(text)?;
    let name = type_name(params::text(params, "name"));
    let name = if name == "Type" { "Root".to_owned() } else { name };
    let mut types = Types::default();
    let root = types.merged(&name, &[&value]);
    if root != name {
        types.done.push(format!("export type {name} = {root};\n"));
    }
    Ok(types.done.join("\n"))
}

/// Sorted keys and one layout, so only real differences show.
fn canonical(text: &str) -> Result<String, String> {
    use super::json_format::{Indent, format};
    format(text, Indent::Spaces(2), true)
}

pub fn json_diff(text: &str, params: &Params) -> Result<String, String> {
    let old = canonical(text).map_err(|e| format!("first JSON: {e}"))?;
    let new = canonical(params::text(params, "changed_json")).map_err(|e| format!("second JSON: {e}"))?;
    Ok(diff::compare(&old, &new, false))
}
