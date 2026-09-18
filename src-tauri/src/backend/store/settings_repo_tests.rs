use super::*;

#[tokio::test]
async fn locale_first_writer_wins_and_unrelated_save_preserves_it() {
    use crate::backend::app_settings::AppLocale;
    use serde_json::json;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE app_settings (settings_id TEXT PRIMARY KEY NOT NULL, schema_version INTEGER NOT NULL, settings_json TEXT NOT NULL, updated_at TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    save_app_settings_sqlx(&pool, 4, &json!({"theme":"sunlight"}))
        .await
        .unwrap();
    initialize_app_locale_sqlx(&pool, AppLocale::En)
        .await
        .unwrap();
    let second = initialize_app_locale_sqlx(&pool, AppLocale::Zh)
        .await
        .unwrap();
    assert_eq!(second["locale"], "en");
    save_app_settings_sqlx(&pool, 4, &json!({"theme":"sunlight", "locale":null}))
        .await
        .unwrap();
    let (_, stored) = load_app_settings_sqlx(&pool).await.unwrap().unwrap();
    assert_eq!(stored["locale"], "en");
    assert_eq!(stored["theme"], "sunlight");
}

#[tokio::test]
async fn concurrent_locale_initialization_first_wins_and_explicit_save_updates() {
    use crate::backend::app_settings::AppLocale;
    use serde_json::json;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query("CREATE TABLE app_settings (settings_id TEXT PRIMARY KEY NOT NULL, schema_version INTEGER NOT NULL, settings_json TEXT NOT NULL, updated_at TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    save_app_settings_sqlx(&pool, 4, &json!({"theme":"sunlight"}))
        .await
        .unwrap();

    let (res_en, res_zh) = tokio::join!(
        initialize_app_locale_sqlx(&pool, AppLocale::En),
        initialize_app_locale_sqlx(&pool, AppLocale::Zh),
    );
    let val_en = res_en.unwrap();
    let val_zh = res_zh.unwrap();
    assert_eq!(val_en["locale"], val_zh["locale"]);
    let final_locale = val_en["locale"].as_str().unwrap().to_string();
    assert!(final_locale == "en" || final_locale == "zh");

    // Subsequent reverse candidate does not change it:
    let reverse = if final_locale == "en" {
        AppLocale::Zh
    } else {
        AppLocale::En
    };
    let after_reverse = initialize_app_locale_sqlx(&pool, reverse).await.unwrap();
    assert_eq!(after_reverse["locale"], final_locale);

    // Explicit save with new locale DOES change it:
    save_app_settings_sqlx(
        &pool,
        4,
        &json!({"theme":"sunlight", "locale": reverse.as_str()}),
    )
    .await
    .unwrap();
    let (_, stored) = load_app_settings_sqlx(&pool).await.unwrap().unwrap();
    assert_eq!(stored["locale"], reverse.as_str());
}
