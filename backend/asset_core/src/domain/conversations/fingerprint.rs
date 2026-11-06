use chrono::{DateTime, Utc};

pub fn parse_conversation_timestamp(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Some(timestamp.with_timezone(&Utc));
    }
    let raw = value.parse::<i64>().ok()?;
    let (seconds, nanoseconds) = if raw.unsigned_abs() >= 100_000_000_000 {
        (
            raw.div_euclid(1_000),
            u32::try_from(raw.rem_euclid(1_000)).ok()? * 1_000_000,
        )
    } else {
        (raw, 0)
    };
    DateTime::from_timestamp(seconds, nanoseconds)
}

pub fn sanitize_sync_error_message(message: &str, home_paths: &[&str]) -> String {
    let mut sanitized = message.to_string();
    for home in home_paths {
        if !home.is_empty() {
            sanitized = sanitized.replace(home, "~");
        }
    }
    let re_mac = regex_replace_user_path(&sanitized, "/Users/");
    let re_linux = regex_replace_user_path(&re_mac, "/home/");
    re_linux.chars().take(1000).collect()
}

fn regex_replace_user_path(text: &str, prefix: &str) -> String {
    let mut result = String::new();
    let mut remaining = text;
    while let Some(idx) = remaining.find(prefix) {
        result.push_str(&remaining[..idx]);
        let after_prefix = &remaining[idx + prefix.len()..];
        if let Some(slash_idx) = after_prefix.find('/') {
            result.push_str("~/");
            remaining = &after_prefix[slash_idx + 1..];
        } else {
            result.push_str("~");
            remaining = "";
            break;
        }
    }
    result.push_str(remaining);
    result
}
