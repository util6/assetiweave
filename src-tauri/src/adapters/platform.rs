use crate::backend::{dto::AppResult, path_utils::expand_path};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

struct FileManagerInvocation {
    program: &'static str,
    args: Vec<OsString>,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum FileManagerPlatform {
    Macos,
    Windows,
    Linux,
}

pub(crate) fn reveal_path(path: String) -> AppResult<()> {
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

fn resolve_reveal_path(path: &str) -> AppResult<PathBuf> {
    let path = expand_path(path)?;
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    Ok(path)
}

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

fn command_status(invocation: &FileManagerInvocation) -> AppResult<()> {
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
