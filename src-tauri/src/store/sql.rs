pub(crate) const INIT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    root_path TEXT NOT NULL,
    scanner_kind TEXT NOT NULL DEFAULT 'mixed',
    source_origin TEXT NOT NULL DEFAULT 'local_folder',
    repo_root TEXT,
    scan_root TEXT NOT NULL DEFAULT '',
    origin_app_kind TEXT,
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
    label_zh TEXT,
    label_en TEXT,
    icon TEXT NOT NULL,
    scope TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    position TEXT NOT NULL,
    sort_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS header_tab_items (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    label_zh TEXT,
    label_en TEXT,
    asset_kind TEXT,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sub_nav_items (
    parent_tab_id TEXT NOT NULL,
    id TEXT NOT NULL,
    label TEXT NOT NULL,
    label_zh TEXT,
    label_en TEXT,
    route_key TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    PRIMARY KEY (parent_tab_id, id)
);

CREATE TABLE IF NOT EXISTS app_shortcut_items (
    profile_id TEXT PRIMARY KEY,
    display_icon TEXT NOT NULL,
    icon_svg TEXT,
    accent_color TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS asset_mounts (
    asset_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    strategy TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (asset_id, profile_id)
);
"#;

pub(crate) const ADD_SOURCE_SCANNER_KIND: &str =
    "ALTER TABLE sources ADD COLUMN scanner_kind TEXT NOT NULL DEFAULT 'mixed'";
pub(crate) const ADD_SOURCE_ORIGIN: &str =
    "ALTER TABLE sources ADD COLUMN source_origin TEXT NOT NULL DEFAULT 'local_folder'";
pub(crate) const ADD_SOURCE_REPO_ROOT: &str = "ALTER TABLE sources ADD COLUMN repo_root TEXT";
pub(crate) const ADD_SOURCE_SCAN_ROOT: &str =
    "ALTER TABLE sources ADD COLUMN scan_root TEXT NOT NULL DEFAULT ''";
pub(crate) const ADD_SOURCE_ORIGIN_APP_KIND: &str =
    "ALTER TABLE sources ADD COLUMN origin_app_kind TEXT";
pub(crate) const ADD_RAIL_MENU_LABEL_ZH: &str =
    "ALTER TABLE rail_menu_items ADD COLUMN label_zh TEXT";
pub(crate) const ADD_RAIL_MENU_LABEL_EN: &str =
    "ALTER TABLE rail_menu_items ADD COLUMN label_en TEXT";
pub(crate) const ADD_HEADER_TAB_LABEL_ZH: &str =
    "ALTER TABLE header_tab_items ADD COLUMN label_zh TEXT";
pub(crate) const ADD_HEADER_TAB_LABEL_EN: &str =
    "ALTER TABLE header_tab_items ADD COLUMN label_en TEXT";
pub(crate) const ADD_SUB_NAV_LABEL_ZH: &str =
    "ALTER TABLE sub_nav_items ADD COLUMN label_zh TEXT";
pub(crate) const ADD_SUB_NAV_LABEL_EN: &str =
    "ALTER TABLE sub_nav_items ADD COLUMN label_en TEXT";
pub(crate) const ADD_APP_SHORTCUT_ICON_SVG: &str =
    "ALTER TABLE app_shortcut_items ADD COLUMN icon_svg TEXT";

pub(crate) const MIGRATE_DEPLOYMENT_STATE_STRATEGY_NAMES: &str = r#"
UPDATE deployment_state
SET strategy = CASE strategy
    WHEN 'symlink' THEN 'symlink_to_source'
    WHEN 'copy' THEN 'copy_to_target'
    ELSE strategy
END
WHERE strategy IN ('symlink', 'copy')
"#;

pub(crate) const MIGRATE_ASSET_MOUNT_STRATEGY_NAMES: &str = r#"
UPDATE asset_mounts
SET strategy = CASE strategy
    WHEN 'symlink' THEN 'symlink_to_source'
    WHEN 'copy' THEN 'copy_to_target'
    ELSE strategy
END
WHERE strategy IN ('symlink', 'copy')
"#;

pub(crate) const LATEST_SCAN_STATUS: &str =
    "SELECT last_scan_status FROM sources ORDER BY last_scanned_at DESC NULLS LAST LIMIT 1";

pub(crate) const LIST_SOURCES: &str = r#"
SELECT id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
       origin_app_kind, include_globs, exclude_globs, default_kind, enabled, priority,
       last_scanned_at, last_scan_status
FROM sources
ORDER BY priority ASC, name ASC
"#;

pub(crate) const LIST_SKILL_SOURCES: &str = r#"
SELECT id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
       origin_app_kind, include_globs, exclude_globs, default_kind, enabled, priority,
       last_scanned_at, last_scan_status
FROM sources
WHERE scanner_kind = 'skill'
ORDER BY priority ASC, name ASC
"#;

pub(crate) const LIST_ASSETS: &str = r#"
SELECT id, source_id, name, kind, format, relative_path, absolute_path,
       entry_file, description, content_hash, discovered_at, updated_at
FROM assets
ORDER BY name ASC
"#;

pub(crate) const LIST_ASSETS_BY_KIND: &str = r#"
SELECT id, source_id, name, kind, format, relative_path, absolute_path,
       entry_file, description, content_hash, discovered_at, updated_at
FROM assets
WHERE kind = ?1
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
SELECT id, label, label_zh, label_en, icon, scope, enabled, position
FROM rail_menu_items
ORDER BY sort_order ASC, id ASC
"#;

pub(crate) const LIST_HEADER_TAB_ITEMS: &str = r#"
SELECT id, label, label_zh, label_en, asset_kind, enabled
FROM header_tab_items
ORDER BY sort_order ASC, id ASC
"#;

pub(crate) const LIST_SUB_NAV_ITEMS: &str = r#"
SELECT parent_tab_id, id, label, label_zh, label_en, route_key, enabled
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
INSERT INTO rail_menu_items (id, label, label_zh, label_en, icon, scope, enabled, position, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
ON CONFLICT(id) DO UPDATE SET
    label = excluded.label,
    label_zh = excluded.label_zh,
    label_en = excluded.label_en,
    icon = excluded.icon,
    scope = excluded.scope,
    enabled = excluded.enabled,
    position = excluded.position,
    sort_order = excluded.sort_order
"#;

pub(crate) const UPSERT_HEADER_TAB_ITEM: &str = r#"
INSERT INTO header_tab_items (id, label, label_zh, label_en, asset_kind, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(id) DO UPDATE SET
    label = excluded.label,
    label_zh = excluded.label_zh,
    label_en = excluded.label_en,
    asset_kind = excluded.asset_kind,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

pub(crate) const UPSERT_SUB_NAV_ITEM: &str = r#"
INSERT INTO sub_nav_items (parent_tab_id, id, label, label_zh, label_en, route_key, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(parent_tab_id, id) DO UPDATE SET
    label = excluded.label,
    label_zh = excluded.label_zh,
    label_en = excluded.label_en,
    route_key = excluded.route_key,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

pub(crate) const LIST_APP_SHORTCUTS: &str = r#"
SELECT shortcut.profile_id, shortcut.display_icon, shortcut.icon_svg, shortcut.accent_color,
       shortcut.enabled, profile.payload
FROM app_shortcut_items shortcut
JOIN profiles profile ON profile.id = shortcut.profile_id
WHERE shortcut.enabled = 1
ORDER BY shortcut.sort_order ASC, shortcut.profile_id ASC
"#;

pub(crate) const LIST_APP_SHORTCUT_SETTINGS: &str = r#"
SELECT profile.id, profile.payload, shortcut.display_icon, shortcut.icon_svg, shortcut.accent_color,
       COALESCE(shortcut.enabled, 1) AS enabled,
       COALESCE(shortcut.sort_order, 9999) AS sort_order
FROM profiles profile
LEFT JOIN app_shortcut_items shortcut ON shortcut.profile_id = profile.id
ORDER BY sort_order ASC, profile.id ASC
"#;

pub(crate) const UPSERT_APP_SHORTCUT: &str = r#"
INSERT INTO app_shortcut_items (profile_id, display_icon, icon_svg, accent_color, enabled, sort_order)
VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(profile_id) DO UPDATE SET
    display_icon = excluded.display_icon,
    icon_svg = excluded.icon_svg,
    accent_color = excluded.accent_color,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order
"#;

pub(crate) const LIST_ASSET_MOUNTS: &str = r#"
SELECT asset_id, profile_id, enabled, strategy, created_at, updated_at
FROM asset_mounts
WHERE (?1 IS NULL OR asset_id = ?1)
ORDER BY asset_id ASC, profile_id ASC
"#;

pub(crate) const LIST_ENABLED_ASSET_MOUNTS: &str = r#"
SELECT asset_id, profile_id, enabled, strategy, created_at, updated_at
FROM asset_mounts
WHERE enabled = 1 AND (?1 IS NULL OR profile_id = ?1)
ORDER BY profile_id ASC, asset_id ASC
"#;

pub(crate) const DELETE_ORPHAN_ASSET_MOUNTS: &str = r#"
DELETE FROM asset_mounts
WHERE NOT EXISTS (
    SELECT 1 FROM assets WHERE assets.id = asset_mounts.asset_id
)
"#;

pub(crate) const GET_ASSET_MOUNT: &str = r#"
SELECT asset_id, profile_id, enabled, strategy, created_at, updated_at
FROM asset_mounts
WHERE asset_id = ?1 AND profile_id = ?2
"#;

pub(crate) const UPSERT_ASSET_MOUNT: &str = r#"
INSERT INTO asset_mounts (
    asset_id, profile_id, enabled, strategy, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(asset_id, profile_id) DO UPDATE SET
    enabled = excluded.enabled,
    strategy = excluded.strategy,
    updated_at = excluded.updated_at
"#;

pub(crate) const UPSERT_SOURCE: &str = r#"
INSERT INTO sources (
    id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
    origin_app_kind, include_globs, exclude_globs, default_kind, enabled, priority,
    last_scanned_at, last_scan_status
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    kind = excluded.kind,
    root_path = excluded.root_path,
    scanner_kind = excluded.scanner_kind,
    source_origin = excluded.source_origin,
    repo_root = excluded.repo_root,
    scan_root = excluded.scan_root,
    origin_app_kind = excluded.origin_app_kind,
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

pub(crate) const DELETE_DEPLOYMENT_STATE: &str =
    "DELETE FROM deployment_state WHERE profile_id = ?1 AND asset_id = ?2 AND target_path = ?3";

pub(crate) const DELETE_ORPHAN_DEPLOYMENT_STATE: &str = r#"
DELETE FROM deployment_state
WHERE NOT EXISTS (
    SELECT 1 FROM assets WHERE assets.id = deployment_state.asset_id
)
"#;
