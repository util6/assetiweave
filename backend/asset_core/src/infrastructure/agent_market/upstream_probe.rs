use std::{collections::HashMap, path::Path, time::Duration};

use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::backend::domain::agents::{CatalogItem, Distribution};

pub const DEFAULT_ACP_REGISTRY_BASE_URL: &str =
    "https://raw.githubusercontent.com/agentclientprotocol/registry/main";
pub const DEFAULT_NPM_REGISTRY_BASE_URL: &str = "https://registry.npmjs.org";
pub const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamProbeSource {
    AcpRegistry,
    Npm,
}

#[derive(Clone, Debug, Default, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamBinaryTarget {
    pub archive: String,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamVersionInfo {
    pub version: String,
    pub source: UpstreamProbeSource,
    pub checked_at: String,
    #[serde(default)]
    pub binary_targets: HashMap<String, UpstreamBinaryTarget>,
}

impl UpstreamVersionInfo {
    pub fn find_binary_target(&self, os: &str, arch: &str) -> Option<&UpstreamBinaryTarget> {
        let norm_os = match os {
            "macos" | "darwin" => "darwin",
            "windows" => "windows",
            "linux" => "linux",
            other => other,
        };
        let arch_variants = match arch {
            "aarch64" | "arm64" => vec!["aarch64", "arm64"],
            "x86_64" | "amd64" => vec!["x86_64", "amd64"],
            other => vec![other],
        };
        let os_variants = if norm_os == "darwin" {
            vec!["darwin", "macos"]
        } else {
            vec![norm_os]
        };

        for o in &os_variants {
            for a in &arch_variants {
                let key = format!("{o}-{a}");
                if let Some(target) = self.binary_targets.get(&key) {
                    return Some(target);
                }
            }
        }
        None
    }
}

pub struct UpstreamProbe;

impl UpstreamProbe {
    pub fn probe_agent(
        client: &reqwest::blocking::Client,
        registry_id: Option<&str>,
        npm_package: Option<&str>,
    ) -> Option<UpstreamVersionInfo> {
        Self::probe_agent_with_urls(
            client,
            registry_id,
            npm_package,
            DEFAULT_ACP_REGISTRY_BASE_URL,
            DEFAULT_NPM_REGISTRY_BASE_URL,
            DEFAULT_PROBE_TIMEOUT,
        )
    }

    pub fn probe_agent_with_urls(
        client: &reqwest::blocking::Client,
        registry_id: Option<&str>,
        npm_package: Option<&str>,
        registry_base_url: &str,
        npm_base_url: &str,
        timeout: Duration,
    ) -> Option<UpstreamVersionInfo> {
        // Priority 1: Query official ACP Registry
        if let Some(reg_id) = registry_id.map(str::trim).filter(|s| !s.is_empty()) {
            let url = format!("{registry_base_url}/{reg_id}/agent.json");
            if let Some(info) = Self::probe_acp_registry(client, &url, timeout) {
                return Some(info);
            }
        }

        // Priority 2: Fallback to NPM Registry
        if let Some(pkg) = npm_package.map(str::trim).filter(|s| !s.is_empty()) {
            let url = format!("{npm_base_url}/{pkg}/latest");
            if let Some(info) = Self::probe_npm_registry(client, &url, timeout) {
                return Some(info);
            }
        }

        None
    }

