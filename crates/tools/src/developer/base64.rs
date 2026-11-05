//! Base64 Encode / Decode.

use base64::{
    Engine,
    alphabet::{STANDARD, URL_SAFE},
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use saqviro_jobs::Params;

use crate::params;

fn engine(url_safe: bool, padded: bool) -> GeneralPurpose {
    let config = GeneralPurposeConfig::new()
        .with_encode_padding(padded)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent);
    GeneralPurpose::new(if url_safe { &URL_SAFE } else { &STANDARD }, config)
}

pub fn encode(text: &str, url_safe: bool) -> String {
    engine(url_safe, !url_safe).encode(text)
}

/// Decodes either alphabet, with or without padding, ignoring line breaks.
pub fn decode_bytes(text: &str) -> Result<Vec<u8>, String> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let url_safe = clean.contains(['-', '_']);
    engine(url_safe, false)
        .decode(&clean)
        .map_err(|e| format!("this is not valid Base64: {e}"))
}

pub fn decode(text: &str) -> Result<String, String> {
    String::from_utf8(decode_bytes(text)?)
        .map_err(|_| "the decoded data is binary, not text, so it cannot be shown here".into())
}

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    match params::text(params, "mode") {
        "decode" => decode(text),
        _ => Ok(encode(text, params::flag(params, "url_safe"))),
    }
}
