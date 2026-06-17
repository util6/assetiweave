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
const MANAGED_LOG_FILE_PREFIXES: &[&str] = &[APP_LOG_FILE_PREFIX, CODEX_API_LOG_FILE_PREFIX];
const DEFAULT_LOG_TAIL_LINES: usize = 200;
const MIN_LOG_TAIL_LINES: usize = 20;
const MAX_LOG_TAIL_LINES: usize = 5000;
const LOG_TAIL_SCAN_CHUNK_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperationLogLevel {
    Info,
    Warn,
    Error,
}

impl OperationLogLevel {
    fn from_str(level: &str) -> Result<Self, String> {
        match level.trim().to_ascii_uppercase().as_str() {
            "INFO" => Ok(Self::Info),
            "WARN" | "WARNING" => Ok(Self::Warn),
            "ERROR" => Ok(Self::Error),
            other => Err(format!("不支持的日志级别: {other}")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

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
    write_operation_log(
        OperationLogLevel::Info,
        "app.startup",
        "AssetIWeave 启动",
        &[],
    )
}

pub(crate) fn record_operation(
    level: OperationLogLevel,
    operation: &str,
    message: &str,
    fields: &[(&str, String)],
) {
    #[cfg(test)]
    {
        let timestamp = Local::now().to_rfc3339();
        let _ = format_operation_log_line(&timestamp, level, operation, message, fields);
    }

    #[cfg(not(test))]
    if let Err(error) = write_operation_log(level, operation, message, fields) {
        eprintln!("failed to write AssetIWeave operation log: {error}");
    }
}

pub(crate) fn record_info(operation: &str, message: &str, fields: &[(&str, String)]) {
    record_operation(OperationLogLevel::Info, operation, message, fields);
}

pub(crate) fn record_warn(operation: &str, message: &str, fields: &[(&str, String)]) {
    record_operation(OperationLogLevel::Warn, operation, message, fields);
}

pub(crate) fn record_error(operation: &str, message: &str, fields: &[(&str, String)]) {
    record_operation(OperationLogLevel::Error, operation, message, fields);
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
        Ok(()) => record_info(
            "log.open_directory",
            "打开日志目录成功",
            &[("path", log_dir.to_string_lossy().to_string())],
        ),
        Err(error) => record_error(
            "log.open_directory",
            "打开日志目录失败",
            &[
                ("path", log_dir.to_string_lossy().to_string()),
                ("error", error.to_string()),
            ],
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
    let level = OperationLogLevel::from_str(&level)?;
    let field_pairs = fields
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| (key, value))
        .collect::<Vec<_>>();
    let borrowed_fields = field_pairs
        .iter()
        .map(|(key, value)| (key.as_str(), value.clone()))
        .collect::<Vec<_>>();

    write_operation_log(level, &operation, &message, &borrowed_fields)
}

fn get_log_dir() -> Result<PathBuf, String> {
    if let Some(log_dir) = std::env::var_os("ASSETIWEAVE_LOG_DIR") {
        let log_dir = PathBuf::from(log_dir);
        fs::create_dir_all(&log_dir).map_err(|error| format!("创建日志目录失败: {error}"))?;
        return Ok(log_dir);
    }

    let mut data_dir = dirs::data_dir().ok_or("无法确定系统数据目录")?;
    data_dir.push("AssetIWeave");
    data_dir.push("logs");
    fs::create_dir_all(&data_dir).map_err(|error| format!("创建日志目录失败: {error}"))?;
    Ok(data_dir)
}

fn ensure_default_log_file() -> Result<(), String> {
    if !list_managed_log_files()?.is_empty() {
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

fn is_managed_log_file_name(name: &str) -> bool {
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

fn write_operation_log(
    level: OperationLogLevel,
    operation: &str,
    message: &str,
    fields: &[(&str, String)],
) -> Result<(), String> {
    let log_dir = get_log_dir()?;
    write_operation_log_to_dir(&log_dir, level, operation, message, fields)
}

fn write_operation_log_to_dir(
    log_dir: &Path,
    level: OperationLogLevel,
    operation: &str,
    message: &str,
    fields: &[(&str, String)],
) -> Result<(), String> {
    let line = format_operation_log_line(
        &Local::now().to_rfc3339(),
        level,
        operation,
        message,
        fields,
    );
    append_app_log_line(log_dir, &line)
}

fn append_app_log_line(log_dir: &Path, line: &str) -> Result<(), String> {
    let log_file = log_dir.join(APP_LOG_FILE_PREFIX);
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
        .map_err(|error| format!("打开日志文件失败: {error}"))?;

    writeln!(file, "{line}").map_err(|error| format!("写入日志文件失败: {error}"))
}

fn format_operation_log_line(
    timestamp: &str,
    level: OperationLogLevel,
    operation: &str,
    message: &str,
    fields: &[(&str, String)],
) -> String {
    let mut line = format!(
        "{} {} [{}] {}",
        sanitize_log_text(timestamp),
        level.as_str(),
        sanitize_log_key(operation),
        sanitize_log_text(message)
    );

    for (key, value) in fields {
        line.push(' ');
        line.push_str(&sanitize_log_key(key));
        line.push_str("=\"");
        line.push_str(&sanitize_log_value(value));
        line.push('"');
    }

    line
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

fn sanitize_log_value(value: &str) -> String {
    sanitize_log_text(value).replace('"', "\\\"")
}

fn open_directory(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("打开目录失败: {error}"))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOG_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn operation_log_line_escapes_multiline_fields() {
        let line = format_operation_log_line(
            "2026-06-01T15:00:00+08:00",
            OperationLogLevel::Error,
            "skill mount!",
            "挂载失败\n需要查看异常",
            &[
                ("skill name", "frontend-ui\nengineering".to_string()),
                ("error", "path contains \"target\"".to_string()),
            ],
        );

        assert_eq!(
            line,
            "2026-06-01T15:00:00+08:00 ERROR [skill_mount] 挂载失败\\n需要查看异常 skill_name=\"frontend-ui\\nengineering\" error=\"path contains \\\"target\\\"\""
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn log_level_parser_accepts_expected_levels() {
        assert_eq!(
            OperationLogLevel::from_str("info").expect("info level"),
            OperationLogLevel::Info
        );
        assert_eq!(
            OperationLogLevel::from_str("warning").expect("warning level"),
            OperationLogLevel::Warn
        );
        assert!(OperationLogLevel::from_str("debug").is_err());
    }

    #[test]
    fn operation_log_writer_appends_to_log_viewer_file() {
        let log_dir = std::env::temp_dir().join(format!(
            "assetiweave-log-test-{}",
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&log_dir).expect("create log dir");

        write_operation_log_to_dir(
            &log_dir,
            OperationLogLevel::Info,
            "source.create",
            "添加数据来源成功",
            &[
                ("source_id", "source-a".to_string()),
                ("root_path", "/tmp/skills".to_string()),
            ],
        )
        .expect("write operation log");

        let content =
            read_log_tail_lines(&log_dir.join(APP_LOG_FILE_PREFIX), 20).expect("read log tail");
        assert!(content.contains("INFO [source.create] 添加数据来源成功"));
        assert!(content.contains("source_id=\"source-a\""));
        assert!(content.contains("root_path=\"/tmp/skills\""));

        fs::remove_dir_all(log_dir).expect("remove log dir");
    }

    #[test]
    fn write_operation_command_is_read_by_snapshot_command() {
        let _guard = LOG_ENV_LOCK.lock().expect("lock log env");
        let log_dir = std::env::temp_dir().join(format!(
            "assetiweave-log-command-test-{}",
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&log_dir).expect("create log dir");
        std::env::set_var("ASSETIWEAVE_LOG_DIR", &log_dir);

        logs_write_operation(
            "INFO".to_string(),
            "source.create".to_string(),
            "添加数据来源成功".to_string(),
            Some(BTreeMap::from([
                ("source_id".to_string(), "source-a".to_string()),
                ("root_path".to_string(), "/tmp/skills".to_string()),
            ])),
        )
        .expect("write operation command");
        let snapshot = logs_get_snapshot(Some(APP_LOG_FILE_PREFIX.to_string()), Some(20))
            .expect("get log snapshot command");

        assert!(snapshot
            .content
            .contains("INFO [source.create] 添加数据来源成功"));
        assert!(snapshot.content.contains("source_id=\"source-a\""));
        assert!(snapshot.content.contains("root_path=\"/tmp/skills\""));
        assert_eq!(snapshot.log_file_name, APP_LOG_FILE_PREFIX);

        std::env::remove_var("ASSETIWEAVE_LOG_DIR");
        fs::remove_dir_all(log_dir).expect("remove log dir");
    }
}
