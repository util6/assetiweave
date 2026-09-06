use crate::backend::host_process::configure_background_process;
use chrono::Local;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const APP_LOG_FILE_PREFIX: &str = "app.log";
const CODEX_API_LOG_FILE_PREFIX: &str = "codex-api.log";
const PANIC_LOG_FILE_PREFIX: &str = "panic.log";
const MANAGED_LOG_FILE_PREFIXES: &[&str] = &[
    APP_LOG_FILE_PREFIX,
    CODEX_API_LOG_FILE_PREFIX,
    PANIC_LOG_FILE_PREFIX,
];
const DEFAULT_LOG_TAIL_LINES: usize = 200;
const MIN_LOG_TAIL_LINES: usize = 20;
const MAX_LOG_TAIL_LINES: usize = 5000;
const LOG_TAIL_SCAN_CHUNK_BYTES: usize = 8192;

#[derive(Debug, Clone, Serialize)]
pub struct ManagedLogFile {
    pub log_file_path: String,
    pub log_file_name: String,
    pub file_size: u64,
    pub modified_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogSnapshot {
    pub log_dir_path: String,
    pub log_file_path: String,
    pub log_file_name: String,
    pub content: String,
    pub line_limit: usize,
    pub file_size: u64,
    pub modified_at_ms: Option<i64>,
    pub available_files: Vec<ManagedLogFile>,
}

pub(crate) fn write_startup_log() -> Result<(), String> {
    tracing::info!(
        target: "assetiweave.operation",
        operation = "app.startup",
        "AssetIWeave 启动"
    );
    Ok(())
}

pub(crate) fn record_fatal_panic(message: &str) {
    let mut paths = Vec::new();
    if let Ok(log_dir) = get_log_dir() {
        paths.push(log_dir.join(PANIC_LOG_FILE_PREFIX));
    }

    let fallback = std::env::temp_dir()
        .join("AssetIWeave")
        .join(PANIC_LOG_FILE_PREFIX);
    if !paths.iter().any(|path| path == &fallback) {
        paths.push(fallback);
    }

    let _ = write_fatal_panic_log(&paths, message);
}

pub(crate) fn logs_get_snapshot(
    file_name: Option<String>,
    line_limit: Option<usize>,
) -> Result<LogSnapshot, String> {
    let line_limit = clamp_log_tail_lines(line_limit);
    let log_dir = get_log_dir()?;
    ensure_default_log_file()?;
    let log_file = resolve_managed_log_file(file_name.as_deref())?;
    let content = read_log_tail_lines(&log_file, line_limit)?;
    let metadata =
        fs::metadata(&log_file).map_err(|error| format!("读取日志文件元数据失败: {error}"))?;
    let available_files = build_available_log_files(list_managed_log_files()?)?;

    Ok(LogSnapshot {
        log_dir_path: log_dir.to_string_lossy().to_string(),
        log_file_path: log_file.to_string_lossy().to_string(),
        log_file_name: log_file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        content,
        line_limit,
        file_size: metadata.len(),
        modified_at_ms: metadata.modified().ok().and_then(to_unix_millis),
        available_files,
    })
}

pub(crate) fn logs_open_log_directory() -> Result<(), String> {
    let log_dir = get_log_dir()?;
    let result = open_directory(&log_dir);
    match &result {
        Ok(()) => tracing::info!(
            target: "assetiweave.operation",
            operation = "log.open_directory",
            path = %log_dir.to_string_lossy(),
            "打开日志目录成功"
        ),
        Err(error) => tracing::error!(
            target: "assetiweave.operation",
            operation = "log.open_directory",
            path = %log_dir.to_string_lossy(),
            error = %error,
            "打开日志目录失败"
        ),
    }
    result
}

pub(crate) fn logs_write_operation(
    level: String,
    operation: String,
    message: String,
    fields: Option<BTreeMap<String, String>>,
) -> Result<(), String> {
    let level_str = level.trim().to_ascii_uppercase();
    let operation = sanitize_log_key(&operation);
    let message = sanitize_log_text(&message);
    let fields_map = fields.unwrap_or_default();

    match level_str.as_str() {
        "INFO" => {
            tracing::info!(
                target: "assetiweave.operation",
                operation = %operation,
                fields = ?fields_map,
                "{}",
                message
            );
        }
        "WARN" | "WARNING" => {
            tracing::warn!(
                target: "assetiweave.operation",
                operation = %operation,
                fields = ?fields_map,
                "{}",
                message
            );
        }
        "ERROR" => {
            tracing::error!(
                target: "assetiweave.operation",
                operation = %operation,
                fields = ?fields_map,
                "{}",
                message
            );
        }
        other => return Err(format!("不支持的日志级别: {other}")),
    }
    Ok(())
}

fn get_log_dir() -> Result<PathBuf, String> {
    let log_dir = crate::backend::runtime::config::runtime_config()
        .map_err(|error| format!("无法获取运行配置: {error}"))?
        .log_dir
        .clone();
    fs::create_dir_all(&log_dir).map_err(|error| format!("创建日志目录失败: {error}"))?;
    Ok(log_dir)
}

fn ensure_default_log_file() -> Result<(), String> {
    if get_log_dir()?.join(APP_LOG_FILE_PREFIX).is_file() || !list_managed_log_files()?.is_empty() {
        return Ok(());
    }

    write_startup_log()
}

fn is_log_file_with_prefix(name: &str, prefix: &str) -> bool {
    name == prefix
        || name
            .strip_prefix(prefix)
            .map(|suffix| suffix.starts_with('.'))
            .unwrap_or(false)
}

pub(crate) fn is_managed_log_file_name(name: &str) -> bool {
    MANAGED_LOG_FILE_PREFIXES
        .iter()
        .any(|prefix| is_log_file_with_prefix(name, prefix))
}

fn list_managed_log_files() -> Result<Vec<PathBuf>, String> {
    let log_dir = get_log_dir()?;
    let entries = fs::read_dir(&log_dir).map_err(|error| format!("读取日志目录失败: {error}"))?;

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取日志目录项失败: {error}"))?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_file() && is_managed_log_file_name(name) {
            paths.push(path);
        }
    }

    paths.sort_by(compare_log_paths_by_recency);
    Ok(paths)
}

