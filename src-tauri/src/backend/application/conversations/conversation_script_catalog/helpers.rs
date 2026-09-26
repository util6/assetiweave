use super::types::*;
use crate::backend::application::conversations::conversation_adapter_installer;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use chrono::Utc;
use std::path::{Path, PathBuf};

pub(crate) fn classify_runtime_gate_error(
    _install_dir: &Path,
    error: &str,
) -> ConversationAdapterRuntimeGateStatus {
    if error.contains("hash mismatch") || error.contains("no trusted hash") {
        ConversationAdapterRuntimeGateStatus::HashMismatch
    } else if error.contains("requires AssetIWeave core") {
        ConversationAdapterRuntimeGateStatus::CoreIncompatible
    } else if error.contains("root is not a directory")
        || error.contains("runtime is not registered")
    {
        ConversationAdapterRuntimeGateStatus::RuntimeMissing
    } else {
        ConversationAdapterRuntimeGateStatus::ManifestInvalid
    }
}

pub(crate) fn validate_managed_package_delete_target(
    managed_root: &Path,
    package_id: &str,
    install_dir: &Path,
) -> AppResult<PathBuf> {
    let package_id = package_id.trim();
    let package_id_path = Path::new(package_id);
    if package_id.is_empty()
        || package_id == "."
        || package_id == ".."
        || package_id_path.components().count() != 1
        || !matches!(
            package_id_path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package id is not a safe path segment: {package_id}"
        )));
    }

    let packages_root = managed_root.join("packages");
    if !install_dir.exists() || !install_dir.starts_with(&packages_root) {
        return Err(AppError::Validation(format!(
            "conversation adapter package delete target does not exist in the managed library: {}",
            install_dir.display()
        )));
    }

    let canonical_packages_root = packages_root
        .canonicalize()
        .map_err(|error| {
            format!(
                "resolve managed conversation adapter packages root failed ({}): {error}",
                packages_root.display()
            )
        })
        .map_err(AppError::external)?;
    let canonical_install_dir = install_dir
        .canonicalize()
        .map_err(|error| {
            format!(
                "resolve conversation adapter package install directory failed ({}): {error}",
                install_dir.display()
            )
        })
        .map_err(AppError::external)?;
    let relative_install = canonical_install_dir
        .strip_prefix(&canonical_packages_root)
        .map_err(|_| {
            format!(
                "conversation adapter package install directory escapes the managed library: {}",
                install_dir.display()
            )
        })
        .map_err(AppError::external)?;
    let package_segment = relative_install
        .components()
        .next()
        .and_then(|component| match component {
            std::path::Component::Normal(value) => Some(value),
            _ => None,
        })
        .ok_or_else(|| {
            "conversation adapter package install directory has no package root".to_string()
        })
        .map_err(AppError::external)?;
    let package_root = packages_root.join(package_segment);
    let canonical_package_root = package_root
        .canonicalize()
        .map_err(|error| {
            format!(
                "resolve managed conversation adapter package root failed ({}): {error}",
                package_root.display()
            )
        })
        .map_err(AppError::external)?;
    if canonical_package_root.parent() != Some(canonical_packages_root.as_path())
        || canonical_install_dir == canonical_package_root
        || !canonical_install_dir.starts_with(&canonical_package_root)
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package root escapes the managed library: {}",
            package_root.display()
        )));
    }

    Ok(package_root)
}

pub(crate) fn validate_managed_package_version_delete_target(
    managed_root: &Path,
    package_id: &str,
    version: &str,
    install_dir: &Path,
) -> AppResult<PathBuf> {
    let version = validated_package_version(version)?;
    let package_root =
        validate_managed_package_delete_target(managed_root, package_id, install_dir)?;
    let expected = package_root.join("versions").join(version);
    let canonical_expected = expected
        .canonicalize()
        .map_err(|error| format!("resolve managed package version directory failed: {error}"))
        .map_err(AppError::external)?;
    let canonical_install = install_dir.canonicalize().map_err(AppError::external)?;
    if canonical_install != canonical_expected {
        return Err(AppError::Validation("conversation adapter version delete target is not the requested managed version directory".to_string()));
    }
    Ok(canonical_install)
}

