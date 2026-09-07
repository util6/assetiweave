use crate::backend::{
    app_settings::AppLocale,
    runtime::{AppError, AppResult},
};
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

pub(crate) const APP_SETTINGS_ID: &str = "global";

pub(crate) async fn load_app_settings_sqlx(pool: &SqlitePool) -> AppResult<Option<(u32, Value)>> {
    let row = sqlx::query_as::<_, (i64, String)>(
        "SELECT schema_version, settings_json
         FROM app_settings
         WHERE settings_id = ?1",
    )
    .bind(APP_SETTINGS_ID)
    .fetch_optional(pool)
    .await?;
    row.map(|(schema_version, settings_json)| {
        let schema_version = u32::try_from(schema_version).map_err(|_| {
            AppError::Storage("settings schema version is out of range".to_string())
        })?;
        let settings = super::codec::decode_json(&settings_json)?;
        Ok((schema_version, settings))
    })
    .transpose()
}

pub(crate) async fn save_app_settings_sqlx(
    pool: &SqlitePool,
    schema_version: u32,
    settings: &Value,
) -> AppResult<()> {
    let settings_json = super::codec::encode_json(settings)?;
    let mut query = QueryBuilder::<Sqlite>::new(
        "INSERT INTO app_settings (settings_id, schema_version, settings_json, updated_at) ",
    );
    query.push("VALUES (");
    query.push_bind(APP_SETTINGS_ID);
    query.push(", ");
    query.push_bind(i64::from(schema_version));
    query.push(", ");
    query.push_bind(settings_json);
    query.push(", datetime('now')) ");
    query.push(
        "ON CONFLICT(settings_id) DO UPDATE SET
            schema_version = excluded.schema_version,
            settings_json = CASE
                WHEN json_extract(app_settings.settings_json, '$.locale') IS NOT NULL
                     AND json_extract(excluded.settings_json, '$.locale') IS NULL
                THEN json_set(excluded.settings_json, '$.locale', json_extract(app_settings.settings_json, '$.locale'))
                ELSE excluded.settings_json
            END,
            updated_at = excluded.updated_at",
    );
    query.build().execute(pool).await?;
    Ok(())
}

pub(crate) async fn initialize_app_locale_sqlx(
    pool: &SqlitePool,
    locale: AppLocale,
) -> AppResult<Value> {
    sqlx::query(
        "UPDATE app_settings
         SET settings_json = json_set(settings_json, '$.locale', ?1),
             updated_at = datetime('now')
         WHERE settings_id = ?2
           AND json_extract(settings_json, '$.locale') IS NULL",
    )
    .bind(locale.as_str())
    .bind(APP_SETTINGS_ID)
    .execute(pool)
    .await?;

    let (_, stored) = load_app_settings_sqlx(pool)
        .await?
        .ok_or_else(|| AppError::Storage("settings not found".to_string()))?;
    Ok(stored)
}

#[cfg(test)]
mod tests {
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
}
