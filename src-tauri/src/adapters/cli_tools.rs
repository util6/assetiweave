//! CLI 工具包集成与 PATH 配置模块
//!
//! 负责检查应用随包附带的 CLI / Engine 二进制文件状态，
//! 并提供将其安装（或创建 Shim 脚本）到系统 PATH 路径及更新用户 Shell 配置文件（如 `.zshrc`, `.profile` 等）的功能。

#[cfg(windows)]
use crate::backend::host_process::configure_background_process;
use crate::backend::runtime::{AppError, AppResult};
use serde::Serialize;
#[cfg(windows)]
use std::process::Command;
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, Manager};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// 附带 CLI 工具在资源目录下的相对路径
const TOOL_DIR: &str = "bundled-cli/cli";
/// CLI 主程序名称
const CLI_NAME: &str = "assetiweave-cli";
/// CLI 简写入口名称
const CLI_ALIAS: &str = "aiwc";
/// Engine 引擎程序名称
const ENGINE_NAME: &str = "assetiweave-engine";

/// CLI 工具在系统中的状态快照
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CliToolsStatus {
    /// 应用内部是否打包包含了 CLI 与 Engine 可执行文件
    pub(crate) bundled: bool,
    /// CLI Shim / 快捷脚本是否已安装至系统安装目录
    pub(crate) installed: bool,
    /// 系统环境变量 PATH 中是否已配置该安装目录
    pub(crate) path_configured: bool,
    /// 目标安装目录路径字符串
    pub(crate) install_dir: String,
    /// 应该加入 PATH 的条目路径字符串
    pub(crate) path_entry: String,
    /// 主 CLI 程序 Shim 脚本的完整路径字符串
    pub(crate) shim_path: String,
    /// 应用打包自带的 CLI 可执行文件绝对路径（若存在）
    pub(crate) bundled_cli_path: Option<String>,
    /// 应用打包自带的 Engine 可执行文件绝对路径（若存在）
    pub(crate) bundled_engine_path: Option<String>,
    /// 描述当前状态的可读提示信息
    pub(crate) message: String,
}

/// 查询当前 CLI 工具在本地系统中的安装与环境变量配置状态
pub(crate) fn status(app: &AppHandle) -> AppResult<CliToolsStatus> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("resolve app resource directory: {error}"))
        .map_err(AppError::external)?;
    Ok(build_status(&resource_dir, current_path_env()))
}

/// 将随包附带的 CLI 工具安装/链接到本地系统，并自动写 Shell 配置添加环境变量 PATH
pub(crate) fn install(app: &AppHandle) -> AppResult<CliToolsStatus> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("resolve app resource directory: {error}"))
        .map_err(AppError::external)?;
    let tools = bundled_tools(&resource_dir);
    if !tools.cli_path.is_file() || !tools.engine_path.is_file() {
        return Err(AppError::External(format!(
            "bundled CLI tools are missing from {}",
            tools.dir.display()
        )));
    }

    let install_dir = default_install_dir()?;
    fs::create_dir_all(&install_dir)
        .map_err(|error| {
            format!(
                "create CLI install directory {}: {error}",
                install_dir.display()
            )
        })
        .map_err(AppError::external)?;
    write_shim(&install_dir, CLI_NAME, &tools.cli_path)?;
    write_shim(&install_dir, CLI_ALIAS, &tools.cli_path)?;
    write_shim(&install_dir, ENGINE_NAME, &tools.engine_path)?;
    configure_user_path(&install_dir)?;

    let mut next = build_status(&resource_dir, current_path_env());
    next.message = format!(
        "CLI shims (assetiweave-cli and aiwc) installed in {}",
        install_dir.display()
    );
    Ok(next)
}

