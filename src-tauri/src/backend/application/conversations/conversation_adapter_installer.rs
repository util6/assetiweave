use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};
use crate::backend::infrastructure::extensions::DomainPackageSystem;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Seek},
    time::Duration,
};

const CONVERSATION_SCRIPT_SECURITY_NOTICE: &str =
    "Review remote conversation adapter package contents before installing; AssetIWeave registers the downloaded adapter package as trusted for local execution.";

pub(crate) use super::conversation_adapter_installer_fetch::*;

/// Canonical package installer entry point. The installer core consumes the
/// version-neutral spec directly; legacy Script Catalog items only reverse-map
/// into this boundary in `install_conversation_adapter_package_from_item`.
pub(crate) async fn install_conversation_adapter_package_from_spec(
    service: &AppService,
    spec: &ConversationAdapterPackageInstallSpec,
    dry_run: bool,
    catalog_url: Option<&str>,
) -> AppResult<Value> {
    let version_dir = conversation_adapter_package_version_dir(spec)?;
    let package_manifest_path = version_dir.join(
        spec.package_manifest_file_name()
            .map_err(AppError::external)?,
    );
    let adapter_manifest_path =
        version_dir.join(spec.manifest_file_name().map_err(AppError::external)?);

    if dry_run {
        return Ok(json!({
            "dry_run": true,
            "installed": false,
            "package_id": spec.package_id(),
            "spec": spec,
            "install_path": version_dir,
            "package_manifest_path": package_manifest_path,
            "manifest_path": adapter_manifest_path,
            "security_notice": CONVERSATION_SCRIPT_SECURITY_NOTICE,
        }));
    }

    let previous_package = service
        .load_conversation_adapter_package(spec.package_id())
        .await?;
    let installed = match install_conversation_adapter_package_files(spec, &version_dir).await {
        Ok(installed) => installed,
        Err(error) => {
            if previous_package.is_none() {
                let error_message = error.to_string();
                persist_failed_conversation_adapter_package(
                    service,
                    spec,
                    &version_dir,
                    &error_message,
                )
                .await?;
            }
            return Err(error);
        }
    };

    let settings = service.app_settings_value();
    let preview =
        crate::backend::infrastructure::conversations::register_external_adapter_with_settings(
            crate::backend::infrastructure::conversations::ExternalAdapterRegisterParams {
                manifest_path: installed.validation.adapter_manifest_path.clone(),
                dry_run: false,
                yes: true,
            },
            &settings,
        )
        .await
        .map_err(AppError::external)?;
    let adapter =
        crate::backend::infrastructure::conversations::adapter_from_registration_preview(preview)
            .map_err(AppError::external)?;
    let now = Utc::now().to_rfc3339();
    let package = ConversationAdapterPackage {
        package_id: spec.package_id().to_string(),
        adapter_id: adapter.id.clone(),
        name: installed.validation.manifest.name.clone(),
        version: installed.validation.manifest.version.clone(),
        record_kind: spec.record_kind,
        install_dir: version_dir.to_string_lossy().to_string(),
        manifest_path: installed.validation.manifest_path.clone(),
        adapter_manifest_path: installed.validation.adapter_manifest_path.clone(),
        runtime_protocol: installed
            .validation
            .manifest
            .runtime
            .protocol
            .as_str()
            .to_string(),
        runtime_ready: true,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: Some(spec.source.url.clone()),
        git_ref: spec.source.branch.clone(),
        git_commit: None,
        catalog_url: catalog_url.and_then(clean_non_empty_string),
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some(spec.version.clone()),
        last_checked_at: Some(now.clone()),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        runtime_validated_at: Some(now.clone()),
        installed_content_hash: Some(installed.validation.content_hash.clone()),
        trusted_package_hash: Some(
            spec.expected_package_hash
                .as_deref()
                .and_then(clean_non_empty_string)
                .unwrap_or_else(|| installed.validation.content_hash.clone()),
        ),
        error_message: None,
        created_at: previous_package
            .as_ref()
            .map(|package| package.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    let version = crate::backend::domain::ConversationAdapterPackageVersion {
        package_id: package.package_id.clone(),
        version: package.version.clone(),
        install_dir: package.install_dir.clone(),
        artifact_hash: spec
            .expected_artifact_hash
            .as_deref()
            .and_then(clean_non_empty_string),
        content_hash: installed.validation.content_hash.clone(),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::Ready,
        installed_at: package.updated_at.clone(),
    };
    let pool = service.db.pool().clone();
    let activation =
        crate::backend::application::conversations::conversation_storage::activate_package(
            &pool, &adapter, &package, &version,
        )
        .await;
    if let Err(error) = activation {
        if installed.created_version_dir {
            let _ = fs::remove_dir_all(&version_dir);
        }
        if previous_package.is_none() {
            let error_message = error.to_string();
            persist_failed_conversation_adapter_package(
                service,
                spec,
                &version_dir,
                &error_message,
            )
            .await?;
        }
        return Err(error);
    }

    Ok(json!({
        "dry_run": false,
        "installed": true,
        "package_id": spec.package_id(),
        "spec": spec,
        "install_path": version_dir,
        "package_manifest_path": installed.validation.manifest_path,
        "manifest_path": installed.validation.adapter_manifest_path,
        "package": package,
        "adapter": adapter,
        "validation": installed.validation,
        "security_notice": CONVERSATION_SCRIPT_SECURITY_NOTICE,
    }))
}

async fn persist_failed_conversation_adapter_package(
    service: &AppService,
    spec: &ConversationAdapterPackageInstallSpec,
    current_dir: &Path,
    error: &str,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let package = ConversationAdapterPackage {
        package_id: spec.package_id().to_string(),
        adapter_id: spec.adapter_key().to_string(),
        name: spec.name.clone(),
        version: spec.version.clone(),
        record_kind: spec.record_kind,
        install_dir: current_dir.to_string_lossy().to_string(),
        manifest_path: current_dir
            .join(
                spec.package_manifest_file_name()
                    .map_err(AppError::external)?,
            )
            .to_string_lossy()
            .to_string(),
        adapter_manifest_path: current_dir
            .join(spec.manifest_file_name().map_err(AppError::external)?)
            .to_string_lossy()
            .to_string(),
        runtime_protocol: "stdio-ndjson-v1".to_string(),
        runtime_ready: false,
        origin: ConversationAdapterPackageOrigin::ManagedRelease,
        source_url: Some(spec.source.url.clone()),
        git_ref: spec.source.branch.clone(),
        git_commit: None,
        catalog_url: None,
        update_policy: ConversationPackageUpdatePolicy::Manual,
        latest_version: Some(spec.version.clone()),
        last_checked_at: Some(now.clone()),
        runtime_gate_status: ConversationAdapterRuntimeGateStatus::ManifestInvalid,
        runtime_validated_at: Some(now.clone()),
        installed_content_hash: None,
        trusted_package_hash: spec
            .expected_package_hash
            .as_deref()
            .and_then(clean_non_empty_string),
        error_message: Some(error.to_string()),
        created_at: now.clone(),
        updated_at: now,
    };
    service.save_conversation_adapter_package(&package).await
}

pub(crate) fn validate_installed_package_for_spec(
    spec: &ConversationAdapterPackageInstallSpec,
    validation: &crate::backend::infrastructure::conversations::ConversationAdapterPackageValidationResult,
) -> AppResult<()> {
    if validation.manifest.package_id != spec.package_id() {
        return Err(AppError::Validation(format!(
            "installed package id {} does not match install package id {}",
            validation.manifest.package_id,
            spec.package_id()
        )));
    }
    if validation.manifest.version != spec.version {
        return Err(AppError::Validation(format!(
            "installed package version {} does not match install package version {}",
            validation.manifest.version, spec.version
        )));
    }
    if validation.manifest.record_kind != spec.record_kind {
        return Err(AppError::Validation(format!(
            "installed package record kind does not match install spec: {}",
            spec.id
        )));
    }
    if validation.manifest.runtime.protocol
        != crate::backend::infrastructure::conversations::ConversationAdapterPackageRuntimeProtocol::StdioNdjsonV1
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package {} only supports stdio-ndjson-v1 in this release",
            spec.id
        )));
    }
    validate_installed_manifest_for_spec(spec, &validation.adapter_validation)?;
    if let Some(expected) = spec
        .expected_package_hash
        .as_deref()
        .and_then(clean_non_empty_string)
    {
        if validation.content_hash != expected {
            return Err(AppError::Validation(format!(
                "conversation adapter package {} content hash mismatch",
                spec.id
            )));
        }
    }
    Ok(())
}

