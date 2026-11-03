use super::common::begin_immediate_with_retry;
use super::*;

pub async fn list_conversation_adapter_packages_sqlx(
    pool: &SqlitePool,
) -> StoreResult<Vec<ConversationAdapterPackage>> {
    let rows = sqlx::query(LIST_CONVERSATION_ADAPTER_PACKAGES_SQL)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    rows.iter()
        .map(map_sqlx_conversation_adapter_package)
        .collect()
}

pub async fn load_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package_id: &str,
) -> StoreResult<Option<ConversationAdapterPackage>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(package_id)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        .as_ref()
        .map(map_sqlx_conversation_adapter_package)
        .transpose()
}

pub async fn load_conversation_adapter_package_by_adapter_sqlx(
    pool: &SqlitePool,
    adapter_id: &str,
) -> StoreResult<Option<ConversationAdapterPackage>> {
    sqlx::query(LOAD_CONVERSATION_ADAPTER_PACKAGE_BY_ADAPTER_SQL)
        .bind(adapter_id)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)?
        .as_ref()
        .map(map_sqlx_conversation_adapter_package)
        .transpose()
}

pub async fn upsert_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package: &ConversationAdapterPackage,
) -> StoreResult<()> {
    upsert_conversation_adapter_package_with_executor(pool, package).await
}

