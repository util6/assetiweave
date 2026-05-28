use crate::types::{HeaderTabItem, NavigationModel, RailMenuItem, SubNavItem};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

use super::{codec::db_error, sql};

pub(crate) fn seed_navigation_model(
    conn: &Connection,
    model: &NavigationModel,
) -> Result<(), String> {
    save_navigation_model(conn, model)
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
                icon: row.get(2)?,
                scope: row.get(3)?,
                enabled: row.get::<_, i64>(4)? == 1,
                position: row.get(5)?,
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
                asset_kind: row.get(2)?,
                enabled: row.get::<_, i64>(3)? == 1,
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
                    route_key: row.get(3)?,
                    enabled: row.get::<_, i64>(4)? == 1,
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
