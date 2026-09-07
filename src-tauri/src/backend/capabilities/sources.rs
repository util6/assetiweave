use super::prelude::*;
use sqlx::SqlitePool;

pub(crate) fn scan_source(source: &Source) -> AppResult<Vec<Asset>> {
    crate::backend::scanner::scan_source(source)
}

pub(crate) fn scan_skill_source(source: &Source) -> AppResult<Vec<Asset>> {
    crate::backend::scanner::scan_skill_source(source)
}

pub(crate) async fn scan_selected_sources(
    pool: &SqlitePool,
    tenant_id: &str,
    sources: Vec<Source>,
    scan: fn(&Source) -> AppResult<Vec<Asset>>,
) -> AppResult<Vec<Asset>> {
    scan_selected_sources_with_progress(pool, tenant_id, sources, scan, |_, _, _| Ok(())).await
}

pub(crate) async fn scan_selected_sources_with_progress<F>(
    pool: &SqlitePool,
    tenant_id: &str,
    sources: Vec<Source>,
    scan: fn(&Source) -> AppResult<Vec<Asset>>,
    mut before_source: F,
) -> AppResult<Vec<Asset>>
where
    F: FnMut(usize, usize, &Source) -> AppResult<()>,
{
    let sources = prune_missing_sources(pool, tenant_id, sources).await?;
    let total = sources.len();
    for (index, mut source) in sources.into_iter().enumerate() {
        before_source(index, total, &source)?;
        if !source.enabled {
            tracing::info!(
                action = "source.scan.skip",
                source_id = %source.id,
                source_name = %source.name,
                root_path = %source.root_path,
                "跳过已禁用来源"
            );
            continue;
        }

        tracing::info!(
            action = "source.scan.start",
            source_id = %source.id,
            source_name = %source.name,
            root_path = %source.root_path,
            "开始扫描来源"
        );
        let now = Utc::now().to_rfc3339();
        match scan(&source) {
            Ok(assets) => {
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("ok: {} assets", assets.len()));
                persist_source_scan_result_sqlx(pool, tenant_id, &source, &assets).await?;
                tracing::info!(
                    action = "source.scan.success",
                    source_id = %source.id,
                    source_name = %source.name,
                    root_path = %source.root_path,
                    asset_count = assets.len(),
                    "扫描来源成功"
                );
                for asset in &assets {
                    if matches!(asset.kind, AssetKind::Skill) {
                        tracing::info!(
                            action = "skill.scan.success",
                            asset_id = %asset.id,
                            skill_name = %asset.name,
                            source_id = %asset.source_id,
                            relative_path = %asset.relative_path,
                            "扫描到 skill"
                        );
                    }
                }
            }
            Err(error) => {
                let error_message = error.to_string();
                if should_remove_source_on_scan_error(&error_message) {
                    tracing::warn!(
                        action = "source.scan.removed",
                        source_id = %source.id,
                        source_name = %source.name,
                        root_path = %source.root_path,
                        error = %error_message,
                        "来源路径不存在，已移除"
                    );
                    delete_source_sqlx(pool, tenant_id, &source.id).await?;
                    continue;
                }
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("error: {error_message}"));
                upsert_source_sqlx(pool, tenant_id, &source).await?;
                tracing::error!(
                    action = "source.scan.error",
                    source_id = %source.id,
                    source_name = %source.name,
                    root_path = %source.root_path,
                    error = %error_message,
                    "扫描来源失败"
                );
            }
        }
    }

    cleanup_orphan_asset_records(pool, tenant_id).await?;
    crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await
}