fn build_status(resource_dir: &Path, path_env: Option<String>) -> CliToolsStatus {
    let tools = bundled_tools(resource_dir);
    let install_dir = default_install_dir().unwrap_or_else(|_| fallback_install_dir());
    let primary_shim_path = shim_path(&install_dir, CLI_NAME);
    let bundled = tools.cli_path.is_file() && tools.engine_path.is_file();
    let installed = cli_tools_installed(&install_dir);
    let path_configured = path_env
        .as_deref()
        .map(|path| path_contains_entry(path, &install_dir))
        .unwrap_or(false)
        || user_path_configuration_mentions(&install_dir);
    let message = match (bundled, installed, path_configured) {
        (false, _, _) => "Bundled CLI tools are not available in this app build.".to_string(),
        (true, false, _) => "CLI tools are bundled but not installed in PATH.".to_string(),
        (true, true, false) => {
            "CLI shims are installed; restart your terminal or add the install directory to PATH."
                .to_string()
        }
        (true, true, true) => {
            "assetiweave-cli and aiwc are installed and reachable from PATH.".to_string()
        }
    };

    CliToolsStatus {
        bundled,
        installed,
        path_configured,
        install_dir: install_dir.to_string_lossy().to_string(),
        path_entry: install_dir.to_string_lossy().to_string(),
        shim_path: primary_shim_path.to_string_lossy().to_string(),
        bundled_cli_path: tools
            .cli_path
            .is_file()
            .then(|| tools.cli_path.to_string_lossy().to_string()),
        bundled_engine_path: tools
            .engine_path
            .is_file()
            .then(|| tools.engine_path.to_string_lossy().to_string()),
        message,
    }
}

fn cli_tools_installed(install_dir: &Path) -> bool {
    shim_path(install_dir, CLI_NAME).is_file()
        && shim_path(install_dir, CLI_ALIAS).is_file()
        && shim_path(install_dir, ENGINE_NAME).is_file()
}

struct BundledTools {
    dir: PathBuf,
    cli_path: PathBuf,
    engine_path: PathBuf,
}

fn bundled_tools(resource_dir: &Path) -> BundledTools {
    let dir = resource_dir.join(TOOL_DIR);
    BundledTools {
        cli_path: dir.join(executable_name(CLI_NAME)),
        engine_path: dir.join(executable_name(ENGINE_NAME)),
        dir,
    }
}

fn executable_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn default_install_dir() -> AppResult<PathBuf> {
    #[cfg(windows)]
    {
        dirs::data_local_dir()
            .map(|dir| dir.join("AssetIWeave").join("bin"))
            .ok_or_else(|| {
                AppError::NotFound("resolve local application data directory".to_string())
            })
    }
    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .map(|home| home.join(".local").join("bin"))
            .ok_or_else(|| AppError::NotFound("resolve home directory".to_string()))
    }
}

fn fallback_install_dir() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"%LOCALAPPDATA%\AssetIWeave\bin")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("~/.local/bin")
    }
}

fn shim_path(install_dir: &Path, tool_name: &str) -> PathBuf {
    #[cfg(windows)]
    {
        install_dir.join(format!("{tool_name}.cmd"))
    }
    #[cfg(not(windows))]
    {
        install_dir.join(tool_name)
    }
}

fn write_shim(install_dir: &Path, tool_name: &str, target: &Path) -> AppResult<()> {
    let path = shim_path(install_dir, tool_name);
    #[cfg(windows)]
    let contents = windows_cmd_contents(target);
    #[cfg(not(windows))]
    let contents = unix_shim_contents(target);
    fs::write(&path, contents)
        .map_err(|error| format!("write {}: {error}", path.display()))
        .map_err(AppError::external)?;
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&path)
            .map_err(|error| format!("read {} permissions: {error}", path.display()))
            .map_err(AppError::external)?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)
            .map_err(|error| format!("set {} executable: {error}", path.display()))
            .map_err(AppError::external)?;
    }
    Ok(())
}

fn unix_shim_contents(target: &Path) -> String {
    format!(
        "#!/bin/sh\nexec {} \"$@\"\n",
        shell_single_quote(&target.to_string_lossy())
    )
}

