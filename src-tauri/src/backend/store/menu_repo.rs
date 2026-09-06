use crate::backend::dto::{
    HeaderTabItem, LocalizedNavigationLabels, NavigationModel, RailMenuItem, SubNavItem,
};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

use super::sql;

pub(crate) async fn seed_navigation_model_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    model: &NavigationModel,
) -> Result<(), String> {
    save_navigation_model_sqlx(pool, tenant_id, model).await
}

pub(crate) async fn ensure_navigation_model_items_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    defaults: &NavigationModel,
) -> Result<(), String> {
    let mut current = load_navigation_model_sqlx(pool, tenant_id).await?;
    for item in &defaults.rail_items {
        if !current
            .rail_items
            .iter()
            .any(|candidate| candidate.id == item.id)
        {
            current.rail_items.push(item.clone());
        }
    }
    for tab in &defaults.header_tabs {
        if !current
            .header_tabs
            .iter()
            .any(|candidate| candidate.id == tab.id)
        {
            current.header_tabs.push(tab.clone());
        }
    }
    for (parent_id, default_items) in &defaults.sub_nav_items {
        if parent_id == "memory" {
            current
                .sub_nav_items
                .insert(parent_id.clone(), default_items.clone());
            continue;
        }
        let current_items = current
            .sub_nav_items
            .entry(parent_id.clone())
            .or_insert_with(Vec::new);
        for item in default_items {
            if !current_items
                .iter()
                .any(|candidate| candidate.id == item.id)
            {
                current_items.push(item.clone());
            }
        }
    }
    if current.active_header_tab_id == "memory"
        && !current.sub_nav_items["memory"]
            .iter()
            .any(|item| item.id == current.active_sub_nav_id)
    {
        current.active_sub_nav_id = current.sub_nav_items["memory"]
            .first()
            .map(|item| item.id.clone())
            .unwrap_or_default();
    }
    save_navigation_model_sqlx(pool, tenant_id, &current).await
}