pub(crate) async fn install_conversation_adapter_package_from_item(
    service: &AppService,
    item: &ConversationScriptCatalogItem,
    dry_run: bool,
    catalog_url: Option<&str>,
) -> AppResult<Value> {
    let spec = item.to_install_spec();
    conversation_adapter_installer::install_conversation_adapter_package_from_spec(
        service,
        &spec,
        dry_run,
        catalog_url,
    )
    .await
}

pub(crate) fn conversation_catalog_manifest_path(
    package: Option<&ConversationAdapterPackage>,
    adapter: Option<&ConversationAdapter>,
    install_path: Option<&str>,
    manifest_file: Option<&str>,
) -> Option<String> {
    package
        .map(|package| package.adapter_manifest_path.clone())
        .or_else(|| adapter.and_then(|adapter| adapter.manifest_path.clone()))
        .or_else(|| {
            install_path.map(|install_path| {
                format!(
                    "{}/{}",
                    install_path.trim_end_matches(['/', '\\']),
                    manifest_file
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("conversation-adapter.json")
                )
            })
        })
}

pub(crate) fn package_origin_label(origin: ConversationAdapterPackageOrigin) -> &'static str {
    match origin {
        ConversationAdapterPackageOrigin::BuiltIn => "built_in",
        ConversationAdapterPackageOrigin::ManagedRelease => "managed_release",
        ConversationAdapterPackageOrigin::LocalDirectory => "local_directory",
        ConversationAdapterPackageOrigin::GitRef => "git_ref",
        ConversationAdapterPackageOrigin::LegacyExternal => "legacy_external",
        ConversationAdapterPackageOrigin::DevOverride => "dev_override",
    }
}

pub(crate) fn conversation_adapter_package_status(
    installed: bool,
    package: Option<&ConversationAdapterPackage>,
    update_available: bool,
    ahead_of_release: bool,
    runtime_ready: bool,
    adapter: Option<&ConversationAdapter>,
) -> String {
    if !installed {
        return "not_installed".to_string();
    }
    if let Some(package) = package {
        if package.origin == ConversationAdapterPackageOrigin::ManagedRelease && adapter.is_none() {
            return "uninstalled".to_string();
        }
        if !package.runtime_ready {
            return match package.runtime_gate_status {
                ConversationAdapterRuntimeGateStatus::RuntimeMissing => "runtime_missing",
                ConversationAdapterRuntimeGateStatus::HashMismatch => "hash_mismatch",
                ConversationAdapterRuntimeGateStatus::ManifestInvalid => "manifest_invalid",
                ConversationAdapterRuntimeGateStatus::CoreIncompatible => "core_incompatible",
                ConversationAdapterRuntimeGateStatus::Ready => "verification_failed",
            }
            .to_string();
        }
        match package.origin {
            ConversationAdapterPackageOrigin::LocalDirectory => {
                return "local_registered".to_string()
            }
            ConversationAdapterPackageOrigin::GitRef => return "git_registered".to_string(),
            ConversationAdapterPackageOrigin::DevOverride => return "dev_override".to_string(),
            ConversationAdapterPackageOrigin::BuiltIn => return "built_in".to_string(),
            ConversationAdapterPackageOrigin::ManagedRelease
            | ConversationAdapterPackageOrigin::LegacyExternal => {}
        }
    } else if adapter.is_some_and(|adapter| {
        adapter.trust_state == crate::backend::domain::ConversationAdapterTrustState::BuiltIn
    }) {
        return if runtime_ready {
            "built_in"
        } else {
            "uninstalled"
        }
        .to_string();
    } else if adapter.is_some() {
        return "legacy_installed".to_string();
    }
    if update_available {
        return "update_available".to_string();
    }
    if ahead_of_release {
        return "ahead_of_release".to_string();
    }
    if runtime_ready {
        "installed".to_string()
    } else {
        "verification_failed".to_string()
    }
}

