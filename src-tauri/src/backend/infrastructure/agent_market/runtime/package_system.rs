use std::path::Path;

use crate::backend::{
    domain::agents::AgentInstallation,
    infrastructure::{
        agent_market::{error::AgentMarketError, manifest::AgentInstallationExt},
        extensions::{DomainPackageSystem, ExtensionError, InspectedPackage, PackageKind},
    },
};

/// Agent Market's domain seam over the installation record. ACP/native
/// manifest details stay here; the kernel only receives the normalized
/// identity and compatibility projection.
pub(crate) struct AgentPackageSystem {
    manifest: crate::backend::infrastructure::agent_market::manifest::AgentPackageManifest,
}

impl AgentPackageSystem {
    pub(crate) fn from_installation(
        installation: &AgentInstallation,
    ) -> Result<Self, AgentMarketError> {
        Ok(Self {
            manifest: installation.package_manifest()?,
        })
    }
}

impl DomainPackageSystem for AgentPackageSystem {
    fn kind(&self) -> PackageKind {
        PackageKind::Agent
    }

    fn inspect(&self, dir: &Path) -> Result<InspectedPackage, ExtensionError> {
        if !dir.exists() {
            return Err(ExtensionError::ManifestInvalid {
                package_id: self.manifest.identity.package_id.clone(),
                reason: format!("Agent install directory does not exist: {}", dir.display()),
            });
        }
        Ok(InspectedPackage {
            identity: self.manifest.identity.clone(),
            compatibility: self.manifest.compatibility.clone(),
            invocation: self.manifest.invocation.clone(),
            availability_probe: self.manifest.availability_probe.clone(),
            model_discovery_probe: self.manifest.model_discovery_probe.clone(),
            install_dir: dir.to_path_buf(),
        })
    }
}
