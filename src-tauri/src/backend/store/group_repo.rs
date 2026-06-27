use crate::backend::dto::AppResult;
use crate::backend::models::{
    Asset, AssetGroup, AssetGroupDetail, AssetGroupMemberOrigin, AssetGroupResolvedMember,
    AssetGroupRules, AssetKind,
};
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    codec::{decode_enum, decode_json, encode_enum, encode_json},
    sql,
};

pub(crate) async fn load_skill_group_details_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    assets: &[Asset],
) -> AppResult<Vec<AssetGroupDetail>> {
    let groups = load_asset_groups_by_kind_sqlx(pool, tenant_id, AssetKind::Skill).await?;
    let manual_members = load_group_members_sqlx(pool, tenant_id).await?;
    groups
        .into_iter()
        .map(|group| build_group_detail(group, assets, &manual_members))
        .collect()
}

pub(crate) async fn load_skill_group_details_by_ids_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    group_ids: &BTreeSet<String>,
    assets: &[Asset],
) -> AppResult<Vec<AssetGroupDetail>> {
    let groups = load_asset_groups_by_kind_sqlx(pool, tenant_id, AssetKind::Skill)
        .await?
        .into_iter()
        .filter(|group| group_ids.contains(&group.id))
        .collect::<Vec<_>>();
    let manual_members = load_group_members_sqlx(pool, tenant_id).await?;
    groups
        .into_iter()
        .map(|group| build_group_detail(group, assets, &manual_members))
        .collect()
}

pub(crate) async fn load_skill_group_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    group_id: &str,
    assets: &[Asset],
) -> AppResult<AssetGroupDetail> {
    let group = load_asset_group_sqlx(pool, tenant_id, group_id)
        .await?
        .ok_or_else(|| format!("asset group not found: {group_id}"))?;
    if group.asset_kind != AssetKind::Skill {
        return Err("only skill groups are supported".to_string());
    }
    let manual_members = load_group_members_sqlx(pool, tenant_id).await?;
    build_group_detail(group, assets, &manual_members)
}

pub(crate) async fn upsert_asset_group_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    group: &AssetGroup,
) -> AppResult<()> {
    validate_asset_group(group)?;
    let icon_svg = group.icon_svg.as_ref().map(encode_json).transpose()?;
    sqlx::query(sql::UPSERT_ASSET_GROUP)
        .bind(tenant_id)
        .bind(&group.id)
        .bind(group.name.trim())
        .bind(
            group
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(group.color.trim())
        .bind(encode_enum(group.asset_kind)?)
        .bind(
            group
                .display_icon
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        )
        .bind(icon_svg)
        .bind(if group.enabled { 1 } else { 0 })
        .bind(group.sort_order)
        .bind(encode_json(&normalize_rules(&group.rules))?)
        .bind(&group.created_at)
        .bind(&group.updated_at)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn delete_asset_group_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    group_id: &str,
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSET_GROUP_MEMBERS)
        .bind(tenant_id)
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSET_GROUP)
        .bind(tenant_id)
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn replace_asset_group_members_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    group_id: &str,
    asset_ids: &[String],
    assets: &[Asset],
) -> AppResult<()> {
    let group = load_asset_group_sqlx(pool, tenant_id, group_id)
        .await?
        .ok_or_else(|| format!("asset group not found: {group_id}"))?;
    if group.asset_kind != AssetKind::Skill {
        return Err("only skill groups are supported".to_string());
    }

    let skill_asset_ids = assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Skill)
        .map(|asset| asset.id.as_str())
        .collect::<BTreeSet<_>>();
    let deduped = asset_ids
        .iter()
        .map(|asset_id| asset_id.trim())
        .filter(|asset_id| !asset_id.is_empty())
        .collect::<BTreeSet<_>>();

    if let Some(missing_or_invalid) = deduped
        .iter()
        .find(|asset_id| !skill_asset_ids.contains(**asset_id))
    {
        return Err(format!(
            "asset is not a scanned skill: {missing_or_invalid}"
        ));
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::DELETE_ASSET_GROUP_MEMBERS)
        .bind(tenant_id)
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    for asset_id in deduped {
        sqlx::query(sql::INSERT_ASSET_GROUP_MEMBER)
            .bind(tenant_id)
            .bind(group_id)
            .bind(asset_id)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn delete_orphan_asset_group_members_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_ASSET_GROUP_MEMBERS)
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn validate_asset_group(group: &AssetGroup) -> AppResult<()> {
    if group.name.trim().is_empty() {
        return Err("asset group name is required".to_string());
    }
    if group.asset_kind != AssetKind::Skill {
        return Err("only skill groups are supported".to_string());
    }
    if group.color.trim().is_empty() {
        return Err("asset group color is required".to_string());
    }
    build_glob_set(&group.rules.relative_path_globs).map(|_| ())
}

