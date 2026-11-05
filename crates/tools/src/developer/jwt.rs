//! JWT Decoder. Decodes only: checking a signature needs the key, so the
//! output says plainly that nothing has been verified.

use chrono::{DateTime, Utc};
use saqviro_jobs::Params;
use serde_json::Value;

use super::{
    base64::decode_bytes,
    json_format::{Indent, format},
};

fn section(part: &str, name: &str) -> Result<(String, Value), String> {
    let bytes = decode_bytes(part).map_err(|_| format!("the {name} is not valid Base64"))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("the {name} is not text"))?;
    let value = serde_json::from_str(&text).map_err(|_| format!("the {name} is not JSON"))?;
    Ok((format(&text, Indent::Spaces(2), false)?, value))
}

fn date(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(value.as_f64()? as i64, 0)
}

pub fn decode(token: &str, now: DateTime<Utc>) -> Result<String, String> {
    let token = token.trim();
    let token = token.strip_prefix("Bearer ").unwrap_or(token).trim();
    let parts: Vec<&str> = token.split('.').collect();
    let [header, payload, _signature] = parts.as_slice() else {
        return Err("a JWT has three parts separated by dots".into());
    };
    let (header_text, header) = section(header, "header")?;
    let (payload_text, payload) = section(payload, "payload")?;

    let mut notes = Vec::new();
    for (claim, label) in [("iat", "Issued"), ("nbf", "Valid from"), ("exp", "Expires")] {
        if let Some(when) = date(&payload[claim]) {
            notes.push(format!("{label}: {}", when.format("%Y-%m-%d %H:%M:%S UTC")));
        }
    }
    if date(&payload["exp"]).is_some_and(|exp| exp <= now) {
        notes.push("This token has expired.".to_owned());
    }
    if date(&payload["nbf"]).is_some_and(|nbf| nbf > now) {
        notes.push("This token is not valid yet.".to_owned());
    }
    if header["alg"]
        .as_str()
        .is_some_and(|alg| alg.eq_ignore_ascii_case("none"))
    {
        notes.push("This token has no signature (alg: none), so it proves nothing.".to_owned());
    }
    notes.push("The signature was not checked: decoding proves nothing about who made this token.".to_owned());

    Ok(format!(
        "Header\n{header_text}\n\nPayload\n{payload_text}\n\n{}",
        notes.join("\n")
    ))
}

pub fn run(text: &str, _: &Params) -> Result<String, String> {
    decode(text, Utc::now())
}