pub(crate) fn format_package_not_ready_error(package: &ConversationAdapterPackage) -> String {
    format!(
        "conversation adapter package runtime is not ready: {}{}",
        package.package_id,
        package
            .error_message
            .as_deref()
            .map(|message| format!(": {message}"))
            .unwrap_or_default()
    )
}

pub(crate) fn validated_package_version(value: &str) -> AppResult<String> {
    semver::Version::parse(value.trim())
        .map(|version| version.to_string())
        .map_err(|error| {
            AppError::Validation(format!(
                "conversation adapter package version must be SemVer: {error}"
            ))
        })
}

pub(crate) fn parse_github_catalog_location(
    source: &ConversationScriptCatalogSource,
) -> AppResult<GitHubCatalogLocation> {
    if source.kind != ConversationScriptCatalogSourceKind::Github {
        return Err(AppError::Validation(
            "conversation adapter package source must be github".to_string(),
        ));
    }
    let trimmed = source
        .url
        .trim()
        .split('#')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .trim_end_matches('/');
    let path = trimmed
        .strip_prefix("https://github.com/")
        .ok_or_else(|| {
            "conversation adapter package source only supports https://github.com URLs".to_string()
        })
        .map_err(AppError::external)?;
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(AppError::Validation(
            "GitHub URL must include owner and repository".to_string(),
        ));
    }

    let owner = parts[0];
    let repo = parts[1].trim_end_matches(".git");
    if repo.is_empty() {
        return Err(AppError::Validation(
            "GitHub URL must include repository name".to_string(),
        ));
    }

    let mut branch = source.branch.as_deref().and_then(clean_non_empty_string);
    let mut source_path = source.path.as_deref().and_then(clean_catalog_subpath);
    if source_path.is_none() && parts.len() >= 4 && matches!(parts[2], "tree" | "blob") {
        branch = branch.or_else(|| clean_non_empty_string(parts[3]));
        if parts.len() > 4 {
            source_path = clean_catalog_subpath(&parts[4..].join("/"));
        }
    }

    Ok(GitHubCatalogLocation {
        repo_url: format!("https://github.com/{owner}/{repo}.git"),
        branch,
        path: source_path,
    })
}

pub(crate) fn clean_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn clean_catalog_subpath(value: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in value.trim().trim_matches('/').split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part == ".git" || part.contains('\\') || part.contains(':') {
            return None;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

pub(crate) fn clean_relative_file_name(value: &str) -> AppResult<String> {
    let trimmed = value.trim().trim_matches('/');
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains(':')
    {
        return Err(AppError::Validation(format!(
            "manifest_file must be a file name: {value}"
        )));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

pub(crate) fn replacement_manifest_path(
    install_dir: &str,
    previous_path: &str,
    fallback: &str,
) -> String {
    let file_name = Path::new(previous_path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback);
    Path::new(install_dir)
        .join(file_name)
        .to_string_lossy()
        .to_string()
}

pub(crate) fn package_for_uninstalled_replacement(
    package: &ConversationAdapterPackage,
    replacement: &crate::backend::domain::ConversationAdapterPackageVersion,
) -> ConversationAdapterPackage {
    let mut replacement_package = package.clone();
    replacement_package.version = replacement.version.clone();
    replacement_package.install_dir = replacement.install_dir.clone();
    replacement_package.manifest_path = replacement_manifest_path(
        &replacement.install_dir,
        &package.manifest_path,
        "conversation-adapter-package.json",
    );
    replacement_package.adapter_manifest_path = replacement_manifest_path(
        &replacement.install_dir,
        &package.adapter_manifest_path,
        "conversation-adapter.json",
    );
    replacement_package.installed_content_hash = Some(replacement.content_hash.clone());
    replacement_package.trusted_package_hash = Some(replacement.content_hash.clone());
    replacement_package.updated_at = Utc::now().to_rfc3339();
    replacement_package
}
