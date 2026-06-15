use crate::models::TargetProfile;
use crate::types::{AppResult, AppShortcut, AppShortcutIconSvg};
use rusqlite::{params, Connection};

use super::{
    codec::{db_error, decode_json, encode_enum, encode_json, to_sql_error},
    sql,
};

pub(crate) fn seed_app_shortcuts(
    conn: &Connection,
    shortcuts: &[(&str, &str, &str, bool)],
) -> AppResult<()> {
    for (sort_order, (profile_id, display_icon, accent_color, enabled)) in
        shortcuts.iter().enumerate()
    {
        conn.execute(
            sql::UPSERT_APP_SHORTCUT,
            params![
                profile_id,
                display_icon,
                Option::<String>::None,
                accent_color,
                if *enabled { 1 } else { 0 },
                sort_order as i32
            ],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

pub(crate) fn load_app_shortcuts(conn: &Connection) -> AppResult<Vec<AppShortcut>> {
    let mut stmt = conn.prepare(sql::LIST_APP_SHORTCUTS).map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            let profile_id = row.get::<_, String>(0)?;
            let display_icon = row.get::<_, String>(1)?;
            let icon_svg = decode_icon_svg(row.get::<_, Option<String>>(2)?)?;
            let accent_color = row.get::<_, String>(3)?;
            let enabled = row.get::<_, i64>(4)? == 1;
            let profile: TargetProfile =
                decode_json(row.get::<_, String>(5)?).map_err(to_sql_error)?;

            Ok(AppShortcut {
                profile_id,
                profile_name: profile.name,
                app_kind: encode_enum(profile.app_kind).map_err(to_sql_error)?,
                display_icon,
                icon_svg,
                accent_color,
                enabled,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn load_app_shortcut_settings(conn: &Connection) -> AppResult<Vec<AppShortcut>> {
    let mut stmt = conn
        .prepare(sql::LIST_APP_SHORTCUT_SETTINGS)
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| {
            let profile_id = row.get::<_, String>(0)?;
            let profile: TargetProfile =
                decode_json(row.get::<_, String>(1)?).map_err(to_sql_error)?;
            let display_icon = row
                .get::<_, Option<String>>(2)?
                .unwrap_or_else(|| profile.name.chars().next().unwrap_or('?').to_string());
            let icon_svg = decode_icon_svg(row.get::<_, Option<String>>(3)?)?;
            let accent_color = row
                .get::<_, Option<String>>(4)?
                .unwrap_or_else(|| "#8c909f".to_string());
            let enabled = row.get::<_, i64>(5)? == 1;

            Ok(AppShortcut {
                profile_id,
                profile_name: profile.name,
                app_kind: encode_enum(profile.app_kind).map_err(to_sql_error)?,
                display_icon,
                icon_svg,
                accent_color,
                enabled,
            })
        })
        .map_err(db_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) fn save_app_shortcuts(conn: &Connection, shortcuts: &[AppShortcut]) -> AppResult<()> {
    for (sort_order, shortcut) in shortcuts.iter().enumerate() {
        let icon_svg = shortcut.icon_svg.as_ref().map(encode_json).transpose()?;
        conn.execute(
            sql::UPSERT_APP_SHORTCUT,
            params![
                shortcut.profile_id,
                shortcut.display_icon,
                icon_svg,
                shortcut.accent_color,
                if shortcut.enabled { 1 } else { 0 },
                sort_order as i32
            ],
        )
        .map_err(db_error)?;
    }
    Ok(())
}

fn decode_icon_svg(value: Option<String>) -> Result<Option<AppShortcutIconSvg>, rusqlite::Error> {
    value.map(decode_json).transpose().map_err(to_sql_error)
}
