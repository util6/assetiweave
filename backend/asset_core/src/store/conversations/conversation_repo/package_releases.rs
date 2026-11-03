use super::*;
use crate::domain::conversations::ConversationAdapterPackageRecordKind;
use sqlx::Row;

pub async fn list_conversation_adapter_catalog_releases_sqlx(
    pool: &SqlitePool,
    catalog_url: &str,
    package_id: Option<&str>,
) -> StoreResult<Vec<ConversationAdapterCatalogRelease>> {
    let rows = sqlx::query(
        r#"
        SELECT catalog_source, extension_id, version, capability_kind, target_id,
               display_name, publisher, channel, released_at, artifact_url,
               artifact_size, artifact_sha256, release_meta_json, fetched_at
        FROM app_extension_catalog_releases
        WHERE capability_kind = 'conversation.adapter'
          AND catalog_source = ?1
          AND (?2 IS NULL OR extension_id = ?2)
        ORDER BY extension_id ASC, released_at DESC, version DESC
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct ConversationAdapterReleaseMeta {
    pub core_compatibility: String,
    pub changelog_markdown: String,
    #[serde(deserialize_with = "row_mappers::deserialize_bool_flexible")]
    pub breaking_change: bool,
    pub runtime_protocol: String,
    pub record_kind: ConversationAdapterPackageRecordKind,
    pub package_manifest_file: String,
    pub adapter_manifest_file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_manifest_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

pub(super) fn map_sqlx_conversation_adapter_catalog_release(
    row: &SqliteRow,
) -> StoreResult<ConversationAdapterCatalogRelease> {
    let catalog_url: String = row.get("catalog_source");
    let package_id: String = row.get("extension_id");
    let version: String = row.get("version");
    let adapter_id: String = row.get("target_id");
    let name: String = row.get("display_name");
    let publisher: String = row.get("publisher");
    let channel_str: String = row.get("channel");
    let released_at: Option<String> = row.get("released_at");
    let artifact_url: String = row.get("artifact_url");
    let artifact_size: Option<i64> = row.get("artifact_size");
    let artifact_sha256: String = row
        .get::<Option<String>, _>("artifact_sha256")
        .unwrap_or_default();
    let fetched_at: String = row.get("fetched_at");
    let meta_json: String = row.get("release_meta_json");
    let meta: ConversationAdapterReleaseMeta = serde_json::from_str(&meta_json).map_err(|e| {
        StoreError::external(format!(
            "failed to parse conversation adapter release meta: {e}"
        ))
    })?;

    Ok(ConversationAdapterCatalogRelease {
        catalog_url,
        package_id,
        adapter_id,
        name,
        publisher,
        version,
        channel: decode_enum(channel_str)?,
        released_at,
        core_compatibility: meta.core_compatibility,
        artifact_url,
        artifact_size,
        artifact_sha256,
        changelog_markdown: meta.changelog_markdown,
        breaking_change: meta.breaking_change,
        runtime_protocol: meta.runtime_protocol,
        record_kind: meta.record_kind,
        package_manifest_file: meta.package_manifest_file,
        adapter_manifest_file: meta.adapter_manifest_file,
        adapter_manifest_json: meta.adapter_manifest_json,
        source_json: meta.source_json,
        etag: meta.etag,
        fetched_at,
    })
}
