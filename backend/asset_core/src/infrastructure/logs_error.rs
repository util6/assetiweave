use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LogAccessError {
    #[error("无法获取运行配置: {0}")]
    RuntimeConfig(String),

    #[error("{action}失败: {source}")]
    Io {
        action: &'static str,
        path: Option<PathBuf>,
        #[source]
        source: std::io::Error,
    },

    #[error("未找到可用日志文件")]
    NoAvailableLogFiles,

    #[error("未找到指定日志文件: {0}")]
    FileNotFound(String),

    #[error("不支持的日志级别: {0}")]
    InvalidLogLevel(String),

    #[error("没有可用的 panic 日志路径: {0}")]
    PanicLogFailed(String),

    #[error("非法日志路径访问: {0}")]
    PathEscape(String),

    #[error("打开目录失败: {0}")]
    OpenDirectory(#[source] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl LogAccessError {
    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl PartialEq<&str> for LogAccessError {
    fn eq(&self, other: &&str) -> bool {
        self.to_string() == *other
    }
}

impl PartialEq<String> for LogAccessError {
    fn eq(&self, other: &String) -> bool {
        self.to_string() == *other
    }
}
