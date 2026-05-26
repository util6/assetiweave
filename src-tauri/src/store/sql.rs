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

CREATE TABLE IF NOT EXISTS navigation_state (
    id TEXT PRIMARY KEY,
    active_rail_id TEXT NOT NULL,
    active_header_tab_id TEXT NOT NULL,
    active_sub_nav_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS rail_menu_items (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    icon TEXT NOT NULL,
    scope TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    position TEXT NOT NULL,
    sort_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS header_tab_items (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    asset_kind TEXT,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sub_nav_items (
    parent_tab_id TEXT NOT NULL,
    id TEXT NOT NULL,
    label TEXT NOT NULL,
    route_key TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    PRIMARY KEY (parent_tab_id, id)
);

CREATE TABLE IF NOT EXISTS app_shortcut_items (
    profile_id TEXT PRIMARY KEY,
    display_icon TEXT NOT NULL,
    accent_color TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL
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
pub(crate) const COUNT_NAVIGATION_STATE: &str = "SELECT COUNT(*) FROM navigation_state";
pub(crate) const COUNT_APP_SHORTCUTS: &str = "SELECT COUNT(*) FROM app_shortcut_items";

pub(crate) const GET_NAVIGATION_STATE: &str = r#"
SELECT active_rail_id, active_header_tab_id, active_sub_nav_id
FROM navigation_state
WHERE id = 'default'
"#;

pub(crate) const LIST_RAIL_MENU_ITEMS: &str = r#"
SELECT id, label, icon, scope, enabled, position
FROM rail_menu_items
ORDER BY sort_order ASC, id ASC
"#;

pub(crate) const LIST_HEADER_TAB_ITEMS: &str = r#"
SELECT id, label, asset_kind, enabled
FROM header_tab_items
ORDER BY sort_order ASC, id ASC
"#;

pub(crate) const LIST_SUB_NAV_ITEMS: &str = r#"
SELECT parent_tab_id, id, label, route_key, enabled
FROM sub_nav_items
ORDER BY parent_tab_id ASC, sort_order ASC, id ASC
"#;

pub(crate) const UPSERT_NAVIGATION_STATE: &str = r#"
INSERT INTO navigation_state (id, active_rail_id, active_header_tab_id, active_sub_nav_id)
VALUES ('default', ?1, ?2, ?3)
ON CONFLICT(id) DO UPDATE SET
    active_rail_id = excluded.active_rail_id,
    active_header_tab_id = excluded.active_header_tab_id,
    active_sub_nav_id = excluded.active_sub_nav_id
"#;

pub(crate) const UPSERT_RAIL_MENU_ITEM: &str = r#"
INSERT INTO rail_menu_items (id, label, icon, scope, enabled, position, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(id) DO UPDATE SET
    label = excluded.label,
    icon = excluded.icon,
    scope = excluded.scope,
    enabled = excluded.enabled,
    position = excluded.position,
    sort_order = excluded.sort_order
"#;

pub(crate) const UPSERT_HEADER_TAB_ITEM: &str = r#"
INSERT INTO header_tab_items (id, label, asset_kind, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(id) DO UPDATE SET
    label = excluded.label,
    asset_kind = excluded.asset_kind,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

pub(crate) const UPSERT_SUB_NAV_ITEM: &str = r#"
INSERT INTO sub_nav_items (parent_tab_id, id, label, route_key, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(parent_tab_id, id) DO UPDATE SET
    label = excluded.label,
    route_key = excluded.route_key,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

pub(crate) const LIST_APP_SHORTCUTS: &str = r#"
SELECT shortcut.profile_id, shortcut.display_icon, shortcut.accent_color,
       shortcut.enabled, profile.payload
FROM app_shortcut_items shortcut
JOIN profiles profile ON profile.id = shortcut.profile_id
WHERE shortcut.enabled = 1
ORDER BY shortcut.sort_order ASC, shortcut.profile_id ASC
"#;

pub(crate) const UPSERT_APP_SHORTCUT: &str = r#"
INSERT INTO app_shortcut_items (profile_id, display_icon, accent_color, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(profile_id) DO UPDATE SET
    display_icon = excluded.display_icon,
    accent_color = excluded.accent_color,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

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