pub(crate) fn normalize_rules(rules: &AssetGroupRules) -> AssetGroupRules {
    AssetGroupRules {
        source_ids: normalize_string_list(&rules.source_ids),
        relative_path_globs: normalize_string_list(&rules.relative_path_globs),
        name_contains: rules
            .name_contains
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    }
}

pub(crate) fn build_group_detail(
    group: AssetGroup,
    assets: &[Asset],
    manual_members: &BTreeMap<String, BTreeSet<String>>,
) -> AppResult<AssetGroupDetail> {
    let manual_asset_ids = manual_members
        .get(&group.id)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    let rule_asset_ids = resolve_rule_asset_ids(&group.rules, assets)?;
    let mut origins = BTreeMap::new();

    for asset_id in &manual_asset_ids {
        origins.insert(asset_id.clone(), AssetGroupMemberOrigin::Manual);
    }
    for asset_id in rule_asset_ids {
        origins
            .entry(asset_id)
            .and_modify(|origin| *origin = AssetGroupMemberOrigin::ManualAndRule)
            .or_insert(AssetGroupMemberOrigin::Rule);
    }

    Ok(AssetGroupDetail {
        group,
        members: origins
            .into_iter()
            .map(|(asset_id, origin)| AssetGroupResolvedMember { asset_id, origin })
            .collect(),
        manual_asset_ids,
    })
}

async fn load_asset_groups_by_kind_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    kind: AssetKind,
) -> AppResult<Vec<AssetGroup>> {
    let rows = sqlx::query(sql::LIST_ASSET_GROUPS_BY_KIND)
        .bind(tenant_id)
        .bind(encode_enum(kind)?)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_asset_group_row).collect()
}

async fn load_asset_group_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    group_id: &str,
) -> AppResult<Option<AssetGroup>> {
    let row = sqlx::query(sql::GET_ASSET_GROUP)
        .bind(tenant_id)
        .bind(group_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    row.as_ref().map(map_sqlx_asset_group_row).transpose()
}

async fn load_group_members_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<BTreeMap<String, BTreeSet<String>>> {
    let rows = sqlx::query(sql::LIST_ASSET_GROUP_MEMBERS)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let mut grouped = BTreeMap::new();
    for row in rows {
        let group_id = row
            .try_get::<String, _>(0)
            .map_err(|error| error.to_string())?;
        let asset_id = row
            .try_get::<String, _>(1)
            .map_err(|error| error.to_string())?;
        grouped
            .entry(group_id)
            .or_insert_with(BTreeSet::new)
            .insert(asset_id);
    }
    Ok(grouped)
}

fn map_sqlx_asset_group_row(row: &SqliteRow) -> AppResult<AssetGroup> {
    let rules_payload: String = row.try_get(9).map_err(|error| error.to_string())?;
    Ok(AssetGroup {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        name: row.try_get(1).map_err(|error| error.to_string())?,
        description: row.try_get(2).map_err(|error| error.to_string())?,
        color: row.try_get(3).map_err(|error| error.to_string())?,
        asset_kind: decode_enum(
            row.try_get::<String, _>(4)
                .map_err(|error| error.to_string())?,
        )?,
        display_icon: row.try_get(5).map_err(|error| error.to_string())?,
        icon_svg: row
            .try_get::<Option<String>, _>(6)
            .map_err(|error| error.to_string())?
            .map(decode_json)
            .transpose()?,
        enabled: row
            .try_get::<i64, _>(7)
            .map_err(|error| error.to_string())?
            == 1,
        sort_order: row.try_get(8).map_err(|error| error.to_string())?,
        rules: decode_json(rules_payload)?,
        created_at: row.try_get(10).map_err(|error| error.to_string())?,
        updated_at: row.try_get(11).map_err(|error| error.to_string())?,
    })
}

fn resolve_rule_asset_ids(rules: &AssetGroupRules, assets: &[Asset]) -> AppResult<Vec<String>> {
    let rules = normalize_rules(rules);
    if rules.source_ids.is_empty()
        && rules.relative_path_globs.is_empty()
        && rules.name_contains.is_none()
    {
        return Ok(Vec::new());
    }

    let source_ids = rules.source_ids.into_iter().collect::<BTreeSet<_>>();
    let glob_set = build_glob_set(&rules.relative_path_globs)?;
    let name_contains = rules.name_contains.map(|value| value.to_lowercase());

    Ok(assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Skill)
        .filter(|asset| source_ids.is_empty() || source_ids.contains(&asset.source_id))
        .filter(|asset| {
            glob_set
                .as_ref()
                .map(|set| set.is_match(&asset.relative_path))
                .unwrap_or(true)
        })
        .filter(|asset| {
            name_contains
                .as_ref()
                .map(|needle| asset.name.to_lowercase().contains(needle))
                .unwrap_or(true)
        })
        .map(|asset| asset.id.clone())
        .collect())
}

