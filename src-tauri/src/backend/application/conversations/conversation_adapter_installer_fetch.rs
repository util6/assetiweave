use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek},
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};

use crate::backend::application::prelude::*;
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSource, ConversationAdapterPackageInstallSourceKind,
    ConversationAdapterPackageInstallSpec,
};
use crate::backend::infrastructure::extensions::DomainPackageSystem;

use super::conversation_adapter_installer::{
    clean_catalog_subpath, clean_non_empty_string, conversation_adapter_package_prepared_dir,
    conversation_script_staging_dir, validate_installed_package_for_spec,
};

pub(crate) struct GitHubInstallLocation {
    pub(crate) repo_url: String,
    pub(crate) branch: Option<String>,
    pub(crate) path: Option<String>,
}

impl GitHubInstallLocation {
    pub(crate) fn source_dir(&self, staging_dir: &Path) -> PathBuf {
        self.path
            .as_deref()
            .map(|path| staging_dir.join(path))
            .unwrap_or_else(|| staging_dir.to_path_buf())
    }
}

pub(crate) struct InstalledConversationAdapterPackage {
    pub(crate) validation:
        crate::backend::infrastructure::conversations::ConversationAdapterPackageValidationResult,
    pub(crate) created_version_dir: bool,
}

pub(crate) async fn install_conversation_adapter_package_files(
    spec: &ConversationAdapterPackageInstallSpec,
    version_dir: &Path,
) -> AppResult<InstalledConversationAdapterPackage> {
    let staging_dir = conversation_script_staging_dir(spec)?;
    let prepared_dir = conversation_adapter_package_prepared_dir(spec)?;
    let install_result: AppResult<InstalledConversationAdapterPackage> = async {
        let source_dir = match spec.source.kind {
            ConversationAdapterPackageInstallSourceKind::Github => {
                let location = parse_github_install_source(&spec.source)?;
                clone_github_catalog_source(&location, &staging_dir).await?;
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
        let package_manifest_file = spec
            .package_manifest_file_name()
            .map_err(AppError::external)?;
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
        crate::backend::infrastructure::filesystem::copy_dir(&source_dir, &prepared_dir)?;
        let prepared_validation =
            crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(&prepared_dir)
                .map_err(AppError::external)?;
        let kernel_inspection = crate::backend::infrastructure::conversations::ConversationAdapterPackageSystem
            .inspect(&prepared_dir)
            .map_err(AppError::from)?;
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
                crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(
                    version_dir,
                )
                .map_err(AppError::external)?;
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
            crate::backend::infrastructure::conversations::validate_conversation_adapter_package_dir(version_dir)
                .map_err(AppError::external)
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
                Err(error)
            }
        }
    }
    .await;

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
    let artifact_part = staging_dir.join("artifact.zip.part");
    let client = crate::backend::infrastructure::http_client::shared_http_client()?;
    let cancelled = || false;
    let download_spec = crate::backend::infrastructure::http_client::DownloadSpec {
        url: &spec.source.url,
        path: &artifact_part,
        max_bytes: 512 * 1024 * 1024,
        expected_size: spec.artifact_size,
        timeout: Duration::from_secs(60 * 5),
    };
    crate::backend::infrastructure::http_client::download_to_file(
        &client,
        download_spec,
        &cancelled,
    )?;

    let mut file =
        fs::File::open(&artifact_part).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| AppError::Storage(error.to_string()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual_hash = format!("{:x}", hasher.finalize());
    if !actual_hash.eq_ignore_ascii_case(&expected_hash) {
        let _ = fs::remove_file(&artifact_part);
        return Err(AppError::Validation(
            "conversation adapter package artifact hash mismatch".to_string(),
        ));
    }
    file.seek(std::io::SeekFrom::Start(0))
        .map_err(|error| AppError::Storage(error.to_string()))?;

    let result = extract_install_artifact_reader(spec, file, staging_dir);
    let _ = fs::remove_file(&artifact_part);
    result
}

pub(crate) fn extract_install_artifact_reader<R: Read + Seek>(
    spec: &ConversationAdapterPackageInstallSpec,
    reader: R,
    staging_dir: &Path,
) -> AppResult<PathBuf> {
    let extract_root = staging_dir.join("extracted");
    fs::create_dir_all(&extract_root).map_err(|error| AppError::Storage(error.to_string()))?;
    let mut archive = zip::ZipArchive::new(reader).map_err(|error| {
        AppError::Validation(format!(
            "open conversation adapter package artifact failed: {error}"
        ))
    })?;
    if archive.len() > 10_000 {
        return Err(AppError::Validation(
            "conversation adapter package artifact contains too many entries".to_string(),
        ));
    }
    let portable_filesystem = crate::backend::infrastructure::host_filesystem::HostFilesystem::new(
        crate::backend::infrastructure::host_paths::HostPlatform::Windows,
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

    let package_manifest = spec
        .package_manifest_file_name()
        .map_err(AppError::external)?;
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

pub(crate) fn parse_github_install_source(
    source: &crate::backend::infrastructure::conversations::ConversationAdapterPackageInstallSource,
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

async fn clone_github_catalog_source(
    location: &GitHubInstallLocation,
    target: &Path,
) -> AppResult<()> {
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
    let spec = crate::backend::infrastructure::host_process::HostCommandSpec {
        program: PathBuf::from("git"),
        args: command_args,
        env: Vec::new(),
        working_dir: None,
        stdin: crate::backend::infrastructure::host_process::HostInput::Null,
        timeout: Duration::from_secs(120),
        stdout_limit: 1024 * 1024,
        stderr_limit: 256 * 1024,
    };
    let output = crate::backend::infrastructure::host_process::run_host_command_async(spec, None)
        .await
        .map_err(|error| AppError::Process(format!("failed to run git clone: {error:?}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::External(format!("git clone failed: {stderr}")));
    }
    Ok(())
}
