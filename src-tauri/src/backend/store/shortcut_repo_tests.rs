use super::*;
use crate::backend::dto::{AppShortcutIconPath, AppShortcutIconSvg};
use uuid::Uuid;

#[tokio::test]
async fn sqlx_app_shortcuts_round_trip_settings_and_enabled_list() {
    let db_path = std::env::temp_dir().join(format!(
        "assetiweave-shortcuts-sqlx-{}.sqlite",
        Uuid::new_v4()
    ));
    let database = crate::backend::store::Database::open_async(&db_path)
        .await
        .expect("open database");
    let catalog = crate::backend::target_catalog::TargetCatalog::builtin_for_tests()
        .expect("builtin target descriptors");
    let profiles = crate::backend::defaults::default_profiles_from_catalog(&catalog)
        .into_iter()
        .take(2)
        .collect::<Vec<_>>();

    for profile in &profiles {
        crate::backend::store::upsert_profile_sqlx(database.pool(), "default", profile)
            .await
            .expect("upsert profile");
    }
    let mut settings = load_app_shortcut_settings_sqlx(database.pool(), "default")
        .await
        .expect("load settings");
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
    save_app_shortcuts_sqlx(database.pool(), "default", &settings)
        .await
        .expect("save shortcuts");
    let settings = load_app_shortcut_settings_sqlx(database.pool(), "default")
        .await
        .expect("reload settings");
    let enabled = load_app_shortcuts_sqlx(database.pool(), "default")
        .await
        .expect("reload enabled shortcuts");

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
