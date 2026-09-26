use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterCatalogRelease, ConversationAdapterPackageRecordKind,
    ConversationAdapterReleaseChannel,
};
use chrono::Duration;
use semver::{Version, VersionReq};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub(crate) const DEFAULT_CATALOG_V2_URL: &str =
    "https://raw.githubusercontent.com/util6/assetiweave/main/builtin-assets/index.json";
pub(crate) const CATALOG_CACHE_MAX_AGE_HOURS: i64 = 24;

#[derive(Debug, Deserialize)]
pub(crate) struct CatalogV2Index {
    pub(crate) schema_version: u32,
    pub(crate) packages: Vec<CatalogV2PackageIndex>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CatalogV2PackageIndex {
    pub(crate) package_id: String,
    pub(crate) stable_version: Option<String>,
    pub(crate) beta_version: Option<String>,
    pub(crate) history_url: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CatalogV2History {
    pub(crate) schema_version: u32,
    pub(crate) package_id: String,
    pub(crate) adapter_id: String,
    pub(crate) name: String,
    pub(crate) publisher: String,
    pub(crate) record_kind: ConversationAdapterPackageRecordKind,
    #[serde(default = "default_package_manifest_file")]
    pub(crate) package_manifest_file: String,
    #[serde(default = "default_adapter_manifest_file")]
    pub(crate) adapter_manifest_file: String,
    pub(crate) releases: Vec<CatalogV2Release>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CatalogV2Release {
    pub(crate) version: String,
    pub(crate) channel: ConversationAdapterReleaseChannel,
    pub(crate) released_at: Option<String>,
    pub(crate) core_compatibility: String,
    pub(crate) artifact_url: String,
    pub(crate) artifact_size: Option<i64>,
    pub(crate) artifact_sha256: String,
    pub(crate) changelog_markdown: String,
    #[serde(default)]
    pub(crate) breaking_change: bool,
    pub(crate) runtime_protocol: String,
    pub(crate) adapter_manifest: Option<Value>,
    pub(crate) source: Option<CatalogV2Source>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CatalogV2Source {
    #[serde(rename = "type")]
    kind: CatalogV2SourceKind,
    url: String,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CatalogV2SourceKind {
    Github,
    ArtifactZip,
    LocalDirectory,
}

impl CatalogV2Release {
    pub(crate) fn into_model(
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
                .map_err(|error| AppError::Validation(error.to_string()))?,
            source_json: self
                .source
                .map(|source| serde_json::to_string(&source))
                .transpose()
                .map_err(|error| AppError::Validation(error.to_string()))?,
            etag,
            fetched_at: fetched_at.to_string(),
        })
    }
}

pub(crate) enum CatalogFetchResult {
    NotModified,
    Text { text: String, etag: Option<String> },
}

pub(crate) fn fetch_catalog_document(
    url: &str,
    etag: Option<&str>,
) -> AppResult<CatalogFetchResult> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        let path = crate::backend::infrastructure::path_utils::expand_path(url)?;
        return fs::read_to_string(&path)
            .map(|text| CatalogFetchResult::Text { text, etag: None })
            .map_err(|error| {
                AppError::Storage(format!(
                    "read conversation adapter Catalog v2 failed: {error}"
                ))
            });
    }
    let client = crate::backend::infrastructure::http_client::shared_http_client()?;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "AssetIWeave/0.5 conversation-adapter-catalog-v2",
        ),
    );
    if let Some(etag) = etag {
        headers.insert(
            reqwest::header::IF_NONE_MATCH,
            reqwest::header::HeaderValue::from_str(etag).map_err(AppError::external)?,
        );
    }
    let response = crate::backend::infrastructure::http_client::get_with_redirects(
        &client,
        url,
        headers,
        std::time::Duration::from_secs(15),
    )
    .map_err(|error| {
        AppError::External(format!(
            "conversation adapter Catalog v2 request failed: {error}"
        ))
    })?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(CatalogFetchResult::NotModified);
    }
    let response = response.error_for_status().map_err(|error| {
        AppError::External(format!(
            "conversation adapter Catalog v2 request failed: {error}"
        ))
    })?;
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let text = crate::backend::infrastructure::http_client::read_response_text_with_limit(
        response,
        crate::backend::infrastructure::http_client::DEFAULT_MAX_TEXT_RESPONSE_BYTES,
    )?;
    Ok(CatalogFetchResult::Text { text, etag })
}

pub(crate) fn normalized_catalog_v2_url(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_CATALOG_V2_URL)
        .to_string()
}

pub(crate) fn resolve_catalog_document_url(
    index_url: &str,
    history_url: &str,
) -> AppResult<String> {
    if history_url.starts_with("https://") || history_url.starts_with("http://") {
        return Ok(history_url.to_string());
    }
    if index_url.starts_with("https://") || index_url.starts_with("http://") {
        return url::Url::parse(index_url)
            .and_then(|url| url.join(history_url))
            .map(|url| url.to_string())
            .map_err(|error| {
                AppError::Validation(format!("resolve Catalog v2 history URL failed: {error}"))
            });
    }
    let index_path = crate::backend::infrastructure::path_utils::expand_path(index_url)?;
    Ok(index_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(history_url)
        .to_string_lossy()
        .to_string())
}