#[cfg(windows)]
fn windows_cmd_contents(target: &Path) -> String {
    format!("@echo off\r\n\"{}\" %*\r\n", target.display())
}

fn configure_user_path(install_dir: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        configure_windows_user_path(install_dir)
    }
    #[cfg(not(windows))]
    {
        configure_unix_user_path(install_dir)
    }
}

#[cfg(not(windows))]
fn configure_unix_user_path(install_dir: &Path) -> AppResult<()> {
    let home =
        dirs::home_dir().ok_or_else(|| AppError::NotFound("resolve home directory".to_string()))?;
    let profiles = shell_profile_paths(&home);
    let path_text = install_dir.to_string_lossy();
    let quoted_path = shell_double_quote(&path_text);
    let marker = "# AssetIWeave CLI";
    let block = format!(
        "\n{marker}\ncase \":$PATH:\" in\n  *\":{path_text}:\"*) ;;\n  *) export PATH={quoted_path}:$PATH ;;\nesac\n"
    );
    let mut wrote_profile = false;

    for (index, profile) in profiles.iter().enumerate() {
        if index > 0 && !profile.exists() {
            continue;
        }
        let existing = fs::read_to_string(profile).unwrap_or_default();
        if !existing.contains(marker) && !existing.contains(path_text.as_ref()) {
            fs::write(profile, format!("{existing}{block}"))
                .map_err(|error| format!("update shell profile {}: {error}", profile.display()))
                .map_err(AppError::external)?;
        }
        wrote_profile = true;
    }

    if !wrote_profile {
        return Err(AppError::External(
            "no writable shell profile was available".to_string(),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn shell_profile_paths(home: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![
            home.join(".zprofile"),
            home.join(".zshrc"),
            home.join(".profile"),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        vec![
            home.join(".profile"),
            home.join(".bashrc"),
            home.join(".zshrc"),
        ]
    }
}

#[cfg(windows)]
fn configure_windows_user_path(install_dir: &Path) -> AppResult<()> {
    let script = format!(
        "$dir = '{}'; $current = [Environment]::GetEnvironmentVariable('Path', 'User'); if ([string]::IsNullOrWhiteSpace($current)) {{ [Environment]::SetEnvironmentVariable('Path', $dir, 'User') }} elseif (((';' + $current + ';').ToLowerInvariant()).IndexOf((';' + $dir + ';').ToLowerInvariant()) -lt 0) {{ [Environment]::SetEnvironmentVariable('Path', $dir + ';' + $current, 'User') }}",
        powershell_single_quote(&install_dir.to_string_lossy())
    );
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &script,
    ]);
    configure_background_process(&mut command);
    let status = command
        .status()
        .map_err(|error| AppError::Process(format!("update user PATH: {error}")))?;
    if !status.success() {
        return Err(AppError::Process(format!(
            "update user PATH exited with {status}"
        )));
    }
    Ok(())
}

fn current_path_env() -> Option<String> {
    env::var_os("PATH").map(|value| value.to_string_lossy().to_string())
}

fn path_contains_entry(path_env: &str, entry: &Path) -> bool {
    let entry = normalize_path_text(entry);
    env::split_paths(path_env).any(|path| normalize_path_text(&path) == entry)
}

fn user_path_configuration_mentions(install_dir: &Path) -> bool {
    #[cfg(windows)]
    {
        let process_path_configured = current_path_env()
            .as_deref()
            .map(|path| path_contains_entry(path, install_dir))
            .unwrap_or(false);
        process_path_configured
            || windows_user_path().is_some_and(|path| path_contains_entry(&path, install_dir))
    }
    #[cfg(not(windows))]
    {
        let Some(home) = dirs::home_dir() else {
            return false;
        };
        let install_dir = install_dir.to_string_lossy();
        shell_profile_paths(&home)
            .iter()
            .filter_map(|path| fs::read_to_string(path).ok())
            .any(|content| content.contains(install_dir.as_ref()))
    }
}

#[cfg(windows)]
fn windows_user_path() -> Option<String> {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-Command",
        "[Environment]::GetEnvironmentVariable('Path', 'User')",
    ]);
    configure_background_process(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!path.is_empty()).then_some(path)
}

