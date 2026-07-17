use super::prelude::*;
use crate::backend::models::{
    ConversationAdapterCatalogRelease, ConversationAdapterPackageRecordKind,
    ConversationAdapterReleaseChannel,
};
use chrono::Duration;
use semver::{Version, VersionReq};

const DEFAULT_CATALOG_V2_URL: &str =
    "https://raw.githubusercontent.com/util6/assetiweave/main/parser-catalog/index.json";
const CATALOG_CACHE_MAX_AGE_HOURS: i64 = 24;

impl AppService {
    pub(super) fn install_conversation_adapter_package_release(
        &self,
        params: ConversationAdapterPackageInstallParams,
    ) -> AppResult<Value> {
        let version = params
            .version
            .as_deref()
            .map(str::trim)
            .filter(|version| !version.is_empty())
            .ok_or_else(|| {
                "conversation adapter package release version is required".to_string()
            })?;
        Version::parse(version).map_err(|error| {
            format!("conversation adapter package release version must be SemVer: {error}")
        })?;
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let releases = self.list_conversation_adapter_package_releases(
            ConversationAdapterPackageReleaseListParams {
                catalog_url: Some(catalog_url.clone()),
                package_id: params.package_id.clone(),
                refresh: false,
            },
        )?;
        let release = releases
            .into_iter()
            .find(|release| release.version == version)
            .ok_or_else(|| {
                format!(
                    "conversation adapter package release not found: {}@{}",
                    params.package_id, version
                )
            })?;
        if !release_is_core_compatible(&release) {
            return Err(format!(
                "conversation adapter package release is not compatible with this Core: {}@{} ({})",
                release.package_id, release.version, release.core_compatibility
            ));
        }
        let item = super::ConversationScriptCatalogItem {
            id: release.package_id.clone(),
            name: release.name.clone(),
            version: release.version.clone(),
            record_kind: match release.record_kind {
                ConversationAdapterPackageRecordKind::Session => {
                    super::ConversationScriptRecordKind::Session
                }
                ConversationAdapterPackageRecordKind::Web => {
                    super::ConversationScriptRecordKind::Web
                }
            },
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
            source: super::ConversationScriptCatalogSource {
                kind: super::ConversationScriptCatalogSourceKind::ArtifactZip,
                url: release.artifact_url.clone(),
                branch: None,
                path: None,
            },
        };
        super::conversation_script_catalog::install_conversation_adapter_package_from_item(
            self,
            &item,
            params.dry_run,
            Some(&catalog_url),
        )
    }

    pub(crate) fn list_conversation_adapter_package_releases(
        &self,
        params: ConversationAdapterPackageReleaseListParams,
    ) -> AppResult<Vec<ConversationAdapterCatalogRelease>> {
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let package_id = params.package_id.trim();
        if package_id.is_empty() {
            return Err(
                "conversation adapter package release list requires package_id".to_string(),
            );
        }
        let mut releases =
            self.load_cached_conversation_adapter_catalog_releases(&catalog_url, Some(package_id))?;
        if params.refresh || releases.is_empty() || catalog_cache_is_stale(&releases) {
            self.refresh_conversation_adapter_catalogs(ConversationAdapterCatalogRefreshParams {
                catalog_url: Some(catalog_url.clone()),
                force: params.refresh,
            })?;
            releases = self.load_cached_conversation_adapter_catalog_releases(
                &catalog_url,
                Some(package_id),
            )?;
        }
        sort_releases_newest_first(&mut releases);
        Ok(releases)
    }

