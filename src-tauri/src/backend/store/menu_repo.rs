use crate::backend::dto::{
    HeaderTabItem, LocalizedNavigationLabels, NavigationModel, RailMenuItem, SubNavItem,
};
#[cfg(test)]
use rusqlite::{params, Connection};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};
use std::collections::BTreeMap;

#[cfg(test)]
use super::codec::db_error;
use super::sql;

#[cfg(test)]
pub(crate) fn seed_navigation_model(
    conn: &Connection,
    model: &NavigationModel,
) -> Result<(), String> {
    save_navigation_model(conn, model)
}

pub(crate) async fn seed_navigation_model_sqlx(
    pool: &SqlitePool,
    model: &NavigationModel,
) -> Result<(), String> {
    save_navigation_model_sqlx(pool, model).await
}

#[cfg(test)]
pub(crate) fn ensure_navigation_model_items(
    conn: &Connection,
    defaults: &NavigationModel,
) -> Result<(), String> {
    let mut current = load_navigation_model(conn)?;
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
    save_navigation_model(conn, &current)
}

pub(crate) async fn ensure_navigation_model_items_sqlx(
    pool: &SqlitePool,
    defaults: &NavigationModel,
) -> Result<(), String> {
    let mut current = load_navigation_model_sqlx(pool).await?;
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
    save_navigation_model_sqlx(pool, &current).await
}

#[cfg(test)]
pub(crate) fn save_navigation_model(
    conn: &Connection,
    model: &NavigationModel,
) -> Result<(), String> {
    conn.execute(
        sql::UPSERT_NAVIGATION_STATE,
        params![
            model.active_rail_id,
            model.active_header_tab_id,
            model.active_sub_nav_id
        ],
    )
    .map_err(db_error)?;

    for (sort_order, item) in model.rail_items.iter().enumerate() {
        conn.execute(
            sql::UPSERT_RAIL_MENU_ITEM,
            params![
                item.id,
                item.label,
                localized_label(&item.labels, "zh"),
                localized_label(&item.labels, "en"),
                item.icon,
                item.scope,
                enabled_value(item.enabled),
                item.position,
                sort_order as i32
            ],
        )
        .map_err(db_error)?;
    }

    for (sort_order, tab) in model.header_tabs.iter().enumerate() {
        conn.execute(
            sql::UPSERT_HEADER_TAB_ITEM,
            params![
                tab.id,
                tab.label,
                localized_label(&tab.labels, "zh"),
                localized_label(&tab.labels, "en"),
                tab.asset_kind,
                enabled_value(tab.enabled),
                sort_order as i32
            ],
        )
        .map_err(db_error)?;
    }

    for (parent_tab_id, items) in &model.sub_nav_items {
        for (sort_order, item) in items.iter().enumerate() {
            conn.execute(
                sql::UPSERT_SUB_NAV_ITEM,
                params![
                    parent_tab_id,
                    item.id,
                    item.label,
                    localized_label(&item.labels, "zh"),
                    localized_label(&item.labels, "en"),
                    item.route_key,
                    enabled_value(item.enabled),
                    sort_order as i32
                ],
            )
            .map_err(db_error)?;
        }
    }

    Ok(())
}

