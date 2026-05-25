pub(crate) const INIT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    root_path TEXT NOT NULL,
    include_globs TEXT NOT NULL,
    exclude_globs TEXT NOT NULL,
    default_kind TEXT,
    enabled INTEGER NOT NULL,
    priority INTEGER NOT NULL,
    last_scanned_at TEXT,
    last_scan_status TEXT
);

CREATE TABLE IF NOT EXISTS assets (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    format TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    absolute_path TEXT NOT NULL,
    entry_file TEXT,
    description TEXT,
    content_hash TEXT,
    discovered_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS profiles (
    id TEXT PRIMARY KEY,
    payload TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS deployment_state (
    profile_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    target_path TEXT NOT NULL,
    strategy TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    deployed_at TEXT NOT NULL,
    managed_by TEXT NOT NULL,
    PRIMARY KEY (profile_id, asset_id, target_path)
);
"#;

pub(crate) const LATEST_SCAN_STATUS: &str =
    "SELECT last_scan_status FROM sources ORDER BY last_scanned_at DESC NULLS LAST LIMIT 1";

pub(crate) const LIST_SOURCES: &str = r#"
SELECT id, name, kind, root_path, include_globs, exclude_globs, default_kind,
       enabled, priority, last_scanned_at, last_scan_status
FROM sources
ORDER BY priority ASC, name ASC
"#;

pub(crate) const LIST_ASSETS: &str = r#"
SELECT id, source_id, name, kind, format, relative_path, absolute_path,
       entry_file, description, content_hash, discovered_at, updated_at
FROM assets
ORDER BY name ASC
"#;

pub(crate) const LIST_PROFILES: &str = "SELECT payload FROM profiles ORDER BY id ASC";

pub(crate) const COUNT_SOURCES: &str = "SELECT COUNT(*) FROM sources";
pub(crate) const COUNT_ASSETS: &str = "SELECT COUNT(*) FROM assets";
pub(crate) const COUNT_PROFILES: &str = "SELECT COUNT(*) FROM profiles";

pub(crate) const UPSERT_SOURCE: &str = r#"
INSERT INTO sources (
    id, name, kind, root_path, include_globs, exclude_globs, default_kind,
    enabled, priority, last_scanned_at, last_scan_status
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    kind = excluded.kind,
    root_path = excluded.root_path,
    include_globs = excluded.include_globs,
    exclude_globs = excluded.exclude_globs,
    default_kind = excluded.default_kind,
    enabled = excluded.enabled,
    priority = excluded.priority,
    last_scanned_at = excluded.last_scanned_at,
    last_scan_status = excluded.last_scan_status
"#;

pub(crate) const DELETE_SOURCE: &str = "DELETE FROM sources WHERE id = ?1";

pub(crate) const UPSERT_PROFILE: &str =
    "INSERT INTO profiles (id, payload) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET payload = excluded.payload";

pub(crate) const DELETE_ASSETS_BY_SOURCE: &str = "DELETE FROM assets WHERE source_id = ?1";

pub(crate) const INSERT_ASSET: &str = r#"
INSERT INTO assets (
    id, source_id, name, kind, format, relative_path, absolute_path,
    entry_file, description, content_hash, discovered_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
"#;

pub(crate) const UPSERT_DEPLOYMENT_STATE: &str = r#"
INSERT INTO deployment_state (
    profile_id, asset_id, target_path, strategy, source_hash, deployed_at, managed_by
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(profile_id, asset_id, target_path) DO UPDATE SET
    strategy = excluded.strategy,
    source_hash = excluded.source_hash,
    deployed_at = excluded.deployed_at,
    managed_by = excluded.managed_by
"#;

pub(crate) const GET_MANAGED_DEPLOYMENT: &str =
    "SELECT managed_by FROM deployment_state WHERE profile_id = ?1 AND asset_id = ?2 AND target_path = ?3";
