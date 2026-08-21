use std::{collections::HashMap, path::PathBuf};

use super::types::{CatalogItem, Distribution, DistributionCandidate, DistributionType};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SystemObservation {
    pub(crate) resolved_program: Option<PathBuf>,
    pub(crate) version: Option<String>,
    pub(crate) error_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DistributionSelectionContext {
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) node_available: bool,
    pub(crate) npm_available: bool,
    pub(crate) uv_available: bool,
    pub(crate) system: HashMap<String, SystemObservation>,
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

pub(crate) struct DistributionSelector;

impl DistributionSelector {
    pub(crate) fn select(
        item: &CatalogItem,
        context: &DistributionSelectionContext,
        explicit_distribution_id: Option<&str>,
    ) -> Result<Vec<DistributionCandidate>, String> {
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
                return Err(format!("distribution_unsupported: {explicit}"));
            };
            if !selected.selectable {
                return Err(selected
                    .reason_code
                    .clone()
                    .unwrap_or_else(|| "distribution_unsupported".to_string()));
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
        for candidate in &mut candidates {
            candidate.recommended = recommended_id
                .as_deref()
                .is_some_and(|id| id == candidate.distribution_id);
        }
        Ok(candidates)
    }
}

fn candidate(
    item: &CatalogItem,
    distribution: &Distribution,
    context: &DistributionSelectionContext,
) -> DistributionCandidate {
    let distribution_type = distribution.distribution_type();
    let mut result = DistributionCandidate {
        distribution_id: distribution.id().to_string(),
        distribution_type: distribution_type.clone(),
        selectable: true,
        recommended: false,
        ownership: distribution_type.ownership(),
        reason_code: None,
        required_runtime: None,
        resolved_version: None,
        download_size: None,
        target_path: None,
    };

    match distribution {
        Distribution::System {
            command_candidates,
            version_range: _,
            ..
        } => {
            let observation = command_candidates
                .iter()
                .find_map(|command| context.system.get(command));
            let Some(observation) = observation else {
                result.selectable = false;
                result.reason_code = Some("runtime_missing".to_string());
                return result;
            };
            result.target_path = observation.resolved_program.clone();
            result.resolved_version = observation.version.clone();
            if observation.resolved_program.is_none() {
                result.selectable = false;
                result.reason_code = Some(
                    observation
                        .error_code
                        .clone()
                        .unwrap_or_else(|| "runtime_missing".to_string()),
                );
            }
        }
        Distribution::Binary { target, size, .. } => {
            result.download_size = *size;
            if normalized_os(&target.os) != context.os
                || normalized_arch(&target.arch) != context.arch
            {
                result.selectable = false;
                result.reason_code = Some("distribution_unsupported".to_string());
            } else {
                result.resolved_version = Some(item.version.clone());
            }
        }
        Distribution::Npx {
            node_range,
            version,
            ..
        } => {
            result.resolved_version = Some(version.clone());
            if !context.node_available || !context.npm_available {
                result.selectable = false;
                result.reason_code = Some("runtime_missing".to_string());
                result.required_runtime = Some("node_and_npm".to_string());
            } else if let Some(range) = node_range {
                result.required_runtime = Some(format!("node {range}"));
            }
        }
        Distribution::Uvx {
            python_range,
            version,
            ..
        } => {
            result.resolved_version = Some(version.clone());
            if !context.uv_available {
                result.selectable = false;
                result.reason_code = Some("runtime_missing".to_string());
                result.required_runtime = Some("uv".to_string());
            } else if let Some(range) = python_range {
                result.required_runtime = Some(format!("python {range}"));
            }
        }
    }
    result
}

fn type_rank(distribution_type: &DistributionType) -> u8 {
    match distribution_type {
        DistributionType::System => 10,
        DistributionType::Binary => 20,
        DistributionType::Npx => 30,
        DistributionType::Uvx => 40,
    }
}

pub(crate) fn normalized_arch(arch: &str) -> String {
    match arch {
        "arm64" | "aarch64" => "aarch64".to_string(),
        "x86_64" | "amd64" => "x86_64".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn normalized_os(os: &str) -> String {
    match os {
        "macos" | "darwin" => "darwin".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::agent_market::catalog::bundled_catalog;

    #[test]
    fn recommends_platform_binary_when_no_system_distribution_exists() {
        let item = bundled_catalog()
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.id == "opencode")
            .unwrap();
        let context = DistributionSelectionContext {
            os: "darwin".to_string(),
            arch: "aarch64".to_string(),
            node_available: true,
            npm_available: true,
            uv_available: false,
            system: HashMap::from([(
                "opencode".to_string(),
                SystemObservation {
                    resolved_program: Some(PathBuf::from("/usr/local/bin/opencode")),
                    version: Some("1.2.0".to_string()),
                    error_code: None,
                },
            )]),
        };
        let candidates = DistributionSelector::select(&item, &context, None).unwrap();
        assert_eq!(candidates[0].distribution_id, "binary-darwin-aarch64");
        assert!(candidates[0].recommended);
        assert!(candidates
            .iter()
            .any(|candidate| candidate.distribution_type == DistributionType::Binary));
    }

    #[test]
    fn rust_macos_host_name_matches_darwin_catalog_target() {
        let item = bundled_catalog()
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.id == "opencode")
            .unwrap();
        let context = DistributionSelectionContext {
            os: normalized_os("macos"),
            arch: normalized_arch("aarch64"),
            node_available: false,
            npm_available: false,
            uv_available: false,
            system: HashMap::new(),
        };

        let candidate = DistributionSelector::select(&item, &context, None)
            .unwrap()
            .remove(0);

        assert!(candidate.selectable);
        assert!(candidate.recommended);
    }

    #[test]
    fn explicit_unavailable_choice_is_not_silently_replaced() {
        let item = bundled_catalog()
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.id == "qoder")
            .unwrap();
        let error = DistributionSelector::select(
            &item,
            &DistributionSelectionContext::default(),
            Some("npx-qoder"),
        )
        .expect_err("missing uv must reject explicit choice");
        assert_eq!(error, "runtime_missing");
    }

    #[test]
    fn npx_requires_both_node_and_npm() {
        let item = bundled_catalog()
            .unwrap()
            .items
            .into_iter()
            .find(|item| item.id == "claude")
            .unwrap();
        let mut context = DistributionSelectionContext::default();
        context.node_available = true;
        context.npm_available = false;
        let candidate = DistributionSelector::select(&item, &context, None)
            .unwrap()
            .remove(0);
        assert!(!candidate.selectable);
        assert_eq!(candidate.reason_code.as_deref(), Some("runtime_missing"));
    }
}
