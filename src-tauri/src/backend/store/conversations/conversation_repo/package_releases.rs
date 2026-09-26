use super::*;

pub(crate) async fn list_conversation_adapter_catalog_releases_sqlx(
    pool: &SqlitePool,
    catalog_url: &str,
    package_id: Option<&str>,
) -> StoreResult<Vec<ConversationAdapterCatalogRelease>> {
    let rows = sqlx::query(
        r#"
        SELECT catalog_url, package_id, adapter_id, name, publisher, version,
               channel, released_at, core_compatibility, artifact_url,
               artifact_size, artifact_sha256, changelog_markdown,
               breaking_change, runtime_protocol, record_kind,
               package_manifest_file, adapter_manifest_file,
               adapter_manifest_json, source_json, etag, fetched_at
        FROM app_conversation_adapter_catalog_releases
        WHERE catalog_url = ?1
          AND (?2 IS NULL OR package_id = ?2)
        ORDER BY package_id ASC, released_at DESC, version DESC
        "#,
    )
    .bind(catalog_url)
    .bind(package_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    rows.iter()
        .map(map_sqlx_conversation_adapter_catalog_release)
        .collect()
}

#[derive(Debug, FromRow)]
pub(super) struct ConversationAdapterCatalogReleaseRow {
    catalog_url: String,
    package_id: String,
    adapter_id: String,
    name: String,
    publisher: String,
    version: String,
    channel: String,
    released_at: Option<String>,
    core_compatibility: String,
    artifact_url: String,
    artifact_size: Option<i64>,
    artifact_sha256: String,
    changelog_markdown: String,
    breaking_change: i64,
    runtime_protocol: String,
    record_kind: String,
    package_manifest_file: String,
    adapter_manifest_file: String,
    adapter_manifest_json: Option<String>,
    source_json: Option<String>,
    etag: Option<String>,
    fetched_at: String,
}

impl ConversationAdapterCatalogReleaseRow {
    fn into_domain(self) -> StoreResult<ConversationAdapterCatalogRelease> {
        Ok(ConversationAdapterCatalogRelease {
            catalog_url: self.catalog_url,
            package_id: self.package_id,
            adapter_id: self.adapter_id,
            name: self.name,
            publisher: self.publisher,
            version: self.version,
            channel: decode_enum(self.channel)?,
            released_at: self.released_at,
            core_compatibility: self.core_compatibility,
            artifact_url: self.artifact_url,
            artifact_size: self.artifact_size,
            artifact_sha256: self.artifact_sha256,
            changelog_markdown: self.changelog_markdown,
            breaking_change: self.breaking_change == 1,
            runtime_protocol: self.runtime_protocol,
            record_kind: decode_enum(self.record_kind)?,
            package_manifest_file: self.package_manifest_file,
            adapter_manifest_file: self.adapter_manifest_file,
            adapter_manifest_json: self.adapter_manifest_json,
            source_json: self.source_json,
            etag: self.etag,
            fetched_at: self.fetched_at,
        })
    }
}

pub(super) fn map_sqlx_conversation_adapter_catalog_release(
    row: &SqliteRow,
) -> StoreResult<ConversationAdapterCatalogRelease> {
    ConversationAdapterCatalogReleaseRow::from_row(row)
        .map_err(StoreError::external)?
        .into_domain()
}
