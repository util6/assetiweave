use crate::backend::{
    dto::{HeaderTabItem, LocalizedNavigationLabels, NavigationModel, RailMenuItem, SubNavItem},
    runtime::AppResult,
};
use sqlx::SqlitePool;
use std::collections::BTreeMap;

use super::sql;

pub(crate) async fn seed_navigation_model_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    model: &NavigationModel,
) -> AppResult<()> {
    save_navigation_model_sqlx(pool, tenant_id, model).await
}

pub(crate) async fn ensure_navigation_model_items_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    defaults: &NavigationModel,
) -> AppResult<()> {
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
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(sql::UPSERT_NAVIGATION_STATE)
        .bind(tenant_id)
        .bind(&model.active_rail_id)
        .bind(&model.active_header_tab_id)
        .bind(&model.active_sub_nav_id)
        .execute(&mut *tx)
        .await?;

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
            .await?;
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
            .await?;
    }

    for (parent_tab_id, items) in &model.sub_nav_items {
        sqlx::query("DELETE FROM sub_nav_items WHERE parent_tab_id = ?1")
            .bind(parent_tab_id)
            .execute(&mut *tx)
            .await?;
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
                .await?;
        }
    }

    tx.commit().await?;
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
) -> AppResult<NavigationModel> {
    let state = sqlx::query_as::<_, NavigationStateRow>(sql::GET_NAVIGATION_STATE)
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;
    let rail_rows = sqlx::query_as::<_, RailMenuItemRow>(sql::LIST_RAIL_MENU_ITEMS)
        .fetch_all(pool)
        .await?;
    let header_rows = sqlx::query_as::<_, HeaderTabItemRow>(sql::LIST_HEADER_TAB_ITEMS)
        .fetch_all(pool)
        .await?;
    let sub_nav_rows = sqlx::query_as::<_, SubNavItemRow>(sql::LIST_SUB_NAV_ITEMS)
        .fetch_all(pool)
        .await?;

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
#[path = "menu_repo_tests.rs"]
mod tests;
