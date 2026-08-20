use std::{
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};

use super::{
    catalog::{is_core_compatible, CatalogService},
    types::Catalog,
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

    pub(crate) fn read(&self) -> Result<Option<(Catalog, Option<String>)>, String> {
        let bytes = match std::fs::read(&self.catalog_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err("Agent catalog cache exceeds the size limit".to_string());
        }
        let catalog = CatalogService::from_bytes(&bytes)
            .map_err(|error| error.to_string())?
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

    pub(crate) fn write_atomic(&self, bytes: &[u8], etag: Option<&str>) -> Result<Catalog, String> {
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err("Agent catalog exceeds the size limit".to_string());
        }
        let service = CatalogService::from_bytes(bytes).map_err(|error| error.to_string())?;
        let parent = self
            .catalog_path
            .parent()
            .ok_or_else(|| "catalog cache has no parent directory".to_string())?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let previous_catalog = std::fs::read(&self.catalog_path).ok();
        let previous_etag = std::fs::read(&self.etag_path).ok();
        let previous_meta = std::fs::read(&self.meta_path).ok();
        let suffix = uuid::Uuid::new_v4().to_string();
        let temp_catalog = parent.join(format!(".catalog-v1.{suffix}.tmp"));
        std::fs::write(&temp_catalog, bytes).map_err(|error| error.to_string())?;
        if let Err(error) = std::fs::rename(&temp_catalog, &self.catalog_path) {
            let _ = std::fs::remove_file(&temp_catalog);
            return Err(error.to_string());
        }
        if let Some(etag) = etag.map(str::trim).filter(|value| !value.is_empty()) {
            let temp_etag = parent.join(format!(".catalog-v1.{suffix}.etag.tmp"));
            if let Err(error) = std::fs::write(&temp_etag, etag) {
                let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
                return Err(error.to_string());
            }
            if let Err(error) = std::fs::rename(&temp_etag, &self.etag_path) {
                let _ = std::fs::remove_file(&temp_etag);
                let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
                let _ = restore_cache_file(&self.etag_path, previous_etag.as_deref());
                return Err(error.to_string());
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
        if let Err(error) = std::fs::write(
            &temp_meta,
            serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?,
        ) {
            let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
            let _ = restore_cache_file(&self.etag_path, previous_etag.as_deref());
            let _ = restore_cache_file(&self.meta_path, previous_meta.as_deref());
            return Err(error.to_string());
        }
        if let Err(error) = std::fs::rename(&temp_meta, &self.meta_path) {
            let _ = std::fs::remove_file(&temp_meta);
            let _ = restore_cache_file(&self.catalog_path, previous_catalog.as_deref());
            let _ = restore_cache_file(&self.etag_path, previous_etag.as_deref());
            let _ = restore_cache_file(&self.meta_path, previous_meta.as_deref());
            return Err(error.to_string());
        }
        Ok((*service.catalog()).clone())
    }

    pub(crate) fn best_available() -> Result<CatalogService, String> {
        let bundled = CatalogService::bundled()
            .map_err(|error| error.to_string())?
            .catalog();
        let cached = Self::in_app_cache()
            .and_then(|cache| cache.read().ok().flatten())
            .map(|(catalog, _etag)| catalog);
        Ok(CatalogService::from_catalog(select_active_catalog(
            (*bundled).clone(),
            cached,
        )))
    }

    pub(crate) fn refresh_default() -> Result<CatalogRefreshOutcome, String> {
        let cache = Self::in_app_cache();
        let cached = cache.as_ref().and_then(|cache| cache.read().ok().flatten());
        let etag = cached.as_ref().and_then(|(_, etag)| etag.as_deref());
        let request = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(15))
            .build()
            .get(DEFAULT_CATALOG_URL)
            .set("Accept", "application/json")
            .set("User-Agent", "AssetIWeave/0.5 agent-market-catalog");
        let request = if let Some(etag) = etag {
            request.set("If-None-Match", etag)
        } else {
            request
        };
        let response = match request.call() {
            Ok(response) => response,
            Err(ureq::Error::Status(304, _)) => {
                let Some((catalog, etag)) = cached else {
                    return Err("Agent catalog returned 304 without a valid cache".to_string());
                };
                return Ok(CatalogRefreshOutcome::NotModified { catalog, etag });
            }
            Err(error) => return Err(format!("Agent catalog refresh failed: {error}")),
        };
        let final_url = url::Url::parse(response.get_url())
            .map_err(|_| "Agent catalog redirect URL is invalid".to_string())?;
        let host = final_url
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !matches!(
            host.as_str(),
            "raw.githubusercontent.com" | "github.com" | "raw.github.com"
        ) && !host.ends_with(".githubusercontent.com")
        {
            return Err("Agent catalog redirect host is not allowlisted".to_string());
        }
        let response_etag = response.header("ETag").map(str::to_string);
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take((MAX_CATALOG_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| format!("Agent catalog response could not be read: {error}"))?;
        if bytes.len() > MAX_CATALOG_BYTES {
            return Err("Agent catalog exceeds the 5 MiB limit".to_string());
        }
        let service = CatalogService::from_bytes(&bytes).map_err(|error| error.to_string())?;
        let catalog = (*service.catalog()).clone();
        if let Some(cache) = cache {
            cache.write_atomic(&bytes, response_etag.as_deref())?;
        }
        Ok(CatalogRefreshOutcome::Updated {
            catalog,
            etag: response_etag,
        })
    }

    pub(crate) fn is_within_cache(&self, path: &Path) -> bool {
        self.catalog_path
            .parent()
            .is_some_and(|root| path.starts_with(root))
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

    let bundled_compatible = bundled
        .items
        .iter()
        .filter(|item| is_core_compatible(item))
        .count();
    let cached_compatible = cached
        .items
        .iter()
        .filter(|item| is_core_compatible(item))
        .count();

    if cached_compatible == 0 && bundled_compatible > 0 {
        return bundled;
    }
    if cached_compatible > bundled_compatible
        || (cached_compatible == bundled_compatible
            && cached.catalog_version > bundled.catalog_version)
    {
        cached
    } else {
        bundled
    }
}

fn catalog_fingerprint(catalog: &Catalog) -> String {
    let bytes = serde_json::to_vec(catalog).expect("catalog serialization must be infallible");
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "assetiweave-agent-market-cache-{label}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn atomic_cache_round_trip_preserves_valid_catalog_and_etag() {
        let root = cache_root("round-trip");
        let cache = CatalogCache::new(root.clone());
        let bytes = include_bytes!("../../../../builtin-assets/agent-market/catalog-v1.json");
        cache
            .write_atomic(bytes, Some("fixture-etag"))
            .expect("write catalog cache");
        let (catalog, etag) = cache.read().expect("read catalog cache").expect("cache");
        assert_eq!(catalog.schema, "assetiweave.agent-market/v1");
        assert_eq!(etag.as_deref(), Some("fixture-etag"));
        let metadata = std::fs::read_to_string(&cache.meta_path).expect("cache metadata");
        assert!(metadata.contains("fixture-etag"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn invalid_cache_is_not_used_as_a_catalog() {
        let root = cache_root("invalid");
        let cache = CatalogCache::new(root.clone());
        std::fs::create_dir_all(&root).expect("cache directory");
        std::fs::write(&cache.catalog_path, br#"{"schema":"invalid"}"#).expect("invalid cache");
        assert!(cache.read().is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn incompatible_cache_does_not_mask_compatible_bundled_catalog() {
        let bundled = super::super::catalog::bundled_catalog().expect("bundled catalog");
        let mut cached = bundled.clone();
        cached.catalog_version = "2026.08.16.1".to_string();
        for item in &mut cached.items {
            item.core_compatibility.min = "0.5.0".to_string();
            item.core_compatibility.max_exclusive = "0.6.0".to_string();
        }

        let selected = select_active_catalog(bundled.clone(), Some(cached));

        assert_eq!(selected.catalog_version, bundled.catalog_version);
        assert!(selected.items.iter().all(is_core_compatible));
    }

    #[test]
    fn same_revision_different_hash_fails_closed_to_bundled_catalog() {
        let bundled = super::super::catalog::bundled_catalog().expect("bundled catalog");
        let mut cached = bundled.clone();
        cached.items[0].description.push_str(" tampered");

        let selected = select_active_catalog(bundled.clone(), Some(cached));

        assert_eq!(
            catalog_fingerprint(&selected),
            catalog_fingerprint(&bundled)
        );
    }
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
