//! Reading resolved tool parameters.

use saqviro_jobs::Params;
use serde_json::Value;

/// A text value exactly as entered.
pub fn text<'a>(params: &'a Params, id: &str) -> &'a str {
    params.get(id).and_then(Value::as_str).unwrap_or_default()
}

pub fn flag(params: &Params, id: &str) -> bool {
    params.get(id).and_then(Value::as_bool).unwrap_or(false)
}

pub fn number(params: &Params, id: &str, default: u32) -> u32 {
    params
        .get(id)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(default)
}
