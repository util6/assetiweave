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
    text.replace('\r', "\\r").replace('\n', "\\n")
}

pub(crate) fn redact_sensitive_data(text: &str) -> String {
    let mut result = text.to_string();
    if let Some(pos) = result.find("Bearer ") {
        let rest = &result[pos + 7..];
        let token_len = rest.find(' ').unwrap_or(rest.len());
        if token_len > 0 {
            result.replace_range(pos + 7..pos + 7 + token_len, "[REDACTED]");
        }
    }
    while let Some(start) = result.find("sk-") {
        let rest = &result[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(rest.len());
        if end > 3 {
            result.replace_range(start..start + end, "[REDACTED_SECRET]");
        } else {
            break;
        }
    }
    result
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
}
