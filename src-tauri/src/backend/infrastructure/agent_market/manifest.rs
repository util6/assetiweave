use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::error::AgentMarketError;
use crate::backend::domain::agents::{
    AgentInstallation, InstallationStatus, ProtocolStatus, RuntimeStatus,
};
use crate::backend::infrastructure::extensions::{
    Compatibility, EnvEntry, PackageIdentity, ProbeKind, ProbeSpec, ProcessInvocation,
    RuntimeProgramKind, TrustGate,
};

#[derive(Clone, Debug)]
pub(crate) struct AgentPackageManifest {
    pub(crate) identity: PackageIdentity,
    pub(crate) compatibility: Compatibility,
    pub(crate) invocation: ProcessInvocation,
    pub(crate) availability_probe: ProbeSpec,
    pub(crate) model_discovery_probe: Option<ProbeSpec>,
}

use crate::backend::domain::agents::market::VerificationStatus;

impl TrustGate for VerificationStatus {
    #[cfg(test)]
    fn can_enable(&self) -> bool {
        true
    }

    fn needs_confirmation(&self) -> bool {
        matches!(self, Self::Experimental)
    }

    #[cfg(test)]
    fn integrity_changed(&self) -> bool {
        false
    }
}

pub(crate) trait AgentInstallationExt {
    fn package_manifest(&self) -> Result<AgentPackageManifest, AgentMarketError>;
    fn process_invocation(&self) -> ProcessInvocation;
    fn package_identity(&self) -> Result<PackageIdentity, AgentMarketError>;
    fn installed(&self) -> bool;
    fn connected(&self) -> bool;
    fn execution_ready(&self) -> bool;
}

impl AgentInstallationExt for AgentInstallation {
    fn package_identity(&self) -> Result<PackageIdentity, AgentMarketError> {
        let version = semver::Version::parse(&self.agent_version).map_err(|e| {
            AgentMarketError::new(
                "invalid_version",
                &format!("invalid semver version {}: {e}", self.agent_version),
                false,
            )
        })?;
        Ok(PackageIdentity {
            kind: crate::backend::infrastructure::extensions::PackageKind::Agent,
            package_id: self.agent_id.clone(),
            version,
        })
    }

    fn process_invocation(&self) -> ProcessInvocation {
        let env = self
            .definition_json
            .get("env")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                Some(EnvEntry {
                    key: entry.get("name")?.as_str()?.to_string(),
                    value: entry.get("value")?.as_str()?.to_string(),
                })
            })
            .collect();
        ProcessInvocation {
            kind: RuntimeProgramKind::Executable,
            entry: self.resolved_program.to_string_lossy().to_string(),
            args: self.args.clone(),
            env,
            working_dir: self.install_dir.clone(),
            version_req: None,
            immutable_install_dir: self.install_dir.clone().unwrap_or_else(|| {
                self.resolved_program
                    .parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf()
            }),
        }
    }

    fn package_manifest(&self) -> Result<AgentPackageManifest, AgentMarketError> {
        let identity = self.package_identity()?;
        let invocation = self.process_invocation();
        let availability_probe = ProbeSpec {
            program: Some(invocation.entry.clone()),
            args: vec!["--version".to_string()],
            env: invocation.env.clone(),
            timeout: Duration::from_secs(8),
            output_limit: 1024 * 1024,
            kind: ProbeKind::Availability,
        };
        let model_discovery_probe = self
            .definition_json
            .get("model_discovery_args")
            .or_else(|| self.definition_json.get("modelDiscoveryArgs"))
            .and_then(serde_json::Value::as_array)
            .map(|args| ProbeSpec {
                program: Some(invocation.entry.clone()),
                args: args
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_string)
                    .collect(),
                env: invocation.env.clone(),
                timeout: Duration::from_secs(8),
                output_limit: 1024 * 1024,
                kind: ProbeKind::ModelDiscovery,
            });
        Ok(AgentPackageManifest {
            identity,
            compatibility: Compatibility {
                protocol_version: 1,
                core_requirement: None,
            },
            invocation,
            availability_probe,
            model_discovery_probe,
        })
    }

    fn installed(&self) -> bool {
        true
    }

    fn connected(&self) -> bool {
        self.enabled
            && self.installation_status == InstallationStatus::Ready
            && self.protocol_status == ProtocolStatus::Ready
    }

    fn execution_ready(&self) -> bool {
        self.installed()
            && self.enabled
            && self.installation_status == InstallationStatus::Ready
            && self.runtime_status == RuntimeStatus::Ready
            && self.protocol_status == ProtocolStatus::Ready
    }
}
