//! Timestamp Converter. Reads Unix time in seconds or milliseconds, or a
//! date; empty input means now.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
use saqviro_jobs::Params;

/// Unix times with more digits than this are milliseconds: as seconds they
/// would be past the year 5000.
const MAX_SECONDS_DIGITS: usize = 11;

/// Parses input. Dates without a zone are read as UTC.
pub fn parse(text: &str) -> Result<DateTime<Utc>, String> {
    let text = text.trim();
    let digits = text.strip_prefix('-').unwrap_or(text);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        let number: i64 = text.parse().map_err(|_| "this number is too large".to_owned())?;
        let when = if digits.len() > MAX_SECONDS_DIGITS {
            DateTime::from_timestamp_millis(number)
        } else {
            DateTime::from_timestamp(number, 0)
        };
        return when.ok_or_else(|| "this time is out of range".to_owned());
    }
    if let Ok(when) = DateTime::parse_from_rfc3339(text) {
        return Ok(when.with_timezone(&Utc));
    }
    for format in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(when) = NaiveDateTime::parse_from_str(text, format) {
            return Ok(Utc.from_utc_datetime(&when));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Ok(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).expect("midnight exists")));
    }
    Err(format!(
        "`{text}` is not a timestamp or a date, try 1730000000 or 2026-11-05 14:30"
    ))
}

pub fn describe(when: DateTime<Utc>) -> String {
    let local = when.with_timezone(&Local);
    format!(
        "Unix seconds: {}\nUnix milliseconds: {}\nUTC: {}\nLocal: {}",
        when.timestamp(),
        when.timestamp_millis(),
        when.format("%Y-%m-%dT%H:%M:%S%.3fZ"),
        local.format("%Y-%m-%d %H:%M:%S %:z")
    )
}

pub fn run(text: &str, _: &Params) -> Result<String, String> {
    let when = if text.trim().is_empty() {
        Utc::now()
    } else {
        parse(text)?
    };
    Ok(describe(when))
}
