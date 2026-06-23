use crate::backend::dto::{AppResult, AppShortcut, AppShortcutIconSvg};
use crate::backend::models::TargetProfile;
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

use super::{
    codec::{decode_json, encode_enum, encode_json},
    sql,
};

pub(crate) async fn seed_app_shortcuts_sqlx(
    pool: &SqlitePool,
    shortcuts: &[(&str, &str, &str, bool)],
) -> AppResult<()> {
    for (sort_order, (profile_id, display_icon, accent_color, enabled)) in
        shortcuts.iter().enumerate()
    {
        sqlx::query(sql::UPSERT_APP_SHORTCUT)
            .bind(profile_id)
            .bind(display_icon)
            .bind(Option::<String>::None)
            .bind(accent_color)
            .bind(if *enabled { 1 } else { 0 })
            .bind(sort_order as i32)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) async fn load_app_shortcuts_sqlx(pool: &SqlitePool) -> AppResult<Vec<AppShortcut>> {
    let rows = sqlx::query(sql::LIST_APP_SHORTCUTS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_app_shortcut).collect()
}

pub(crate) async fn load_app_shortcut_settings_sqlx(
    pool: &SqlitePool,
) -> AppResult<Vec<AppShortcut>> {
    let rows = sqlx::query(sql::LIST_APP_SHORTCUT_SETTINGS)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_app_shortcut_setting).collect()
}

pub(crate) async fn save_app_shortcuts_sqlx(
    pool: &SqlitePool,
    shortcuts: &[AppShortcut],
) -> AppResult<()> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    for (sort_order, shortcut) in shortcuts.iter().enumerate() {
        let icon_svg = shortcut.icon_svg.as_ref().map(encode_json).transpose()?;
        sqlx::query(sql::UPSERT_APP_SHORTCUT)
            .bind(&shortcut.profile_id)
            .bind(&shortcut.display_icon)
            .bind(icon_svg)
            .bind(&shortcut.accent_color)
            .bind(shortcut.enabled)
            .bind(sort_order as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

fn decode_icon_svg_sqlx(value: Option<String>) -> AppResult<Option<AppShortcutIconSvg>> {
    value.map(decode_json).transpose()
}

fn map_sqlx_app_shortcut(row: &SqliteRow) -> AppResult<AppShortcut> {
    let profile: TargetProfile = decode_json(
        row.try_get::<String, _>(5)
            .map_err(|error| error.to_string())?,
    )?;
    Ok(AppShortcut {
        profile_id: row.try_get(0).map_err(|error| error.to_string())?,
        profile_name: profile.name,
        app_kind: encode_enum(profile.app_kind)?,
        display_icon: row.try_get(1).map_err(|error| error.to_string())?,
        icon_svg: decode_icon_svg_sqlx(row.try_get(2).map_err(|error| error.to_string())?)?,
        accent_color: row.try_get(3).map_err(|error| error.to_string())?,
        enabled: row
            .try_get::<i64, _>(4)
            .map_err(|error| error.to_string())?
            == 1,
    })
}

fn map_sqlx_app_shortcut_setting(row: &SqliteRow) -> AppResult<AppShortcut> {
    let profile: TargetProfile = decode_json(
        row.try_get::<String, _>(1)
            .map_err(|error| error.to_string())?,
    )?;
    let profile_name = profile.name;
    Ok(AppShortcut {
        profile_id: row.try_get(0).map_err(|error| error.to_string())?,
        app_kind: encode_enum(profile.app_kind)?,
        display_icon: row
            .try_get::<Option<String>, _>(2)
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| profile_name.chars().next().unwrap_or('?').to_string()),
        icon_svg: decode_icon_svg_sqlx(row.try_get(3).map_err(|error| error.to_string())?)?,
        accent_color: row
            .try_get::<Option<String>, _>(4)
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| "#8c909f".to_string()),
        enabled: row
            .try_get::<i64, _>(5)
            .map_err(|error| error.to_string())?
            == 1,
        profile_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dto::{AppShortcutIconPath, AppShortcutIconSvg};
    use uuid::Uuid;

    #[test]
    fn sqlx_app_shortcuts_round_trip_settings_and_enabled_list() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-shortcuts-sqlx-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let profiles = crate::backend::defaults::default_profiles()
            .into_iter()
            .take(2)
            .collect::<Vec<_>>();

        let (settings, enabled) = database
            .block_on(async {
                for profile in &profiles {
                    crate::backend::store::upsert_profile_sqlx(database.pool(), profile).await?;
                }
                let mut settings = load_app_shortcut_settings_sqlx(database.pool()).await?;
                settings[0].display_icon = "X".to_string();
                settings[0].accent_color = "#123456".to_string();
                settings[0].enabled = false;
                settings[0].icon_svg = Some(AppShortcutIconSvg {
                    paths: vec![AppShortcutIconPath {
                        clip_rule: None,
                        d: "M0 0h1v1z".to_string(),
                        fill_rule: Some("evenodd".to_string()),
                    }],
                    view_box: Some("0 0 1 1".to_string()),
                });
                save_app_shortcuts_sqlx(database.pool(), &settings).await?;
                AppResult::Ok((
                    load_app_shortcut_settings_sqlx(database.pool()).await?,
                    load_app_shortcuts_sqlx(database.pool()).await?,
                ))
            })
            .expect("round trip SQLx app shortcuts");

        assert_eq!(settings.len(), 2);
        assert_eq!(settings[0].display_icon, "X");
        assert_eq!(settings[0].accent_color, "#123456");
        assert!(!settings[0].enabled);
        assert_eq!(
            settings[0]
                .icon_svg
                .as_ref()
                .and_then(|icon| icon.view_box.as_deref()),
            Some("0 0 1 1")
        );
        assert_eq!(enabled.len(), 1);
        assert_ne!(enabled[0].profile_id, settings[0].profile_id);

        drop(database);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
