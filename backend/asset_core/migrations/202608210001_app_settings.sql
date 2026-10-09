CREATE TABLE IF NOT EXISTS app_settings (
    settings_id TEXT PRIMARY KEY NOT NULL,
    schema_version INTEGER NOT NULL,
    settings_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