pub(crate) async fn save_navigation_model_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    model: &NavigationModel,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::UPSERT_NAVIGATION_STATE)
        .bind(tenant_id)
        .bind(&model.active_rail_id)
        .bind(&model.active_header_tab_id)
        .bind(&model.active_sub_nav_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;

    for (sort_order, item) in model.rail_items.iter().enumerate() {
        sqlx::query(sql::UPSERT_RAIL_MENU_ITEM)
            .bind(&item.id)
            .bind(&item.label)
            .bind(localized_label(&item.labels, "zh"))
            .bind(localized_label(&item.labels, "en"))
            .bind(&item.icon)
            .bind(&item.scope)
            .bind(enabled_value(item.enabled))
            .bind(&item.position)
            .bind(sort_order as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }

    for (sort_order, tab) in model.header_tabs.iter().enumerate() {
        sqlx::query(sql::UPSERT_HEADER_TAB_ITEM)
            .bind(&tab.id)
            .bind(&tab.label)
            .bind(localized_label(&tab.labels, "zh"))
            .bind(localized_label(&tab.labels, "en"))
            .bind(&tab.asset_kind)
            .bind(enabled_value(tab.enabled))
            .bind(sort_order as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }

    for (parent_tab_id, items) in &model.sub_nav_items {
        sqlx::query("DELETE FROM sub_nav_items WHERE parent_tab_id = ?1")
            .bind(parent_tab_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
        for (sort_order, item) in items.iter().enumerate() {
            sqlx::query(sql::UPSERT_SUB_NAV_ITEM)
                .bind(parent_tab_id)
                .bind(&item.id)
                .bind(&item.label)
                .bind(localized_label(&item.labels, "zh"))
                .bind(localized_label(&item.labels, "en"))
                .bind(&item.route_key)
                .bind(enabled_value(item.enabled))
                .bind(sort_order as i32)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
        }
    }

    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
struct NavigationStateRow {
    active_rail_id: String,
    active_header_tab_id: String,
    active_sub_nav_id: String,
}

#[derive(Debug, sqlx::FromRow)]
struct RailMenuItemRow {
    id: String,
    label: String,
    label_zh: Option<String>,
    label_en: Option<String>,
    icon: String,
    scope: String,
    enabled: i64,
    position: String,
}

#[derive(Debug, sqlx::FromRow)]
struct HeaderTabItemRow {
    id: String,
    label: String,
    label_zh: Option<String>,
    label_en: Option<String>,
    asset_kind: Option<String>,
    enabled: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct SubNavItemRow {
    parent_tab_id: String,
    id: String,
    label: String,
    label_zh: Option<String>,
    label_en: Option<String>,
    route_key: String,
    enabled: i64,
}

pub(crate) async fn load_navigation_model_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> Result<NavigationModel, String> {
    let state = sqlx::query_as::<_, NavigationStateRow>(sql::GET_NAVIGATION_STATE)
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    let rail_rows = sqlx::query_as::<_, RailMenuItemRow>(sql::LIST_RAIL_MENU_ITEMS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let header_rows = sqlx::query_as::<_, HeaderTabItemRow>(sql::LIST_HEADER_TAB_ITEMS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let sub_nav_rows = sqlx::query_as::<_, SubNavItemRow>(sql::LIST_SUB_NAV_ITEMS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    Ok(NavigationModel {
        active_rail_id: state.active_rail_id,
        active_header_tab_id: state.active_header_tab_id,
        active_sub_nav_id: state.active_sub_nav_id,
        rail_items: rail_rows.into_iter().map(map_sqlx_rail_item).collect(),
        header_tabs: header_rows.into_iter().map(map_sqlx_header_tab).collect(),
        sub_nav_items: map_sqlx_sub_nav_items(sub_nav_rows),
    })
}

fn map_sqlx_rail_item(row: RailMenuItemRow) -> RailMenuItem {
    RailMenuItem {
        id: row.id,
        label: row.label,
        labels: localized_labels(row.label_zh, row.label_en),
        icon: row.icon,
        scope: row.scope,
        enabled: row.enabled == 1,
        position: row.position,
    }
}

fn map_sqlx_header_tab(row: HeaderTabItemRow) -> HeaderTabItem {
    HeaderTabItem {
        id: row.id,
        label: row.label,
        labels: localized_labels(row.label_zh, row.label_en),
        asset_kind: row.asset_kind,
        enabled: row.enabled == 1,
    }
}

fn map_sqlx_sub_nav_items(rows: Vec<SubNavItemRow>) -> BTreeMap<String, Vec<SubNavItem>> {
    let mut grouped = BTreeMap::new();
    for row in rows {
        let parent_tab_id = row.parent_tab_id;
        let item = SubNavItem {
            id: row.id,
            label: row.label,
            labels: localized_labels(row.label_zh, row.label_en),
            route_key: row.route_key,
            enabled: row.enabled == 1,
        };
        grouped
            .entry(parent_tab_id)
            .or_insert_with(Vec::new)
            .push(item);
    }
    grouped
}

fn enabled_value(enabled: bool) -> i64 {
    if enabled {
        1
    } else {
        0
    }
}

fn localized_label<'a>(
    labels: &'a Option<LocalizedNavigationLabels>,
    locale: &str,
) -> Option<&'a str> {
    let value = match (labels, locale) {
        (Some(labels), "zh") => labels.zh.as_deref(),
        (Some(labels), "en") => labels.en.as_deref(),
        _ => None,
    };
    value.and_then(non_empty_label)
}

fn localized_labels(zh: Option<String>, en: Option<String>) -> Option<LocalizedNavigationLabels> {
    let labels = LocalizedNavigationLabels {
        zh: zh.and_then(non_empty_label_string),
        en: en.and_then(non_empty_label_string),
    };
    if labels.zh.is_none() && labels.en.is_none() {
        None
    } else {
        Some(labels)
    }
}

fn non_empty_label(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn non_empty_label_string(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[cfg(test)]
mod tests {
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
}
