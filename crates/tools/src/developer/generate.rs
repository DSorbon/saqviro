//! Generators and small encoders.

use std::fmt::Write;

use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use rand::{Rng, seq::SliceRandom};
use saqviro_jobs::Params;
use sha1::Sha1;
use sha2::{Sha256, Sha512};

use crate::params;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?/";
const SIMILAR: &str = "Il1O0o|";

/// A password with at least one character of every chosen kind.
fn password(rng: &mut impl Rng, kinds: &[Vec<char>], length: usize) -> String {
    let all: Vec<char> = kinds.concat();
    let mut chars: Vec<char> = kinds.iter().map(|k| k[rng.random_range(0..k.len())]).collect();
    while chars.len() < length {
        chars.push(all[rng.random_range(0..all.len())]);
    }
    chars.shuffle(rng);
    chars.into_iter().collect()
}

pub fn passwords(_: &str, params: &Params) -> Result<String, String> {
    let keep = |set: &str| -> Vec<char> {
        let avoid = params::flag(params, "avoid_similar");
        set.chars().filter(|c| !avoid || !SIMILAR.contains(*c)).collect()
    };
    let mut kinds = vec![keep(LOWER)];
    for (id, set) in [("uppercase", UPPER), ("digits", DIGITS), ("symbols", SYMBOLS)] {
        if params::flag(params, id) {
            kinds.push(keep(set));
        }
    }
    let length = (params::number(params, "length", 20) as usize).max(kinds.len());
    let mut rng = rand::rng();
    let made: Vec<String> = (0..params::number(params, "count", 1).max(1))
        .map(|_| password(&mut rng, &kinds, length))
        .collect();
    Ok(made.join("\n"))
}

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 48 bits of time then 80 random bits, in 26 characters.
pub fn encode_ulid(millis: u64, random: u128) -> String {
    let value = (u128::from(millis) << 80) | (random & ((1 << 80) - 1));
    (0..26)
        .rev()
        .map(|i| CROCKFORD[((value >> (i * 5)) & 31) as usize] as char)
        .collect()
}

pub fn ulids(_: &str, params: &Params) -> Result<String, String> {
    let millis = chrono::Utc::now().timestamp_millis() as u64;
    let mut rng = rand::rng();
    let made: Vec<String> = (0..params::number(params, "count", 1).max(1))
        .map(|_| encode_ulid(millis, rng.random()))
        .collect();
    Ok(made.join("\n"))
}

fn sign<M: Mac + hmac::digest::KeyInit>(key: &[u8], text: &str) -> Vec<u8> {
    let mut mac = <M as hmac::digest::KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(text.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

pub fn hmac(text: &str, params: &Params) -> Result<String, String> {
    let key = params::text(params, "key").as_bytes();
    if key.is_empty() {
        return Err("enter a key".into());
    }
    let bytes = match params::text(params, "algorithm") {
        "sha1" => sign::<Hmac<Sha1>>(key, text),
        "sha512" => sign::<Hmac<Sha512>>(key, text),
        _ => sign::<Hmac<Sha256>>(key, text),
    };
    if params::text(params, "output") == "base64" {
        return Ok(STANDARD.encode(bytes));
    }
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

pub fn regex_escape(text: &str, _: &Params) -> Result<String, String> {
    Ok(regex::escape(text))
}

/// Base prefixes and separators people paste along with a number.
fn parse_number(text: &str, base: Option<u32>) -> Result<u128, String> {
    let clean: String = text.trim().chars().filter(|c| !matches!(c, '_' | ' ' | ',')).collect();
    let lower = clean.to_lowercase();
    let (digits, radix) = match (base, lower.get(..2)) {
        (Some(16) | None, Some("0x")) => (&lower[2..], 16),
        (Some(2) | None, Some("0b")) => (&lower[2..], 2),
        (Some(8) | None, Some("0o")) => (&lower[2..], 8),
        (Some(radix), _) => (lower.as_str(), radix),
        (None, _) => (lower.as_str(), 10),
    };
    u128::from_str_radix(digits, radix).map_err(|e| match e.kind() {
        std::num::IntErrorKind::PosOverflow => "this number is too large (the limit is 128 bits)".to_owned(),
        _ => format!("`{}` is not a whole number in base {radix}", text.trim()),
    })
}

pub fn number_base(text: &str, params: &Params) -> Result<String, String> {
    let n = parse_number(text, params::text(params, "base").parse().ok())?;
    Ok(format!("Binary: {n:b}\nOctal: {n:o}\nDecimal: {n}\nHex: {n:X}"))
}
