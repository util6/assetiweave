use super::*;
use uuid::Uuid;

#[tokio::test]
async fn sqlx_navigation_model_round_trips_updates_and_localized_labels() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-navigation-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let mut model = crate::backend::defaults::default_navigation_model();
    model.active_rail_id = "settings".to_string();
    model.rail_items[0].labels = Some(LocalizedNavigationLabels {
        zh: Some("  资产  ".to_string()),
        en: Some("Assets".to_string()),
    });

    save_navigation_model_sqlx(database.pool(), "default", &model)
        .await
        .expect("save model");
    let mut loaded = load_navigation_model_sqlx(database.pool(), "default")
        .await
        .expect("load model");
    loaded.active_sub_nav_id = "updated-sub-nav".to_string();
    save_navigation_model_sqlx(database.pool(), "default", &loaded)
        .await
        .expect("save updated model");
    let loaded = load_navigation_model_sqlx(database.pool(), "default")
        .await
        .expect("load updated model");

    assert_eq!(loaded.active_rail_id, "settings");
    assert_eq!(loaded.active_sub_nav_id, "updated-sub-nav");
    assert_eq!(loaded.rail_items.len(), model.rail_items.len());
    assert_eq!(loaded.header_tabs.len(), model.header_tabs.len());
    assert_eq!(loaded.sub_nav_items.len(), model.sub_nav_items.len());
    let labels = loaded.rail_items[0]
        .labels
        .as_ref()
        .expect("localized labels");
    assert_eq!(labels.zh.as_deref(), Some("资产"));
    assert_eq!(labels.en.as_deref(), Some("Assets"));

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

#[tokio::test]
async fn existing_navigation_gains_memory_without_overwriting_custom_labels() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-navigation-memory-upgrade-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let defaults = crate::backend::defaults::default_navigation_model();
    let mut legacy = defaults.clone();
    legacy.header_tabs.retain(|tab| tab.id != "memory");
    legacy.sub_nav_items.remove("memory");
    legacy.header_tabs[0].label = "My Skills".to_string();
    legacy.header_tabs[0].labels = Some(LocalizedNavigationLabels {
        zh: Some("我的技能".to_string()),
        en: Some("My Skills".to_string()),
    });

    save_navigation_model_sqlx(database.pool(), "default", &legacy)
        .await
        .expect("save legacy navigation");
    ensure_navigation_model_items_sqlx(database.pool(), "default", &defaults)
        .await
        .expect("ensure navigation model items");
    let loaded = load_navigation_model_sqlx(database.pool(), "default")
        .await
        .expect("load upgraded navigation");

    let skills = loaded
        .header_tabs
        .iter()
        .find(|tab| tab.id == "skills")
        .expect("skills header tab");
    assert_eq!(skills.label, "My Skills");
    assert_eq!(
        skills
            .labels
            .as_ref()
            .and_then(|labels| labels.zh.as_deref()),
        Some("我的技能")
    );
    assert!(loaded.header_tabs.iter().any(|tab| tab.id == "memory"));
    assert_eq!(loaded.sub_nav_items["memory"].len(), 2);
    assert!(loaded.header_tabs.iter().any(|tab| tab.id == "team"));
    assert_eq!(loaded.sub_nav_items["team"].len(), 1);

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}

#[tokio::test]
async fn existing_memory_navigation_is_replaced_by_the_two_public_workspaces() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-navigation-memory-cutover-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let defaults = crate::backend::defaults::default_navigation_model();
    let mut legacy = defaults.clone();
    legacy.active_header_tab_id = "memory".to_string();
    legacy.active_sub_nav_id = "overview".to_string();
    let memory_items = legacy
        .sub_nav_items
        .get_mut("memory")
        .expect("default memory navigation");
    let mut overview = memory_items[0].clone();
    overview.id = "overview".to_string();
    overview.label = "Overview".to_string();
    overview.route_key = "memory.overview".to_string();
    let mut dreams = memory_items[0].clone();
    dreams.id = "dreams".to_string();
    dreams.label = "Dreams".to_string();
    dreams.route_key = "memory.dreams".to_string();
    let mut library = memory_items[0].clone();
    library.id = "library".to_string();
    library.label = "Library".to_string();
    library.route_key = "memory.library".to_string();
    *memory_items = vec![overview, dreams, memory_items[1].clone(), library];

    save_navigation_model_sqlx(database.pool(), "default", &legacy)
        .await
        .expect("save legacy navigation");
    ensure_navigation_model_items_sqlx(database.pool(), "default", &defaults)
        .await
        .expect("ensure defaults");
    let loaded = load_navigation_model_sqlx(database.pool(), "default")
        .await
        .expect("load cutover navigation");

    assert_eq!(
        loaded.sub_nav_items["memory"]
            .iter()
            .map(|item| (item.id.as_str(), item.route_key.as_str()))
            .collect::<Vec<_>>(),
        vec![("recent", "memory.recent"), ("recall", "memory.recall")]
    );
    assert_eq!(loaded.active_sub_nav_id, "recent");

    drop(database);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
    let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
}
