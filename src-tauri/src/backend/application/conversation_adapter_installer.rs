use super::prelude::*;
use crate::backend::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};
use crate::backend::extension_kernel::DomainPackageSystem;
use crate::backend::models::{
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Cursor, Read},
    time::Duration,
};

const CONVERSATION_SCRIPT_SECURITY_NOTICE: &str =
    "Review remote conversation adapter package contents before installing; AssetIWeave registers the downloaded adapter package as trusted for local execution.";

#[derive(Debug)]
pub(super) struct GitHubInstallLocation {
    pub(super) repo_url: String,
    pub(super) branch: Option<String>,
    pub(super) path: Option<String>,
}

impl GitHubInstallLocation {
    fn source_dir(&self, staging_dir: &Path) -> PathBuf {
        self.path
            .as_deref()
            .map(|path| staging_dir.join(path))
            .unwrap_or_else(|| staging_dir.to_path_buf())
    }
}

/// Canonical package installer entry point. The installer core consumes the
/// version-neutral spec directly; legacy Script Catalog items only reverse-map
/// into this boundary in `install_conversation_adapter_package_from_item`.
pub(super) fn install_conversation_adapter_package_from_spec(
    service: &AppService,
    spec: &ConversationAdapterPackageInstallSpec,
    dry_run: bool,
    catalog_url: Option<&str>,
) -> AppResult<Value> {
    let version_dir = conversation_adapter_package_version_dir(spec)?;
    let package_manifest_path = version_dir.join(spec.package_manifest_file_name()?);
    let adapter_manifest_path = version_dir.join(spec.manifest_file_name()?);

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

    let previous_package = service.load_conversation_adapter_package(spec.package_id())?;
    let installed = match install_conversation_adapter_package_files(spec, &version_dir) {
        Ok(installed) => installed,
        Err(error) => {
            if previous_package.is_none() {
                let error_message = error.to_string();
                persist_failed_conversation_adapter_package(
                    service,
                    spec,
                    &version_dir,
                    &error_message,
                )?;
            }
            return Err(error);
        }
    };

    let preview = crate::backend::conversations::register_external_adapter(
        crate::backend::conversations::ExternalAdapterRegisterParams {
            manifest_path: installed.validation.adapter_manifest_path.clone(),
            dry_run: false,
            yes: true,
        },
    )?;
    let adapter = crate::backend::conversations::adapter_from_registration_preview(preview)?;
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
    let version = crate::backend::models::ConversationAdapterPackageVersion {
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
    let tenant_id = service.tenant_id().to_string();
    let adapter_to_save = adapter.clone();
    let package_to_save = package.clone();
    let activation = service.db.block_on(async move {
        crate::backend::store::activate_conversation_adapter_package_sqlx(
            &pool,
            &tenant_id,
            &adapter_to_save,
            &package_to_save,
            &version,
        )
        .await
    });
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
            )?;
        }
        return Err(AppError::Storage(error));
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

struct InstalledConversationAdapterPackage {
    validation: crate::backend::conversations::ConversationAdapterPackageValidationResult,
    created_version_dir: bool,
}

