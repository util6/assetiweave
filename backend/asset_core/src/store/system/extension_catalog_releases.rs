use sqlx::{FromRow, SqlitePool};

use crate::backend::domain::agents::{CatalogItem, Distribution, UpstreamVersionInfo};
use crate::backend::store::error::StoreResult;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionCatalogRelease {
    pub catalog_source: String,
    pub extension_id: String,
    pub version: String,
    pub capability_kind: String,
    pub target_id: String,
    pub display_name: String,
    pub publisher: String,
    pub channel: String,
    pub released_at: Option<String>,
    pub artifact_url: String,
    pub artifact_size: Option<i64>,
    pub artifact_sha256: Option<String>,
    pub release_meta_json: String,
    pub fetched_at: String,
}

#[derive(Debug, FromRow)]
pub struct ExtensionCatalogReleaseRow {
    pub catalog_source: String,
    pub extension_id: String,
    pub version: String,
    pub capability_kind: String,
    pub target_id: String,
    pub display_name: String,
    pub publisher: String,
    pub channel: String,
    pub released_at: Option<String>,
    pub artifact_url: String,
    pub artifact_size: Option<i64>,
    pub artifact_sha256: Option<String>,
    pub release_meta_json: String,
    pub fetched_at: String,
}

impl From<ExtensionCatalogReleaseRow> for ExtensionCatalogRelease {
    fn from(row: ExtensionCatalogReleaseRow) -> Self {
        Self {
            catalog_source: row.catalog_source,
            extension_id: row.extension_id,
            version: row.version,
            capability_kind: row.capability_kind,
            target_id: row.target_id,
            display_name: row.display_name,
            publisher: row.publisher,
            channel: row.channel,
            released_at: row.released_at,
            artifact_url: row.artifact_url,
            artifact_size: row.artifact_size,
            artifact_sha256: row.artifact_sha256,
            release_meta_json: row.release_meta_json,
            fetched_at: row.fetched_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCatalogReleaseMeta {
    pub catalog_version: String,
    pub item: CatalogItem,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_info: Option<UpstreamVersionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

impl ExtensionCatalogRelease {
    pub fn parse_agent_meta(&self) -> Result<AgentCatalogReleaseMeta, serde_json::Error> {
        serde_json::from_str(&self.release_meta_json)
    }

    pub fn from_agent_item(
        catalog_source: &str,
        catalog_version: &str,
        item: &CatalogItem,
        upstream_info: Option<&UpstreamVersionInfo>,
        etag: Option<&str>,
        fetched_at: &str,
    ) -> Result<Self, serde_json::Error> {
        let meta = AgentCatalogReleaseMeta {
            catalog_version: catalog_version.to_string(),
            item: item.clone(),
            upstream_info: upstream_info.cloned(),
            etag: etag.map(str::to_string),
        };
        let release_meta_json = serde_json::to_string(&meta)?;

        let mut artifact_url = String::new();
        let mut artifact_size = None;
        let mut artifact_sha256 = None;

        for dist in &item.distributions {
            match dist {
                Distribution::Binary {
                    url, size, sha256, ..
                } => {
                    artifact_url = url.clone();
                    artifact_size = size.map(|s| s as i64);
                    artifact_sha256 = Some(sha256.clone());
                    break;
                }
                Distribution::Npx {
                    package, version, ..
                } => {
                    artifact_url = format!("npm://{package}/{version}");
                    break;
                }
            }
        }

        Ok(Self {
            catalog_source: catalog_source.to_string(),
            extension_id: item.id.clone(),
            version: item.version.clone(),
            capability_kind: "agent".to_string(),
            target_id: item.id.clone(),
            display_name: item.display_name.clone(),
            publisher: if !item.upstream.registry_id.is_empty() {
                item.upstream.registry_id.clone()
            } else {
                "AssetIWeave Curated".to_string()
            },
            channel: "stable".to_string(),
            released_at: Some(item.verification.tested_at.clone()),
            artifact_url,
            artifact_size,
            artifact_sha256,
            release_meta_json,
            fetched_at: fetched_at.to_string(),
        })
    }
}

pub struct ExtensionCatalogReleaseRepository {
    pool: SqlitePool,
}

impl ExtensionCatalogReleaseRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn batch_upsert(&self, releases: &[ExtensionCatalogRelease]) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        for release in releases {
            sqlx::query(
                r#"
                INSERT INTO app_extension_catalog_releases (
                    catalog_source, extension_id, version, capability_kind, target_id,
                    display_name, publisher, channel, released_at, artifact_url,
                    artifact_size, artifact_sha256, release_meta_json, fetched_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
                )
                ON CONFLICT(catalog_source, extension_id, version) DO UPDATE SET
                    capability_kind = excluded.capability_kind,
                    target_id = excluded.target_id,
                    display_name = excluded.display_name,
                    publisher = excluded.publisher,
                    channel = excluded.channel,
                    released_at = excluded.released_at,
                    artifact_url = excluded.artifact_url,
                    artifact_size = excluded.artifact_size,
                    artifact_sha256 = excluded.artifact_sha256,
                    release_meta_json = excluded.release_meta_json,
                    fetched_at = excluded.fetched_at
                "#,
            )
            .bind(&release.catalog_source)
            .bind(&release.extension_id)
            .bind(&release.version)
            .bind(&release.capability_kind)
            .bind(&release.target_id)
            .bind(&release.display_name)
            .bind(&release.publisher)
            .bind(&release.channel)
            .bind(&release.released_at)
            .bind(&release.artifact_url)
            .bind(release.artifact_size)
            .bind(&release.artifact_sha256)
            .bind(&release.release_meta_json)
            .bind(&release.fetched_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_by_capability(
        &self,
        capability_kind: &str,
    ) -> StoreResult<Vec<ExtensionCatalogRelease>> {
        let rows = sqlx::query_as::<_, ExtensionCatalogReleaseRow>(
            r#"
            SELECT
                catalog_source, extension_id, version, capability_kind, target_id,
                display_name, publisher, channel, released_at, artifact_url,
                artifact_size, artifact_sha256, release_meta_json, fetched_at
            FROM app_extension_catalog_releases
            WHERE capability_kind = ?1
            ORDER BY extension_id ASC, released_at DESC, version DESC
            "#,
        )
        .bind(capability_kind)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(ExtensionCatalogRelease::from)
            .collect())
    }

    pub async fn get_latest_by_target(
        &self,
        capability_kind: &str,
        target_id: &str,
    ) -> StoreResult<Option<ExtensionCatalogRelease>> {
        let row = sqlx::query_as::<_, ExtensionCatalogReleaseRow>(
            r#"
            SELECT
                catalog_source, extension_id, version, capability_kind, target_id,
                display_name, publisher, channel, released_at, artifact_url,
                artifact_size, artifact_sha256, release_meta_json, fetched_at
            FROM app_extension_catalog_releases
            WHERE capability_kind = ?1 AND target_id = ?2
            ORDER BY released_at DESC, version DESC
            LIMIT 1
            "#,
        )
        .bind(capability_kind)
        .bind(target_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(ExtensionCatalogRelease::from))
    }

    pub async fn get_latest_etag(&self, catalog_source: &str) -> StoreResult<Option<String>> {
        let row = sqlx::query_as::<_, ExtensionCatalogReleaseRow>(
            r#"
            SELECT
                catalog_source, extension_id, version, capability_kind, target_id,
                display_name, publisher, channel, released_at, artifact_url,
                artifact_size, artifact_sha256, release_meta_json, fetched_at
            FROM app_extension_catalog_releases
            WHERE catalog_source = ?1
            ORDER BY fetched_at DESC
            LIMIT 1
            "#,
        )
        .bind(catalog_source)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(release) = row {
            if let Ok(meta) = release.release_meta_json.parse::<serde_json::Value>() {
                if let Some(etag) = meta.get("etag").and_then(|v| v.as_str()) {
                    return Ok(Some(etag.to_string()));
                }
            }
        }
        Ok(None)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn ensure_agent_seeded(&self) -> StoreResult<()> {
        let count = self.count_by_capability("agent").await?;
        if count == 0 {
            if let Ok(bundled) =
                crate::backend::domain::agents::market::catalog::CatalogService::bundled()
            {
                let now = chrono::Utc::now().to_rfc3339();
                let mut releases = Vec::new();
                for item in &bundled.catalog().items {
                    if let Ok(release) = ExtensionCatalogRelease::from_agent_item(
                        "assetiweave-curated",
                        &bundled.catalog().catalog_version,
                        item,
                        None,
                        None,
                        &now,
                    ) {
                        releases.push(release);
                    }
                }
                if !releases.is_empty() {
                    self.batch_upsert(&releases).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn get_latest_agent_meta(
        &self,
        agent_id: &str,
    ) -> StoreResult<Option<(ExtensionCatalogRelease, AgentCatalogReleaseMeta)>> {
        if let Some(release) = self.get_latest_by_target("agent", agent_id).await? {
            if let Ok(meta) = release.parse_agent_meta() {
                return Ok(Some((release, meta)));
            }
        }
        Ok(None)
    }

    pub async fn count_by_capability(&self, capability_kind: &str) -> StoreResult<i64> {
        let count: (i64,) = sqlx::query_as(
            "SELECT count(*) FROM app_extension_catalog_releases WHERE capability_kind = ?1",
        )
        .bind(capability_kind)
        .fetch_one(&self.pool)
        .await?;
        Ok(count.0)
    }
}

#[cfg(test)]
#[path = "extension_catalog_releases_tests.rs"]
mod tests;
