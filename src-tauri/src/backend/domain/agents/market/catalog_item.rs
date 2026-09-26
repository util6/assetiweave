use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::{
    distribution_spec::Distribution,
    types::{
        validate_max_120_bytes, validate_max_500_bytes, validate_no_null_bytes, validate_non_blank,
        AgentMarketProtocol, CatalogCapabilities, CoreCompatibility, UpstreamSource, Verification,
    },
    validation::{
        is_fixed_version, is_safe_artifact_url, is_safe_command_candidate, is_safe_relative_path,
        is_valid_id, is_valid_npm_package, is_valid_python_project, CatalogValidationError,
    },
};

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize, validator::Validate)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub id: String,
    #[validate(custom(function = "validate_non_blank"))]
    #[validate(custom(function = "validate_max_120_bytes"))]
    pub display_name: String,
    #[validate(custom(function = "validate_non_blank"))]
    #[validate(custom(function = "validate_max_500_bytes"))]
    pub description: String,
    pub protocol: AgentMarketProtocol,
    #[validate(custom(function = "validate_non_blank"))]
    #[validate(custom(function = "validate_max_120_bytes"))]
    #[validate(custom(function = "validate_no_null_bytes"))]
    pub version: String,
    #[serde(default)]
    pub core_compatibility: CoreCompatibility,
    pub capabilities: CatalogCapabilities,
    pub verification: Verification,
    pub upstream: UpstreamSource,
    #[validate(length(min = 1))]
    pub distributions: Vec<Distribution>,
}

impl CatalogItem {
    pub fn validate_basic(&self) -> Result<(), CatalogValidationError> {
        if !is_valid_id(&self.id) {
            return Err(CatalogValidationError::InvalidId(self.id.clone()));
        }
        if let Err(errors) = self.validate() {
            let field_errors = errors.field_errors();
            if field_errors.contains_key("display_name") {
                return Err(CatalogValidationError::InvalidDisplayName(self.id.clone()));
            }
            if field_errors.contains_key("description") {
                return Err(CatalogValidationError::InvalidDescription(self.id.clone()));
            }
            if field_errors.contains_key("version") {
                return Err(CatalogValidationError::InvalidVersion(self.id.clone()));
            }
            if field_errors.contains_key("distributions") {
                return Err(CatalogValidationError::NoDistributions(self.id.clone()));
            }
            return Err(CatalogValidationError::FieldValidation {
                id: self.id.clone(),
                message: format!("{:?}", errors),
            });
        }
        let mut ids = std::collections::HashSet::new();
        for distribution in &self.distributions {
            if !ids.insert(distribution.id()) {
                // duplicate distribution id
                return Err(CatalogValidationError::DuplicateDistributionId(
                    distribution.id().to_string(),
                ));
            }
            if distribution.id().is_empty()
                || distribution
                    .launch_args()
                    .iter()
                    .any(|arg| arg.contains('\0'))
            {
                return Err(CatalogValidationError::InvalidDistribution(
                    distribution.id().to_string(),
                ));
            }
            if let Some(args) = distribution.session_cleanup_args() {
                let placeholder_count = args
                    .iter()
                    .filter(|arg| arg.as_str() == "{session_id}")
                    .count();
                if placeholder_count != 1
                    || args.iter().any(|arg| {
                        arg.contains('\0')
                            || (arg.as_str() != "{session_id}" && arg.contains(['{', '}']))
                    })
                {
                    return Err(CatalogValidationError::InvalidSessionCleanupArgs(
                        distribution.id().to_string(),
                    ));
                }
            }
            if distribution
                .session_cleanup_not_found_markers()
                .iter()
                .any(|marker| marker.is_empty() || marker.contains('\0'))
            {
                return Err(CatalogValidationError::InvalidSessionCleanupMarker(
                    distribution.id().to_string(),
                ));
            }
            if let Distribution::System {
                command_candidates, ..
            } = distribution
            {
                if command_candidates
                    .iter()
                    .any(|command| !is_safe_command_candidate(command))
                {
                    return Err(CatalogValidationError::InvalidSystemDistribution(
                        distribution.id().to_string(),
                    ));
                }
            }
            if let Distribution::Binary {
                url,
                sha256,
                executable,
                ..
            } = distribution
            {
                if !is_safe_artifact_url(url)
                    || sha256.len() != 64
                    || !sha256
                        .chars()
                        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
                {
                    return Err(CatalogValidationError::InvalidBinaryIntegrity(
                        distribution.id().to_string(),
                    ));
                }
                if !is_safe_relative_path(executable) {
                    return Err(CatalogValidationError::InvalidBinaryExecutable(
                        executable.clone(),
                    ));
                }
            }
            if matches!(distribution, Distribution::Npx { package, version, bin, .. } if !is_valid_npm_package(package) || !is_fixed_version(version) || !is_safe_relative_path(bin))
            {
                return Err(CatalogValidationError::InvalidNpxDistribution(
                    distribution.id().to_string(),
                ));
            }
            if matches!(distribution, Distribution::Uvx { package, version, command, .. } if !is_valid_python_project(package) || !is_fixed_version(version) || !is_safe_relative_path(command))
            {
                return Err(CatalogValidationError::InvalidUvxDistribution(
                    distribution.id().to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSource {
    pub kind: String,
    pub upstream: String,
    pub upstream_revision: String,
}

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub struct Catalog {
    pub schema: String,
    #[serde(rename = "catalogVersion")]
    pub catalog_version: String,
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    pub source: CatalogSource,
    pub items: Vec<CatalogItem>,
}
