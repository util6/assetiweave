pub(crate) use super::conversation_adapter_catalog_v2_fetch::*;
use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterCatalogRelease, ConversationAdapterPackageRecordKind,
    ConversationAdapterReleaseChannel,
};
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSource, ConversationAdapterPackageInstallSourceKind,
    ConversationAdapterPackageInstallSpec,
};
use chrono::Duration;
use semver::{Version, VersionReq};

impl AppService {
    pub(crate) async fn install_conversation_adapter_package_release(
        &self,
        params: ConversationAdapterPackageInstallParams,
    ) -> AppResult<Value> {
        let version = params
            .version
            .as_deref()
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .ok_or_else(|| {
                AppError::Validation(
                    "conversation adapter package release version is required".to_string(),
                )
            })?;
        Version::parse(version).map_err(|error| {
            AppError::Validation(format!(
                "conversation adapter package release version must be SemVer: {error}"
            ))
        })?;
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let releases = self
            .list_conversation_adapter_package_releases(
                ConversationAdapterPackageReleaseListParams {
                    catalog_url: Some(catalog_url.clone()),
                    package_id: params.package_id.clone(),
                    refresh: false,
                },
            )
            .await?;
        let release = releases
            .into_iter()
            .find(|release| release.version == version)
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "conversation adapter package release not found: {}@{}",
                    params.package_id, version
                ))
            })?;
        if !release_is_core_compatible(&release) {
            return Err(AppError::Validation(format!(
                "conversation adapter package release is not compatible with this Core: {}@{} ({})",
                release.package_id, release.version, release.core_compatibility
            )));
        }
        let spec = ConversationAdapterPackageInstallSpec {
            id: release.package_id.clone(),
            name: release.name.clone(),
            version: release.version.clone(),
            record_kind: release.record_kind,
            provider: Some(release.publisher.clone()),
            adapter_id: Some(release.adapter_id.clone()),
            description: None,
            homepage_url: None,
            repository_url: None,
            tags: Vec::new(),
            manifest_file: Some(release.adapter_manifest_file.clone()),
            package_manifest_file: Some(release.package_manifest_file.clone()),
            expected_content_hash: None,
            expected_package_hash: None,
            expected_artifact_hash: Some(release.artifact_sha256.clone()),
            artifact_size: release.artifact_size.map(|size| size as u64),
            source: ConversationAdapterPackageInstallSource {
                kind: ConversationAdapterPackageInstallSourceKind::ArtifactZip,
                url: release.artifact_url.clone(),
                branch: None,
                path: None,
            },
        };
        super::conversation_adapter_installer::install_conversation_adapter_package_from_spec(
            self,
            &spec,
            params.dry_run,
            Some(&catalog_url),
        )
        .await
    }

    pub(crate) async fn list_conversation_adapter_package_releases(
        &self,
        params: ConversationAdapterPackageReleaseListParams,
    ) -> AppResult<Vec<ConversationAdapterCatalogRelease>> {
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let package_id = params.package_id.trim();
        if package_id.is_empty() {
            return Err(AppError::Validation(
                "conversation adapter package release list requires package_id".to_string(),
            ));
        }
        let mut releases = self
            .load_cached_conversation_adapter_catalog_releases(&catalog_url, Some(package_id))
            .await?;
        if params.refresh || releases.is_empty() || catalog_cache_is_stale(&releases) {
            self.refresh_conversation_adapter_catalogs(ConversationAdapterCatalogRefreshParams {
                catalog_url: Some(catalog_url.clone()),
                force: params.refresh,
            })
            .await?;
            releases = self
                .load_cached_conversation_adapter_catalog_releases(&catalog_url, Some(package_id))
                .await?;
        }
        sort_releases_newest_first(&mut releases);
        Ok(releases)
    }

    pub(crate) async fn refresh_conversation_adapter_catalogs(
        &self,
        params: ConversationAdapterCatalogRefreshParams,
    ) -> AppResult<Vec<ConversationAdapterCatalogRelease>> {
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let cached = self
            .load_cached_conversation_adapter_catalog_releases(&catalog_url, None)
            .await?;
        if !params.force && !cached.is_empty() && !catalog_cache_is_stale(&cached) {
            return Ok(cached);
        }
        let etag = cached.first().and_then(|release| release.etag.as_deref());
        let (index_text, response_etag) = match fetch_catalog_document(&catalog_url, etag) {
            Ok(CatalogFetchResult::NotModified) => return Ok(cached),
            Ok(CatalogFetchResult::Text { text, etag }) => (text, etag),
            Err(error) if catalog_url == DEFAULT_CATALOG_V2_URL => (
                bundled_catalog_document("index.json")
                    .ok_or_else(|| {
                        AppError::External(format!("{error}; bundled Catalog v2 index is missing"))
                    })?
                    .to_string(),
                None,
            ),
            Err(error) => return Err(error),
        };
        let index: CatalogV2Index = serde_json::from_str(&index_text).map_err(|error| {
            AppError::Validation(format!(
                "conversation adapter Catalog v2 index is invalid: {error}"
            ))
        })?;
        validate_catalog_v2_index(&index)?;

        let fetched_at = Utc::now().to_rfc3339();
        let mut releases = Vec::new();
        for package in index.packages {
            let history_url = resolve_catalog_document_url(&catalog_url, &package.history_url)?;
            let history_text = match fetch_catalog_document(&history_url, None) {
                Ok(CatalogFetchResult::Text { text, .. }) => text,
                Ok(CatalogFetchResult::NotModified) => {
                    return Err(AppError::External(
                        "unexpected 304 for uncached Catalog v2 history".to_string(),
                    ));
                }
                Err(error) if catalog_url == DEFAULT_CATALOG_V2_URL => {
                    let bundled_path = format!("history/{}.json", package.package_id);
                    bundled_catalog_document(&bundled_path)
                        .ok_or_else(|| {
                            AppError::External(format!(
                                "{error}; bundled Catalog v2 history is missing: {bundled_path}"
                            ))
                        })?
                        .to_string()
                }
                Err(error) => return Err(error),
            };
            let history: CatalogV2History =
                serde_json::from_str(&history_text).map_err(|error| {
                    AppError::Validation(format!(
                        "conversation adapter Catalog v2 history is invalid ({}): {error}",
                        package.package_id
                    ))
                })?;
            validate_catalog_v2_history(&package, &history)?;
            for release in history.releases.clone() {
                releases.push(release.into_model(
                    &catalog_url,
                    &history,
                    response_etag.clone(),
                    &fetched_at,
                )?);
            }
        }

        let pool = self.db.pool().clone();
        for release in &releases {
            crate::backend::store::upsert_conversation_adapter_catalog_release_sqlx(&pool, release)
                .await?;
        }
        sort_releases_newest_first(&mut releases);
        Ok(releases)
    }

    pub(crate) async fn check_conversation_adapter_package_updates(
        &self,
        params: ConversationAdapterPackageUpdateCheckParams,
    ) -> AppResult<Vec<ConversationAdapterPackageUpdateStatus>> {
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let releases = self
            .refresh_conversation_adapter_catalogs(ConversationAdapterCatalogRefreshParams {
                catalog_url: Some(catalog_url),
                force: params.force,
            })
            .await?;
        let mut packages = self.load_conversation_adapter_packages().await?;
        let now = Utc::now().to_rfc3339();
        let mut statuses = Vec::new();
        for package in &mut packages {
            if matches!(
                package.origin,
                crate::backend::domain::ConversationAdapterPackageOrigin::LocalDirectory
                    | crate::backend::domain::ConversationAdapterPackageOrigin::GitRef
                    | crate::backend::domain::ConversationAdapterPackageOrigin::DevOverride
            ) {
                continue;
            }
            if package.update_policy
                == crate::backend::domain::ConversationPackageUpdatePolicy::PinExact
            {
                package.latest_version = None;
                package.last_checked_at = Some(now.clone());
                self.save_conversation_adapter_package(package).await?;
                statuses.push(ConversationAdapterPackageUpdateStatus {
                    package_id: package.package_id.clone(),
                    current_version: package.version.clone(),
                    latest_compatible_release: None,
                    update_available: false,
                });
                continue;
            }
            let mut compatible = releases
                .iter()
                .filter(|release| release.package_id == package.package_id)
                .filter(|release| release_is_core_compatible(release))
                .filter(|release| {
                    package.update_policy
                        == crate::backend::domain::ConversationPackageUpdatePolicy::FollowBeta
                        || release.channel
                            == crate::backend::domain::ConversationAdapterReleaseChannel::Stable
                })
                .cloned()
                .collect::<Vec<_>>();
            sort_releases_newest_first(&mut compatible);
            let latest = compatible.first().cloned();
            let update_available = latest
                .as_ref()
                .is_some_and(|release| semver_is_newer(&release.version, &package.version));
            package.latest_version = latest.as_ref().map(|release| release.version.clone());
            package.last_checked_at = Some(now.clone());
            self.save_conversation_adapter_package(package).await?;
            statuses.push(ConversationAdapterPackageUpdateStatus {
                package_id: package.package_id.clone(),
                current_version: package.version.clone(),
                latest_compatible_release: latest,
                update_available,
            });
        }
        Ok(statuses)
    }

    pub(crate) async fn set_conversation_adapter_package_update_policy(
        &self,
        params: ConversationAdapterPackageUpdatePolicyParams,
    ) -> AppResult<crate::backend::domain::ConversationAdapterPackage> {
        let package_id = params.package_id.trim();
        let mut package = self
            .load_conversation_adapter_package(package_id)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "conversation adapter package not found: {package_id}"
                ))
            })?;
        if package.origin
            != crate::backend::domain::ConversationAdapterPackageOrigin::ManagedRelease
            && params.update_policy
                != crate::backend::domain::ConversationPackageUpdatePolicy::PinExact
        {
            return Err(AppError::Validation(
                "local, Git, dev, built-in, and legacy packages must remain pinned".to_string(),
            ));
        }
        package.update_policy = params.update_policy;
        package.updated_at = Utc::now().to_rfc3339();
        self.save_conversation_adapter_package(&package).await?;
        Ok(package)
    }

    async fn load_cached_conversation_adapter_catalog_releases(
        &self,
        catalog_url: &str,
        package_id: Option<&str>,
    ) -> AppResult<Vec<ConversationAdapterCatalogRelease>> {
        let pool = self.db.pool().clone();
        let catalog_url = catalog_url.to_string();
        Ok(
            crate::backend::store::list_conversation_adapter_catalog_releases_sqlx(
                &pool,
                &catalog_url,
                package_id,
            )
            .await?,
        )
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageUpdateStatus {
    pub(crate) package_id: String,
    pub(crate) current_version: String,
    pub(crate) latest_compatible_release: Option<ConversationAdapterCatalogRelease>,
    pub(crate) update_available: bool,
}

#[cfg(test)]
#[path = "conversation_adapter_catalog_v2_tests.rs"]
mod tests;