fn validate_installed_manifest_for_spec(
    spec: &ConversationAdapterPackageInstallSpec,
    validation: &crate::backend::infrastructure::conversations::ExternalAdapterValidationResult,
) -> AppResult<()> {
    if validation.manifest.id != spec.adapter_key() {
        return Err(AppError::Validation(format!(
            "installed adapter id {} does not match install adapter id {}",
            validation.manifest.id,
            spec.adapter_key()
        )));
    }
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == "read_session")
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package {} must declare read_session",
            spec.id
        )));
    }
    if spec.record_kind == ConversationAdapterPackageRecordKind::Web
        && !validation
            .manifest
            .capabilities
            .iter()
            .any(|capability| capability == "web_records")
    {
        return Err(AppError::Validation(format!(
            "web conversation adapter package {} must declare web_records",
            spec.id
        )));
    }
    if let Some(expected) = spec
        .expected_content_hash
        .as_deref()
        .and_then(clean_non_empty_string)
    {
        if validation.content_hash != expected {
            return Err(AppError::Validation(format!(
                "conversation adapter {} content hash mismatch",
                spec.id
            )));
        }
    }
    Ok(())
}

fn conversation_adapter_package_dir(
    spec: &ConversationAdapterPackageInstallSpec,
) -> AppResult<PathBuf> {
    Ok(
        crate::backend::infrastructure::app_settings::conversation_adapter_dir()?
            .join("packages")
            .join(spec.package_id()),
    )
}

fn conversation_adapter_package_version_dir(
    spec: &ConversationAdapterPackageInstallSpec,
) -> AppResult<PathBuf> {
    Ok(conversation_adapter_package_dir(spec)?
        .join("versions")
        .join(validated_package_version(&spec.version)?))
}

fn validated_package_version(value: &str) -> AppResult<String> {
    semver::Version::parse(value.trim())
        .map(|version| version.to_string())
        .map_err(|error| {
            AppError::Validation(format!(
                "conversation adapter package version must be SemVer: {error}"
            ))
        })
}

pub(crate) fn conversation_adapter_package_prepared_dir(
    spec: &ConversationAdapterPackageInstallSpec,
) -> AppResult<PathBuf> {
    Ok(conversation_adapter_package_dir(spec)?
        .join("prepared")
        .join(short_uuid()))
}

pub(crate) fn conversation_script_staging_dir(
    spec: &ConversationAdapterPackageInstallSpec,
) -> AppResult<PathBuf> {
    Ok(
        crate::backend::infrastructure::app_settings::conversation_adapter_dir()?
            .join("staging")
            .join(format!(
                "{}-{}",
                slug_path_segment(spec.package_id()),
                short_uuid()
            )),
    )
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

fn short_uuid() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}
