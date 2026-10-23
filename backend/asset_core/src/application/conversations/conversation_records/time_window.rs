use crate::backend::application::prelude::*;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// Unified time resolution: accepts 1 or 2 inputs from a single `--time` flag.
/// Modes:
/// 1. Single pure integer (e.g. "24") -> Length in hours (Mode 2: duration).
/// 2. Single datetime (e.g. "2026-10-13") -> Start time A to now (Mode 1: A to now).
/// 3. Two datetimes (e.g. "2026-10-13", "2026-10-20") or range string (e.g. "2026-10-13..2026-10-20") -> Endpoints [A, B] (Mode 3: two endpoints).
pub fn resolve_unified_time(
    inputs: &[&str],
) -> AppResult<(Option<String>, Option<String>, Option<String>)> {
    let now = Utc::now();
    let cleaned: Vec<&str> = inputs
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if cleaned.is_empty() {
        return Ok((None, None, None));
    }

    if cleaned.len() == 1 {
        let single = cleaned[0];

        // 1. Single pure integer -> default unit is Hours (H)
        if let Ok(hours) = single.parse::<i64>() {
            if hours <= 0 {
                return Err(AppError::Validation(format!(
                    "invalid hour length '{hours}'; expected a positive number of hours"
                )));
            }
            let since = (now - Duration::hours(hours)).to_rfc3339();
            let desc = format!("in past {hours} hours");
            return Ok((Some(since), None, Some(desc)));
        }

        // 2. Single string containing range delimiter (e.g. "2026-10-13..2026-10-20")
        if let Some((start, end)) = split_range_expr(single) {
            let s_dt = parse_since_expr(start, now).ok_or_else(|| {
                AppError::Validation(format!(
                    "invalid range start '{start}'; expected format 'YYYY-MM-DD' or RFC3339"
                ))
            })?;
            let u_dt = parse_until_expr(end, now).ok_or_else(|| {
                AppError::Validation(format!(
                    "invalid range end '{end}'; expected format 'YYYY-MM-DD' or RFC3339"
                ))
            })?;
            let desc = format!("between {start} and {end}");
            return Ok((Some(s_dt.to_rfc3339()), Some(u_dt.to_rfc3339()), Some(desc)));
        }

        // 3. Single datetime -> Mode 1: A to now
        if let Some(s_dt) = parse_since_expr(single, now) {
            let desc = format!("since {single}");
            return Ok((Some(s_dt.to_rfc3339()), None, Some(desc)));
        }

        return Err(AppError::Validation(format!(
            "invalid time expression '{single}'; expected a single number in hours (e.g. '24'), a start datetime (e.g. '2026-10-13'), or a range (e.g. '2026-10-13..2026-10-20')"
        )));
    }

    if cleaned.len() == 2 {
        let start = cleaned[0];
        let end = cleaned[1];
        let s_dt = parse_since_expr(start, now).ok_or_else(|| {
            AppError::Validation(format!(
                "invalid start time '{start}'; expected format 'YYYY-MM-DD' or RFC3339"
            ))
        })?;
        let u_dt = parse_until_expr(end, now).ok_or_else(|| {
            AppError::Validation(format!(
                "invalid end time '{end}'; expected format 'YYYY-MM-DD' or RFC3339"
            ))
        })?;
        let desc = format!("between {start} and {end}");
        return Ok((Some(s_dt.to_rfc3339()), Some(u_dt.to_rfc3339()), Some(desc)));
    }

    Err(AppError::Validation(
        "--time accepts at most 2 arguments (e.g. '--time 24', '--time 2026-10-13', or '--time 2026-10-13 2026-10-20')".to_string()
    ))
}

