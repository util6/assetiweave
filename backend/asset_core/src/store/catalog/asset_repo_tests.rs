use super::*;

#[tokio::test]
async fn sqlx_asset_repo_replaces_filters_and_updates_descriptions() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-asset-sqlx-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let mut skill = test_asset("skill-a", AssetKind::Skill);
    let rule = test_asset("design", AssetKind::Rule);

    replace_source_assets_sqlx(
        database.pool(),
        "default",
        "source-a",
        &[skill.clone(), rule],
    )
    .await
    .expect("replace source assets");
    let scoped_assets = load_assets_sqlx(database.pool(), "default", Some(AssetKind::Skill))
        .await
        .expect("load scoped assets");
    let loaded_skill = load_asset_sqlx(database.pool(), "default", &skill.id)
        .await
        .expect("load asset");
    let missing_asset = load_asset_sqlx(database.pool(), "default", "missing")
        .await
        .expect("load missing asset");
    skill.description = Some("Updated".to_string());
    update_asset_description_sqlx(database.pool(), "default", &skill)
        .await
        .expect("update description");
    let all_assets = load_assets_sqlx(database.pool(), "default", None)
        .await
        .expect("load all assets");

    assert_eq!(scoped_assets.len(), 1);
    assert_eq!(scoped_assets[0].name, "skill-a");
    assert_eq!(loaded_skill.expect("load asset by id").id, skill.id);
    assert!(missing_asset.is_none());
    let updated = all_assets
        .iter()
        .find(|asset| asset.id == "asset-skill-a")
        .expect("updated asset");
    assert_eq!(updated.description.as_deref(), Some("Updated"));

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

#[tokio::test]
async fn sqlx_asset_repo_isolates_source_replacement_by_tenant() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-asset-tenant-sqlx-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let mut default_asset = test_asset("skill-a", AssetKind::Skill);
    default_asset.absolute_path = "/tmp/default-skill-a.md".to_string();
    let mut tenant_asset = test_asset("skill-a", AssetKind::Skill);
    tenant_asset.absolute_path = "/tmp/tenant-skill-a.md".to_string();

    replace_source_assets_sqlx(database.pool(), "default", "source-a", &[default_asset])
        .await
        .expect("replace default assets");
    replace_source_assets_sqlx(database.pool(), "tenant-a", "source-a", &[tenant_asset])
        .await
        .expect("replace tenant assets");
    replace_source_assets_sqlx(database.pool(), "default", "source-a", &[])
        .await
        .expect("clear default assets");
    let default_assets = load_assets_sqlx(database.pool(), "default", None)
        .await
        .expect("load default assets");
    let tenant_assets = load_assets_sqlx(database.pool(), "tenant-a", None)
        .await
        .expect("load tenant assets");

    assert!(default_assets.is_empty());
    assert_eq!(tenant_assets.len(), 1);
    assert_eq!(tenant_assets[0].absolute_path, "/tmp/tenant-skill-a.md");
    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

fn test_asset(name: &str, kind: AssetKind) -> Asset {
    Asset {
        id: format!("asset-{name}"),
        source_id: "source-a".to_string(),
        name: name.to_string(),
        kind,
        detector_id: "legacy.classifier".to_string(),
        detector_version: 1,
        format: AssetFormat::Markdown,
        relative_path: format!("{name}.md"),
        absolute_path: format!("/tmp/{name}.md"),
        entry_file: None,
        description: None,
        content_hash: None,
        discovered_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}
