use std::{path::PathBuf, process::Command};

use crate::backend::host_process::resolve_host_executable;

use super::{run_host_command, InstallContext, InstallError, Installer};
use crate::backend::agent_market::types::{Distribution, MaterializedRuntime, Ownership};

#[derive(Clone, Debug, Default)]
pub(crate) struct SystemInstaller {
    pub(crate) resolver: Option<PathBuf>,
}

impl Installer for SystemInstaller {
    async fn materialize(
        &self,
        distribution: &Distribution,
        context: &InstallContext,
    ) -> Result<MaterializedRuntime, InstallError> {
        let Distribution::System {
            command_candidates,
            version_args,
            launch_args,
            ..
        } = distribution
        else {
            return Err(InstallError::Unsupported(
                "system installer received a non-system distribution".to_string(),
            ));
        };
        let program = self
            .resolver
            .clone()
            .or_else(|| {
                command_candidates
                    .iter()
                    .find_map(|command| resolve_host_executable(command))
            })
            .ok_or_else(|| {
                InstallError::RuntimeMissing("system executable is not installed".to_string())
            })?;
        let version = probe_version(&program, version_args, context).await?;
        if version.trim().is_empty() {
            return Err(InstallError::Failed(
                "system version probe returned no version".to_string(),
            ));
        }
        Ok(MaterializedRuntime {
            installation_id: context.installation_id.clone(),
            ownership: Ownership::System,
            install_dir: None,
            resolved_program: program,
            args: launch_args.clone(),
            env: Vec::new(),
            integrity: None,
            version,
        })
    }
}

async fn probe_version(
    program: &PathBuf,
    args: &[String],
    context: &InstallContext,
) -> Result<String, InstallError> {
    let mut command = Command::new(program);
    command.args(args);
    let output = run_host_command(&mut command, context, 1024 * 1024, 256 * 1024).await?;
    if !output.status.success() {
        return Err(InstallError::Failed(
            "system version probe failed".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
#[path = "system_tests.rs"]
mod tests;
