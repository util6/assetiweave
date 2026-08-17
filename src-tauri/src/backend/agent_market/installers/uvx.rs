use std::{path::PathBuf, process::Command};

use crate::backend::host_process::resolve_host_executable;

use super::{
    ensure_staging_root, is_cancelled, run_host_command, runtime_from_local, InstallContext,
    InstallError, Installer,
};
use crate::backend::agent_market::types::{Distribution, MaterializedRuntime};

#[derive(Clone, Debug, Default)]
pub(crate) struct UvxInstaller {
    pub(crate) uv_path: Option<PathBuf>,
}

impl UvxInstaller {
    pub(crate) fn uv_args(
        package: &str,
        version: &str,
        tool_dir: &std::path::Path,
        bin_dir: &std::path::Path,
    ) -> Vec<String> {
        vec![
            "tool".to_string(),
            "install".to_string(),
            format!("{package}=={version}"),
            "--force".to_string(),
            "--no-cache".to_string(),
            "--directory".to_string(),
            tool_dir.to_string_lossy().to_string(),
            "--bin-dir".to_string(),
            bin_dir.to_string_lossy().to_string(),
        ]
    }
}

impl Installer for UvxInstaller {
    fn materialize(
        &self,
        distribution: &Distribution,
        context: &InstallContext,
    ) -> Result<MaterializedRuntime, InstallError> {
        let Distribution::Uvx {
            package,
            version,
            command: entry,
            launch_args,
            ..
        } = distribution
        else {
            return Err(InstallError::Unsupported(
                "uvx installer received a non-uvx distribution".to_string(),
            ));
        };
        let uv = self
            .uv_path
            .clone()
            .or_else(|| resolve_host_executable("uv"))
            .ok_or_else(|| {
                InstallError::RuntimeMissing("uv is required for Uvx installation".to_string())
            })?;
        if is_cancelled(context) {
            return Err(InstallError::Cancelled);
        }
        ensure_staging_root(&context.staging_dir)?;
        let tool_dir = context.staging_dir.join("tool");
        let bin_dir = context.staging_dir.join("bin");
        let args = Self::uv_args(package, version, &tool_dir, &bin_dir);
        let mut command = Command::new(uv);
        command.args(&args);
        command
            .env("UV_TOOL_DIR", &tool_dir)
            .env("UV_TOOL_BIN_DIR", &bin_dir);
        let output = run_host_command(&mut command, context, 1024 * 1024, 256 * 1024)?;
        if !output.status.success() {
            return Err(InstallError::Failed("uv tool install failed".to_string()));
        }
        let program = bin_dir.join(entry);
        runtime_from_local(
            context,
            program,
            launch_args.clone(),
            Some(serde_json::json!({ "package": package, "version": version })),
        )
    }
}
