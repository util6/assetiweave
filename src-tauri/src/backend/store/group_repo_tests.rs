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
            detector_id: "legacy.classifier".to_string(),
            detector_version: 1,
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

#[tokio::test]
async fn sqlx_group_repo_round_trips_members_and_cleans_orphans() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-group-sqlx-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let assets = vec![
        test_asset("frontend-ui", "source-a", "frontend/frontend-ui"),
        test_asset("tampermonkey", "source-a", "scripts/tampermonkey"),
        Asset {
            kind: AssetKind::Rule,
            detector_id: "legacy.classifier".to_string(),
            detector_version: 1,
            ..test_asset("frontend-rule", "source-a", "frontend-rule")
        },
    ];
    let group = test_group(AssetGroupRules {
        source_ids: vec!["source-a".to_string()],
        relative_path_globs: vec!["frontend/**".to_string()],
        name_contains: Some("ui".to_string()),
    });

    crate::backend::store::replace_source_assets_sqlx(
        database.pool(),
        "default",
        "source-a",
        &assets,
    )
    .await
    .expect("replace source assets");
    upsert_asset_group_sqlx(database.pool(), "default", &group)
        .await
        .expect("upsert asset group");
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
    .await
    .expect("replace asset group members");
    let details = load_skill_group_details_sqlx(database.pool(), "default", &assets)
        .await
        .expect("load skill group details");
    let detail = load_skill_group_detail_sqlx(database.pool(), "default", &group.id, &assets)
        .await
        .expect("load skill group detail");
    sqlx::query(sql::INSERT_ASSET_GROUP_MEMBER)
        .bind("default")
        .bind(&group.id)
        .bind("missing-skill")
        .bind("2026-01-01T00:00:00Z")
        .execute(database.pool())
        .await
        .expect("insert asset group member");
    delete_orphan_asset_group_members_sqlx(database.pool(), "default")
        .await
        .expect("delete orphan asset group members");
    let cleaned_detail =
        load_skill_group_detail_sqlx(database.pool(), "default", &group.id, &assets)
            .await
            .expect("load cleaned skill group detail");
    delete_asset_group_sqlx(database.pool(), "default", &group.id)
        .await
        .expect("delete asset group");
    let after_delete = load_skill_group_details_sqlx(database.pool(), "default", &assets)
        .await
        .expect("load skill group details after delete");

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
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
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
