use super::*;

pub(crate) async fn list_conversation_adapter_packages_sqlx(
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

pub(crate) async fn load_conversation_adapter_package_sqlx(
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

pub(crate) async fn load_conversation_adapter_package_by_adapter_sqlx(
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

pub(crate) async fn upsert_conversation_adapter_package_sqlx(
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
    sqlx::query(UPSERT_CONVERSATION_ADAPTER_PACKAGE_SQL)
        .bind(&package.package_id)
        .bind(&package.adapter_id)
        .bind(&package.name)
        .bind(&package.version)
        .bind(encode_enum(package.record_kind)?)
        .bind(&package.install_dir)
        .bind(&package.manifest_path)
        .bind(&package.adapter_manifest_path)
        .bind(&package.runtime_protocol)
        .bind(if package.runtime_ready { 1 } else { 0 })
        .bind(encode_enum(package.origin)?)
        .bind(&package.source_url)
        .bind(&package.git_ref)
        .bind(&package.git_commit)
        .bind(&package.catalog_url)
        .bind(encode_enum(package.update_policy)?)
        .bind(&package.latest_version)
        .bind(&package.last_checked_at)
        .bind(encode_enum(package.runtime_gate_status)?)
        .bind(&package.runtime_validated_at)
        .bind(&package.installed_content_hash)
        .bind(&package.trusted_package_hash)
        .bind(&package.error_message)
        .bind(&package.created_at)
        .bind(&package.updated_at)
        .execute(executor)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn activate_conversation_adapter_package_sqlx(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
    version: &ConversationAdapterPackageVersion,
) -> StoreResult<()> {
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    #[derive(Debug, FromRow)]
    struct AdapterPackageVersionHashRow {
        artifact_hash: Option<String>,
        content_hash: String,
    }

    let existing = sqlx::query_as::<_, AdapterPackageVersionHashRow>(
        r#"
        SELECT artifact_hash, content_hash
        FROM app_conversation_adapter_package_versions
        WHERE package_id = ?1 AND version = ?2
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
        INSERT INTO app_conversation_adapter_package_versions (
            package_id, version, install_dir, artifact_hash,
            content_hash, runtime_gate_status, installed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(package_id, version) DO UPDATE SET
            install_dir = excluded.install_dir,
            runtime_gate_status = excluded.runtime_gate_status
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

pub(crate) async fn activate_conversation_adapter_workspace_sqlx(
    pool: &SqlitePool,
    adapter: &ConversationAdapter,
    package: &ConversationAdapterPackage,
) -> StoreResult<()> {
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    upsert_conversation_adapter_for_all_tenants(&mut tx, adapter).await?;
    upsert_conversation_adapter_package_with_executor(&mut *tx, package).await?;
    sqlx::query("DELETE FROM app_conversation_adapter_package_versions WHERE package_id = ?1")
        .bind(&package.package_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)
}

pub(crate) async fn deactivate_conversation_adapter_package_sqlx(
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
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
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
    sqlx::query(
        "UPDATE session_memories SET status = 'invalid', updated_at = ?1 WHERE source_id IN (SELECT id FROM conversation_sources WHERE adapter_id = ?2)",
    )
    .bind(&now)
    .bind(adapter_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        r#"
        UPDATE app_conversation_adapter_packages
        SET runtime_ready = 0,
            runtime_gate_status = 'runtime_missing',
            runtime_validated_at = ?1,
            error_message = 'conversation adapter package is uninstalled',
            updated_at = ?1
        WHERE package_id = ?2
        "#,
    )
    .bind(&now)
    .bind(package_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)?;

    package.runtime_ready = false;
    package.runtime_gate_status = ConversationAdapterRuntimeGateStatus::RuntimeMissing;
    package.runtime_validated_at = Some(now.clone());
    package.error_message = Some("conversation adapter package is uninstalled".to_string());
    package.updated_at = now;
    Ok(package)
}

pub(crate) async fn upsert_conversation_adapter_catalog_release_sqlx(
    pool: &SqlitePool,
    release: &ConversationAdapterCatalogRelease,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO app_conversation_adapter_catalog_releases (
            catalog_url, package_id, version, channel, released_at,
            core_compatibility, artifact_url, artifact_size, artifact_sha256,
            changelog_markdown, breaking_change, runtime_protocol,
            adapter_manifest_json, etag, fetched_at, adapter_id, name, publisher,
            record_kind, package_manifest_file, adapter_manifest_file, source_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22
        )
        ON CONFLICT(catalog_url, package_id, version) DO UPDATE SET
            channel = excluded.channel,
            released_at = excluded.released_at,
            core_compatibility = excluded.core_compatibility,
            artifact_url = excluded.artifact_url,
            artifact_size = excluded.artifact_size,
            artifact_sha256 = excluded.artifact_sha256,
            changelog_markdown = excluded.changelog_markdown,
            breaking_change = excluded.breaking_change,
            runtime_protocol = excluded.runtime_protocol,
            adapter_manifest_json = excluded.adapter_manifest_json,
            etag = excluded.etag,
            fetched_at = excluded.fetched_at,
            adapter_id = excluded.adapter_id,
            name = excluded.name,
            publisher = excluded.publisher,
            record_kind = excluded.record_kind,
            package_manifest_file = excluded.package_manifest_file,
            adapter_manifest_file = excluded.adapter_manifest_file,
            source_json = excluded.source_json
        "#,
    )
    .bind(&release.catalog_url)
    .bind(&release.package_id)
    .bind(&release.version)
    .bind(encode_enum(release.channel)?)
    .bind(&release.released_at)
    .bind(&release.core_compatibility)
    .bind(&release.artifact_url)
    .bind(release.artifact_size)
    .bind(&release.artifact_sha256)
    .bind(&release.changelog_markdown)
    .bind(if release.breaking_change { 1 } else { 0 })
    .bind(&release.runtime_protocol)
    .bind(&release.adapter_manifest_json)
    .bind(&release.etag)
    .bind(&release.fetched_at)
    .bind(&release.adapter_id)
    .bind(&release.name)
    .bind(&release.publisher)
    .bind(encode_enum(release.record_kind)?)
    .bind(&release.package_manifest_file)
    .bind(&release.adapter_manifest_file)
    .bind(&release.source_json)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn list_conversation_adapter_package_versions_sqlx(
    pool: &SqlitePool,
    package_id: &str,
) -> StoreResult<Vec<ConversationAdapterPackageVersion>> {
    #[derive(Debug, FromRow)]
    struct AdapterPackageVersionRow {
        package_id: String,
        version: String,
        install_dir: String,
        artifact_hash: Option<String>,
        content_hash: String,
        runtime_gate_status: String,
        installed_at: String,
    }

    impl AdapterPackageVersionRow {
        fn into_domain(self) -> StoreResult<ConversationAdapterPackageVersion> {
            Ok(ConversationAdapterPackageVersion {
                package_id: self.package_id,
                version: self.version,
                install_dir: self.install_dir,
                artifact_hash: self.artifact_hash,
                content_hash: self.content_hash,
                runtime_gate_status: decode_enum(self.runtime_gate_status)?,
                installed_at: self.installed_at,
            })
        }
    }

    let rows = sqlx::query_as::<_, AdapterPackageVersionRow>(
        r#"
        SELECT package_id, version, install_dir, artifact_hash, content_hash,
               runtime_gate_status, installed_at
        FROM app_conversation_adapter_package_versions
        WHERE package_id = ?1
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

pub(crate) async fn update_conversation_adapter_package_version_install_dir_sqlx(
    pool: &SqlitePool,
    package_id: &str,
    version: &str,
    install_dir: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        UPDATE app_conversation_adapter_package_versions
        SET install_dir = ?1
        WHERE package_id = ?2 AND version = ?3
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

pub(crate) async fn delete_conversation_adapter_package_version_sqlx(
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
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let result = sqlx::query(
        "DELETE FROM app_conversation_adapter_package_versions WHERE package_id = ?1 AND version = ?2",
    )
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

pub(crate) async fn has_running_conversation_sync_for_adapter_sqlx(
    pool: &SqlitePool,
    adapter_id: &str,
) -> StoreResult<bool> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM conversation_sync_runs
            WHERE adapter_id = ?1 AND status = 'running'
        )
        "#,
    )
    .bind(adapter_id)
    .fetch_one(pool)
    .await
    .map(|value| value == 1)
    .map_err(StoreError::external)
}
