use crate::{path_utils::expand_path, types::AppResult};
use std::process::Command;

pub(crate) fn reveal_path(path: String) -> AppResult<()> {
    let path = expand_path(&path)?;
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }

    #[cfg(target_os = "macos")]
    {
        return command_status(Command::new("open").arg("-R").arg(&path));
    }

    #[cfg(target_os = "windows")]
    {
        return command_status(Command::new("explorer").arg("/select,").arg(&path));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let target = if path.is_dir() {
            path
        } else {
            path.parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf()
        };
        return command_status(Command::new("xdg-open").arg(target));
    }

    #[allow(unreachable_code)]
    Err("unsupported platform".to_string())
}

fn command_status(command: &mut Command) -> AppResult<()> {
    let status = command.status().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("file manager command failed: {status}"))
    }
}
