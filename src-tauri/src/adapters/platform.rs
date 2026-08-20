//! 平台原生交互与文件管理器适配模块
//!
//! 提供在操作系统文件管理器（Finder / Explorer / xdg-open）中定位与打开指定路径的能力。

use crate::backend::{compat::LegacyResult, path_utils::expand_path};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

/// 文件管理器命令行调用指令
struct FileManagerInvocation {
    /// 执行程序名称（如 `open`, `explorer`, `xdg-open`）
    program: &'static str,
    /// 命令行参数列表
    args: Vec<OsString>,
}

/// 支持的操作系统文件管理器平台类型
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum FileManagerPlatform {
    Macos,
    Windows,
    Linux,
}

/// 在操作系统的文件管理器中打开或定位指定的路径
///
/// # 参数
/// * `path` - 目标文件或目录的绝对路径或含 `~` 缩写的路径字符串
pub(crate) fn reveal_path(path: String) -> LegacyResult<()> {
    let path = resolve_reveal_path(&path)?;

    #[cfg(target_os = "macos")]
    {
        let invocation =
            build_file_manager_invocation(&path, path.is_dir(), FileManagerPlatform::Macos);
        return command_status(&invocation);
    }

    #[cfg(target_os = "windows")]
    {
        let invocation =
            build_file_manager_invocation(&path, path.is_dir(), FileManagerPlatform::Windows);
        return command_status(&invocation);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let invocation =
            build_file_manager_invocation(&path, path.is_dir(), FileManagerPlatform::Linux);
        return command_status(&invocation);
    }

    #[allow(unreachable_code)]
    Err("unsupported platform".to_string())
}

/// 解析路径并校验其是否存在
fn resolve_reveal_path(path: &str) -> LegacyResult<PathBuf> {
    let path = expand_path(path)?;
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    Ok(path)
}

/// 根据目标路径类型与操作系统构建对应的文件管理器命令参数
fn build_file_manager_invocation(
    path: &Path,
    is_dir: bool,
    platform: FileManagerPlatform,
) -> FileManagerInvocation {
    match platform {
        FileManagerPlatform::Macos => FileManagerInvocation {
            program: "open",
            args: vec![OsString::from("-R"), path.as_os_str().to_os_string()],
        },
        FileManagerPlatform::Windows if is_dir => FileManagerInvocation {
            program: "explorer",
            args: vec![path.as_os_str().to_os_string()],
        },
        FileManagerPlatform::Windows => FileManagerInvocation {
            program: "explorer",
            args: vec![OsString::from(format!(
                "/select,{}",
                path.to_string_lossy()
            ))],
        },
        FileManagerPlatform::Linux if is_dir => FileManagerInvocation {
            program: "xdg-open",
            args: vec![path.as_os_str().to_os_string()],
        },
        FileManagerPlatform::Linux => FileManagerInvocation {
            program: "xdg-open",
            args: vec![path
                .parent()
                .unwrap_or(Path::new("."))
                .as_os_str()
                .to_os_string()],
        },
    }
}

/// 执行文件管理器命令并检查返回状态
fn command_status(invocation: &FileManagerInvocation) -> LegacyResult<()> {
    let status = Command::new(invocation.program)
        .args(&invocation.args)
        .status()
        .map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("file manager command failed: {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_path_resolves_home_shorthand_before_invoking_file_manager() {
        let resolved = resolve_reveal_path("~").expect("resolve home");

        assert!(resolved.is_absolute());
        assert!(resolved.is_dir());
        assert_ne!(resolved.file_name(), Some(std::ffi::OsStr::new("~")));
    }

    #[test]
    fn windows_opens_directories_without_select_flag() {
        let path = Path::new(r"C:\Users\95853\.codex\skills");
        let invocation = build_file_manager_invocation(path, true, FileManagerPlatform::Windows);

        assert_eq!(invocation.program, "explorer");
        assert_eq!(invocation.args, vec![path.as_os_str().to_os_string()]);
    }

    #[test]
    fn windows_selects_files_with_single_select_argument() {
        let path = Path::new(r"C:\Users\95853\.codex\skills\README.md");
        let invocation = build_file_manager_invocation(path, false, FileManagerPlatform::Windows);

        assert_eq!(invocation.program, "explorer");
        assert_eq!(
            invocation.args,
            vec![OsString::from(format!(
                "/select,{}",
                path.to_string_lossy()
            ))]
        );
    }
}