    pub fn probe_acp_registry(
        client: &reqwest::blocking::Client,
        url: &str,
        timeout: Duration,
    ) -> Option<UpstreamVersionInfo> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("AssetIWeave/", env!("CARGO_PKG_VERSION"))),
        );

        let response = client
            .get(url)
            .headers(headers)
            .timeout(timeout)
            .send()
            .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let body: serde_json::Value = response.json().ok()?;
        let version = body
            .get("version")
            .and_then(serde_json::Value::as_str)?
            .trim();

        if version.is_empty() {
            return None;
        }

        let mut binary_targets = HashMap::new();
        if let Some(binaries) = body
            .get("distribution")
            .and_then(|d| d.get("binary"))
            .and_then(|b| b.as_object())
        {
            for (target_key, target_val) in binaries {
                if let Some(archive) = target_val
                    .get("archive")
                    .and_then(serde_json::Value::as_str)
                {
                    let cmd = target_val
                        .get("cmd")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    let args = target_val
                        .get("args")
                        .and_then(serde_json::Value::as_array)
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        });
                    let sha256 = target_val
                        .get("sha256")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                    binary_targets.insert(
                        target_key.to_string(),
                        UpstreamBinaryTarget {
                            archive: archive.to_string(),
                            cmd,
                            args,
                            sha256,
                        },
                    );
                }
            }
        }

        Some(UpstreamVersionInfo {
            version: version.to_string(),
            source: UpstreamProbeSource::AcpRegistry,
            checked_at: Utc::now().to_rfc3339(),
            binary_targets,
        })
    }

    pub fn probe_npm_registry(
        client: &reqwest::blocking::Client,
        url: &str,
        timeout: Duration,
    ) -> Option<UpstreamVersionInfo> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("AssetIWeave/", env!("CARGO_PKG_VERSION"))),
        );

        let response = client
            .get(url)
            .headers(headers)
            .timeout(timeout)
            .send()
            .ok()?;

        if !response.status().is_success() {
            return None;
        }

        let body: serde_json::Value = response.json().ok()?;
        let version = body
            .get("version")
            .and_then(serde_json::Value::as_str)?
            .trim();

        if version.is_empty() {
            return None;
        }

        Some(UpstreamVersionInfo {
            version: version.to_string(),
            source: UpstreamProbeSource::Npm,
            checked_at: Utc::now().to_rfc3339(),
            binary_targets: HashMap::new(),
        })
    }

    pub fn probe_catalog_items(
        client: &reqwest::blocking::Client,
        items: &[CatalogItem],
    ) -> HashMap<String, UpstreamVersionInfo> {
        let results = std::sync::Mutex::new(HashMap::new());
        let results_ref = &results;
        std::thread::scope(|s| {
            for item in items {
                let registry_id = if item.upstream.registry_id.is_empty() {
                    None
                } else {
                    Some(item.upstream.registry_id.as_str())
                };
                let npm_pkg = Self::extract_package_name(item);
                s.spawn(move || {
                    if let Some(info) = Self::probe_agent(client, registry_id, npm_pkg.as_deref()) {
                        if let Ok(mut map) = results_ref.lock() {
                            map.insert(item.id.clone(), info);
                        }
                    }
                });
            }
        });
        results.into_inner().unwrap_or_default()
    }

    pub fn extract_package_name(item: &CatalogItem) -> Option<String> {
        for dist in &item.distributions {
            match dist {
                Distribution::Npx { package, .. } => return Some(package.clone()),
                Distribution::Uvx { package, .. } => return Some(package.clone()),
                _ => {}
            }
        }
        None
    }

    pub fn semver_is_newer(candidate: &str, current: &str) -> bool {
        let candidate_trim = candidate.trim();
        let current_trim = current.trim();

        if let (Ok(c_ver), Ok(cur_ver)) = (
            semver::Version::parse(candidate_trim),
            semver::Version::parse(current_trim),
        ) {
            return c_ver > cur_ver;
        }

        candidate_trim != current_trim
    }

    pub fn load_cached_upstream_versions(path: &Path) -> HashMap<String, UpstreamVersionInfo> {
        let Ok(bytes) = std::fs::read(path) else {
            return HashMap::new();
        };
        serde_json::from_slice(&bytes).unwrap_or_default()
    }

    pub fn save_cached_upstream_versions(
        path: &Path,
        data: &HashMap<String, UpstreamVersionInfo>,
    ) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp_path = path.with_extension(format!("tmp.{}", uuid::Uuid::new_v4()));
        let bytes = serde_json::to_vec_pretty(data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&temp_path, bytes)?;
        std::fs::rename(&temp_path, path)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "upstream_probe_tests.rs"]
mod tests;