fn resolve_managed_log_file(file_name: Option<&str>) -> Result<PathBuf, String> {
    let log_files = list_managed_log_files()?;
    if log_files.is_empty() {
        return Err("未找到可用日志文件".to_string());
    }

    if let Some(file_name) = file_name.map(str::trim).filter(|name| !name.is_empty()) {
        return log_files
            .into_iter()
            .find(|path| path.file_name().and_then(|name| name.to_str()) == Some(file_name))
            .ok_or_else(|| format!("未找到指定日志文件: {file_name}"));
    }

    log_files
        .into_iter()
        .next()
        .ok_or_else(|| "未找到可用日志文件".to_string())
}

fn read_log_tail_lines(log_file: &Path, line_limit: usize) -> Result<String, String> {
    let line_limit = line_limit.max(1);
    let mut file =
        fs::File::open(log_file).map_err(|error| format!("打开日志文件失败: {error}"))?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("读取日志文件元数据失败: {error}"))?
        .len();

    if file_len == 0 {
        return Ok(String::new());
    }

    let mut pos = file_len;
    let mut newline_count = 0usize;
    let mut start_offset = 0u64;
    let mut buffer = [0u8; LOG_TAIL_SCAN_CHUNK_BYTES];

    'scan: while pos > 0 {
        let read_size = usize::min(LOG_TAIL_SCAN_CHUNK_BYTES, pos as usize);
        pos -= read_size as u64;

        file.seek(SeekFrom::Start(pos))
            .map_err(|error| format!("读取日志定位失败: {error}"))?;
        file.read_exact(&mut buffer[..read_size])
            .map_err(|error| format!("读取日志内容失败: {error}"))?;

        for idx in (0..read_size).rev() {
            if buffer[idx] != b'\n' {
                continue;
            }

            newline_count += 1;
            if newline_count > line_limit {
                start_offset = pos + idx as u64 + 1;
                break 'scan;
            }
        }
    }

    file.seek(SeekFrom::Start(start_offset))
        .map_err(|error| format!("读取日志定位失败: {error}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("读取日志内容失败: {error}"))?;

    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn clamp_log_tail_lines(line_limit: Option<usize>) -> usize {
    line_limit
        .unwrap_or(DEFAULT_LOG_TAIL_LINES)
        .clamp(MIN_LOG_TAIL_LINES, MAX_LOG_TAIL_LINES)
}

fn build_managed_log_file(path: &Path) -> Result<ManagedLogFile, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("读取日志文件元数据失败: {error}"))?;

    Ok(ManagedLogFile {
        log_file_path: path.to_string_lossy().to_string(),
        log_file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        file_size: metadata.len(),
        modified_at_ms: metadata.modified().ok().and_then(to_unix_millis),
    })
}