    pub(crate) fn refresh_conversation_adapter_catalogs(
        &self,
        params: ConversationAdapterCatalogRefreshParams,
    ) -> AppResult<Vec<ConversationAdapterCatalogRelease>> {
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let cached = self.load_cached_conversation_adapter_catalog_releases(&catalog_url, None)?;
        if !params.force && !cached.is_empty() && !catalog_cache_is_stale(&cached) {
            return Ok(cached);
        }
        let etag = cached.first().and_then(|release| release.etag.as_deref());
        let (index_text, response_etag) = match fetch_catalog_document(&catalog_url, etag) {
            Ok(CatalogFetchResult::NotModified) => return Ok(cached),
            Ok(CatalogFetchResult::Text { text, etag }) => (text, etag),
            Err(error) if catalog_url == DEFAULT_CATALOG_V2_URL => (
                bundled_catalog_document("index.json")
                    .ok_or_else(|| format!("{error}; bundled Catalog v2 index is missing"))?
                    .to_string(),
                None,
            ),
            Err(error) => return Err(error),
        };
        let index: CatalogV2Index = serde_json::from_str(&index_text).map_err(|error| {
            format!("conversation adapter Catalog v2 index is invalid: {error}")
        })?;
        validate_catalog_v2_index(&index)?;

        let fetched_at = Utc::now().to_rfc3339();
        let mut releases = Vec::new();
        for package in index.packages {
            let history_url = resolve_catalog_document_url(&catalog_url, &package.history_url)?;
            let history_text = match fetch_catalog_document(&history_url, None) {
                Ok(CatalogFetchResult::Text { text, .. }) => text,
                Ok(CatalogFetchResult::NotModified) => {
                    return Err("unexpected 304 for uncached Catalog v2 history".to_string());
                }
                Err(error) if catalog_url == DEFAULT_CATALOG_V2_URL => {
                    let bundled_path = format!("history/{}.json", package.package_id);
                    bundled_catalog_document(&bundled_path)
                        .ok_or_else(|| {
                            format!(
                                "{error}; bundled Catalog v2 history is missing: {bundled_path}"
                            )
                        })?
                        .to_string()
                }
                Err(error) => return Err(error),
            };
            let history: CatalogV2History =
                serde_json::from_str(&history_text).map_err(|error| {
                    format!(
                        "conversation adapter Catalog v2 history is invalid ({}): {error}",
                        package.package_id
                    )
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
        let tenant_id = self.tenant_id().to_string();
        let releases_to_save = releases.clone();
        self.db.block_on(async move {
            for release in &releases_to_save {
                crate::backend::store::upsert_conversation_adapter_catalog_release_sqlx(
                    &pool, &tenant_id, release,
                )
                .await?;
            }
            AppResult::Ok(())
        })?;
        sort_releases_newest_first(&mut releases);
        Ok(releases)
    }

    pub(crate) fn check_conversation_adapter_package_updates(
        &self,
        params: ConversationAdapterPackageUpdateCheckParams,
    ) -> AppResult<Vec<ConversationAdapterPackageUpdateStatus>> {
        let catalog_url = normalized_catalog_v2_url(params.catalog_url.as_deref());
        let releases =
            self.refresh_conversation_adapter_catalogs(ConversationAdapterCatalogRefreshParams {
                catalog_url: Some(catalog_url),
                force: params.force,
            })?;
        let mut packages = self.load_conversation_adapter_packages()?;
        let now = Utc::now().to_rfc3339();
        let mut statuses = Vec::new();
        for package in &mut packages {
            if matches!(
                package.origin,
                crate::backend::models::ConversationAdapterPackageOrigin::LocalDirectory
                    | crate::backend::models::ConversationAdapterPackageOrigin::GitRef
                    | crate::backend::models::ConversationAdapterPackageOrigin::DevOverride
            ) {
                continue;
            }
            if package.update_policy
                == crate::backend::models::ConversationPackageUpdatePolicy::PinExact
            {
                package.latest_version = None;
                package.last_checked_at = Some(now.clone());
                self.save_conversation_adapter_package(package)?;
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
                        == crate::backend::models::ConversationPackageUpdatePolicy::FollowBeta
                        || release.channel
                            == crate::backend::models::ConversationAdapterReleaseChannel::Stable
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
            self.save_conversation_adapter_package(package)?;
            statuses.push(ConversationAdapterPackageUpdateStatus {
                package_id: package.package_id.clone(),
                current_version: package.version.clone(),
                latest_compatible_release: latest,
                update_available,
            });
        }
        Ok(statuses)
    }

    pub(crate) fn set_conversation_adapter_package_update_policy(
        &self,
        params: ConversationAdapterPackageUpdatePolicyParams,
    ) -> AppResult<crate::backend::models::ConversationAdapterPackage> {
        let package_id = params.package_id.trim();
        let mut package = self
            .load_conversation_adapter_package(package_id)?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))?;
        if package.origin
            != crate::backend::models::ConversationAdapterPackageOrigin::ManagedRelease
            && params.update_policy
                != crate::backend::models::ConversationPackageUpdatePolicy::PinExact
        {
            return Err(
                "local, Git, dev, built-in, and legacy packages must remain pinned".to_string(),
            );
        }
        package.update_policy = params.update_policy;
        package.updated_at = Utc::now().to_rfc3339();
        self.save_conversation_adapter_package(&package)?;
        Ok(package)
    }

    fn load_cached_conversation_adapter_catalog_releases(
        &self,
        catalog_url: &str,
        package_id: Option<&str>,
    ) -> AppResult<Vec<ConversationAdapterCatalogRelease>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let catalog_url = catalog_url.to_string();
        let package_id = package_id.map(str::to_string);
        self.db.block_on(async move {
            crate::backend::store::list_conversation_adapter_catalog_releases_sqlx(
                &pool,
                &tenant_id,
                &catalog_url,
                package_id.as_deref(),
            )
            .await
        })
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub(crate) struct ConversationAdapterPackageUpdateStatus {
    pub(crate) package_id: String,
    pub(crate) current_version: String,
    pub(crate) latest_compatible_release: Option<ConversationAdapterCatalogRelease>,
    pub(crate) update_available: bool,
}

#[derive(Debug, Deserialize)]
struct CatalogV2Index {
    schema_version: u32,
    packages: Vec<CatalogV2PackageIndex>,
}

#[derive(Debug, Deserialize)]
struct CatalogV2PackageIndex {
    package_id: String,
    stable_version: Option<String>,
    beta_version: Option<String>,
    history_url: String,
}

#[derive(Debug, Deserialize)]
struct CatalogV2History {
    schema_version: u32,
    package_id: String,
    adapter_id: String,
    name: String,
    publisher: String,
    record_kind: ConversationAdapterPackageRecordKind,
    #[serde(default = "default_package_manifest_file")]
    package_manifest_file: String,
    #[serde(default = "default_adapter_manifest_file")]
    adapter_manifest_file: String,
    releases: Vec<CatalogV2Release>,
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogV2Release {
    version: String,
    channel: ConversationAdapterReleaseChannel,
    released_at: Option<String>,
    core_compatibility: String,
    artifact_url: String,
    artifact_size: Option<i64>,
    artifact_sha256: String,
    changelog_markdown: String,
    #[serde(default)]
    breaking_change: bool,
    runtime_protocol: String,
    adapter_manifest: Option<Value>,
    source: Option<super::ConversationScriptCatalogSource>,
}

impl CatalogV2Release {
    fn into_model(
        self,
        catalog_url: &str,
        history: &CatalogV2History,
        etag: Option<String>,
        fetched_at: &str,
    ) -> AppResult<ConversationAdapterCatalogRelease> {
        Ok(ConversationAdapterCatalogRelease {
            catalog_url: catalog_url.to_string(),
            package_id: history.package_id.clone(),
            adapter_id: history.adapter_id.clone(),
            name: history.name.clone(),
            publisher: history.publisher.clone(),
            version: self.version,
            channel: self.channel,
            released_at: self.released_at,
            core_compatibility: self.core_compatibility,
            artifact_url: self.artifact_url,
            artifact_size: self.artifact_size,
            artifact_sha256: self.artifact_sha256,
            changelog_markdown: self.changelog_markdown,
            breaking_change: self.breaking_change,
            runtime_protocol: self.runtime_protocol,
            record_kind: history.record_kind,
            package_manifest_file: history.package_manifest_file.clone(),
            adapter_manifest_file: history.adapter_manifest_file.clone(),
            adapter_manifest_json: self
                .adapter_manifest
                .map(|value| serde_json::to_string(&value))
                .transpose()
                .map_err(|error| error.to_string())?,
            source_json: self
                .source
                .map(|source| serde_json::to_string(&source))
                .transpose()
                .map_err(|error| error.to_string())?,
            etag,
            fetched_at: fetched_at.to_string(),
        })
    }
}

enum CatalogFetchResult {
    NotModified,
    Text { text: String, etag: Option<String> },
}

fn fetch_catalog_document(url: &str, etag: Option<&str>) -> AppResult<CatalogFetchResult> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        let path = crate::backend::path_utils::expand_path(url)?;
        return fs::read_to_string(&path)
            .map(|text| CatalogFetchResult::Text { text, etag: None })
            .map_err(|error| format!("read conversation adapter Catalog v2 failed: {error}"));
    }
    let mut request = ureq::get(url).set(
        "User-Agent",
        "AssetIWeave/0.5 conversation-adapter-catalog-v2",
    );
    if let Some(etag) = etag {
        request = request.set("If-None-Match", etag);
    }
    match request.call() {
        Ok(response) => {
            let etag = response.header("ETag").map(str::to_string);
            let text = response
                .into_string()
                .map_err(|error| format!("Catalog v2 response was not text: {error}"))?;
            Ok(CatalogFetchResult::Text { text, etag })
        }
        Err(ureq::Error::Status(304, _)) => Ok(CatalogFetchResult::NotModified),
        Err(error) => Err(format!(
            "conversation adapter Catalog v2 request failed: {error}"
        )),
    }
}

fn normalized_catalog_v2_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_CATALOG_V2_URL)
        .to_string()
}