pub(crate) fn validate_catalog_v2_index(index: &CatalogV2Index) -> AppResult<()> {
    if index.schema_version != 2 {
        return Err(AppError::Validation(
            "conversation adapter Catalog index schema_version must be 2".to_string(),
        ));
    }
    let mut ids = HashSet::new();
    for package in &index.packages {
        validate_catalog_package_id(&package.package_id)?;
        if !ids.insert(package.package_id.clone()) {
            return Err(AppError::Validation(format!(
                "duplicate Catalog v2 package: {}",
                package.package_id
            )));
        }
        if package.history_url.trim().is_empty() {
            return Err(AppError::Validation(format!(
                "Catalog v2 history URL is required: {}",
                package.package_id
            )));
        }
        for version in [
            package.stable_version.as_deref(),
            package.beta_version.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            Version::parse(version).map_err(|error| {
                AppError::Validation(format!(
                    "Catalog v2 latest version must be SemVer ({version}): {error}"
                ))
            })?;
        }
    }
    Ok(())
}

pub(crate) fn validate_catalog_v2_history(
    package: &CatalogV2PackageIndex,
    history: &CatalogV2History,
) -> AppResult<()> {
    if history.schema_version != 2 || history.package_id != package.package_id {
        return Err(AppError::Validation(format!(
            "Catalog v2 history identity mismatch: {}",
            package.package_id
        )));
    }
    let mut versions = HashSet::new();
    for release in &history.releases {
        Version::parse(&release.version).map_err(|error| {
            AppError::Validation(format!(
                "Catalog v2 release version must be SemVer ({}): {error}",
                release.version
            ))
        })?;
        VersionReq::parse(&release.core_compatibility).map_err(|error| {
            AppError::Validation(format!(
                "Catalog v2 Core compatibility is invalid ({}): {error}",
                release.version
            ))
        })?;
        if !versions.insert(release.version.clone()) {
            return Err(AppError::Validation(format!(
                "duplicate Catalog v2 release: {}@{}",
                history.package_id, release.version
            )));
        }
        if release.runtime_protocol != "stdio-ndjson-v1" {
            return Err(AppError::Validation(format!(
                "unsupported Catalog v2 runtime protocol: {}",
                release.runtime_protocol
            )));
        }
        if release.artifact_url.trim().is_empty()
            || release.artifact_sha256.len() != 64
            || !release
                .artifact_sha256
                .chars()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(AppError::Validation(format!(
                "Catalog v2 artifact metadata is invalid: {}@{}",
                history.package_id, release.version
            )));
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
                return Err(AppError::Validation(format!(
                    "Catalog v2 latest channel version is missing: {}@{}",
                    history.package_id, expected
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_catalog_package_id(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || !value.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_' | '.')
        })
    {
        Err(AppError::Validation(format!(
            "Catalog v2 package_id is unsafe: {value}"
        )))
    } else {
        Ok(())
    }
}

pub(crate) fn release_is_core_compatible(release: &ConversationAdapterCatalogRelease) -> bool {
    VersionReq::parse(&release.core_compatibility)
        .ok()
        .zip(Version::parse(env!("CARGO_PKG_VERSION")).ok())
        .is_some_and(|(requirement, current)| requirement.matches(&current))
}

pub(crate) fn semver_is_newer(candidate: &str, current: &str) -> bool {
    Version::parse(candidate)
        .ok()
        .zip(Version::parse(current).ok())
        .is_some_and(|(candidate, current)| candidate > current)
}

pub(crate) fn sort_releases_newest_first(releases: &mut [ConversationAdapterCatalogRelease]) {
    releases.sort_by(|left, right| {
        let left = Version::parse(&left.version).ok();
        let right = Version::parse(&right.version).ok();
        right.cmp(&left)
    });
}

pub(crate) fn catalog_cache_is_stale(releases: &[ConversationAdapterCatalogRelease]) -> bool {
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

pub(crate) fn default_package_manifest_file() -> String {
    "conversation-adapter-package.json".to_string()
}

pub(crate) fn default_adapter_manifest_file() -> String {
    "conversation-adapter.json".to_string()
}

pub(crate) fn bundled_catalog_document(path: &str) -> Option<&'static str> {
    match path {
        "index.json" => Some(include_str!("../../../../../builtin-assets/index.json")),
        "history/io.github.util6.codex-session.json" => Some(include_str!(
            "../../../../../builtin-assets/history/io.github.util6.codex-session.json"
        )),
        "history/io.github.util6.opencode-session.json" => Some(include_str!(
            "../../../../../builtin-assets/history/io.github.util6.opencode-session.json"
        )),
        "history/io.github.util6.claude-code-session.json" => Some(include_str!(
            "../../../../../builtin-assets/history/io.github.util6.claude-code-session.json"
        )),
        "history/io.github.util6.zcode-session.json" => Some(include_str!(
            "../../../../../builtin-assets/history/io.github.util6.zcode-session.json"
        )),
        "history/io.github.util6.chatgpt-web.json" => Some(include_str!(
            "../../../../../builtin-assets/history/io.github.util6.chatgpt-web.json"
        )),
        "history/io.github.util6.qwen-web.json" => Some(include_str!(
            "../../../../../builtin-assets/history/io.github.util6.qwen-web.json"
        )),
        "history/io.github.util6.gemini-web.json" => Some(include_str!(
            "../../../../../builtin-assets/history/io.github.util6.gemini-web.json"
        )),
        _ => None,
    }
}
