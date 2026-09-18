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
#[path = "settings_repo_tests.rs"]
mod tests;
