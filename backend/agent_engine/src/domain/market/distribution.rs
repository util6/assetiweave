use std::{collections::HashMap, path::PathBuf};

use super::types::{CatalogItem, Distribution, DistributionCandidate, DistributionType};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DistributionError {
    #[error("The selected Agent distribution is unavailable on this platform.")]
    Unsupported {
        code: String,
        message: String,
        agent_id: Option<String>,
        distribution_id: Option<String>,
    },
    #[error("The selected Agent distribution requires a runtime that is not installed.")]
    RuntimeMissing {
        agent_id: Option<String>,
        distribution_id: Option<String>,
    },
    #[error("The selected system Agent runtime could not be used.")]
    SystemVersionIncompatible {
        agent_id: Option<String>,
        distribution_id: Option<String>,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemObservation {
    pub resolved_program: Option<PathBuf>,
    pub version: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionSelectionContext {
    pub os: String,
    pub arch: String,
    pub node_available: bool,
    pub npm_available: bool,
    pub uv_available: bool,
    pub system: HashMap<String, SystemObservation>,
}

impl Default for DistributionSelectionContext {
    fn default() -> Self {
        Self {
            os: normalized_os(std::env::consts::OS),
            arch: normalized_arch(std::env::consts::ARCH),
            node_available: false,
            npm_available: false,
            uv_available: false,
            system: HashMap::new(),
        }
    }
}

pub struct DistributionSelector;

impl DistributionSelector {
    pub fn select(
        item: &CatalogItem,
        context: &DistributionSelectionContext,
        explicit_distribution_id: Option<&str>,
    ) -> Result<Vec<DistributionCandidate>, DistributionError> {
        let mut candidates = item
            .distributions
            .iter()
            .map(|distribution| candidate(item, distribution, context))
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| {
            type_rank(&left.distribution_type)
                .cmp(&type_rank(&right.distribution_type))
                .then(left.distribution_id.cmp(&right.distribution_id))
        });

        if let Some(explicit) = explicit_distribution_id {
            let Some(selected) = candidates
                .iter()
                .find(|candidate| candidate.distribution_id == explicit)
            else {
                return Err(DistributionError::Unsupported {
                    code: "distribution_unsupported".to_string(),
                    message: "The selected Agent distribution is unavailable on this platform."
                        .to_string(),
                    agent_id: Some(item.id.clone()),
                    distribution_id: Some(explicit.to_string()),
                });
            };
            if !selected.selectable {
                let code = selected
                    .reason_code
                    .clone()
                    .unwrap_or_else(|| "distribution_unsupported".to_string());
                return match code.as_str() {
                    "runtime_missing" => Err(DistributionError::RuntimeMissing {
                        agent_id: Some(item.id.clone()),
                        distribution_id: Some(explicit.to_string()),
                    }),
                    "system_version_incompatible" => {
                        Err(DistributionError::SystemVersionIncompatible {
                            agent_id: Some(item.id.clone()),
                            distribution_id: Some(explicit.to_string()),
                        })
                    }
                    _ => Err(DistributionError::Unsupported {
                        code,
                        message: "The selected Agent distribution is unavailable on this platform."
                            .to_string(),
                        agent_id: Some(item.id.clone()),
                        distribution_id: Some(explicit.to_string()),
                    }),
                };
            }
            for candidate in &mut candidates {
                candidate.recommended = candidate.distribution_id == explicit;
            }
            return Ok(candidates);
        }

        let recommended_id = candidates
            .iter()
            .find(|candidate| candidate.selectable)
            .map(|candidate| candidate.distribution_id.clone());
        if let Some(id) = recommended_id {
            for candidate in &mut candidates {
                candidate.recommended = candidate.distribution_id == id;
            }
        }
        Ok(candidates)
    }
}

fn candidate(
    item: &CatalogItem,
    distribution: &Distribution,
    context: &DistributionSelectionContext,
) -> DistributionCandidate {
    match distribution {
        Distribution::System {
            id,
            command_candidates,
            version_range,
            ..
        } => {
            let observation = context.system.get(id);
            let resolved_program = observation.and_then(|obs| obs.resolved_program.clone());
            let resolved_version = observation.and_then(|obs| obs.version.clone());
            let error_code = observation.and_then(|obs| obs.error_code.clone());
            let selectable = resolved_program.is_some()
                && (version_range.is_empty()
                    || resolved_version.as_deref().is_some_and(|v| {
                        semver::VersionReq::parse(version_range)
                            .and_then(|req| semver::Version::parse(v).map(|ver| req.matches(&ver)))
                            .unwrap_or(true)
                    }));
            let reason_code = if selectable {
                None
            } else if error_code.as_deref() == Some("version_incompatible") {
                Some("system_version_incompatible".to_string())
            } else {
                Some("runtime_missing".to_string())
            };
            DistributionCandidate {
                distribution_id: id.clone(),
                distribution_type: DistributionType::System,
                selectable,
                recommended: false,
                ownership: DistributionType::System.ownership(),
                reason_code,
                required_runtime: command_candidates.first().cloned(),
                resolved_version,
                download_size: None,
                target_path: resolved_program,
            }
        }
        Distribution::Binary {
            id, target, size, ..
        } => {
            let os_matches = target.os == context.os;
            let arch_matches = target.arch == context.arch;
            let selectable = os_matches && arch_matches;
            DistributionCandidate {
                distribution_id: id.clone(),
                distribution_type: DistributionType::Binary,
                selectable,
                recommended: false,
                ownership: DistributionType::Binary.ownership(),
                reason_code: (!selectable).then(|| "binary_target_unsupported".to_string()),
                required_runtime: None,
                resolved_version: Some(item.version.clone()),
                download_size: *size,
                target_path: None,
            }
        }
        Distribution::Npx {
            id,
            version,
            node_range,
            ..
        } => {
            let selectable = context.node_available && context.npm_available;
            let reason_code = if !selectable {
                Some("runtime_missing".to_string())
            } else {
                None
            };
            DistributionCandidate {
                distribution_id: id.clone(),
                distribution_type: DistributionType::Npx,
                selectable,
                recommended: false,
                ownership: DistributionType::Npx.ownership(),
                reason_code,
                required_runtime: node_range
                    .as_ref()
                    .map(|r| format!("node {r}"))
                    .or_else(|| Some("node".to_string())),
                resolved_version: Some(version.clone()),
                download_size: None,
                target_path: None,
            }
        }
        Distribution::Uvx {
            id,
            version,
            python_range,
            ..
        } => {
            let selectable = context.uv_available;
            let reason_code = if !selectable {
                Some("runtime_missing".to_string())
            } else {
                None
            };
            DistributionCandidate {
                distribution_id: id.clone(),
                distribution_type: DistributionType::Uvx,
                selectable,
                recommended: false,
                ownership: DistributionType::Uvx.ownership(),
                reason_code,
                required_runtime: python_range
                    .as_ref()
                    .map(|r| format!("python {r}"))
                    .or_else(|| Some("python".to_string())),
                resolved_version: Some(version.clone()),
                download_size: None,
                target_path: None,
            }
        }
    }
}

fn type_rank(distribution_type: &DistributionType) -> u8 {
    match distribution_type {
        DistributionType::System => 0,
        DistributionType::Binary => 1,
        DistributionType::Npx => 2,
        DistributionType::Uvx => 3,
    }
}

pub fn normalized_os(value: &str) -> String {
    match value {
        "macos" | "darwin" => "darwin".to_string(),
        "windows" => "windows".to_string(),
        "linux" => "linux".to_string(),
        other => other.to_string(),
    }
}

pub fn normalized_arch(value: &str) -> String {
    match value {
        "x86_64" | "amd64" => "x86_64".to_string(),
        "aarch64" | "arm64" => "aarch64".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
#[path = "distribution_tests.rs"]
mod tests;
