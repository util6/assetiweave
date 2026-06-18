#[cfg(test)]
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

CREATE TABLE IF NOT EXISTS asset_mount_observations (
    asset_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    target_dir TEXT NOT NULL,
    target_path TEXT NOT NULL,
    state TEXT NOT NULL,
    linked_source TEXT,
    observed_at TEXT NOT NULL,
    PRIMARY KEY (asset_id, profile_id)
);

CREATE TABLE IF NOT EXISTS asset_groups (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    color TEXT NOT NULL,
    asset_kind TEXT NOT NULL,
    display_icon TEXT,
    icon_svg TEXT,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    rules_payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS asset_group_members (
    group_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (group_id, asset_id)
);

CREATE TABLE IF NOT EXISTS skill_remote_sources (
    asset_id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    source_url TEXT NOT NULL,
    repo_url TEXT NOT NULL,
    branch TEXT NOT NULL,
    path TEXT,
    acquired_at TEXT NOT NULL,
    acquired_tree_sha TEXT,
    local_content_hash TEXT,
    last_checked_at TEXT,
    latest_tree_sha TEXT,
    status TEXT NOT NULL,
    message TEXT
);

CREATE TABLE IF NOT EXISTS conversation_adapters (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    version TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    manifest_path TEXT,
    executable_path TEXT,
    content_hash TEXT,
    trusted_hash TEXT,
    trust_state TEXT NOT NULL,
    protocol_version INTEGER,
    capabilities TEXT NOT NULL,
    input_kinds TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS conversation_sources (
    id TEXT PRIMARY KEY,
    adapter_id TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    location TEXT NOT NULL,
    config_json TEXT,
    enabled INTEGER NOT NULL,
    last_synced_at TEXT,
    last_sync_status TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS conversation_sessions (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title TEXT NOT NULL,
    project_path TEXT,
    started_at TEXT,
    updated_at TEXT,
    source_locator TEXT,
    source_fingerprint TEXT,
    missing INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    UNIQUE(source_id, external_id)
);

CREATE TABLE IF NOT EXISTS conversation_turns (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    user_text TEXT NOT NULL,
    title TEXT,
    started_at TEXT,
    ended_at TEXT,
    fingerprint TEXT NOT NULL,
    missing INTEGER NOT NULL,
    imported_at TEXT NOT NULL,
    UNIQUE(session_id, external_id)
);

CREATE TABLE IF NOT EXISTS conversation_parts (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL,
    part_index INTEGER NOT NULL,
    role TEXT NOT NULL,
    kind TEXT NOT NULL,
    text TEXT,
    language TEXT,
    command TEXT,
    cwd TEXT,
    status TEXT,
    exit_code INTEGER,
    metadata_json TEXT,
    UNIQUE(turn_id, part_index)
);

CREATE TABLE IF NOT EXISTS conversation_questions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    question_index INTEGER NOT NULL,
    title TEXT,
    question_text TEXT NOT NULL,
    answer_text TEXT NOT NULL,
    code_text TEXT NOT NULL,
    command_text TEXT NOT NULL,
    grouping_origin TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(session_id, question_index)
);

CREATE TABLE IF NOT EXISTS conversation_question_turns (
    question_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    turn_order INTEGER NOT NULL,
    PRIMARY KEY (question_id, turn_id),
    UNIQUE(turn_id)
);

CREATE TABLE IF NOT EXISTS web_record_sessions (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL,
    adapter_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    title TEXT NOT NULL,
    project_path TEXT,
    started_at TEXT,
    updated_at TEXT,
    source_locator TEXT,
    source_fingerprint TEXT,
    missing INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    UNIQUE(source_id, external_id)
);

CREATE TABLE IF NOT EXISTS web_record_turns (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    external_id TEXT NOT NULL,
    turn_index INTEGER NOT NULL,
    user_text TEXT NOT NULL,
    title TEXT,
    started_at TEXT,
    ended_at TEXT,
    fingerprint TEXT NOT NULL,
    missing INTEGER NOT NULL,
    imported_at TEXT NOT NULL,
    UNIQUE(session_id, external_id)
);

CREATE TABLE IF NOT EXISTS web_record_parts (
    id TEXT PRIMARY KEY,
    turn_id TEXT NOT NULL,
    part_index INTEGER NOT NULL,
    role TEXT NOT NULL,
    kind TEXT NOT NULL,
    text TEXT,
    language TEXT,
    command TEXT,
    cwd TEXT,
    status TEXT,
    exit_code INTEGER,
    metadata_json TEXT,
    UNIQUE(turn_id, part_index)
);

CREATE TABLE IF NOT EXISTS web_record_questions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    question_index INTEGER NOT NULL,
    title TEXT,
    question_text TEXT NOT NULL,
    answer_text TEXT NOT NULL,
    code_text TEXT NOT NULL,
    command_text TEXT NOT NULL,
    grouping_origin TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(session_id, question_index)
);

CREATE TABLE IF NOT EXISTS web_record_question_turns (
    question_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    turn_order INTEGER NOT NULL,
    PRIMARY KEY (question_id, turn_id),
    UNIQUE(turn_id)
);

CREATE TABLE IF NOT EXISTS conversation_sync_runs (
    id TEXT PRIMARY KEY,
    source_id TEXT,
    adapter_id TEXT,
    status TEXT NOT NULL,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    session_count INTEGER NOT NULL,
    turn_count INTEGER NOT NULL,
    warning_count INTEGER NOT NULL,
    error_message TEXT
);

CREATE VIRTUAL TABLE IF NOT EXISTS conversation_question_fts USING fts5(
    question_id UNINDEXED,
    session_id UNINDEXED,
    question_text,
    answer_text,
    code_text,
    command_text
);
"#;

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

pub(crate) const LOAD_SOURCE: &str = r#"
SELECT id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
       origin_app_kind, include_globs, exclude_globs, default_kind, enabled, priority,
       last_scanned_at, last_scan_status
FROM sources
WHERE id = ?1
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

pub(crate) const GET_ASSET_MOUNT_CREATED_AT: &str =
    "SELECT created_at FROM asset_mounts WHERE asset_id = ?1 AND profile_id = ?2";

pub(crate) const UPSERT_ASSET_MOUNT: &str = r#"
INSERT INTO asset_mounts (
    asset_id, profile_id, enabled, strategy, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(asset_id, profile_id) DO UPDATE SET
    enabled = excluded.enabled,
    strategy = excluded.strategy,
    updated_at = excluded.updated_at
"#;

pub(crate) const UPSERT_ASSET_MOUNT_OBSERVATION: &str = r#"
INSERT INTO asset_mount_observations (
    asset_id, profile_id, target_dir, target_path, state, linked_source, observed_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(asset_id, profile_id) DO UPDATE SET
    target_dir = excluded.target_dir,
    target_path = excluded.target_path,
    state = excluded.state,
    linked_source = excluded.linked_source,
    observed_at = excluded.observed_at
"#;

#[cfg(test)]
pub(crate) const LIST_ASSET_MOUNT_OBSERVATIONS: &str = r#"
SELECT asset_id, profile_id, target_dir, target_path, state, linked_source, observed_at
FROM asset_mount_observations
ORDER BY asset_id ASC, profile_id ASC
"#;

pub(crate) const DELETE_ORPHAN_ASSET_MOUNT_OBSERVATIONS: &str = r#"
DELETE FROM asset_mount_observations
WHERE NOT EXISTS (
    SELECT 1 FROM assets WHERE assets.id = asset_mount_observations.asset_id
)
"#;

pub(crate) const LIST_ASSET_GROUPS_BY_KIND: &str = r#"
SELECT id, name, description, color, asset_kind, display_icon, icon_svg, enabled, sort_order, rules_payload,
       created_at, updated_at
FROM asset_groups
WHERE asset_kind = ?1
ORDER BY sort_order ASC, name ASC
"#;

pub(crate) const GET_ASSET_GROUP: &str = r#"
SELECT id, name, description, color, asset_kind, display_icon, icon_svg, enabled, sort_order, rules_payload,
       created_at, updated_at
FROM asset_groups
WHERE id = ?1
"#;

pub(crate) const UPSERT_ASSET_GROUP: &str = r#"
INSERT INTO asset_groups (
    id, name, description, color, asset_kind, display_icon, icon_svg, enabled, sort_order, rules_payload,
    created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    description = excluded.description,
    color = excluded.color,
    asset_kind = excluded.asset_kind,
    display_icon = excluded.display_icon,
    icon_svg = excluded.icon_svg,
    enabled = excluded.enabled,
    sort_order = excluded.sort_order,
    rules_payload = excluded.rules_payload,
    updated_at = excluded.updated_at
"#;

pub(crate) const DELETE_ASSET_GROUP: &str = "DELETE FROM asset_groups WHERE id = ?1";

pub(crate) const DELETE_ASSET_GROUP_MEMBERS: &str =
    "DELETE FROM asset_group_members WHERE group_id = ?1";

pub(crate) const INSERT_ASSET_GROUP_MEMBER: &str = r#"
INSERT INTO asset_group_members (group_id, asset_id, created_at)
VALUES (?1, ?2, ?3)
ON CONFLICT(group_id, asset_id) DO NOTHING
"#;

pub(crate) const LIST_ASSET_GROUP_MEMBERS: &str = r#"
SELECT group_id, asset_id
FROM asset_group_members
ORDER BY group_id ASC, asset_id ASC
"#;

pub(crate) const DELETE_ORPHAN_ASSET_GROUP_MEMBERS: &str = r#"
DELETE FROM asset_group_members
WHERE NOT EXISTS (
    SELECT 1 FROM asset_groups WHERE asset_groups.id = asset_group_members.group_id
) OR NOT EXISTS (
    SELECT 1 FROM assets WHERE assets.id = asset_group_members.asset_id
)
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

pub(crate) const DELETE_PROFILE: &str = "DELETE FROM profiles WHERE id = ?1";

pub(crate) const DELETE_APP_SHORTCUT_BY_PROFILE: &str =
    "DELETE FROM app_shortcut_items WHERE profile_id = ?1";

pub(crate) const DELETE_ASSET_MOUNTS_BY_PROFILE: &str =
    "DELETE FROM asset_mounts WHERE profile_id = ?1";

pub(crate) const DELETE_ASSET_MOUNT_OBSERVATIONS_BY_PROFILE: &str =
    "DELETE FROM asset_mount_observations WHERE profile_id = ?1";

pub(crate) const COUNT_DEPLOYMENT_STATE_BY_PROFILE: &str =
    "SELECT COUNT(*) FROM deployment_state WHERE profile_id = ?1";

pub(crate) const DELETE_ASSETS_BY_SOURCE: &str = "DELETE FROM assets WHERE source_id = ?1";

pub(crate) const LIST_SKILL_REMOTE_SOURCES: &str = r#"
SELECT asset_id, provider, source_url, repo_url, branch, path, acquired_at,
       acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha,
       status, message
FROM skill_remote_sources
ORDER BY acquired_at DESC, asset_id ASC
"#;

pub(crate) const GET_SKILL_REMOTE_SOURCE: &str = r#"
SELECT asset_id, provider, source_url, repo_url, branch, path, acquired_at,
       acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha,
       status, message
FROM skill_remote_sources
WHERE asset_id = ?1
"#;

pub(crate) const UPSERT_SKILL_REMOTE_SOURCE: &str = r#"
INSERT INTO skill_remote_sources (
    asset_id, provider, source_url, repo_url, branch, path, acquired_at,
    acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha,
    status, message
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
ON CONFLICT(asset_id) DO UPDATE SET
    provider = excluded.provider,
    source_url = excluded.source_url,
    repo_url = excluded.repo_url,
    branch = excluded.branch,
    path = excluded.path,
    acquired_at = excluded.acquired_at,
    acquired_tree_sha = excluded.acquired_tree_sha,
    local_content_hash = excluded.local_content_hash,
    last_checked_at = excluded.last_checked_at,
    latest_tree_sha = excluded.latest_tree_sha,
    status = excluded.status,
    message = excluded.message
"#;

pub(crate) const UPDATE_SKILL_REMOTE_CHECK: &str = r#"
UPDATE skill_remote_sources
SET last_checked_at = ?2,
    latest_tree_sha = ?3,
    status = ?4,
    message = ?5
WHERE asset_id = ?1
"#;

pub(crate) const DELETE_ORPHAN_SKILL_REMOTE_SOURCES: &str = r#"
DELETE FROM skill_remote_sources
WHERE NOT EXISTS (
    SELECT 1 FROM assets WHERE assets.id = skill_remote_sources.asset_id
)
"#;

pub(crate) const UPDATE_ASSET_DESCRIPTION: &str = r#"
UPDATE assets
SET description = ?1, updated_at = ?2
WHERE id = ?3
"#;

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
