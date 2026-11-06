//! Cron Parser, for the five standard fields.

use std::fmt::Write;

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike};
use saqviro_jobs::Params;

use crate::params;

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
/// Nothing further ahead is worth searching: some dates never come.
const SEARCH_DAYS: i64 = 366 * 9;

struct Field {
    allowed: Vec<bool>,
    any: bool,
}

impl Field {
    fn has(&self, value: u32) -> bool {
        self.allowed.get(value as usize).copied().unwrap_or(false)
    }
}

fn number(text: &str, names: &[&str], first: u32) -> Option<u32> {
    let lower = text.to_lowercase();
    match names.iter().position(|n| *n == lower) {
        Some(i) => Some(i as u32 + first),
        None => text.parse().ok(),
    }
}

fn field(text: &str, label: &str, min: u32, max: u32, names: &[&str], first: u32) -> Result<Field, String> {
    let wrong = || format!("{label}: `{text}` is not valid");
    let mut allowed = vec![false; max as usize + 1];
    for part in text.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => (range, step.parse().ok().filter(|s| *s > 0).ok_or_else(wrong)?),
            None => (part, 1),
        };
        let (from, to) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            let a = number(a, names, first).ok_or_else(wrong)?;
            (a, number(b, names, first).ok_or_else(wrong)?)
        } else {
            let at = number(range, names, first).ok_or_else(wrong)?;
            (at, if part.contains('/') { max } else { at })
        };
        if from < min || to > max || from > to {
            return Err(format!("{label}: `{part}` must be within {min}–{max}"));
        }
        for value in (from..=to).step_by(step as usize) {
            allowed[value as usize] = true;
        }
    }
    Ok(Field {
        allowed,
        any: text.starts_with('*'),
    })
}

pub struct Schedule {
    minute: Field,
    hour: Field,
    day: Field,
    month: Field,
    weekday: Field,
}

impl Schedule {
    pub fn parse(text: &str) -> Result<Self, String> {
        let fields: Vec<&str> = text.split_whitespace().collect();
        let [minute, hour, day, month, weekday] = fields[..] else {
            return Err(format!("a cron expression has 5 fields, this has {}", fields.len()));
        };
        let mut weekday = field(weekday, "day of week", 0, 7, &DAYS, 0)?;
        // Both 0 and 7 mean Sunday.
        weekday.allowed[0] |= weekday.allowed[7];
        Ok(Self {
            minute: field(minute, "minute", 0, 59, &[], 0)?,
            hour: field(hour, "hour", 0, 23, &[], 0)?,
            day: field(day, "day of month", 1, 31, &[], 0)?,
            month: field(month, "month", 1, 12, &MONTHS, 1)?,
            weekday,
        })
    }

    /// With both day fields set, cron runs when either one matches.
    fn runs_on<T: Datelike>(&self, date: &T) -> bool {
        let day = self.day.has(date.day());
        let weekday = self.weekday.has(date.weekday().num_days_from_sunday());
        self.month.has(date.month())
            && match (self.day.any, self.weekday.any) {
                (false, false) => day || weekday,
                _ => day && weekday,
            }
    }

    /// The first times after `from`, at most `count`.
    pub fn next_runs<Z: TimeZone>(&self, from: DateTime<Z>, count: usize) -> Vec<DateTime<Z>> {
        let mut found = Vec::new();
        let start = from.clone();
        let mut day = from.clone();
        for _ in 0..SEARCH_DAYS {
            if self.runs_on(&day) {
                for hour in (0..24).filter(|h| self.hour.has(*h)) {
                    for minute in (0..60).filter(|m| self.minute.has(*m)) {
                        let time = day
                            .with_hour(hour)
                            .and_then(|t| t.with_minute(minute))
                            .and_then(|t| t.with_second(0));
                        let Some(time) = time.and_then(|t| t.with_nanosecond(0)) else {
                            continue;
                        };
                        if time > start {
                            found.push(time);
                            if found.len() == count {
                                return found;
                            }
                        }
                    }
                }
            }
            day += Duration::days(1);
        }
        found
    }
}

pub fn run(text: &str, params: &Params) -> Result<String, String> {
    let schedule = Schedule::parse(text)?;
    let runs = schedule.next_runs(Local::now(), params::number(params, "runs", 5).max(1) as usize);
    if runs.is_empty() {
        return Ok("This expression never runs".into());
    }
    let mut out = String::from("Next runs, in local time\n");
    for run in runs {
        let _ = writeln!(out, "{}", run.format("%a %Y-%m-%d %H:%M"));
    }
    Ok(out)
}
