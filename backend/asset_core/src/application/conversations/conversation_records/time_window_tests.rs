use super::*;
use chrono::TimeZone;

#[test]
fn test_parse_duration_expr() {
    assert_eq!(parse_duration_expr("24"), Some(Duration::hours(24)));
    assert_eq!(parse_duration_expr("30m"), Some(Duration::minutes(30)));
    assert_eq!(parse_duration_expr("2h"), Some(Duration::hours(2)));
    assert_eq!(parse_duration_expr("24h"), Some(Duration::hours(24)));
    assert_eq!(parse_duration_expr("7d"), Some(Duration::days(7)));
    assert_eq!(parse_duration_expr("2w"), Some(Duration::weeks(2)));
    assert_eq!(parse_duration_expr("invalid"), None);
    assert_eq!(parse_duration_expr(""), None);
}

#[test]
fn test_resolve_unified_time_modes() {
    // Mode 2: Single pure integer (hours)
    let (since_24, until_24, desc_24) = resolve_unified_time(&["24"]).unwrap();
    assert!(since_24.is_some());
    assert!(until_24.is_none());
    assert_eq!(desc_24.as_deref(), Some("in past 24 hours"));

    // Mode 1: Single datetime (start time to now)
    let (since_dt, until_dt, desc_dt) = resolve_unified_time(&["2026-10-13"]).unwrap();
    assert!(since_dt.is_some());
    assert!(until_dt.is_none());
    assert_eq!(desc_dt.as_deref(), Some("since 2026-10-13"));

    // Mode 3: Two datetimes (endpoints)
    let (since_two, until_two, desc_two) =
        resolve_unified_time(&["2026-10-13", "2026-10-20"]).unwrap();
    assert!(since_two.is_some());
    assert!(until_two.is_some());
    assert_eq!(
        desc_two.as_deref(),
        Some("between 2026-10-13 and 2026-10-20")
    );

    // Mode 3: Range string in 1 arg
    let (since_rng, until_rng, desc_rng) =
        resolve_unified_time(&["2026-10-13..2026-10-20"]).unwrap();
    assert!(since_rng.is_some());
    assert!(until_rng.is_some());
    assert_eq!(
        desc_rng.as_deref(),
        Some("between 2026-10-13 and 2026-10-20")
    );
}

#[test]
fn test_parse_since_expr() {
    let now = Utc.with_ymd_and_hms(2026, 10, 20, 12, 0, 0).unwrap();

    let since_2h = parse_since_expr("2h", now).unwrap();
    assert_eq!(
        since_2h,
        Utc.with_ymd_and_hms(2026, 10, 20, 10, 0, 0).unwrap()
    );

    let since_today = parse_since_expr("today", now).unwrap();
    assert_eq!(
        since_today,
        Utc.with_ymd_and_hms(2026, 10, 20, 0, 0, 0).unwrap()
    );

    let since_yesterday = parse_since_expr("yesterday", now).unwrap();
    assert_eq!(
        since_yesterday,
        Utc.with_ymd_and_hms(2026, 10, 19, 0, 0, 0).unwrap()
    );

    let since_rfc = parse_since_expr("2026-10-15T08:30:00Z", now).unwrap();
    assert_eq!(
        since_rfc,
        Utc.with_ymd_and_hms(2026, 10, 15, 8, 30, 0).unwrap()
    );

    let since_naive_space = parse_since_expr("2026-10-13 18:52:16", now);
    assert!(since_naive_space.is_some());

    let since_naive_t = parse_since_expr("2026-10-13T18:52:16", now);
    assert!(since_naive_t.is_some());

    let since_date = parse_since_expr("2026-10-15", now);
    assert!(since_date.is_some());
}

#[test]
fn test_split_range_expr() {
    assert_eq!(
        split_range_expr("2026-10-13..2026-10-16"),
        Some(("2026-10-13", "2026-10-16"))
    );
    assert_eq!(
        split_range_expr("2026-10-13 - 2026-10-16"),
        Some(("2026-10-13", "2026-10-16"))
    );
    assert_eq!(
        split_range_expr("yesterday to today"),
        Some(("yesterday", "today"))
    );
    assert_eq!(
        split_range_expr("2026-10-13,2026-10-16"),
        Some(("2026-10-13", "2026-10-16"))
    );
    assert_eq!(
        split_range_expr("2026-10-13~2026-10-16"),
        Some(("2026-10-13", "2026-10-16"))
    );
    assert_eq!(split_range_expr("2026-10-13"), None);
}