fn normalize_path_text(path: &Path) -> String {
    #[cfg(windows)]
    {
        path.to_string_lossy().replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().to_string()
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(not(windows))]
fn shell_double_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(windows)]
fn powershell_single_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_contains_entry_matches_whole_path_segment() {
        let wanted = PathBuf::from("/Users/example/.local/bin");
        let path_env = env::join_paths([
            Path::new("/usr/bin"),
            Path::new("/Users/example/.local/bin"),
            Path::new("/bin"),
        ])
        .unwrap();
        assert!(path_contains_entry(&path_env.to_string_lossy(), &wanted));
        let bad_path_env = env::join_paths([
            Path::new("/usr/bin"),
            Path::new("/Users/example/.local/bin-extra"),
            Path::new("/bin"),
        ])
        .unwrap();
        assert!(!path_contains_entry(
            &bad_path_env.to_string_lossy(),
            &wanted
        ));
    }

    #[test]
    fn unix_shim_execs_bundled_tool_with_original_arguments() {
        let shim = unix_shim_contents(Path::new(
            "/Applications/AssetIWeave.app/Contents/Resources/cli/assetiweave-cli",
        ));
        assert!(shim.contains(
            "exec '/Applications/AssetIWeave.app/Contents/Resources/cli/assetiweave-cli' \"$@\""
        ));
    }

    #[test]
    fn short_cli_shim_execs_the_canonical_binary() {
        assert_eq!(CLI_ALIAS, "aiwc");
        let root = env::temp_dir().join(format!(
            "assetiweave-cli-alias-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create install dir");
        let target =
            Path::new("/Applications/AssetIWeave.app/Contents/Resources/cli/assetiweave-cli");
        write_shim(&root, CLI_NAME, target).expect("write canonical shim");
        write_shim(&root, CLI_ALIAS, target).expect("write short shim");
        assert_eq!(
            fs::read_to_string(shim_path(&root, CLI_NAME)).expect("read canonical shim"),
            fs::read_to_string(shim_path(&root, CLI_ALIAS)).expect("read short shim")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installed_status_requires_the_short_cli_shim() {
        let root = env::temp_dir().join(format!(
            "assetiweave-cli-shortcut-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create install dir");
        fs::write(shim_path(&root, CLI_NAME), "").expect("write cli shim");
        fs::write(shim_path(&root, ENGINE_NAME), "").expect("write engine shim");
        assert!(!cli_tools_installed(&root));

        fs::write(shim_path(&root, CLI_ALIAS), "").expect("write alias shim");
        assert!(cli_tools_installed(&root));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shell_single_quote_escapes_embedded_quotes() {
        assert_eq!(shell_single_quote("/tmp/it's/cli"), "'/tmp/it'\\''s/cli'");
    }

    #[test]
    fn status_detects_tauri_resource_layout() {
        let root = env::temp_dir().join(format!(
            "assetiweave-cli-tools-test-{}",
            uuid::Uuid::new_v4()
        ));
        let tool_dir = root.join("bundled-cli").join("cli");
        fs::create_dir_all(&tool_dir).expect("create tool dir");
        fs::write(tool_dir.join(executable_name(CLI_NAME)), "").expect("write cli");
        fs::write(tool_dir.join(executable_name(ENGINE_NAME)), "").expect("write engine");

        let status = build_status(
            &root,
            Some(
                default_install_dir()
                    .unwrap_or_else(|_| fallback_install_dir())
                    .to_string_lossy()
                    .to_string(),
            ),
        );
        assert!(status.bundled);
        assert!(status
            .bundled_cli_path
            .as_deref()
            .is_some_and(|path| path.contains("bundled-cli")));

        let _ = fs::remove_dir_all(root);
    }
}
