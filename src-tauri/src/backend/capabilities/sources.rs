use super::prelude::*;

pub(crate) fn scan_selected_sources(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    sources: Vec<Source>,
    scan: fn(&Source) -> AppResult<Vec<Asset>>,
) -> AppResult<Vec<Asset>> {
    for mut source in prune_missing_sources(db, tenant_id, sources)? {
        if !source.enabled {
            log_info(
                "source.scan.skip",
                "跳过已禁用来源",
                &source_log_fields(&source),
            );
            continue;
        }

        log_info(
            "source.scan.start",
            "开始扫描来源",
            &source_log_fields(&source),
        );
        let now = Utc::now().to_rfc3339();
        match scan(&source) {
            Ok(assets) => {
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("ok: {} assets", assets.len()));
                persist_source_scan_result_sqlx(db, tenant_id, &source, &assets)?;
                let mut fields = source_log_fields(&source);
                fields.push(("asset_count", assets.len().to_string()));
                log_info("source.scan.success", "扫描来源成功", &fields);
                for asset in &assets {
                    if matches!(asset.kind, AssetKind::Skill) {
                        log_info(
                            "skill.scan.success",
                            "扫描到 skill",
                            &asset_log_fields(asset),
                        );
                    }
                }
            }
            Err(error) => {
                if should_remove_source_on_scan_error(&error) {
                    let mut fields = source_log_fields(&source);
                    fields.push(("error", error.clone()));
                    log_warn("source.scan.removed", "来源路径不存在，已移除", &fields);
                    delete_source_sqlx(db, tenant_id, &source.id)?;
                    continue;
                }
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("error: {error}"));
                upsert_source_sqlx(db, tenant_id, &source)?;
                log_error(
                    "source.scan.error",
                    "扫描来源失败",
                    &error,
                    &source_log_fields(&source),
                );
            }
        }
    }

    cleanup_orphan_asset_records(db, tenant_id)?;
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    db.block_on(
        async move { crate::backend::store::load_assets_sqlx(&pool, &tenant_id, None).await },
    )
}

pub(crate) fn refresh_all_sources(
    db: &crate::backend::store::Database,
    tenant_id: &str,
) -> AppResult<Vec<Asset>> {
    let pool = db.pool().clone();
    let tenant_id_for_query = tenant_id.to_string();
    let sources = db.block_on(async move {
        crate::backend::store::load_sources_sqlx(&pool, &tenant_id_for_query).await
    })?;
    scan_selected_sources(db, tenant_id, sources, crate::backend::scanner::scan_source)
}

pub(crate) fn refresh_recorded_assets(
    db: &crate::backend::store::Database,
    tenant_id: &str,
) -> AppResult<Vec<Asset>> {
    let pool = db.pool().clone();
    let tenant_id_for_query = tenant_id.to_string();
    let sources = db.block_on(async move {
        crate::backend::store::load_sources_sqlx(&pool, &tenant_id_for_query).await
    })?;
    let sources = prune_missing_sources(db, tenant_id, sources)?;
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

    let pool = db.pool().clone();
    let tenant_id_for_query = tenant_id.to_string();
    let assets = db.block_on(async move {
        crate::backend::store::load_assets_sqlx(&pool, &tenant_id_for_query, None).await
    })?;
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
        persist_source_scan_result_sqlx(db, tenant_id, &source, &retained_assets)?;
    }

    orphan_source_ids.sort();
    orphan_source_ids.dedup();
    for source_id in orphan_source_ids {
        replace_source_assets_sqlx(db, tenant_id, &source_id, &[])?;
    }

    cleanup_orphan_asset_records(db, tenant_id)?;
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    db.block_on(
        async move { crate::backend::store::load_assets_sqlx(&pool, &tenant_id, None).await },
    )
}

pub(crate) fn cleanup_orphan_asset_records(
    db: &crate::backend::store::Database,
    tenant_id: &str,
) -> AppResult<()> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    db.block_on(async move {
        crate::backend::store::delete_orphan_asset_mounts_sqlx(&pool, &tenant_id).await?;
        crate::backend::store::delete_orphan_deployment_state_sqlx(&pool, &tenant_id).await?;
        crate::backend::store::delete_orphan_skill_remote_sources_sqlx(&pool, &tenant_id).await?;
        crate::backend::store::delete_orphan_asset_group_members_sqlx(&pool, &tenant_id).await
    })
}

fn prune_missing_sources(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    sources: Vec<Source>,
) -> AppResult<Vec<Source>> {
    let mut retained_sources = Vec::new();
    let mut missing_source_ids = Vec::new();
    for source in sources {
        if source_root_is_missing(&source) {
            log_warn(
                "source.prune_missing",
                "来源路径不存在，已从索引移除",
                &source_log_fields(&source),
            );
            missing_source_ids.push(source.id);
        } else {
            retained_sources.push(source);
        }
    }
    for source_id in missing_source_ids {
        delete_source_sqlx(db, tenant_id, &source_id)?;
    }
    Ok(retained_sources)
}

fn persist_source_scan_result_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    source: &Source,
    assets: &[Asset],
) -> AppResult<()> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let source_id = source.id.clone();
    let source_to_save = source.clone();
    let assets_to_save = assets.to_vec();
    db.block_on(async move {
        crate::backend::store::replace_source_assets_sqlx(
            &pool,
            &tenant_id,
            &source_id,
            &assets_to_save,
        )
        .await?;
        crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, &source_to_save).await
    })
}

fn replace_source_assets_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    source_id: &str,
    assets: &[Asset],
) -> AppResult<()> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let source_id = source_id.to_string();
    let assets = assets.to_vec();
    db.block_on(async move {
        crate::backend::store::replace_source_assets_sqlx(&pool, &tenant_id, &source_id, &assets)
            .await
    })
}

fn upsert_source_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    source: &Source,
) -> AppResult<()> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let source = source.clone();
    db.block_on(async move {
        crate::backend::store::upsert_source_sqlx(&pool, &tenant_id, &source).await
    })
}

fn delete_source_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    source_id: &str,
) -> AppResult<()> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let source_id = source_id.to_string();
    db.block_on(async move {
        crate::backend::store::delete_source_sqlx(&pool, &tenant_id, &source_id).await
    })
}

fn source_root_is_missing(source: &Source) -> bool {
    expand_path(&source.root_path)
        .map(|root| !root.exists())
        .unwrap_or(false)
}

fn should_remove_source_on_scan_error(error: &str) -> bool {
    error.starts_with("source path does not exist:")
}
