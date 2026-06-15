use crate::models::{
    Asset, AssetGroup, AssetGroupDetail, AssetGroupMemberOrigin, AssetGroupResolvedMember,
    AssetGroupRules, AssetKind,
};
use crate::types::AppResult;
use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::collections::{BTreeMap, BTreeSet};

use super::{
    codec::{db_error, decode_enum, decode_json, encode_enum, encode_json, to_sql_error},
    sql,
};

pub(crate) fn load_skill_group_details(
    conn: &Connection,
    assets: &[Asset],
) -> AppResult<Vec<AssetGroupDetail>> {
    let groups = load_asset_groups_by_kind(conn, AssetKind::Skill)?;
    let manual_members = load_group_members(conn)?;
    groups
        .into_iter()
        .map(|group| build_group_detail(group, assets, &manual_members))
        .collect()
}

pub(crate) fn load_skill_group_detail(
    conn: &Connection,
    group_id: &str,
    assets: &[Asset],
) -> AppResult<AssetGroupDetail> {
    let group = load_asset_group(conn, group_id)?
        .ok_or_else(|| format!("asset group not found: {group_id}"))?;
    if group.asset_kind != AssetKind::Skill {
        return Err("only skill groups are supported".to_string());
    }
    let manual_members = load_group_members(conn)?;
    build_group_detail(group, assets, &manual_members)
}

pub(crate) fn upsert_asset_group(conn: &Connection, group: &AssetGroup) -> AppResult<()> {
    validate_asset_group(group)?;
    conn.execute(
        sql::UPSERT_ASSET_GROUP,
        params![
            group.id,
            group.name.trim(),
            group
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            group.color.trim(),
            encode_enum(group.asset_kind)?,
            group.display_icon.as_deref().map(str::trim).filter(|v| !v.is_empty()),
            group.icon_svg.as_ref().and_then(|svg| encode_json(svg).ok()),
            if group.enabled { 1 } else { 0 },
            group.sort_order,
            encode_json(&normalize_rules(&group.rules))?,
            group.created_at,
            group.updated_at,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) fn delete_asset_group(conn: &Connection, group_id: &str) -> AppResult<()> {
    conn.execute(sql::DELETE_ASSET_GROUP_MEMBERS, params![group_id])
        .map_err(db_error)?;
    conn.execute(sql::DELETE_ASSET_GROUP, params![group_id])
        .map_err(db_error)?;
    Ok(())
}

pub(crate) fn replace_asset_group_members(
    conn: &Connection,
    group_id: &str,
    asset_ids: &[String],
    assets: &[Asset],
) -> AppResult<()> {
    let group = load_asset_group(conn, group_id)?
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
    let tx = conn
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    tx.execute(sql::DELETE_ASSET_GROUP_MEMBERS, params![group_id])
        .map_err(db_error)?;
    for asset_id in deduped {
        tx.execute(
            sql::INSERT_ASSET_GROUP_MEMBER,
            params![group_id, asset_id, now],
        )
        .map_err(db_error)?;
    }
    tx.commit().map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn delete_orphan_asset_group_members(conn: &Connection) -> AppResult<()> {
    conn.execute(sql::DELETE_ORPHAN_ASSET_GROUP_MEMBERS, [])
        .map_err(db_error)?;
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

fn load_asset_groups_by_kind(conn: &Connection, kind: AssetKind) -> AppResult<Vec<AssetGroup>> {
    let mut stmt = conn
        .prepare(sql::LIST_ASSET_GROUPS_BY_KIND)
        .map_err(db_error)?;
    let kind = encode_enum(kind)?;
    let rows = stmt
        .query_map(params![kind], map_asset_group_row)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn load_asset_group(conn: &Connection, group_id: &str) -> AppResult<Option<AssetGroup>> {
    conn.query_row(sql::GET_ASSET_GROUP, params![group_id], map_asset_group_row)
        .optional()
        .map_err(db_error)
}

fn load_group_members(conn: &Connection) -> AppResult<BTreeMap<String, BTreeSet<String>>> {
    let mut stmt = conn
        .prepare(sql::LIST_ASSET_GROUP_MEMBERS)
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(db_error)?;
    let mut grouped = BTreeMap::new();
    for row in rows {
        let (group_id, asset_id) = row.map_err(db_error)?;
        grouped
            .entry(group_id)
            .or_insert_with(BTreeSet::new)
            .insert(asset_id);
    }
    Ok(grouped)
}

fn map_asset_group_row(row: &Row<'_>) -> rusqlite::Result<AssetGroup> {
    let rules_payload: String = row.get(9)?;
    Ok(AssetGroup {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        color: row.get(3)?,
        asset_kind: decode_enum(row.get::<_, String>(4)?).map_err(to_sql_error)?,
        display_icon: row.get::<_, Option<String>>(5)?,
        icon_svg: row.get::<_, Option<String>>(6)?.and_then(|payload| decode_json(payload).ok()),
        enabled: row.get::<_, i64>(7)? == 1,
        sort_order: row.get(8)?,
        rules: decode_json(rules_payload).map_err(to_sql_error)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
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
    use crate::models::{AssetFormat, AssetKind};

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