fn build_glob_set(patterns: &[String]) -> AppResult<Option<GlobSet>> {
    let patterns = normalize_string_list(patterns);
    if patterns.is_empty() {
        return Ok(None);
    }

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(&pattern).map_err(|error| error.to_string())?);
    }
    builder.build().map(Some).map_err(|error| error.to_string())
}

fn normalize_string_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{AssetFormat, AssetKind};

    #[test]
    fn resolves_manual_and_rule_members_without_duplicates() {
        let assets = vec![
            test_asset("frontend-ui", "source-a", "frontend/frontend-ui"),
            test_asset("tampermonkey", "source-b", "scripts/tampermonkey"),
            test_asset("rust-api", "source-a", "backend/rust-api"),
        ];
        let group = test_group(AssetGroupRules {
            source_ids: vec!["source-a".to_string()],
            relative_path_globs: vec!["frontend/**".to_string()],
            name_contains: Some("ui".to_string()),
        });
        let manual_members = BTreeMap::from([(
            group.id.clone(),
            BTreeSet::from(["tampermonkey".to_string(), "frontend-ui".to_string()]),
        )]);

        let detail = build_group_detail(group, &assets, &manual_members).expect("resolve group");

        assert_eq!(detail.members.len(), 2);
        assert_eq!(
            detail
                .members
                .iter()
                .find(|member| member.asset_id == "frontend-ui")
                .map(|member| member.origin),
            Some(AssetGroupMemberOrigin::ManualAndRule)
        );
        assert_eq!(
            detail
                .members
                .iter()
                .find(|member| member.asset_id == "tampermonkey")
                .map(|member| member.origin),
            Some(AssetGroupMemberOrigin::Manual)
        );
    }

    #[test]
    fn empty_rules_do_not_match_every_skill() {
        let group = test_group(AssetGroupRules {
            source_ids: vec![],
            relative_path_globs: vec![],
            name_contains: None,
        });
        let detail = build_group_detail(
            group,
            &[test_asset("frontend-ui", "source-a", "frontend-ui")],
            &BTreeMap::new(),
        )
        .expect("resolve group");

        assert!(detail.members.is_empty());
    }

    #[test]
    fn rule_resolution_only_matches_skills() {
        let assets = vec![
            test_asset("frontend-ui", "source-a", "frontend-ui"),
            Asset {
                kind: AssetKind::Rule,
                ..test_asset("frontend-rule", "source-a", "frontend-rule")
            },
        ];
        let group = test_group(AssetGroupRules {
            source_ids: vec!["source-a".to_string()],
            relative_path_globs: vec![],
            name_contains: Some("frontend".to_string()),
        });

        let detail = build_group_detail(group, &assets, &BTreeMap::new()).expect("resolve group");

        assert_eq!(detail.members.len(), 1);
        assert_eq!(detail.members[0].asset_id, "frontend-ui");
    }

    #[test]
    fn sqlx_group_repo_round_trips_members_and_cleans_orphans() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-group-sqlx-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let assets = vec![
            test_asset("frontend-ui", "source-a", "frontend/frontend-ui"),
            test_asset("tampermonkey", "source-a", "scripts/tampermonkey"),
            Asset {
                kind: AssetKind::Rule,
                ..test_asset("frontend-rule", "source-a", "frontend-rule")
            },
        ];
        let group = test_group(AssetGroupRules {
            source_ids: vec!["source-a".to_string()],
            relative_path_globs: vec!["frontend/**".to_string()],
            name_contains: Some("ui".to_string()),
        });

        let (details, detail, cleaned_detail, after_delete) = database
            .block_on(async {
                crate::backend::store::replace_source_assets_sqlx(
                    database.pool(),
                    "default",
                    "source-a",
                    &assets,
                )
                .await?;
                upsert_asset_group_sqlx(database.pool(), "default", &group).await?;
                replace_asset_group_members_sqlx(
                    database.pool(),
                    "default",
                    &group.id,
                    &[
                        "tampermonkey".to_string(),
                        "frontend-ui".to_string(),
                        "tampermonkey".to_string(),
                    ],
                    &assets,
                )
                .await?;
                let details =
                    load_skill_group_details_sqlx(database.pool(), "default", &assets).await?;
                let detail =
                    load_skill_group_detail_sqlx(database.pool(), "default", &group.id, &assets)
                        .await?;
                sqlx::query(sql::INSERT_ASSET_GROUP_MEMBER)
                    .bind("default")
                    .bind(&group.id)
                    .bind("missing-skill")
                    .bind("2026-01-01T00:00:00Z")
                    .execute(database.pool())
                    .await
                    .map_err(|error| error.to_string())?;
                delete_orphan_asset_group_members_sqlx(database.pool(), "default").await?;
                let cleaned_detail =
                    load_skill_group_detail_sqlx(database.pool(), "default", &group.id, &assets)
                        .await?;
                delete_asset_group_sqlx(database.pool(), "default", &group.id).await?;
                let after_delete =
                    load_skill_group_details_sqlx(database.pool(), "default", &assets).await?;
                AppResult::Ok((details, detail, cleaned_detail, after_delete))
            })
            .expect("round trip SQLx asset groups");

        assert_eq!(details.len(), 1);
        assert_eq!(
            detail.manual_asset_ids,
            vec!["frontend-ui".to_string(), "tampermonkey".to_string()]
        );
        assert_eq!(
            detail
                .members
                .iter()
                .find(|member| member.asset_id == "frontend-ui")
                .map(|member| member.origin),
            Some(AssetGroupMemberOrigin::ManualAndRule)
        );
        assert!(!cleaned_detail
            .manual_asset_ids
            .iter()
            .any(|asset_id| asset_id == "missing-skill"));
        assert!(after_delete.is_empty());

        drop(database);
        cleanup_database(&db_path);
    }

    fn test_group(rules: AssetGroupRules) -> AssetGroup {
        AssetGroup {
            id: "frontend".to_string(),
            name: "Frontend".to_string(),
            description: None,
            color: "#10b981".to_string(),
            asset_kind: AssetKind::Skill,
            display_icon: None,
            icon_svg: None,
            enabled: true,
            sort_order: 0,
            rules,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }

    fn test_asset(id: &str, source_id: &str, relative_path: &str) -> Asset {
        Asset {
            id: id.to_string(),
            source_id: source_id.to_string(),
            name: id.to_string(),
            kind: AssetKind::Skill,
            format: AssetFormat::Directory,
            relative_path: relative_path.to_string(),
            absolute_path: format!("/tmp/{relative_path}"),
            entry_file: Some("SKILL.md".to_string()),
            description: None,
            content_hash: None,
            discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
