use crate::types::{
    HeaderTabItem, LocalizedNavigationLabels, NavigationModel, RailMenuItem, SubNavItem,
};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

use super::{codec::db_error, sql};

pub(crate) fn seed_navigation_model(
    conn: &Connection,
    model: &NavigationModel,
) -> Result<(), String> {
    save_navigation_model(conn, model)
}

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