pub(crate) async fn save_navigation_model_sqlx(
    pool: &SqlitePool,
    model: &NavigationModel,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(sql::UPSERT_NAVIGATION_STATE)
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

#[cfg(test)]
pub(crate) fn load_navigation_model(conn: &Connection) -> Result<NavigationModel, String> {
    let (active_rail_id, active_header_tab_id, active_sub_nav_id) = conn
        .query_row(sql::GET_NAVIGATION_STATE, [], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(db_error)?;

    Ok(NavigationModel {
        active_rail_id,
        active_header_tab_id,
        active_sub_nav_id,
        rail_items: load_rail_items(conn)?,
        header_tabs: load_header_tabs(conn)?,
        sub_nav_items: load_sub_nav_items(conn)?,
    })
}

pub(crate) async fn load_navigation_model_sqlx(
    pool: &SqlitePool,
) -> Result<NavigationModel, String> {
    let state = sqlx::query(sql::GET_NAVIGATION_STATE)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    let rail_rows = sqlx::query(sql::LIST_RAIL_MENU_ITEMS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let header_rows = sqlx::query(sql::LIST_HEADER_TAB_ITEMS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let sub_nav_rows = sqlx::query(sql::LIST_SUB_NAV_ITEMS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    Ok(NavigationModel {
        active_rail_id: state.try_get(0).map_err(|error| error.to_string())?,
        active_header_tab_id: state.try_get(1).map_err(|error| error.to_string())?,
        active_sub_nav_id: state.try_get(2).map_err(|error| error.to_string())?,
        rail_items: rail_rows
            .iter()
            .map(map_sqlx_rail_item)
            .collect::<Result<Vec<_>, _>>()?,
        header_tabs: header_rows
            .iter()
            .map(map_sqlx_header_tab)
            .collect::<Result<Vec<_>, _>>()?,
        sub_nav_items: map_sqlx_sub_nav_items(&sub_nav_rows)?,
    })
}

#[cfg(test)]
fn load_rail_items(conn: &Connection) -> Result<Vec<RailMenuItem>, String> {
    let mut stmt = conn.prepare(sql::LIST_RAIL_MENU_ITEMS).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(RailMenuItem {
                id: row.get(0)?,
                label: row.get(1)?,
                labels: localized_labels(row.get(2)?, row.get(3)?),
                icon: row.get(4)?,
                scope: row.get(5)?,
                enabled: row.get::<_, i64>(6)? == 1,
                position: row.get(7)?,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

#[cfg(test)]
fn load_header_tabs(conn: &Connection) -> Result<Vec<HeaderTabItem>, String> {
    let mut stmt = conn.prepare(sql::LIST_HEADER_TAB_ITEMS).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(HeaderTabItem {
                id: row.get(0)?,
                label: row.get(1)?,
                labels: localized_labels(row.get(2)?, row.get(3)?),
                asset_kind: row.get(4)?,
                enabled: row.get::<_, i64>(5)? == 1,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

#[cfg(test)]
fn load_sub_nav_items(conn: &Connection) -> Result<BTreeMap<String, Vec<SubNavItem>>, String> {
    let mut stmt = conn.prepare(sql::LIST_SUB_NAV_ITEMS).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SubNavItem {
                    id: row.get(1)?,
                    label: row.get(2)?,
                    labels: localized_labels(row.get(3)?, row.get(4)?),
                    route_key: row.get(5)?,
                    enabled: row.get::<_, i64>(6)? == 1,
                },
            ))
        })
        .map_err(db_error)?;

    let mut grouped = BTreeMap::new();
    for row in rows {
        let (parent_tab_id, item) = row.map_err(db_error)?;
        grouped
            .entry(parent_tab_id)
            .or_insert_with(Vec::new)
            .push(item);
    }
    Ok(grouped)
}

fn map_sqlx_rail_item(row: &SqliteRow) -> Result<RailMenuItem, String> {
    Ok(RailMenuItem {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        label: row.try_get(1).map_err(|error| error.to_string())?,
        labels: localized_labels(
            row.try_get(2).map_err(|error| error.to_string())?,
            row.try_get(3).map_err(|error| error.to_string())?,
        ),
        icon: row.try_get(4).map_err(|error| error.to_string())?,
        scope: row.try_get(5).map_err(|error| error.to_string())?,
        enabled: row
            .try_get::<i64, _>(6)
            .map_err(|error| error.to_string())?
            == 1,
        position: row.try_get(7).map_err(|error| error.to_string())?,
    })
}

fn map_sqlx_header_tab(row: &SqliteRow) -> Result<HeaderTabItem, String> {
    Ok(HeaderTabItem {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        label: row.try_get(1).map_err(|error| error.to_string())?,
        labels: localized_labels(
            row.try_get(2).map_err(|error| error.to_string())?,
            row.try_get(3).map_err(|error| error.to_string())?,
        ),
        asset_kind: row.try_get(4).map_err(|error| error.to_string())?,
        enabled: row
            .try_get::<i64, _>(5)
            .map_err(|error| error.to_string())?
            == 1,
    })
}

fn map_sqlx_sub_nav_items(rows: &[SqliteRow]) -> Result<BTreeMap<String, Vec<SubNavItem>>, String> {
    let mut grouped = BTreeMap::new();
    for row in rows {
        let parent_tab_id: String = row.try_get(0).map_err(|error| error.to_string())?;
        let item = SubNavItem {
            id: row.try_get(1).map_err(|error| error.to_string())?,
            label: row.try_get(2).map_err(|error| error.to_string())?,
            labels: localized_labels(
                row.try_get(3).map_err(|error| error.to_string())?,
                row.try_get(4).map_err(|error| error.to_string())?,
            ),
            route_key: row.try_get(5).map_err(|error| error.to_string())?,
            enabled: row
                .try_get::<i64, _>(6)
                .map_err(|error| error.to_string())?
                == 1,
        };
        grouped
            .entry(parent_tab_id)
            .or_insert_with(Vec::new)
            .push(item);
    }
    Ok(grouped)
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

    #[test]
    fn sqlx_navigation_model_round_trips_updates_and_localized_labels() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-navigation-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let mut model = crate::backend::defaults::default_navigation_model();
        model.active_rail_id = "settings".to_string();
        model.rail_items[0].labels = Some(LocalizedNavigationLabels {
            zh: Some("  资产  ".to_string()),
            en: Some("Assets".to_string()),
        });

        let loaded = database
            .block_on(async {
                save_navigation_model_sqlx(database.pool(), &model).await?;
                let mut loaded = load_navigation_model_sqlx(database.pool()).await?;
                loaded.active_sub_nav_id = "updated-sub-nav".to_string();
                save_navigation_model_sqlx(database.pool(), &loaded).await?;
                load_navigation_model_sqlx(database.pool()).await
            })
            .expect("round trip navigation model");

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
}