pub fn resolve_time_window_bounds(
    window: Option<&str>,
    since: Option<&str>,
    until: Option<&str>,
    range: Option<&str>,
) -> AppResult<(Option<String>, Option<String>)> {
    let now = Utc::now();
    let mut resolved_since = None;
    let mut resolved_until = None;

    if let Some(r) = range.map(str::trim).filter(|s| !s.is_empty()) {
        let (s_expr, u_expr) = parse_range_parts(r)?;
        if !s_expr.is_empty() {
            if let Some(dt) = parse_since_expr(s_expr, now) {
                resolved_since = Some(dt.to_rfc3339());
            } else {
                return Err(AppError::Validation(format!(
                    "invalid range start '{s_expr}'; expected e.g. 'YYYY-MM-DD', 'YYYY-MM-DD HH:MM:SS', 'yesterday', or RFC3339"
                )));
            }
        }
        if !u_expr.is_empty() {
            if let Some(dt) = parse_until_expr(u_expr, now) {
                resolved_until = Some(dt.to_rfc3339());
            } else {
                return Err(AppError::Validation(format!(
                    "invalid range end '{u_expr}'; expected e.g. 'YYYY-MM-DD', 'YYYY-MM-DD HH:MM:SS', 'today', 'now', or RFC3339"
                )));
            }
        }
        return Ok((resolved_since, resolved_until));
    }

    if let Some(w) = window.map(str::trim).filter(|s| !s.is_empty()) {
        if split_range_expr(w).is_some() {
            return resolve_time_window_bounds(None, None, None, Some(w));
        }
        if let Some(duration) = parse_duration_expr(w) {
            resolved_since = Some((now - duration).to_rfc3339());
        } else {
            return Err(AppError::Validation(format!(
                "invalid window duration '{w}'; expected e.g. '24', '24h', '48h', or range 'a..b'"
            )));
        }
    }

    if let Some(s) = since.map(str::trim).filter(|s| !s.is_empty()) {
        if split_range_expr(s).is_some() {
            return resolve_time_window_bounds(None, None, None, Some(s));
        }
        if let Some(dt) = parse_since_expr(s, now) {
            resolved_since = Some(dt.to_rfc3339());
        } else {
            return Err(AppError::Validation(format!(
                "invalid since expression '{s}'; expected e.g. '24', 'yesterday', 'today', 'YYYY-MM-DD', 'YYYY-MM-DD HH:MM:SS', or RFC3339"
            )));
        }
    }

    if let Some(u) = until.map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(dt) = parse_until_expr(u, now) {
            resolved_until = Some(dt.to_rfc3339());
        } else {
            return Err(AppError::Validation(format!(
                "invalid until expression '{u}'; expected e.g. 'today', 'YYYY-MM-DD', 'YYYY-MM-DD HH:MM:SS', or RFC3339"
            )));
        }
    }

    Ok((resolved_since, resolved_until))
}

pub fn split_range_expr(s: &str) -> Option<(&str, &str)> {
    if let Some((start, end)) = s.split_once("..") {
        return Some((start.trim(), end.trim()));
    }
    if let Some((start, end)) = s.split_once(" - ") {
        return Some((start.trim(), end.trim()));
    }
    if let Some((start, end)) = s.split_once(" to ") {
        return Some((start.trim(), end.trim()));
    }
    if let Some((start, end)) = s.split_once(" TO ") {
        return Some((start.trim(), end.trim()));
    }
    if let Some((start, end)) = s.split_once('~') {
        return Some((start.trim(), end.trim()));
    }
    if let Some((start, end)) = s.split_once(',') {
        return Some((start.trim(), end.trim()));
    }
    None
}

pub fn parse_range_parts(s: &str) -> AppResult<(&str, &str)> {
    split_range_expr(s).ok_or_else(|| {
        AppError::Validation(format!(
            "invalid range expression '{s}'; expected format 'a..b', 'a - b', or 'a to b'"
        ))
    })
}

pub fn parse_duration_expr(s: &str) -> Option<Duration> {
    let s = s.trim().to_ascii_lowercase();
    if s.is_empty() {
        return None;
    }
    if let Ok(val) = s.parse::<i64>() {
        return Some(Duration::hours(val));
    }
    let non_digit_idx = s.find(|c: char| !c.is_ascii_digit())?;
    let (num_part, unit) = s.split_at(non_digit_idx);
    let val: i64 = num_part.parse().ok()?;
    match unit.trim() {
        "m" | "min" | "mins" | "minute" | "minutes" => Some(Duration::minutes(val)),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(Duration::hours(val)),
        "d" | "day" | "days" => Some(Duration::days(val)),
        "w" | "week" | "weeks" => Some(Duration::weeks(val)),
        _ => None,
    }
}

pub fn parse_since_expr(s: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "now" {
        return Some(now);
    }
    if lower == "today" {
        return now
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc));
    }
    if lower == "yesterday" {
        return (now.date_naive() - Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc));
    }
    if let Some(duration) = parse_duration_expr(trimmed) {
        return Some(now - duration);
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.with_timezone(&Utc));
    }
    if let Some(naive) = parse_naive_datetime(trimmed) {
        return Some(naive_to_utc(naive));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0).map(naive_to_utc);
    }
    None
}

pub fn parse_until_expr(s: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "now" {
        return Some(now);
    }
    if lower == "today" {
        return now
            .date_naive()
            .and_hms_nano_opt(23, 59, 59, 999_999_999)
            .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc));
    }
    if lower == "yesterday" {
        return (now.date_naive() - Duration::days(1))
            .and_hms_nano_opt(23, 59, 59, 999_999_999)
            .map(|naive| DateTime::from_naive_utc_and_offset(naive, Utc));
    }
    if let Some(duration) = parse_duration_expr(trimmed) {
        return Some(now - duration);
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(parsed.with_timezone(&Utc));
    }
    if let Some(naive) = parse_naive_datetime(trimmed) {
        return Some(naive_to_utc(naive));
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return date
            .and_hms_nano_opt(23, 59, 59, 999_999_999)
            .map(naive_to_utc);
    }
    None
}

fn parse_naive_datetime(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M"))
        .ok()
}

fn naive_to_utc(naive: NaiveDateTime) -> DateTime<Utc> {
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| DateTime::from_naive_utc_and_offset(naive, Utc))
}

#[cfg(test)]
#[path = "time_window_tests.rs"]
mod tests;
