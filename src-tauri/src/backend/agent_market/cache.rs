use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};

use super::{
    catalog::{CatalogRevision, CatalogService},
    types::{AgentMarketError, Catalog},
};

pub(crate) const MAX_CATALOG_BYTES: usize = 5 * 1024 * 1024;
pub(crate) const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/util6/assetiweave/main/builtin-assets/agent-market/catalog-v1.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CatalogRefreshOutcome {
    Updated {
        catalog: Catalog,
        etag: Option<String>,
    },
    NotModified {
        catalog: Catalog,
        etag: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct CatalogCache {
    pub(crate) catalog_path: PathBuf,
    pub(crate) etag_path: PathBuf,
    pub(crate) meta_path: PathBuf,
}

impl CatalogCache {
    pub(crate) fn in_app_cache() -> Option<Self> {
        dirs::home_dir()
            .map(|home| Self::new(home.join(".assetiweave").join("cache").join("agent-market")))
    }

    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            catalog_path: root.join("catalog-v1.json"),
            etag_path: root.join("catalog-v1.etag"),
            meta_path: root.join("catalog-v1.meta.json"),
        }
    }

    pub(crate) fn read(&self) -> Result<Option<(Catalog, Option<String>)>, AgentMarketError> {
        let bytes = match std::fs::read(&self.catalog_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(AgentMarketError::Io(error)),
        };
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err(AgentMarketError::Catalog(
                super::catalog::CatalogError::TooLarge,
            ));
        }
        let catalog = CatalogService::from_bytes(&bytes)
            .map_err(AgentMarketError::Catalog)?
            .catalog();
        let etag = std::fs::read_to_string(&self.meta_path)
            .ok()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
            .and_then(|value| {
                value
                    .get("etag")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                std::fs::read_to_string(&self.etag_path)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });
        Ok(Some(((*catalog).clone(), etag)))
    }

    pub(crate) fn write_atomic(
        &self,
        bytes: &[u8],
        etag: Option<&str>,
    ) -> Result<Catalog, AgentMarketError> {
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err(AgentMarketError::Catalog(
                super::catalog::CatalogError::TooLarge,
            ));
        }
        let service = CatalogService::from_bytes(bytes).map_err(AgentMarketError::Catalog)?;
        let parent = self.catalog_path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "catalog cache has no parent directory",
            )
        })?;
        std::fs::create_dir_all(parent)?;
        let previous_catalog = std::fs::read(&self.catalog_path).ok();
        let previous_etag = std::fs::read(&self.etag_path).ok();
        let previous_meta = std::fs::read(&self.meta_path).ok();
        let suffix = uuid::Uuid::new_v4().to_string();
        let temp_catalog = parent.join(format!(".catalog-v1.{suffix}.tmp"));
        std::fs::write(&temp_catalog, bytes)?;
        if let Err(error) = std::fs::rename(&temp_catalog, &self.catalog_path) {
            let _ = std::fs::remove_file(&temp_catalog);
            return Err(AgentMarketError::Io(error));
        }
        if let Some(etag) = etag.map(str::trim).filter(|value| !value.is_empty()) {
            let temp_etag = parent.join(format!(".catalog-v1.{suffix}.etag.tmp"));
            if let Err(error) = std::fs::write(&temp_etag, etag) {
                let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
                return Err(AgentMarketError::Io(error));
            }
            if let Err(error) = std::fs::rename(&temp_etag, &self.etag_path) {
                let _ = std::fs::remove_file(&temp_etag);
                let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
                let _ = restore_cache_file(&self.etag_path, previous_etag.as_deref());
                return Err(AgentMarketError::Io(error));
            }
        } else {
            let _ = restore_cache_file(&self.etag_path, None);
        }
        let metadata = serde_json::json!({
            "etag": etag.map(str::to_string),
            "fetched_at": chrono::Utc::now().to_rfc3339(),
            "source_url_id": "default-curated",
            "schema_version": "assetiweave.agent-market/v1",
            "catalog_version": service.catalog().catalog_version,
        });
        let temp_meta = parent.join(format!(".catalog-v1.{suffix}.meta.tmp"));
        let meta_bytes = serde_json::to_vec_pretty(&metadata)?;
        if let Err(error) = std::fs::write(&temp_meta, meta_bytes) {
            let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
            let _ = restore_cache_file(&self.etag_path, previous_etag.as_deref());
            let _ = restore_cache_file(&self.meta_path, previous_meta.as_deref());
            return Err(AgentMarketError::Io(error));
        }
        if let Err(error) = std::fs::rename(&temp_meta, &self.meta_path) {
            let _ = std::fs::remove_file(&temp_meta);
            let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
            let _ = restore_cache_file(&self.etag_path, previous_etag.as_deref());
            let _ = restore_cache_file(&self.meta_path, previous_meta.as_deref());
            return Err(AgentMarketError::Io(error));
        }
        Ok((*service.catalog()).clone())
    }

    pub(crate) fn best_available() -> Result<CatalogService, AgentMarketError> {
        let bundled = CatalogService::bundled()
            .map_err(AgentMarketError::Catalog)?
            .catalog();
        let cached = Self::in_app_cache()
            .and_then(|cache| cache.read().ok().flatten())
            .map(|(catalog, _etag)| catalog);
        Ok(CatalogService::from_catalog(select_active_catalog(
            (*bundled).clone(),
            cached,
        )))
    }

    pub(crate) fn refresh_default() -> Result<CatalogRefreshOutcome, AgentMarketError> {
        Self::refresh_from_url(DEFAULT_CATALOG_URL)
    }

    pub(crate) fn refresh_from_url(url: &str) -> Result<CatalogRefreshOutcome, AgentMarketError> {
        let client = crate::backend::http_client::shared_http_client().map_err(|error| {
            AgentMarketError::new("http_client_init_failed", &error.to_string(), false)
        })?;
        Self::refresh_from_url_with_client(&client, url)
    }

    pub(crate) fn refresh_from_url_with_client(
        client: &reqwest::blocking::Client,
        url: &str,
    ) -> Result<CatalogRefreshOutcome, AgentMarketError> {
        let cache = Self::in_app_cache();
        let cached = cache.as_ref().and_then(|cache| cache.read().ok().flatten());
        let etag = cached.as_ref().and_then(|(_, etag)| etag.as_deref());

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            reqwest::header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            reqwest::header::USER_AGENT,
            reqwest::header::HeaderValue::from_static("AssetIWeave/0.5 agent-market-catalog"),
        );
        if let Some(etag) = etag {
            headers.insert(
                reqwest::header::IF_NONE_MATCH,
                reqwest::header::HeaderValue::from_str(etag).map_err(|error| {
                    AgentMarketError::new("invalid_etag", &error.to_string(), false)
                })?,
            );
        }

        let response = crate::backend::http_client::get_with_redirects(
            client,
            url,
            headers,
            Duration::from_secs(15),
        )
        .map_err(|error| {
            AgentMarketError::new(
                "catalog_refresh_failed",
                &format!("Agent catalog refresh failed: {error}"),
                true,
            )
        })?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some((catalog, etag)) = cached else {
                return Err(AgentMarketError::new(
                    "catalog_304_without_cache",
                    "Agent catalog returned 304 without a valid cache",
                    false,
                ));
            };
            return Ok(CatalogRefreshOutcome::NotModified { catalog, etag });
        }

        let response = response.error_for_status().map_err(|error| {
            AgentMarketError::new(
                "catalog_refresh_failed",
                &format!("Agent catalog refresh failed: {error}"),
                true,
            )
        })?;

        let final_url = response.url();
        let host = final_url
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(
            host.as_str(),
            "raw.githubusercontent.com" | "github.com" | "raw.github.com"
        ) && !host.ends_with(".githubusercontent.com")
        {
            return Err(AgentMarketError::new(
                "catalog_redirect_disallowed",
                "Agent catalog redirect host is not allowlisted",
                false,
            ));
        }

        let response_etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);

        let mut bytes = Vec::new();
        let reader = response;
        reader
            .take((MAX_CATALOG_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                AgentMarketError::new(
                    "catalog_read_failed",
                    &format!("Agent catalog response could not be read: {error}"),
                    true,
                )
            })?;
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err(AgentMarketError::Catalog(
                super::catalog::CatalogError::TooLarge,
            ));
        }
        let service = CatalogService::from_bytes(&bytes).map_err(AgentMarketError::Catalog)?;
        let catalog = (*service.catalog()).clone();
        let bundled = CatalogService::bundled().map_err(AgentMarketError::Catalog)?;
        let active = select_active_catalog(
            (*bundled.catalog()).clone(),
            cached.as_ref().map(|(catalog, _)| catalog.clone()),
        );
        let active_revision =
            CatalogRevision::parse(&active.catalog_version).map_err(AgentMarketError::Catalog)?;
        let downloaded_revision = service.revision().map_err(AgentMarketError::Catalog)?;
        if downloaded_revision < active_revision {
            return Err(AgentMarketError::Catalog(
                super::catalog::CatalogError::Invalid(format!(
                    "catalog revision rollback rejected: {} < {}",
                    catalog.catalog_version, active.catalog_version
                )),
            ));
        }
        if downloaded_revision == active_revision
            && catalog_fingerprint(&catalog) != catalog_fingerprint(&active)
        {
            return Err(AgentMarketError::Catalog(
                super::catalog::CatalogError::Invalid(format!(
                    "catalog revision collision rejected: {}",
                    catalog.catalog_version
                )),
            ));
        }
        if let Some(cache) = cache {
            cache.write_atomic(&bytes, response_etag.as_deref())?;
        }
        Ok(CatalogRefreshOutcome::Updated {
            catalog,
            etag: response_etag,
        })
    }
}

pub(crate) fn select_active_catalog(bundled: Catalog, cached: Option<Catalog>) -> Catalog {
    let Some(cached) = cached else {
        return bundled;
    };

    // A cache with the same revision but a different payload is not a valid
    // candidate. Keep the bundled payload as the deterministic fallback.
    if cached.catalog_version == bundled.catalog_version
        && catalog_fingerprint(&cached) != catalog_fingerprint(&bundled)
    {
        return bundled;
    }

    let bundled_revision = CatalogRevision::parse(&bundled.catalog_version).ok();
    let cached_revision = CatalogRevision::parse(&cached.catalog_version).ok();
    match (bundled_revision, cached_revision) {
        (Some(bundled_revision), Some(cached_revision)) if cached_revision > bundled_revision => {
            cached
        }
        _ => bundled,
    }
}

fn catalog_fingerprint(catalog: &Catalog) -> String {
    let bytes = serde_json::to_vec(catalog).expect("catalog serialization must be infallible");
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn restore_cache_file(path: &Path, bytes: Option<&[u8]>) -> std::io::Result<()> {
    match bytes {
        Some(bytes) => std::fs::write(path, bytes),
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        },
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
