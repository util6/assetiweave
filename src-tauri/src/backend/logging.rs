use std::fs;
use tracing_appender::non_blocking::{NonBlockingBuilder, WorkerGuard};

use crate::backend::runtime::{config::RuntimeConfig, AppError, AppResult};

pub(crate) struct LoggingGuard {
    pub(crate) _worker: WorkerGuard,
}

pub(crate) fn init_logging(config: &RuntimeConfig) -> AppResult<LoggingGuard> {
    fs::create_dir_all(&config.log_dir).map_err(AppError::Io)?;
    let log_file = config.log_dir.join("app.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .map_err(AppError::Io)?;
    let (writer, worker) = NonBlockingBuilder::default().lossy(false).finish(file);

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_writer(writer)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .map_err(AppError::external)?;

    Ok(LoggingGuard { _worker: worker })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::backend::logs::{record_operation, OperationLogLevel};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn local_subscriber_writes_operation_event_to_app_log() {
        let temp_dir = std::env::temp_dir().join(format!(
            "assetiweave-logging-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let log_file_path = temp_dir.join("app.log");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file_path)
            .expect("open temp app.log");

        let (writer, worker) = NonBlockingBuilder::default().lossy(false).finish(file);

        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_target(false)
            .with_writer(writer)
            .with_max_level(tracing::Level::INFO)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            record_operation(
                OperationLogLevel::Info,
                "source.create",
                "添加数据来源成功\n测试多行转义",
                &[
                    ("source_id", "source-a".to_string()),
                    ("root_path", "/tmp/skills\nnewlines".to_string()),
                ],
            );
        });

        // Dropping worker flushes buffered events
        drop(worker);

        let content = fs::read_to_string(&log_file_path).expect("read app.log");
        assert!(content.contains("INFO"));
        assert!(content.contains("source.create"));
        assert!(content.contains("添加数据来源成功\\n测试多行转义"));
        assert!(content.contains("source_id"));
        assert!(content.contains("source-a"));
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "Expected single line log entry, got: {content}"
        );

        fs::remove_dir_all(temp_dir).expect("remove temp dir");
    }
}
