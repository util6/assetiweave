use super::*;
use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};

const DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/util6/assetiweave/main/builtin-assets/catalog.json";
const LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG: &str =
    include_str!("../../../../../../builtin-assets/catalog.json");

pub(crate) fn load_conversation_script_catalog(
    catalog_url: Option<&str>,
) -> AppResult<ConversationScriptCatalog> {
    let catalog_url = catalog_url
        .and_then(clean_non_empty_string)
        .unwrap_or_else(|| DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL.to_string());
    let text = if catalog_url.starts_with("https://") || catalog_url.starts_with("http://") {
        match fetch_catalog_text(&catalog_url) {
            Ok(text) => text,
            Err(error) if catalog_url == DEFAULT_CONVERSATION_SCRIPT_CATALOG_URL => {
                read_local_default_catalog()
                    .map_err(|fallback_error| {
                        format!("{error}; local default catalog fallback failed: {fallback_error}")
                    })
                    .map_err(AppError::external)?
            }
            Err(error) => return Err(error),
        }
    } else {
        let path = crate::backend::infrastructure::path_utils::expand_path(&catalog_url)?;
        fs::read_to_string(&path)
            .map_err(|error| format!("read conversation adapter package catalog failed: {error}"))
            .map_err(AppError::external)?
    };
    let catalog: ConversationScriptCatalog = serde_json::from_str(&text)
        .map_err(|error| {
            format!("conversation adapter package catalog was not valid JSON: {error}")
        })
        .map_err(AppError::external)?;
    validate_conversation_script_catalog(&catalog)?;
    Ok(catalog)
}

pub(crate) fn fetch_catalog_text(url: &str) -> AppResult<String> {
    let client = crate::backend::infrastructure::http_client::shared_http_client()?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "AssetIWeave/0.5 conversation-adapter-package-catalog",
        ),
    );
    let response = crate::backend::infrastructure::http_client::get_with_redirects(
        &client,
        url,
        headers,
        std::time::Duration::from_secs(15),
    )
    .map_err(|error| {
        AppError::External(format!(
            "conversation adapter package catalog request failed: {error}"
        ))
    })?;
    let response = response.error_for_status().map_err(|error| {
        AppError::External(format!(
            "conversation adapter package catalog request failed: {error}"
        ))
    })?;
    Ok(
        crate::backend::infrastructure::http_client::read_response_text_with_limit(
            response,
            crate::backend::infrastructure::http_client::DEFAULT_MAX_TEXT_RESPONSE_BYTES,
        )?,
    )
}

pub(crate) fn read_local_default_catalog() -> AppResult<String> {
    Ok(LOCAL_DEFAULT_CONVERSATION_SCRIPT_CATALOG.to_string())
}

