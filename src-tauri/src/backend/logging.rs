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
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone)]
    struct TestWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn span_inheritance_captures_parent_fields() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (writer, worker) = NonBlockingBuilder::default()
            .lossy(false)
            .finish(TestWriter(buffer.clone()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                "task_execution",
                task_id = "task-42",
                tenant = "tenant-prod"
            );
            let _enter = span.enter();
            tracing::info!(operation = "source.mount", "mounting source");
        });
        drop(worker);

        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert!(output.contains("task-42"), "missing task-42 in {output}");
        assert!(
            output.contains("tenant-prod"),
            "missing tenant-prod in {output}"
        );
        assert!(
            output.contains("source.mount"),
            "missing source.mount in {output}"
        );
        assert!(
            output.contains("mounting source"),
            "missing message in {output}"
        );
    }

    #[test]
    fn single_line_structured_output_escapes_newlines() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (writer, worker) = NonBlockingBuilder::default()
            .lossy(false)
            .finish(TestWriter(buffer.clone()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let multiline_msg = "error on line 1\nerror on line 2\rerror on line 3";
            let sanitized = sanitize_for_log(multiline_msg);
            tracing::info!(operation = "data.parse", message = %sanitized, "processed");
        });
        drop(worker);

        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "Expected single line log entry, got: {output}"
        );
        assert!(output.contains("\\n"));
        assert!(output.contains("\\r"));
    }

    #[test]
    fn env_filter_filters_levels() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (writer, worker) = NonBlockingBuilder::default()
            .lossy(false)
            .finish(TestWriter(buffer.clone()));
        let filter = EnvFilter::new("info");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .with_env_filter(filter)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::debug!("this is a debug log");
            tracing::info!("this is an info log");
        });
        drop(worker);

        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert!(
            !output.contains("this is a debug log"),
            "debug log should be filtered out"
        );
        assert!(
            output.contains("this is an info log"),
            "info log should be present"
        );
    }

    #[test]
    fn rolling_file_discovery_matches_managed_names() {
        let temp_dir = std::env::temp_dir().join(format!(
            "assetiweave-rolling-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let rolling_appender = tracing_appender::rolling::daily(&temp_dir, "app.log");
        let (writer, worker) = NonBlockingBuilder::default()
            .lossy(false)
            .finish(rolling_appender);
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("test rolling appender creation");
        });
        drop(worker);

        let mut found_managed_file = false;
        for entry in fs::read_dir(&temp_dir).expect("read temp dir") {
            let entry = entry.expect("dir entry");
            let name = entry.file_name().into_string().expect("utf8 file name");
            if crate::backend::logs::is_managed_log_file_name(&name) {
                found_managed_file = true;
                break;
            }
        }
        assert!(
            found_managed_file,
            "Rolling appender file should be recognized by is_managed_log_file_name"
        );
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn redaction_masks_tokens_and_secrets() {
        let raw = "Authorization: Bearer my-secret-token-12345, sk-ant-api03-secretkey-abcdef";
        let redacted = redact_sensitive_data(raw);
        assert!(!redacted.contains("my-secret-token-12345"));
        assert!(!redacted.contains("sk-ant-api03-secretkey-abcdef"));
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("[REDACTED_SECRET]"));
    }

    #[test]
    fn engine_stdout_is_isolated_from_tracing() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (writer, worker) = NonBlockingBuilder::default()
            .lossy(false)
            .finish(TestWriter(buffer.clone()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("engine protocol safe message");
        });
        drop(worker);

        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert!(output.contains("engine protocol safe message"));
    }

    #[test]
    fn production_events_redact_absolute_paths_and_sensitive_values() {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (writer, worker) = NonBlockingBuilder::default()
            .lossy(false)
            .finish(TestWriter(buffer.clone()));
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            let raw_unix_path = "/Users/alice/projects/secret_code";
            let raw_win_path = r"C:\Users\bob\AppData\Local\secret_data";
            let raw_token = "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.token123";
            let raw_secret = "sk-ant-api03-abcdef1234567890";
            let raw_password = "password=SuperSecretPassword123!";
            let raw_prompt = "prompt=Explain how to bypass corporate firewall";
            let raw_env = "DATABASE_URL=postgres://user:super_secret_pw@localhost/db";

            let safe_unix = redact_for_log(raw_unix_path);
            let safe_win = redact_for_log(raw_win_path);
            let safe_token = redact_for_log(raw_token);
            let safe_secret = redact_for_log(raw_secret);
            let safe_password = redact_for_log(raw_password);
            let safe_prompt = redact_for_log(raw_prompt);
            let safe_env = redact_for_log(raw_env);

            tracing::info!(
                action = "deployment.execute",
                resource_id = "res-42",
                unix_path = %safe_unix,
                win_path = %safe_win,
                token = %safe_token,
                secret = %safe_secret,
                password = %safe_password,
                prompt = %safe_prompt,
                env = %safe_env,
                "executing deployment with sanitized fields"
            );
        });
        drop(worker);

        let output = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();

        assert!(
            output.contains("deployment.execute"),
            "missing action in {output}"
        );
        assert!(output.contains("res-42"), "missing resource_id in {output}");

        assert!(
            !output.contains("/Users/alice/projects/secret_code"),
            "raw Unix path leaked: {output}"
        );
        assert!(
            !output.contains(r"C:\Users\bob\AppData\Local\secret_data"),
            "raw Windows path leaked: {output}"
        );
        assert!(
            !output.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.token123"),
            "raw token leaked: {output}"
        );
        assert!(
            !output.contains("sk-ant-api03-abcdef1234567890"),
            "raw secret leaked: {output}"
        );
        assert!(
            !output.contains("SuperSecretPassword123!"),
            "raw password leaked: {output}"
        );
        assert!(
            !output.contains("Explain how to bypass corporate firewall"),
            "raw prompt leaked: {output}"
        );
        assert!(
            !output.contains("super_secret_pw"),
            "raw env secret leaked: {output}"
        );

        assert!(output.contains("[REDACTED_PATH]"));
        assert!(output.contains("[REDACTED]"));
        assert!(output.contains("[REDACTED_SECRET]"));
    }
}