pub(super) async fn upsert_conversation_adapter_package_with_executor<'e, E>(
    executor: E,
    package: &ConversationAdapterPackage,
) -> StoreResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    let meta = row_mappers::ConversationAdapterCapabilityMeta {
        record_kind: package.record_kind,
        manifest_path: package.manifest_path.clone(),
        adapter_manifest_path: package.adapter_manifest_path.clone(),
        runtime_protocol: package.runtime_protocol.clone(),
        runtime_ready: package.runtime_ready,
        installed_content_hash: package.installed_content_hash.clone(),
        trusted_package_hash: package.trusted_package_hash.clone(),
        origin: package.origin,
        source_url: package.source_url.clone(),
        git_ref: package.git_ref.clone(),
        git_commit: package.git_commit.clone(),
        catalog_url: package.catalog_url.clone(),
        update_policy: package.update_policy,
        latest_version: package.latest_version.clone(),
        last_checked_at: package.last_checked_at.clone(),
        runtime_gate_status: package.runtime_gate_status,
        runtime_validated_at: package.runtime_validated_at.clone(),
    };
    let meta_json = serde_json::to_string(&meta).map_err(|e| {
        StoreError::external(format!(
            "failed to serialize conversation adapter capability meta: {e}"
        ))
    })?;

    let distribution_type = match package.origin {
        ConversationAdapterPackageOrigin::LocalDirectory
        | ConversationAdapterPackageOrigin::DevOverride => "workspace",
        _ => "archive",
    };
    let enabled = if package.runtime_ready { 1 } else { 0 };
    let installation_status = match package.runtime_gate_status {
        ConversationAdapterRuntimeGateStatus::CoreIncompatible => "incompatible",
        _ if !package.runtime_ready => "broken",
        _ => "ready",
    };
    let runtime_status = if package.runtime_ready {
        "ready"
    } else {
        "failed"
    };
    let error_code = if !package.runtime_ready {
        Some(encode_enum(package.runtime_gate_status)?)
    } else {
        None
    };
    let checked_at = package
        .last_checked_at
        .as_deref()
        .or(package.runtime_validated_at.as_deref())
        .unwrap_or(&package.updated_at);

    sqlx::query(UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(&package.package_id)
        .bind(&package.adapter_id)
        .bind(&package.name)
        .bind(&package.version)
        .bind(distribution_type)
        .bind(&package.install_dir)
        .bind(enabled)
        .bind(installation_status)
        .bind(runtime_status)
        .bind(error_code)
        .bind(&package.error_message)
        .bind(checked_at)
        .bind(&package.created_at)
        .bind(&package.updated_at)
        .bind(&meta_json)
        .execute(executor)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

pub async fn activate_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
    version: &ConversationAdapterPackageVersion,
) -> StoreResult<()> {
    let mut tx = begin_immediate_with_retry(pool, 8).await?;
    #[derive(Debug, FromRow)]
    struct AdapterPackageVersionHashRow {
        artifact_hash: Option<String>,
        content_hash: String,
    }

    let existing = sqlx::query_as::<_, AdapterPackageVersionHashRow>(
        r#"
        SELECT artifact_sha256 AS artifact_hash, content_hash
        FROM app_extension_versions
        WHERE extension_id = ?1 AND version = ?2
        "#,
    )
    .bind(&version.package_id)
    .bind(&version.version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    if let Some(existing) = existing {
        if existing.artifact_hash != version.artifact_hash
            || existing.content_hash != version.content_hash
        {
            return Err(StoreError::Conflict(format!(
                "conversation adapter package version is immutable: {}@{}",
                version.package_id, version.version
            )));
        }
    }

    upsert_conversation_adapter_for_all_tenants(&mut tx, adapter).await?;
    upsert_conversation_adapter_package_with_executor(&mut *tx, package).await?;
    sqlx::query(
        r#"
        INSERT INTO app_extension_versions (
            extension_id, version, install_dir, artifact_sha256,
            content_hash, verification_status, installed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(extension_id, version) DO UPDATE SET
            install_dir = excluded.install_dir,
            verification_status = excluded.verification_status
        "#,
    )
    .bind(&version.package_id)
    .bind(&version.version)
    .bind(&version.install_dir)
    .bind(&version.artifact_hash)
    .bind(&version.content_hash)
    .bind(encode_enum(version.runtime_gate_status)?)
    .bind(&version.installed_at)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)
}

pub async fn activate_conversation_adapter_workspace_sqlx(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
) -> StoreResult<()> {
    let mut tx = begin_immediate_with_retry(pool, 8).await?;
    upsert_conversation_adapter_for_all_tenants(&mut tx, adapter).await?;
    upsert_conversation_adapter_package_with_executor(&mut *tx, package).await?;
    sqlx::query("DELETE FROM app_extension_versions WHERE extension_id = ?1")
        .bind(&package.package_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)
}

pub async fn deactivate_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    package_id: &str,
    adapter_id: &str,
) -> StoreResult<ConversationAdapterPackage> {
    let mut package = load_conversation_adapter_package_sqlx(pool, package_id)
        .await?
        .ok_or_else(|| {
            StoreError::external(format!(
                "conversation adapter package not found: {package_id}"
            ))
        })?;
    if package.origin != ConversationAdapterPackageOrigin::ManagedRelease {
        return Err(StoreError::Validation(
            "only managed conversation adapter packages can be uninstalled".to_string(),
        ));
    }
    if package.adapter_id != adapter_id {
        return Err(StoreError::Validation(format!(
            "conversation adapter package {package_id} does not own adapter {adapter_id}"
        )));
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = begin_immediate_with_retry(pool, 8).await?;
    sqlx::query("DELETE FROM conversation_adapters WHERE id = ?1")
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    sqlx::query(
        "UPDATE conversation_sources SET enabled = 0, updated_at = ?1 WHERE adapter_id = ?2",
    )
    .bind(&now)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;

    package.runtime_ready = false;
    package.runtime_gate_status = ConversationAdapterRuntimeGateStatus::RuntimeMissing;
    package.runtime_validated_at = Some(now.clone());
    package.error_message = Some("conversation adapter package is uninstalled".to_string());
    package.updated_at = now;

    upsert_conversation_adapter_package_with_executor(&mut *tx, &package).await?;
    tx.commit().await.map_err(StoreError::external)?;

    Ok(package)
}

pub async fn upsert_conversation_adapter_catalog_release_sqlx(
    pool: &SqlitePool,
    release: &ConversationAdapterCatalogRelease,
) -> StoreResult<()> {
    let meta = package_releases::ConversationAdapterReleaseMeta {
        core_compatibility: release.core_compatibility.clone(),
        changelog_markdown: release.changelog_markdown.clone(),
        breaking_change: release.breaking_change,
        runtime_protocol: release.runtime_protocol.clone(),
        record_kind: release.record_kind,
        package_manifest_file: release.package_manifest_file.clone(),
        adapter_manifest_file: release.adapter_manifest_file.clone(),
        adapter_manifest_json: release.adapter_manifest_json.clone(),
        source_json: release.source_json.clone(),
        etag: release.etag.clone(),
    };
    let meta_json = serde_json::to_string(&meta).map_err(|e| {
        StoreError::external(format!(
            "failed to serialize conversation adapter release meta: {e}"
        ))
    })?;

    sqlx::query(
        r#"
        INSERT INTO app_extension_catalog_releases (
            catalog_source, extension_id, version, capability_kind, target_id,
            display_name, publisher, channel, released_at, artifact_url,
            artifact_size, artifact_sha256, release_meta_json, fetched_at
        ) VALUES (
            ?1, ?2, ?3, 'conversation.adapter', ?4,
            ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13
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
    .bind(&release.catalog_url)
    .bind(&release.package_id)
    .bind(&release.version)
    .bind(&release.adapter_id)
    .bind(&release.name)
    .bind(&release.publisher)
    .bind(encode_enum(release.channel)?)
    .bind(&release.released_at)
    .bind(&release.artifact_url)
    .bind(release.artifact_size)
    .bind(&release.artifact_sha256)
    .bind(&meta_json)
    .bind(&release.fetched_at)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub async fn list_conversation_adapter_package_versions_sqlx(
    pool: &SqlitePool,
    package_id: &str,
) -> StoreResult<Vec<ConversationAdapterPackageVersion>> {
    #[derive(Debug, FromRow)]
    struct AdapterPackageVersionRow {
        extension_id: String,
        version: String,
        install_dir: String,
        artifact_sha256: Option<String>,
        content_hash: String,
        verification_status: String,
        installed_at: String,
    }

    impl AdapterPackageVersionRow {
        fn into_domain(self) -> StoreResult<ConversationAdapterPackageVersion> {
            Ok(ConversationAdapterPackageVersion {
                package_id: self.extension_id,
                version: self.version,
                install_dir: self.install_dir,
                artifact_hash: self.artifact_sha256,
                content_hash: self.content_hash,
                runtime_gate_status: decode_enum(self.verification_status)?,
                installed_at: self.installed_at,
            })
        }
    }

    let rows = sqlx::query_as::<_, AdapterPackageVersionRow>(
        r#"
        SELECT extension_id, version, install_dir, artifact_sha256, content_hash,
               verification_status, installed_at
        FROM app_extension_versions
        WHERE extension_id = ?1
        ORDER BY installed_at DESC, version DESC
        "#,
    )
    .bind(package_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    rows.into_iter()
        .map(AdapterPackageVersionRow::into_domain)
        .collect()
}

pub async fn update_conversation_adapter_package_version_install_dir_sqlx(
    pool: &SqlitePool,
    package_id: &str,
    version: &str,
    install_dir: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        UPDATE app_extension_versions
        SET install_dir = ?1
        WHERE extension_id = ?2 AND version = ?3
        "#,
    )
    .bind(install_dir)
    .bind(package_id)
    .bind(version)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub async fn delete_conversation_adapter_package_version_sqlx(
    pool: &SqlitePool,
    package_id: &str,
    version: &str,
    replacement_package: Option<&ConversationAdapterPackage>,
    delete_package: bool,
) -> StoreResult<bool> {
    if replacement_package.is_some() && delete_package {
        return Err(StoreError::Validation(
            "package version deletion cannot replace and delete the package record".to_string(),
        ));
    }
    let mut tx = begin_immediate_with_retry(pool, 8).await?;
    let result =
        sqlx::query("DELETE FROM app_extension_versions WHERE extension_id = ?1 AND version = ?2")
            .bind(package_id)
            .bind(version)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
    if result.rows_affected() != 1 {
        tx.rollback().await.map_err(StoreError::external)?;
        return Ok(false);
    }
    if let Some(package) = replacement_package {
        upsert_conversation_adapter_package_with_executor(&mut *tx, package).await?;
    } else if delete_package {
        sqlx::query(DELETE_CONVERSATION_ADAPTER_PACKAGE_SQL)
            .bind(package_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
    }
    tx.commit().await.map_err(StoreError::external)?;
    Ok(true)
}
