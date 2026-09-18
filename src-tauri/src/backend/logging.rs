use std::fs;
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};
use tracing_subscriber::EnvFilter;

use crate::backend::runtime::{config::RuntimeConfig, AppError, AppResult};

pub(crate) struct LoggingGuard {
    pub(crate) _worker: WorkerGuard,
}

pub(crate) fn build_env_filter() -> EnvFilter {
    if let Ok(env_val) = std::env::var("ASSETIWEAVE_LOG") {
        if let Ok(filter) = EnvFilter::try_new(env_val) {
            return filter;
        }
    }
    if let Ok(env_val) = std::env::var("RUST_LOG") {
        if let Ok(filter) = EnvFilter::try_new(env_val) {
            return filter;
        }
    }
    EnvFilter::new("info")
}

pub(crate) fn sanitize_for_log(text: &str) -> String {
    redact_for_log(text)
}

pub(crate) fn redact_sensitive_data(text: &str) -> String {
    redact_for_log(text)
}

pub(crate) fn redact_for_log(text: &str) -> String {
    const MAX_LOG_FIELD_LEN: usize = 4096;
    let single_line = text.replace('\r', "\\r").replace('\n', "\\n");
    let bounded = if single_line.len() > MAX_LOG_FIELD_LEN {
        format!("{}... [TRUNCATED]", &single_line[..MAX_LOG_FIELD_LEN])
    } else {
        single_line
    };

    let mut result = bounded;

    if let Some(pos) = result.find("Bearer ") {
        let rest = &result[pos + 7..];
        let token_len = rest
            .find(|c: char| c.is_whitespace() || c == ',' || c == '"' || c == '\'')
            .unwrap_or(rest.len());
        if token_len > 0 {
            result.replace_range(pos + 7..pos + 7 + token_len, "[REDACTED]");
        }
    }

    while let Some(start) = result.find("sk-") {
        let rest = &result[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == ',' || c == '"' || c == '\'')
            .unwrap_or(rest.len());
        if end > 3 {
            result.replace_range(start..start + end, "[REDACTED_SECRET]");
        } else {
            break;
        }
    }

    let sensitive_keys = [
        "password=",
        "passwd=",
        "secret=",
        "token=",
        "prompt=",
        "api_key=",
        "apikey=",
    ];
    for key in sensitive_keys {
        let mut search_from = 0;
        while let Some(rel_pos) = result[search_from..].to_ascii_lowercase().find(key) {
            let key_start = search_from + rel_pos;
            let val_start = key_start + key.len();
            let rest = &result[val_start..];
            let val_len = rest
                .find(|c: char| {
                    c.is_whitespace()
                        || c == '&'
                        || c == ';'
                        || c == ','
                        || c == '"'
                        || c == '\''
                        || c == '}'
                        || c == ')'
                })
                .unwrap_or(rest.len());
            if val_len > 0 {
                let placeholder = if key.starts_with("prompt") {
                    "[REDACTED_PROMPT]"
                } else if key.starts_with("secret") || key.starts_with("api") {
                    "[REDACTED_SECRET]"
                } else {
                    "[REDACTED]"
                };
                result.replace_range(val_start..val_start + val_len, placeholder);
                search_from = val_start + placeholder.len();
            } else {
                search_from = val_start;
            }
        }
    }

    if let Some(at_pos) = result.find('@') {
        if let Some(proto_pos) = result[..at_pos].rfind("://") {
            let auth_part = &result[proto_pos + 3..at_pos];
            if let Some(colon_pos) = auth_part.find(':') {
                let pw_start = proto_pos + 3 + colon_pos + 1;
                result.replace_range(pw_start..at_pos, "[REDACTED]");
            }
        }
    }

    redact_absolute_paths(&result)
}

fn redact_absolute_paths(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut remainder = text;

    while !remainder.is_empty() {
        let unix_idx = remainder
            .find("/Users/")
            .or_else(|| remainder.find("/home/"));
        let win_idx = find_windows_drive_path(remainder);

        let next_match = match (unix_idx, win_idx) {
            (Some(u), Some(w)) => Some(u.min(w)),
            (Some(u), None) => Some(u),
            (None, Some(w)) => Some(w),
            (None, None) => None,
        };

        if let Some(start) = next_match {
            output.push_str(&remainder[..start]);
            let path_part = &remainder[start..];
            let path_end = path_part
                .find(|c: char| {
                    c.is_whitespace()
                        || c == '"'
                        || c == '\''
                        || c == ','
                        || c == '>'
                        || c == ']'
                        || c == '}'
                        || c == ')'
                })
                .unwrap_or(path_part.len());
            output.push_str("[REDACTED_PATH]");
            remainder = &remainder[start + path_end..];
        } else {
            output.push_str(remainder);
            break;
        }
    }

    output
}

fn find_windows_drive_path(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    for i in 0..bytes.len() - 2 {
        if bytes[i].is_ascii_alphabetic()
            && bytes[i + 1] == b':'
            && (bytes[i + 2] == b'\\' || bytes[i + 2] == b'/')
        {
            if i == 0
                || bytes[i - 1].is_ascii_whitespace()
                || bytes[i - 1] == b'"'
                || bytes[i - 1] == b'\''
                || bytes[i - 1] == b'='
                || bytes[i - 1] == b'('
                || bytes[i - 1] == b'['
            {
                return Some(i);
            }
        }
    }
    None
}

pub(crate) fn init_logging(config: &RuntimeConfig) -> AppResult<LoggingGuard> {
    fs::create_dir_all(&config.log_dir).map_err(AppError::Io)?;
    let file_appender = tracing_appender::rolling::daily(&config.log_dir, "app.log");
    let (writer, worker) = NonBlockingBuilder::default()
        .lossy(false)
        .finish(file_appender);

    let filter = build_env_filter();

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_writer(writer)
        .with_env_filter(filter)
        .try_init()
        .map_err(AppError::external)?;

    Ok(LoggingGuard { _worker: worker })
}

#[cfg(test)]
#[path = "logging_tests.rs"]
mod tests;