pub(crate) fn validate_conversation_script_catalog(
    catalog: &ConversationScriptCatalog,
) -> AppResult<()> {
    if catalog.schema_version != 1 {
        return Err(AppError::Validation(
            "conversation adapter package catalog schema_version must be 1".to_string(),
        ));
    }
    let mut seen_ids = HashSet::new();
    for item in &catalog.items {
        validate_conversation_script_catalog_item(item)?;
        if !seen_ids.insert(item.id.clone()) {
            return Err(AppError::Validation(format!(
                "duplicate conversation adapter package catalog item: {}",
                item.id
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_conversation_script_catalog_item(
    item: &ConversationScriptCatalogItem,
) -> AppResult<()> {
    if item.id.trim().is_empty() {
        return Err(AppError::Validation(
            "conversation adapter package catalog item id is required".to_string(),
        ));
    }
    let id_path = Path::new(&item.id);
    if item.id == "."
        || item.id == ".."
        || id_path.components().count() != 1
        || !item.id.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(AppError::Validation(format!(
            "conversation adapter package catalog item id must be a safe path segment: {}",
            item.id
        )));
    }
    if item.name.trim().is_empty() {
        return Err(AppError::Validation(format!(
            "conversation adapter package catalog item name is required: {}",
            item.id
        )));
    }
    if item.version.trim().is_empty() {
        return Err(AppError::Validation(format!(
            "conversation adapter package catalog item version is required: {}",
            item.id
        )));
    }
    validated_package_version(&item.version)?;
    if let Some(adapter_id) = item.adapter_id.as_deref() {
        if adapter_id.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "conversation adapter package catalog item adapter_id must not be empty: {}",
                item.id
            )));
        }
    }
    item.manifest_file_name()?;
    item.package_manifest_file_name()?;
    parse_github_catalog_location(&item.source)?;
    Ok(())
}

pub(crate) fn resolve_conversation_adapter_package_catalog_entries(
    items: Vec<ConversationScriptCatalogItem>,
    adapters: &[ConversationAdapter],
    packages: &[ConversationAdapterPackage],
) -> Vec<ConversationAdapterPackageCatalogEntry> {
    let mut entries = items
        .into_iter()
        .map(|item| {
            let installed_package = packages
                .iter()
                .find(|package| package.package_id == item.package_id())
                .cloned();
            let installed_adapter = adapters
                .iter()
                .find(|adapter| adapter.id == item.adapter_key())
                .cloned();
            let install_path = installed_package
                .as_ref()
                .map(|package| package.install_dir.clone())
                .or_else(|| {
                    installed_adapter
                        .as_ref()
                        .and_then(|adapter| adapter.manifest_path.as_deref())
                        .and_then(|path| {
                            Path::new(path).parent().map(|parent| parent.to_path_buf())
                        })
                        .map(|path| path.to_string_lossy().to_string())
                })
                .or_else(|| {
                    (item.source.kind == ConversationScriptCatalogSourceKind::LocalDirectory)
                        .then(|| item.source.url.clone())
                });
            let installed = installed_package.is_some() || installed_adapter.is_some();
            let installed_version = installed_package
                .as_ref()
                .map(|p| &p.version)
                .or_else(|| installed_adapter.as_ref().map(|a| &a.version));

            let (update_available, ahead_of_release) =
                if let Some(installed_ver) = installed_version {
                    if let (Ok(installed_semver), Ok(item_semver)) = (
                        semver::Version::parse(installed_ver),
                        semver::Version::parse(&item.version),
                    ) {
                        (
                            item_semver > installed_semver,
                            installed_semver > item_semver,
                        )
                    } else {
                        (installed_ver != &item.version, false)
                    }
                } else {
                    (false, false)
                };
            let runtime_ready = installed_package
                .as_ref()
                .map(|package| package.runtime_ready)
                .unwrap_or_else(|| {
                    installed_adapter
                        .as_ref()
                        .is_some_and(|adapter| adapter.enabled)
                });
            let error_message = installed_package
                .as_ref()
                .and_then(|package| package.error_message.clone());
            let status = conversation_adapter_package_status(
                installed,
                installed_package.as_ref(),
                update_available,
                ahead_of_release,
                runtime_ready,
                installed_adapter.as_ref(),
            );
            let display_install_path = install_path
                .as_deref()
                .map(crate::backend::infrastructure::path_utils::display_path_or_original);
            let display_manifest_path = conversation_catalog_manifest_path(
                installed_package.as_ref(),
                installed_adapter.as_ref(),
                install_path.as_deref(),
                item.manifest_file.as_deref(),
            )
            .as_deref()
            .map(crate::backend::infrastructure::path_utils::display_path_or_original);
            ConversationAdapterPackageCatalogEntry {
                item,
                installed,
                update_available,
                ahead_of_release,
                runtime_ready,
                status,
                installed_package,
                installed_adapter,
                install_path,
                display_install_path,
                display_manifest_path,
                error_message,
            }
        })
        .collect::<Vec<_>>();
    let mut seen_packages = entries
        .iter()
        .filter_map(|entry| {
            entry
                .installed_package
                .as_ref()
                .map(|package| package.package_id.clone())
        })
        .collect::<HashSet<_>>();
    let mut seen_adapters = entries
        .iter()
        .filter_map(|entry| {
            entry
                .installed_adapter
                .as_ref()
                .map(|adapter| adapter.id.clone())
        })
        .collect::<HashSet<_>>();

    for package in packages {
        if !seen_packages.insert(package.package_id.clone()) {
            continue;
        }
        let adapter = adapters
            .iter()
            .find(|adapter| adapter.id == package.adapter_id)
            .cloned();
        if let Some(adapter) = adapter.as_ref() {
            seen_adapters.insert(adapter.id.clone());
        }
        let latest_version = package
            .latest_version
            .clone()
            .unwrap_or_else(|| package.version.clone());
        let (update_available, ahead_of_release) = semver::Version::parse(&latest_version)
            .ok()
            .zip(semver::Version::parse(&package.version).ok())
            .map(|(latest, current)| (latest > current, current > latest))
            .unwrap_or((false, false));
        let item = ConversationScriptCatalogItem {
            id: package.package_id.clone(),
            name: package.name.clone(),
            version: latest_version,
            record_kind: match package.record_kind {
                ConversationAdapterPackageRecordKind::Session => {
                    ConversationScriptRecordKind::Session
                }
                ConversationAdapterPackageRecordKind::Web => ConversationScriptRecordKind::Web,
            },
            provider: Some(package_origin_label(package.origin).to_string()),
            adapter_id: Some(package.adapter_id.clone()),
            description: None,
            homepage_url: None,
            repository_url: package.source_url.clone(),
            tags: Vec::new(),
            manifest_file: Some(
                Path::new(&package.adapter_manifest_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("conversation-adapter.json")
                    .to_string(),
            ),
            package_manifest_file: Some(
                Path::new(&package.manifest_path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("conversation-adapter-package.json")
                    .to_string(),
            ),
            expected_content_hash: None,
            expected_package_hash: package.trusted_package_hash.clone(),
            expected_artifact_hash: None,
            artifact_size: None,
            source: ConversationScriptCatalogSource {
                kind: ConversationScriptCatalogSourceKind::LocalDirectory,
                url: package
                    .source_url
                    .clone()
                    .unwrap_or_else(|| package.install_dir.clone()),
                branch: package.git_ref.clone(),
                path: None,
            },
        };
        entries.push(ConversationAdapterPackageCatalogEntry {
            status: conversation_adapter_package_status(
                true,
                Some(package),
                update_available,
                ahead_of_release,
                package.runtime_ready,
                adapter.as_ref(),
            ),
            installed: true,
            update_available,
            ahead_of_release,
            runtime_ready: package.runtime_ready,
            install_path: Some(package.install_dir.clone()),
            display_install_path: Some(
                crate::backend::infrastructure::path_utils::display_path_or_original(
                    &package.install_dir,
                ),
            ),
            display_manifest_path: Some(
                crate::backend::infrastructure::path_utils::display_path_or_original(
                    &package.adapter_manifest_path,
                ),
            ),
            error_message: package.error_message.clone(),
            installed_package: Some(package.clone()),
            installed_adapter: adapter,
            item,
        });
    }

    for adapter in adapters {
        if !seen_adapters.insert(adapter.id.clone()) {
            continue;
        }
        let record_kind = if adapter
            .capabilities
            .iter()
            .any(|capability| capability == "web_records")
        {
            ConversationScriptRecordKind::Web
        } else {
            ConversationScriptRecordKind::Session
        };
        let install_path = adapter
            .manifest_path
            .as_deref()
            .and_then(|path| Path::new(path).parent())
            .map(|path| path.to_string_lossy().to_string());
        let display_install_path = install_path
            .as_deref()
            .map(crate::backend::infrastructure::path_utils::display_path_or_original);
        let display_manifest_path = adapter
            .manifest_path
            .as_deref()
            .map(crate::backend::infrastructure::path_utils::display_path_or_original);
        entries.push(ConversationAdapterPackageCatalogEntry {
            item: ConversationScriptCatalogItem {
                id: adapter.id.clone(),
                name: adapter.name.clone(),
                version: adapter.version.clone(),
                record_kind,
                provider: Some(
                    if adapter.trust_state
                        == crate::backend::domain::ConversationAdapterTrustState::BuiltIn
                    {
                        "built_in".to_string()
                    } else {
                        "legacy_external".to_string()
                    },
                ),
                adapter_id: Some(adapter.id.clone()),
                description: None,
                homepage_url: None,
                repository_url: None,
                tags: Vec::new(),
                manifest_file: adapter
                    .manifest_path
                    .as_deref()
                    .and_then(|path| Path::new(path).file_name())
                    .and_then(|value| value.to_str())
                    .map(str::to_string),
                package_manifest_file: None,
                expected_content_hash: adapter.trusted_hash.clone(),
                expected_package_hash: None,
                expected_artifact_hash: None,
                artifact_size: None,
                source: ConversationScriptCatalogSource {
                    kind: ConversationScriptCatalogSourceKind::LocalDirectory,
                    url: install_path.clone().unwrap_or_default(),
                    branch: None,
                    path: None,
                },
            },
            installed: true,
            update_available: false,
            ahead_of_release: false,
            runtime_ready: adapter.enabled,
            status: conversation_adapter_package_status(
                true,
                None,
                false,
                false,
                adapter.enabled,
                Some(adapter),
            ),
            installed_package: None,
            installed_adapter: Some(adapter.clone()),
            install_path,
            display_install_path,
            display_manifest_path,
            error_message: None,
        });
    }
    entries.sort_by(|left, right| left.item.name.cmp(&right.item.name));
    entries
}