fn resolve_catalog_document_url(index_url: &str, history_url: &str) -> AppResult<String> {
    if history_url.starts_with("https://") || history_url.starts_with("http://") {
        return Ok(history_url.to_string());
    }
    if index_url.starts_with("https://") || index_url.starts_with("http://") {
        return url::Url::parse(index_url)
            .and_then(|url| url.join(history_url))
            .map(|url| url.to_string())
            .map_err(|error| format!("resolve Catalog v2 history URL failed: {error}"));
    }
    let index_path = crate::backend::path_utils::expand_path(index_url)?;
    Ok(index_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(history_url)
        .to_string_lossy()
        .to_string())
}

fn validate_catalog_v2_index(index: &CatalogV2Index) -> AppResult<()> {
    if index.schema_version != 2 {
        return Err("conversation adapter Catalog index schema_version must be 2".to_string());
    }
    let mut ids = HashSet::new();
    for package in &index.packages {
        validate_catalog_package_id(&package.package_id)?;
        if !ids.insert(package.package_id.clone()) {
            return Err(format!(
                "duplicate Catalog v2 package: {}",
                package.package_id
            ));
        }
        if package.history_url.trim().is_empty() {
            return Err(format!(
                "Catalog v2 history URL is required: {}",
                package.package_id
            ));
        }
        for version in [
            package.stable_version.as_deref(),
            package.beta_version.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            Version::parse(version).map_err(|error| {
                format!("Catalog v2 latest version must be SemVer ({version}): {error}")
            })?;
        }
    }
    Ok(())
}