fn build_available_log_files(paths: Vec<PathBuf>) -> Result<Vec<ManagedLogFile>, String> {
    paths
        .into_iter()
        .map(|path| build_managed_log_file(path.as_path()))
        .collect()
}

fn compare_log_paths_by_recency(left: &PathBuf, right: &PathBuf) -> std::cmp::Ordering {
    let left_modified = fs::metadata(left)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let right_modified = fs::metadata(right)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    right_modified
        .cmp(&left_modified)
        .then_with(|| right.file_name().cmp(&left.file_name()))
}

fn to_unix_millis(time: std::time::SystemTime) -> Option<i64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
        .and_then(|value| i64::try_from(value).ok())
}

fn write_fatal_panic_log(paths: &[PathBuf], message: &str) -> Result<(), String> {
    let mut errors = Vec::new();
    for path in paths {
        let Some(parent) = path.parent() else {
            errors.push(format!("日志路径没有父目录: {}", path.display()));
            continue;
        };
        if let Err(error) = fs::create_dir_all(parent) {
            errors.push(format!("创建日志目录 {} 失败: {error}", parent.display()));
            continue;
        }

        let result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| {
                writeln!(file, "[{}] FATAL: {message}", Local::now().to_rfc3339())
            });
        match result {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("写入日志文件 {} 失败: {error}", path.display())),
        }
    }

    if errors.is_empty() {
        Err("没有可用的 panic 日志路径".to_string())
    } else {
        Err(errors.join("; "))
    }
}

fn sanitize_log_key(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();

    if sanitized.is_empty() {
        "operation".to_string()
    } else {
        sanitized
    }
}

fn sanitize_log_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

#[cfg(test)]
fn sanitize_log_value(value: &str) -> String {
    sanitize_log_text(value).replace('"', "\\\"")
}

fn open_directory(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        configure_background_process(&mut command);
        command
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
    }

    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer");
        command.arg(path);
        configure_background_process(&mut command);
        command
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        configure_background_process(&mut command);
        command
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_log_helpers_escape_properly() {
        assert_eq!(
            sanitize_log_text("挂载失败\n需要查看异常"),
            "挂载失败\\n需要查看异常"
        );
        assert_eq!(sanitize_log_key("skill mount!"), "skill_mount");
        assert_eq!(
            sanitize_log_value("path contains \"target\""),
            "path contains \\\"target\\\""
        );
    }

    #[test]
    fn panic_log_is_available_through_the_log_viewer() {
        assert!(is_managed_log_file_name("panic.log"));
        assert!(is_managed_log_file_name("panic.log.1"));
    }

    #[test]
    fn panic_log_writer_uses_the_next_path_when_primary_path_fails() {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-panic-log-test-{}",
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create panic log test root");
        let blocking_parent = root.join("blocking-parent");
        fs::write(&blocking_parent, "not a directory").expect("create blocking parent");
        let primary = blocking_parent.join("panic.log");
        let fallback = root.join("fallback").join("panic.log");

        write_fatal_panic_log(&[primary, fallback.clone()], "panic details")
            .expect("write fallback panic log");

        assert!(fs::read_to_string(fallback)
            .expect("read fallback panic log")
            .contains("panic details"));
        fs::remove_dir_all(root).expect("remove panic log test root");
    }

    #[test]
    fn write_operation_command_validates_level_and_accepts_payload() {
        assert!(logs_write_operation(
            "INFO".to_string(),
            "source.create".to_string(),
            "添加数据来源成功".to_string(),
            Some(BTreeMap::from([
                ("source_id".to_string(), "source-a".to_string()),
                ("root_path".to_string(), "/tmp/skills".to_string()),
            ])),
        )
        .is_ok());

        assert!(logs_write_operation(
            "UNKNOWN".to_string(),
            "source.create".to_string(),
            "添加数据来源成功".to_string(),
            None,
        )
        .is_err());
    }

    #[test]
    fn ordinary_logs_no_longer_open_file_for_each_event() {
        let source = include_str!("logs.rs");
        assert!(!source.contains(concat!("fn append_app_", "log_line(")));
        assert!(!source.contains(concat!("fn format_operation_", "log_line(")));
    }
}
