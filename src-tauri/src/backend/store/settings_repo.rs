use crate::backend::compat::LegacyResult;
use serde_json::Value;
use sqlx::{QueryBuilder, Sqlite, SqlitePool};

pub(crate) const APP_SETTINGS_ID: &str = "global";

pub(crate) async fn load_app_settings_sqlx(
    pool: &SqlitePool,
) -> LegacyResult<Option<(u32, Value)>> {
    let row = sqlx::query_as::<_, (i64, String)>(
        "SELECT schema_version, settings_json
         FROM app_settings
         WHERE settings_id = ?1",
    )
    .bind(APP_SETTINGS_ID)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    row.map(|(schema_version, settings_json)| {
        let schema_version = u32::try_from(schema_version)
            .map_err(|_| "settings schema version is out of range".to_string())?;
        let settings = serde_json::from_str(&settings_json).map_err(|error| error.to_string())?;
        Ok((schema_version, settings))
    })
    .transpose()
}

pub(crate) async fn save_app_settings_sqlx(
    pool: &SqlitePool,
    schema_version: u32,
    settings: &Value,
) -> LegacyResult<()> {
    let settings_json = serde_json::to_string(settings).map_err(|error| error.to_string())?;
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
            settings_json = excluded.settings_json,
            updated_at = excluded.updated_at",
    );
    query
        .build()
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