fn validate_catalog_v2_history(
    package: &CatalogV2PackageIndex,
    history: &CatalogV2History,
) -> AppResult<()> {
    if history.schema_version != 2 || history.package_id != package.package_id {
        return Err(format!(
            "Catalog v2 history identity mismatch: {}",
            package.package_id
        ));
    }
    let mut versions = HashSet::new();
    for release in &history.releases {
        Version::parse(&release.version).map_err(|error| {
            format!(
                "Catalog v2 release version must be SemVer ({}): {error}",
                release.version
            )
        })?;
        VersionReq::parse(&release.core_compatibility).map_err(|error| {
            format!(
                "Catalog v2 Core compatibility is invalid ({}): {error}",
                release.version
            )
        })?;
        if !versions.insert(release.version.clone()) {
            return Err(format!(
                "duplicate Catalog v2 release: {}@{}",
                history.package_id, release.version
            ));
        }
        if release.runtime_protocol != "stdio-ndjson-v1" {
            return Err(format!(
                "unsupported Catalog v2 runtime protocol: {}",
                release.runtime_protocol
            ));
        }
        if release.artifact_url.trim().is_empty()
            || release.artifact_sha256.len() != 64
            || !release
                .artifact_sha256
                .chars()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(format!(
                "Catalog v2 artifact metadata is invalid: {}@{}",
                history.package_id, release.version
            ));
        }
    }
    for (channel, expected) in [
        (
            ConversationAdapterReleaseChannel::Stable,
            package.stable_version.as_deref(),
        ),
        (
            ConversationAdapterReleaseChannel::Beta,
            package.beta_version.as_deref(),
        ),
    ] {
        if let Some(expected) = expected {
            if !history
                .releases
                .iter()
                .any(|release| release.channel == channel && release.version == expected)
            {
                return Err(format!(
                    "Catalog v2 latest channel version is missing: {}@{}",
                    history.package_id, expected
                ));
            }
        }
    }
    Ok(())
}

fn validate_catalog_package_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.')
        })
    {
        Err(format!("Catalog v2 package_id is unsafe: {value}"))
    } else {
        Ok(())
    }
}

pub(super) fn release_is_core_compatible(release: &ConversationAdapterCatalogRelease) -> bool {
    VersionReq::parse(&release.core_compatibility)
        .ok()
        .zip(Version::parse(env!("CARGO_PKG_VERSION")).ok())
        .is_some_and(|(requirement, current)| requirement.matches(&current))
}

fn semver_is_newer(candidate: &str, current: &str) -> bool {
    Version::parse(candidate)
        .ok()
        .zip(Version::parse(current).ok())
        .is_some_and(|(candidate, current)| candidate > current)
}

