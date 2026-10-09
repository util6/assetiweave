PRAGMA foreign_keys = OFF;

CREATE TABLE sources_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
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
    last_scan_status TEXT,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO sources_new (
    tenant_id, id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
    origin_app_kind, include_globs, exclude_globs, default_kind, enabled, priority,
    last_scanned_at, last_scan_status
)
SELECT
    'default', id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
    origin_app_kind, include_globs, exclude_globs, default_kind, enabled, priority,
    last_scanned_at, last_scan_status
FROM sources;

DROP TABLE sources;
ALTER TABLE sources_new RENAME TO sources;

CREATE INDEX idx_sources_tenant_order ON sources(tenant_id, priority, name);
CREATE INDEX idx_sources_tenant_scanner ON sources(tenant_id, scanner_kind);

CREATE TABLE assets_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
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
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO assets_new (
    tenant_id, id, source_id, name, kind, format, relative_path, absolute_path,
    entry_file, description, content_hash, discovered_at, updated_at
)
SELECT
    'default', id, source_id, name, kind, format, relative_path, absolute_path,
    entry_file, description, content_hash, discovered_at, updated_at
FROM assets;

DROP TABLE assets;
ALTER TABLE assets_new RENAME TO assets;

CREATE INDEX idx_assets_tenant_kind ON assets(tenant_id, kind, name);
CREATE INDEX idx_assets_tenant_source ON assets(tenant_id, source_id);

CREATE TABLE profiles_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    payload TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO profiles_new (tenant_id, id, payload)
SELECT 'default', id, payload
FROM profiles;

DROP TABLE profiles;
ALTER TABLE profiles_new RENAME TO profiles;

CREATE TABLE deployment_state_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    profile_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    target_path TEXT NOT NULL,
    strategy TEXT NOT NULL,
    source_hash TEXT NOT NULL,
    deployed_at TEXT NOT NULL,
    managed_by TEXT NOT NULL,
    PRIMARY KEY (tenant_id, profile_id, asset_id, target_path)
);

INSERT INTO deployment_state_new (
    tenant_id, profile_id, asset_id, target_path, strategy, source_hash, deployed_at, managed_by
)
SELECT
    'default', profile_id, asset_id, target_path, strategy, source_hash, deployed_at, managed_by
FROM deployment_state;

DROP TABLE deployment_state;
ALTER TABLE deployment_state_new RENAME TO deployment_state;

CREATE INDEX idx_deployment_state_tenant_profile ON deployment_state(tenant_id, profile_id);
CREATE INDEX idx_deployment_state_tenant_asset ON deployment_state(tenant_id, asset_id);

CREATE TABLE navigation_state_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
    active_rail_id TEXT NOT NULL,
    active_header_tab_id TEXT NOT NULL,
    active_sub_nav_id TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO navigation_state_new (
    tenant_id, id, active_rail_id, active_header_tab_id, active_sub_nav_id
)
SELECT 'default', id, active_rail_id, active_header_tab_id, active_sub_nav_id
FROM navigation_state;

DROP TABLE navigation_state;
ALTER TABLE navigation_state_new RENAME TO navigation_state;

CREATE TABLE app_shortcut_items_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    profile_id TEXT NOT NULL,
    display_icon TEXT NOT NULL,
    icon_svg TEXT,
    accent_color TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    sort_order INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, profile_id)
);

INSERT INTO app_shortcut_items_new (
    tenant_id, profile_id, display_icon, icon_svg, accent_color, enabled, sort_order
)
SELECT 'default', profile_id, display_icon, icon_svg, accent_color, enabled, sort_order
FROM app_shortcut_items;

DROP TABLE app_shortcut_items;
ALTER TABLE app_shortcut_items_new RENAME TO app_shortcut_items;

CREATE TABLE asset_mounts_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    asset_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    enabled INTEGER NOT NULL,
    strategy TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, asset_id, profile_id)
);

INSERT INTO asset_mounts_new (
    tenant_id, asset_id, profile_id, enabled, strategy, created_at, updated_at
)
SELECT 'default', asset_id, profile_id, enabled, strategy, created_at, updated_at
FROM asset_mounts;

DROP TABLE asset_mounts;
ALTER TABLE asset_mounts_new RENAME TO asset_mounts;

CREATE INDEX idx_asset_mounts_tenant_profile ON asset_mounts(tenant_id, profile_id);

CREATE TABLE asset_mount_observations_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    asset_id TEXT NOT NULL,
    profile_id TEXT NOT NULL,
    target_dir TEXT NOT NULL,
    target_path TEXT NOT NULL,
    state TEXT NOT NULL,
    linked_source TEXT,
    observed_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, asset_id, profile_id)
);

INSERT INTO asset_mount_observations_new (
    tenant_id, asset_id, profile_id, target_dir, target_path, state, linked_source, observed_at
)
SELECT
    'default', asset_id, profile_id, target_dir, target_path, state, linked_source, observed_at
FROM asset_mount_observations;

DROP TABLE asset_mount_observations;
ALTER TABLE asset_mount_observations_new RENAME TO asset_mount_observations;

CREATE INDEX idx_asset_mount_observations_tenant_profile
ON asset_mount_observations(tenant_id, profile_id);

CREATE TABLE asset_groups_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    id TEXT NOT NULL,
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
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, id)
);

INSERT INTO asset_groups_new (
    tenant_id, id, name, description, color, asset_kind, display_icon, icon_svg, enabled,
    sort_order, rules_payload, created_at, updated_at
)
SELECT
    'default', id, name, description, color, asset_kind, display_icon, icon_svg, enabled,
    sort_order, rules_payload, created_at, updated_at
FROM asset_groups;

DROP TABLE asset_groups;
ALTER TABLE asset_groups_new RENAME TO asset_groups;

CREATE INDEX idx_asset_groups_tenant_kind ON asset_groups(tenant_id, asset_kind, sort_order, name);

CREATE TABLE asset_group_members_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    group_id TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, group_id, asset_id)
);

INSERT INTO asset_group_members_new (tenant_id, group_id, asset_id, created_at)
SELECT 'default', group_id, asset_id, created_at
FROM asset_group_members;

DROP TABLE asset_group_members;
ALTER TABLE asset_group_members_new RENAME TO asset_group_members;

CREATE INDEX idx_asset_group_members_tenant_asset ON asset_group_members(tenant_id, asset_id);

CREATE TABLE skill_remote_sources_new (
    tenant_id TEXT NOT NULL DEFAULT 'default',
    asset_id TEXT NOT NULL,
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
    message TEXT,
    PRIMARY KEY (tenant_id, asset_id)
);

INSERT INTO skill_remote_sources_new (
    tenant_id, asset_id, provider, source_url, repo_url, branch, path, acquired_at,
    acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha, status, message
)
SELECT
    'default', asset_id, provider, source_url, repo_url, branch, path, acquired_at,
    acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha, status, message
FROM skill_remote_sources;

DROP TABLE skill_remote_sources;
ALTER TABLE skill_remote_sources_new RENAME TO skill_remote_sources;

PRAGMA foreign_keys = ON;