fn install_conversation_adapter_package_files(
    spec: &ConversationAdapterPackageInstallSpec,
    version_dir: &Path,
) -> AppResult<InstalledConversationAdapterPackage> {
    let staging_dir = conversation_script_staging_dir(spec)?;
    let prepared_dir = conversation_adapter_package_prepared_dir(spec)?;
    let install_result = (|| {
        let source_dir = match spec.source.kind {
            ConversationAdapterPackageInstallSourceKind::Github => {
                let location = parse_github_install_source(&spec.source)?;
                clone_github_catalog_source(&location, &staging_dir)?;
                location.source_dir(&staging_dir)
            }
            ConversationAdapterPackageInstallSourceKind::ArtifactZip => {
                download_and_extract_install_artifact(spec, &staging_dir)?
            }
            ConversationAdapterPackageInstallSourceKind::LocalDirectory => {
                return Err(AppError::Validation(
                    "local registered packages cannot be installed from Catalog".to_string(),
                ))
            }
        };
        if !source_dir.is_dir() {
            return Err(AppError::Validation(format!(
                "conversation adapter package source path is not a directory: {}",
                source_dir.display()
            )));
        }
        let package_manifest_file = spec.package_manifest_file_name()?;
        if !source_dir.join(&package_manifest_file).is_file() {
            return Err(AppError::Validation(format!(
                "conversation adapter package source does not contain {}: {}",
                package_manifest_file,
                source_dir.display()
            )));
        }

        if prepared_dir.exists() {
            return Err(AppError::Conflict(format!(
                "conversation adapter package prepared path already exists: {}",
                prepared_dir.display()
            )));
        }
        capabilities::copy_dir(&source_dir, &prepared_dir)?;
        let prepared_validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(
                &prepared_dir,
            )?;
        let kernel_inspection = crate::backend::conversations::ConversationAdapterPackageSystem
            .inspect(&prepared_dir)
            .map_err(|error| AppError::Extension(error.to_string()))?;
        crate::backend::conversations::ConversationAdapterPackageSystem
            .on_installed(&kernel_inspection)
            .map_err(|error| AppError::Extension(error.to_string()))?;
        if kernel_inspection.identity.package_id != spec.package_id()
            || kernel_inspection.identity.version
                != semver::Version::parse(&spec.version)
                    .map_err(|error| AppError::Validation(error.to_string()))?
        {
            return Err(AppError::Validation(
                "conversation adapter kernel identity differs from install spec".to_string(),
            ));
        }
        validate_installed_package_for_spec(spec, &prepared_validation)?;

        if version_dir.exists() {
            let existing =
                crate::backend::conversations::validate_conversation_adapter_package_dir(
                    version_dir,
                )?;
            validate_installed_package_for_spec(spec, &existing)?;
            if existing.content_hash != prepared_validation.content_hash {
                return Err(AppError::Conflict(format!(
                    "conversation adapter package version is immutable: {}@{}",
                    spec.package_id(),
                    spec.version
                )));
            }
            fs::remove_dir_all(&prepared_dir)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            return Ok(InstalledConversationAdapterPackage {
                validation: existing,
                created_version_dir: false,
            });
        }
        let parent = version_dir.parent().ok_or_else(|| {
            AppError::Validation("conversation adapter version directory has no parent".to_string())
        })?;
        fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
        fs::rename(&prepared_dir, version_dir)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        let final_validation =
            crate::backend::conversations::validate_conversation_adapter_package_dir(version_dir)
                .and_then(|validation| {
                    validate_installed_package_for_spec(spec, &validation)?;
                    Ok(validation)
                });
        match final_validation {
            Ok(validation) => Ok(InstalledConversationAdapterPackage {
                validation,
                created_version_dir: true,
            }),
            Err(error) => {
                let _ = fs::remove_dir_all(version_dir);
                Err(AppError::Storage(error))
            }
        }
    })();

    let _ = fs::remove_dir_all(&staging_dir);
    if install_result.is_err() {
        let _ = fs::remove_dir_all(&prepared_dir);
    }
    install_result
}

fn download_and_extract_install_artifact(
    spec: &ConversationAdapterPackageInstallSpec,
    staging_dir: &Path,
) -> AppResult<PathBuf> {
    if !spec.source.url.starts_with("https://") {
        return Err(AppError::Validation(
            "conversation adapter package artifacts require HTTPS".to_string(),
        ));
    }
    let expected_hash = spec
        .expected_artifact_hash
        .as_deref()
        .and_then(clean_non_empty_string)
        .ok_or_else(|| {
            AppError::Validation(
                "conversation adapter package artifact sha256 is required".to_string(),
            )
        })?;
    let response = ureq::get(&spec.source.url)
        .set(
            "User-Agent",
            "AssetIWeave/0.5 conversation-adapter-package-artifact",
        )
        .call()
        .map_err(|error| {
            AppError::External(format!(
                "download conversation adapter package artifact failed: {error}"
            ))
        })?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(512 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::External(format!(
                "read conversation adapter package artifact failed: {error}"
            ))
        })?;
    if let Some(expected_size) = spec.artifact_size {
        if bytes.len() as u64 != expected_size {
            return Err(AppError::Validation(format!(
                "conversation adapter package artifact size mismatch: expected {expected_size}, got {}",
                bytes.len()
            )));
        }
    }
    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if !actual_hash.eq_ignore_ascii_case(&expected_hash) {
        return Err(AppError::Validation(
            "conversation adapter package artifact hash mismatch".to_string(),
        ));
    }

    extract_install_artifact_bytes(spec, bytes, staging_dir)
}