pub(crate) async fn refresh_all_sources(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<Asset>> {
    let sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
    scan_selected_sources(pool, tenant_id, sources, scan_source).await
}

pub(crate) async fn refresh_recorded_assets(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<Asset>> {
    let sources = crate::backend::store::load_sources_sqlx(pool, tenant_id).await?;
    let sources = prune_missing_sources(pool, tenant_id, sources).await?;
    let source_map: HashMap<&str, &Source> = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect();
    let mut assets_by_source: HashMap<String, Vec<Asset>> = sources
        .iter()
        .map(|source| (source.id.clone(), Vec::new()))
        .collect();
    let mut removed_by_source: HashMap<String, usize> = HashMap::new();
    let mut updated_by_source: HashMap<String, usize> = HashMap::new();
    let mut orphan_source_ids = Vec::new();
    let now = Utc::now().to_rfc3339();

    let assets = crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await?;
    for asset in assets {
        let Some(source) = source_map.get(asset.source_id.as_str()) else {
            orphan_source_ids.push(asset.source_id.clone());
            continue;
        };

        match crate::backend::scanner::refresh_recorded_asset(source, &asset, &now) {
            Ok(Some(refreshed)) => {
                if refreshed.content_hash != asset.content_hash
                    || refreshed.description != asset.description
                {
                    *updated_by_source.entry(source.id.clone()).or_default() += 1;
                }
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(refreshed);
            }
            Ok(None) => {
                *removed_by_source.entry(source.id.clone()).or_default() += 1;
            }
            Err(_) => {
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(asset);
            }
        }
    }

    for source in sources {
        let retained_assets = assets_by_source.remove(&source.id).unwrap_or_default();
        let retained_count = retained_assets.len();

        let removed_count = removed_by_source.get(&source.id).copied().unwrap_or(0);
        let updated_count = updated_by_source.get(&source.id).copied().unwrap_or(0);
        let mut source = source;
        source.last_scanned_at = Some(now.clone());
        source.last_scan_status = Some(format!(
            "validated: {retained_count} assets, {removed_count} removed, {updated_count} updated"
        ));
        persist_source_scan_result_sqlx(pool, tenant_id, &source, &retained_assets).await?;
    }

    orphan_source_ids.sort();
    orphan_source_ids.dedup();
    for source_id in orphan_source_ids {
        replace_source_assets_sqlx(pool, tenant_id, &source_id, &[]).await?;
    }

    cleanup_orphan_asset_records(pool, tenant_id).await?;
    crate::backend::store::load_assets_sqlx(pool, tenant_id, None).await
}

pub(crate) async fn cleanup_orphan_asset_records(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    crate::backend::store::delete_orphan_asset_mounts_sqlx(pool, tenant_id).await?;
    crate::backend::store::delete_orphan_deployment_state_sqlx(pool, tenant_id).await?;
    crate::backend::store::delete_orphan_skill_remote_sources_sqlx(pool, tenant_id)
        .await
        .map_err(AppError::external)?;
    crate::backend::store::delete_orphan_asset_group_members_sqlx(pool, tenant_id).await
}

async fn prune_missing_sources(
    pool: &SqlitePool,
    tenant_id: &str,
    sources: Vec<Source>,
) -> AppResult<Vec<Source>> {
    let mut retained_sources = Vec::new();
    let mut missing_source_ids = Vec::new();
    for source in sources {
        if source_root_is_missing(&source) {
            tracing::warn!(
                action = "source.prune_missing",
                source_id = %source.id,
                source_name = %source.name,
                root_path = %source.root_path,
                "来源路径不存在，已从索引移除"
            );
            missing_source_ids.push(source.id);
        } else {
            retained_sources.push(source);
        }
    }
    for source_id in missing_source_ids {
        delete_source_sqlx(pool, tenant_id, &source_id).await?;
    }
    Ok(retained_sources)
}

async fn persist_source_scan_result_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &Source,
    assets: &[Asset],
) -> AppResult<()> {
    crate::backend::store::replace_source_assets_sqlx(pool, tenant_id, &source.id, assets).await?;
    crate::backend::store::upsert_source_sqlx(pool, tenant_id, source).await
}

async fn replace_source_assets_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
    assets: &[Asset],
) -> AppResult<()> {
    crate::backend::store::replace_source_assets_sqlx(pool, tenant_id, source_id, assets).await
}

async fn upsert_source_sqlx(pool: &SqlitePool, tenant_id: &str, source: &Source) -> AppResult<()> {
    crate::backend::store::upsert_source_sqlx(pool, tenant_id, source).await
}

async fn delete_source_sqlx(pool: &SqlitePool, tenant_id: &str, source_id: &str) -> AppResult<()> {
    crate::backend::store::delete_source_sqlx(pool, tenant_id, source_id).await
}

fn source_root_is_missing(source: &Source) -> bool {
    expand_path(&source.root_path)
        .map(|root| !root.exists())
        .unwrap_or(false)
}

fn should_remove_source_on_scan_error(error: &str) -> bool {
    error.starts_with("source path does not exist:")
}