fn sort_releases_newest_first(releases: &mut [ConversationAdapterCatalogRelease]) {
    releases.sort_by(|left, right| {
        let left = Version::parse(&left.version).ok();
        let right = Version::parse(&right.version).ok();
        right.cmp(&left)
    });
}

fn catalog_cache_is_stale(releases: &[ConversationAdapterCatalogRelease]) -> bool {
    releases
        .iter()
        .filter_map(|release| chrono::DateTime::parse_from_rfc3339(&release.fetched_at).ok())
        .max()
        .map(|fetched_at| {
            Utc::now().signed_duration_since(fetched_at.with_timezone(&Utc))
                > Duration::hours(CATALOG_CACHE_MAX_AGE_HOURS)
        })
        .unwrap_or(true)
}

fn default_package_manifest_file() -> String {
    "conversation-adapter-package.json".to_string()
}

fn default_adapter_manifest_file() -> String {
    "conversation-adapter.json".to_string()
}

fn bundled_catalog_document(path: &str) -> Option<&'static str> {
    match path {
        "index.json" => Some(include_str!("../../../../parser-catalog/index.json")),
        "history/io.github.util6.codex-session.json" => Some(include_str!(
            "../../../../parser-catalog/history/io.github.util6.codex-session.json"
        )),
        "history/io.github.util6.opencode-session.json" => Some(include_str!(
            "../../../../parser-catalog/history/io.github.util6.opencode-session.json"
        )),
        "history/io.github.util6.claude-code-session.json" => Some(include_str!(
            "../../../../parser-catalog/history/io.github.util6.claude-code-session.json"
        )),
        "history/io.github.util6.zcode-session.json" => Some(include_str!(
            "../../../../parser-catalog/history/io.github.util6.zcode-session.json"
        )),
        "history/io.github.util6.chatgpt-web.json" => Some(include_str!(
            "../../../../parser-catalog/history/io.github.util6.chatgpt-web.json"
        )),
        "history/io.github.util6.qwen-web.json" => Some(include_str!(
            "../../../../parser-catalog/history/io.github.util6.qwen-web.json"
        )),
        "history/io.github.util6.gemini-web.json" => Some(include_str!(
            "../../../../parser-catalog/history/io.github.util6.gemini-web.json"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn catalog_v2_rejects_mutable_or_invalid_release_identity() {
        assert!(validate_catalog_package_id("com.util6.codex-session").is_ok());
        assert!(validate_catalog_package_id("../codex").is_err());
        assert!(semver_is_newer("1.1.0", "1.0.9"));
        assert!(!semver_is_newer("1.0.0", "1.0.0"));
    }

    #[test]
    fn local_catalog_v2_refresh_caches_history_and_changelog() {
        let root = std::env::temp_dir().join(format!("assetiweave-catalog-v2-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create Catalog v2 test root");
        let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
        let index_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("parser-catalog/index.json");

        let releases = service
            .refresh_conversation_adapter_catalogs(ConversationAdapterCatalogRefreshParams {
                catalog_url: Some(index_path.to_string_lossy().to_string()),
                force: true,
            })
            .expect("refresh local Catalog v2");

        assert_eq!(releases.len(), 14);
        assert!(releases
            .iter()
            .all(|release| release.catalog_url == index_path.to_string_lossy()));
        assert!(releases
            .iter()
            .all(|release| !release.changelog_markdown.is_empty()));
        assert!(releases
            .iter()
            .all(|release| release_is_core_compatible(release)));

        drop(service);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn exact_compatible_release_can_be_selected_for_install_preview() {
        let root =
            std::env::temp_dir().join(format!("assetiweave-release-select-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create release selection root");
        let service = AppService::open_with_db_path(root.join("app.db")).expect("open service");
        let index_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace root")
            .join("parser-catalog/index.json");

        let preview = service
            .install_conversation_adapter_package(ConversationAdapterPackageInstallParams {
                catalog_url: Some(index_path.to_string_lossy().to_string()),
                package_id: "io.github.util6.codex-session".to_string(),
                version: Some("1.0.1".to_string()),
                dry_run: true,
                yes: false,
            })
            .expect("preview exact release install");

        assert_eq!(preview["package_id"], "io.github.util6.codex-session");
        assert!(preview["install_path"]
            .as_str()
            .is_some_and(|path| path.ends_with("/versions/1.0.1")));

        drop(service);
        let _ = fs::remove_dir_all(root);
    }
}