pub(super) fn extract_install_artifact_bytes(
    spec: &ConversationAdapterPackageInstallSpec,
    bytes: Vec<u8>,
    staging_dir: &Path,
) -> AppResult<PathBuf> {
    let extract_root = staging_dir.join("extracted");
    fs::create_dir_all(&extract_root).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        AppError::Validation(format!(
            "open conversation adapter package artifact failed: {error}"
        ))
    })?;
    if archive.len() > 10_000 {
        return Err(AppError::Validation(
            "conversation adapter package artifact contains too many entries".to_string(),
        ));
    }
    let portable_filesystem = crate::backend::host_filesystem::HostFilesystem::new(
        crate::backend::host_paths::HostPlatform::Windows,
    );
    let mut extracted_size = 0_u64;
    let mut validated_paths = Vec::with_capacity(archive.len());
    let mut seen_paths = HashMap::<String, String>::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            AppError::Validation(format!(
                "read conversation adapter package artifact entry failed: {error}"
            ))
        })?;
        let validated = portable_filesystem
            .validate_portable_relative_path(entry.name())
            .map_err(|error| {
                AppError::Validation(format!(
                    "conversation adapter package artifact contains an unsafe path: {error}"
                ))
            })?;
        if let Some(previous) = seen_paths.insert(
            validated.comparison_key().to_string(),
            entry.name().to_string(),
        ) {
            return Err(AppError::Validation(format!(
                "conversation adapter package artifact contains colliding paths: {previous} and {}",
                entry.name()
            )));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(AppError::Validation(
                "conversation adapter package artifact must not contain symlinks".to_string(),
            ));
        }
        extracted_size = extracted_size.saturating_add(entry.size());
        if extracted_size > 1024 * 1024 * 1024 {
            return Err(AppError::Validation(
                "conversation adapter package artifact expands beyond 1 GiB".to_string(),
            ));
        }
        validated_paths.push(validated);
    }

    for (index, validated) in validated_paths.iter().enumerate() {
        let mut entry = archive.by_index(index).map_err(|error| {
            AppError::Validation(format!(
                "read conversation adapter package artifact entry failed: {error}"
            ))
        })?;
        let destination = extract_root.join(validated.as_path());
        if entry.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|error| AppError::Storage(error.to_string()))?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
        }
        let mut output =
            fs::File::create(&destination).map_err(|error| AppError::Storage(error.to_string()))?;
        std::io::copy(&mut entry, &mut output)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o777))
                .map_err(|error| AppError::Storage(error.to_string()))?;
        }
    }

    let package_manifest = spec.package_manifest_file_name()?;
    if extract_root.join(&package_manifest).is_file() {
        return Ok(extract_root);
    }
    let mut candidates = Vec::new();
    for entry in
        fs::read_dir(&extract_root).map_err(|error| AppError::Storage(error.to_string()))?
    {
        let path = entry
            .map_err(|error| AppError::Storage(error.to_string()))?
            .path();
        if path.is_dir() && path.join(&package_manifest).is_file() {
            candidates.push(path);
        }
    }
    if candidates.len() == 1 {
        Ok(candidates[0].clone())
    } else {
        Err(AppError::Validation(
            "conversation adapter package artifact must contain one package root".to_string(),
        ))
    }
}

fn persist_failed_conversation_adapter_package(
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
            .join(spec.package_manifest_file_name()?)
            .to_string_lossy()
            .to_string(),
        adapter_manifest_path: current_dir
            .join(spec.manifest_file_name()?)
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
    service.save_conversation_adapter_package(&package)
}

fn validate_installed_package_for_spec(
    spec: &ConversationAdapterPackageInstallSpec,
    validation: &crate::backend::conversations::ConversationAdapterPackageValidationResult,
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
        != crate::backend::conversations::ConversationAdapterPackageRuntimeProtocol::StdioNdjsonV1
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
    validation: &crate::backend::conversations::ExternalAdapterValidationResult,
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
    Ok(crate::backend::app_settings::conversation_adapter_dir()
        .map_err(AppError::Storage)?
        .join("packages")
        .join(spec.package_id()))
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

fn conversation_adapter_package_prepared_dir(
    spec: &ConversationAdapterPackageInstallSpec,
) -> AppResult<PathBuf> {
    Ok(conversation_adapter_package_dir(spec)?
        .join("prepared")
        .join(short_uuid()))
}

fn conversation_script_staging_dir(
    spec: &ConversationAdapterPackageInstallSpec,
) -> AppResult<PathBuf> {
    Ok(crate::backend::app_settings::conversation_adapter_dir()
        .map_err(AppError::Storage)?
        .join("staging")
        .join(format!(
            "{}-{}",
            slug_path_segment(spec.package_id()),
            short_uuid()
        )))
}

pub(super) fn parse_github_install_source(
    source: &crate::backend::conversations::ConversationAdapterPackageInstallSource,
) -> AppResult<GitHubInstallLocation> {
    if source.kind != ConversationAdapterPackageInstallSourceKind::Github {
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
    let path = trimmed.strip_prefix("https://github.com/").ok_or_else(|| {
        AppError::Validation(
            "conversation adapter package source only supports https://github.com URLs".to_string(),
        )
    })?;
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

    Ok(GitHubInstallLocation {
        repo_url: format!("https://github.com/{owner}/{repo}.git"),
        branch,
        path: source_path,
    })
}

fn clone_github_catalog_source(location: &GitHubInstallLocation, target: &Path) -> AppResult<()> {
    if target.exists() {
        return Err(AppError::Conflict(format!(
            "conversation adapter package staging path already exists: {}",
            target.display()
        )));
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| AppError::Storage(error.to_string()))?;
    }

    let mut command_args = vec!["clone".to_string(), "--depth".to_string(), "1".to_string()];
    if let Some(branch) = &location.branch {
        command_args.extend(["--branch".to_string(), branch.clone()]);
    }
    command_args.extend([
        location.repo_url.clone(),
        target.to_string_lossy().to_string(),
    ]);
    let output = crate::backend::host_process::run_program_with_timeout(
        Path::new("git"),
        &command_args,
        None,
        Duration::from_secs(120),
        1024 * 1024,
        256 * 1024,
    )
    .map_err(|error| AppError::Process(format!("failed to run git clone: {error:?}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::External(format!("git clone failed: {stderr}")));
    }
    Ok(())
}

fn clean_non_empty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_catalog_subpath(value: &str) -> Option<String> {
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
