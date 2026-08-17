use std::{path::PathBuf, process::Command};

use crate::backend::host_process::resolve_host_executable;

use super::{run_host_command, InstallContext, InstallError, Installer};
use crate::backend::agent_market::types::{Distribution, MaterializedRuntime, Ownership};

#[derive(Clone, Debug, Default)]
pub(crate) struct SystemInstaller {
    pub(crate) resolver: Option<PathBuf>,
}

impl Installer for SystemInstaller {
    fn materialize(
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
        let version = probe_version(&program, version_args, context)?;
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

fn probe_version(
    program: &PathBuf,
    args: &[String],
    context: &InstallContext,
) -> Result<String, InstallError> {
    let mut command = Command::new(program);
    command.args(args);
    let output = run_host_command(&mut command, context, 1024 * 1024, 256 * 1024)?;
    if !output.status.success() {
        return Err(InstallError::Failed(
            "system version probe failed".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_materialization_never_claims_an_owned_directory() {
        let directory = tempfile_dir();
        let executable = directory.join("agent");
        std::fs::write(&executable, b"#!/bin/sh\necho agent 1.0.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let installer = SystemInstaller {
            resolver: Some(executable.clone()),
        };
        let context = InstallContext::new(directory.join("staging"), "1.0.0");
        let distribution = Distribution::System {
            id: "system".to_string(),
            priority: 10,
            command_candidates: vec!["agent".to_string()],
            version_args: vec![],
            version_range: ">=1.0.0".to_string(),
            launch_args: vec!["acp".to_string()],
            model_discovery_args: None,
        };
        let runtime = installer.materialize(&distribution, &context).unwrap();
        assert_eq!(runtime.ownership, Ownership::System);
        assert!(runtime.install_dir.is_none());
        assert_eq!(runtime.resolved_program, executable);
    }

    fn tempfile_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "assetiweave-system-installer-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
}
