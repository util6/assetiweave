use serde::Deserialize;
use validator::Validate;

use crate::backend::{
    domain::agents::{
        definition::{
            AgentCommandDefinition, AgentDefinition, AgentEnvEntry, AgentId, AgentProtocol,
            DeclaredAgentCapabilities,
        },
        market::CatalogCapabilities,
        AgentInstallation, AgentMarketProtocol, Ownership,
    },
    infrastructure::agent_market::{error::AgentMarketError, manifest::AgentInstallationExt},
};

#[derive(Debug, Deserialize)]
pub(crate) struct ResolvedDefinition {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) protocol: String,
    #[serde(default)]
    pub(crate) capabilities: Option<CatalogCapabilities>,
    #[serde(default, alias = "sessionCleanupArgs")]
    pub(crate) session_cleanup_args: Option<Vec<String>>,
    #[serde(default, alias = "sessionCleanupNotFoundMarkers")]
    pub(crate) session_cleanup_not_found_markers: Vec<String>,
}

#[cfg(test)]
pub(crate) fn definition_json(
    agent_id: &str,
    display_name: &str,
    protocol: &AgentMarketProtocol,
    program: &std::path::Path,
    args: &[String],
) -> serde_json::Value {
    serde_json::json!({
        "id": agent_id,
        "display_name": display_name,
        "protocol": protocol.as_str(),
        "program": program.to_string_lossy(),
        "args": args,
        "env": [],
    })
}

pub(crate) fn definition_from_installation(
    installation: &AgentInstallation,
) -> Result<AgentDefinition, AgentMarketError> {
    let package_manifest = installation.package_manifest()?;
    let resolved: ResolvedDefinition = serde_json::from_value(installation.definition_json.clone())
        .map_err(AgentMarketError::Serialization)?;
    if package_manifest.identity.package_id != resolved.id {
        return Err(AgentMarketError::new(
            "definition_id_mismatch",
            "resolved definition id does not match package identity",
            false,
        ));
    }
    if package_manifest.compatibility.protocol_version != 1 {
        return Err(AgentMarketError::new(
            "unsupported_protocol_version",
            "unsupported Agent package protocol version",
            false,
        ));
    }
    let id = AgentId::parse(resolved.id)
        .map_err(|error| AgentMarketError::new("invalid_agent_id", &error.to_string(), false))?;
    let protocol = match resolved.protocol.as_str() {
        "acp" => AgentProtocol::Acp,
        "native" => AgentProtocol::Native,
        other => {
            return Err(AgentMarketError::new(
                "unsupported_agent_protocol",
                &format!("unsupported agent protocol: {other}"),
                false,
            ))
        }
    };
    let invocation = package_manifest.invocation;
    let availability_probe = package_manifest.availability_probe;
    let model_discovery_probe = package_manifest.model_discovery_probe;
    let program = invocation.entry.clone();
    let expected_program = installation.resolved_program.to_string_lossy();
    if program != expected_program {
        return Err(AgentMarketError::new(
            "definition_program_mismatch",
            "resolved definition program does not match installation record",
            false,
        ));
    }
    let program_path = std::path::PathBuf::from(&program);
    if !program_path.is_file() {
        return Err(AgentMarketError::new(
            "agent_program_missing",
            "resolved Agent program is missing",
            false,
        ));
    }
    if installation.ownership == Ownership::Managed {
        let install_dir = installation.install_dir.as_ref().ok_or_else(|| {
            AgentMarketError::new(
                "missing_install_dir",
                "managed Agent is missing install directory",
                false,
            )
        })?;
        if !program_path.starts_with(install_dir) {
            return Err(AgentMarketError::new(
                "program_escapes_install_dir",
                "resolved Agent program escapes its managed installation",
                false,
            ));
        }
    } else if installation.install_dir.is_some() {
        return Err(AgentMarketError::new(
            "invalid_system_install_dir",
            "system Agent must not have a managed installation directory",
            false,
        ));
    }
    if invocation
        .args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-y" | "npx" | "uvx"))
        || program_path
            .file_name()
            .is_some_and(|name| matches!(name.to_string_lossy().as_ref(), "npx" | "uvx"))
    {
        return Err(AgentMarketError::new(
            "package_manager_invocation_forbidden",
            "runtime definition may not invoke a package manager",
            false,
        ));
    }
    let definition = AgentDefinition {
        id,
        installation_id: Some(installation.installation_id.clone()),
        display_name: resolved.display_name,
        protocol,
        command: program.clone(),
        args: invocation.args,
        env: invocation
            .env
            .into_iter()
            .map(|entry| AgentEnvEntry::new(entry.key, entry.value))
            .collect(),
        declared_capabilities: declared_capabilities_from_catalog(
            resolved.capabilities.as_ref(),
            protocol,
        ),
        availability_probe: Some(AgentCommandDefinition {
            command: availability_probe.program,
            args: availability_probe.args,
        }),
        model_discovery: model_discovery_probe.map(|probe| AgentCommandDefinition {
            command: probe.program,
            args: probe.args,
        }),
        session_cleanup: resolved
            .session_cleanup_args
            .map(AgentCommandDefinition::new),
        session_cleanup_not_found_markers: resolved.session_cleanup_not_found_markers,
    };
    definition
        .validate()
        .map_err(|error| AgentMarketError::new("definition_invalid", &error.to_string(), false))?;
    Ok(definition)
}

fn declared_capabilities_from_catalog(
    capabilities: Option<&CatalogCapabilities>,
    protocol: AgentProtocol,
) -> DeclaredAgentCapabilities {
    let market_protocol = match protocol {
        AgentProtocol::Acp => AgentMarketProtocol::Acp,
        AgentProtocol::Native => AgentMarketProtocol::Native,
    };
    capabilities
        .map(|value| value.to_declared_agent_capabilities(&market_protocol))
        .unwrap_or_else(|| {
            CatalogCapabilities::fallback_for_protocol(&market_protocol)
                .to_declared_agent_capabilities(&market_protocol)
        })
}
