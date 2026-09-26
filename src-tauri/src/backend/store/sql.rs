pub(crate) const LIST_SOURCES: &str = r#"
SELECT id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
       origin_app_kind, origin_provider_id, include_globs, exclude_globs, default_kind, enabled, priority,
       last_scanned_at, last_scan_status
FROM sources
WHERE tenant_id = ?1
ORDER BY priority ASC, name ASC
"#;

pub(crate) const LIST_SKILL_SOURCES: &str = r#"
SELECT id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
       origin_app_kind, origin_provider_id, include_globs, exclude_globs, default_kind, enabled, priority,
       last_scanned_at, last_scan_status
FROM sources
WHERE tenant_id = ?1 AND scanner_kind = 'skill'
ORDER BY priority ASC, name ASC
"#;

pub(crate) const LOAD_SOURCE: &str = r#"
SELECT id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
       origin_app_kind, origin_provider_id, include_globs, exclude_globs, default_kind, enabled, priority,
       last_scanned_at, last_scan_status
FROM sources
WHERE tenant_id = ?1 AND id = ?2
"#;

pub(crate) const LIST_ASSETS: &str = r#"
SELECT id, source_id, name, kind, format, relative_path, absolute_path,
       entry_file, description, content_hash, discovered_at, updated_at,
       detector_id, detector_version
FROM assets
WHERE tenant_id = ?1
ORDER BY name ASC
"#;

pub(crate) const LIST_ASSETS_BY_KIND: &str = r#"
SELECT id, source_id, name, kind, format, relative_path, absolute_path,
       entry_file, description, content_hash, discovered_at, updated_at,
       detector_id, detector_version
FROM assets
WHERE tenant_id = ?1 AND kind = ?2
ORDER BY name ASC
"#;

pub(crate) const LOAD_ASSET: &str = r#"
SELECT id, source_id, name, kind, format, relative_path, absolute_path,
       entry_file, description, content_hash, discovered_at, updated_at,
       detector_id, detector_version
FROM assets
WHERE tenant_id = ?1 AND id = ?2
"#;

pub(crate) const LIST_PROFILES: &str =
    "SELECT payload FROM profiles WHERE tenant_id = ?1 ORDER BY id ASC";
pub(crate) const LOAD_PROFILE: &str =
    "SELECT payload FROM profiles WHERE tenant_id = ?1 AND id = ?2";

pub(crate) use super::sql_system::*;

pub(crate) const LIST_ASSET_MOUNTS: &str = r#"
SELECT asset_id, profile_id, enabled, strategy, created_at, updated_at
FROM asset_mounts
WHERE tenant_id = ?1 AND (?2 IS NULL OR asset_id = ?2)
ORDER BY asset_id ASC, profile_id ASC
"#;

pub(crate) const LIST_ENABLED_ASSET_MOUNTS: &str = r#"
SELECT asset_id, profile_id, enabled, strategy, created_at, updated_at
FROM asset_mounts
WHERE tenant_id = ?1 AND enabled = 1 AND (?2 IS NULL OR profile_id = ?2)
ORDER BY profile_id ASC, asset_id ASC
"#;

pub(crate) const DELETE_ORPHAN_ASSET_MOUNTS: &str = r#"
DELETE FROM asset_mounts
WHERE tenant_id = ?1 AND NOT EXISTS (
    SELECT 1 FROM assets
    WHERE assets.tenant_id = asset_mounts.tenant_id
    AND assets.id = asset_mounts.asset_id
)
"#;

pub(crate) const GET_ASSET_MOUNT: &str = r#"
SELECT asset_id, profile_id, enabled, strategy, created_at, updated_at
FROM asset_mounts
WHERE tenant_id = ?1 AND asset_id = ?2 AND profile_id = ?3
"#;

pub(crate) const GET_ASSET_MOUNT_CREATED_AT: &str = r#"
SELECT created_at
FROM asset_mounts
WHERE tenant_id = ?1 AND asset_id = ?2 AND profile_id = ?3
"#;

pub(crate) const UPSERT_ASSET_MOUNT: &str = r#"
INSERT INTO asset_mounts (
    tenant_id, asset_id, profile_id, enabled, strategy, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
ON CONFLICT(tenant_id, asset_id, profile_id) DO UPDATE SET
    enabled = excluded.enabled,
    strategy = excluded.strategy,
    updated_at = excluded.updated_at
"#;

pub(crate) const UPSERT_ASSET_MOUNT_OBSERVATION: &str = r#"
INSERT INTO asset_mount_observations (
    tenant_id, asset_id, profile_id, target_dir, target_path, state, linked_source, observed_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(tenant_id, asset_id, profile_id) DO UPDATE SET
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
WHERE tenant_id = ?1
ORDER BY asset_id ASC, profile_id ASC
"#;

pub(crate) const DELETE_ORPHAN_ASSET_MOUNT_OBSERVATIONS: &str = r#"
DELETE FROM asset_mount_observations
WHERE tenant_id = ?1 AND NOT EXISTS (
    SELECT 1 FROM assets
    WHERE assets.tenant_id = asset_mount_observations.tenant_id
    AND assets.id = asset_mount_observations.asset_id
)
"#;

pub(crate) const LIST_ASSET_GROUPS_BY_KIND: &str = r#"
SELECT id, name, description, color, asset_kind, display_icon, icon_svg, enabled, sort_order, rules_payload,
       created_at, updated_at
FROM asset_groups
WHERE tenant_id = ?1 AND asset_kind = ?2
ORDER BY sort_order ASC, name ASC
"#;

pub(crate) const GET_ASSET_GROUP: &str = r#"
SELECT id, name, description, color, asset_kind, display_icon, icon_svg, enabled, sort_order, rules_payload,
       created_at, updated_at
FROM asset_groups
WHERE tenant_id = ?1 AND id = ?2
"#;

pub(crate) const UPSERT_ASSET_GROUP: &str = r#"
INSERT INTO asset_groups (
    tenant_id, id, name, description, color, asset_kind, display_icon, icon_svg, enabled,
    sort_order, rules_payload, created_at, updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
ON CONFLICT(tenant_id, id) DO UPDATE SET
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

pub(crate) const DELETE_ASSET_GROUP: &str =
    "DELETE FROM asset_groups WHERE tenant_id = ?1 AND id = ?2";

pub(crate) const DELETE_ASSET_GROUP_MEMBERS: &str =
    "DELETE FROM asset_group_members WHERE tenant_id = ?1 AND group_id = ?2";

pub(crate) const INSERT_ASSET_GROUP_MEMBER: &str = r#"
INSERT INTO asset_group_members (tenant_id, group_id, asset_id, created_at)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(tenant_id, group_id, asset_id) DO NOTHING
"#;

pub(crate) const LIST_ASSET_GROUP_MEMBERS: &str = r#"
SELECT group_id, asset_id
FROM asset_group_members
WHERE tenant_id = ?1
ORDER BY group_id ASC, asset_id ASC
"#;

pub(crate) const DELETE_ORPHAN_ASSET_GROUP_MEMBERS: &str = r#"
DELETE FROM asset_group_members
WHERE tenant_id = ?1
AND (
NOT EXISTS (
    SELECT 1 FROM asset_groups
    WHERE asset_groups.tenant_id = asset_group_members.tenant_id
    AND asset_groups.id = asset_group_members.group_id
) OR NOT EXISTS (
    SELECT 1 FROM assets
    WHERE assets.tenant_id = asset_group_members.tenant_id
    AND assets.id = asset_group_members.asset_id
)
)
"#;

pub(crate) const UPSERT_SOURCE: &str = r#"
INSERT INTO sources (
    tenant_id, id, name, kind, root_path, scanner_kind, source_origin, repo_root, scan_root,
    origin_app_kind, origin_provider_id, include_globs, exclude_globs, default_kind, enabled, priority,
    last_scanned_at, last_scan_status
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
ON CONFLICT(tenant_id, id) DO UPDATE SET
    name = excluded.name,
    kind = excluded.kind,
    root_path = excluded.root_path,
    scanner_kind = excluded.scanner_kind,
    source_origin = excluded.source_origin,
    repo_root = excluded.repo_root,
    scan_root = excluded.scan_root,
    origin_app_kind = excluded.origin_app_kind,
    origin_provider_id = excluded.origin_provider_id,
    include_globs = excluded.include_globs,
    exclude_globs = excluded.exclude_globs,
    default_kind = excluded.default_kind,
    enabled = excluded.enabled,
    priority = excluded.priority,
    last_scanned_at = excluded.last_scanned_at,
    last_scan_status = excluded.last_scan_status
"#;

pub(crate) const DELETE_SOURCE: &str = "DELETE FROM sources WHERE tenant_id = ?1 AND id = ?2";

pub(crate) const UPSERT_PROFILE: &str = r#"
INSERT INTO profiles (tenant_id, id, payload)
VALUES (?1, ?2, ?3)
ON CONFLICT(tenant_id, id) DO UPDATE SET payload = excluded.payload
"#;

pub(crate) const DELETE_PROFILE: &str = "DELETE FROM profiles WHERE tenant_id = ?1 AND id = ?2";

pub(crate) const DELETE_APP_SHORTCUT_BY_PROFILE: &str =
    "DELETE FROM app_shortcut_items WHERE tenant_id = ?1 AND profile_id = ?2";

pub(crate) const DELETE_ASSET_MOUNTS_BY_PROFILE: &str =
    "DELETE FROM asset_mounts WHERE tenant_id = ?1 AND profile_id = ?2";

pub(crate) const DELETE_ASSET_MOUNT_OBSERVATIONS_BY_PROFILE: &str =
    "DELETE FROM asset_mount_observations WHERE tenant_id = ?1 AND profile_id = ?2";

pub(crate) const COUNT_DEPLOYMENT_STATE_BY_PROFILE: &str =
    "SELECT COUNT(*) FROM deployment_state WHERE tenant_id = ?1 AND profile_id = ?2";

pub(crate) const DELETE_ASSETS_BY_SOURCE: &str =
    "DELETE FROM assets WHERE tenant_id = ?1 AND source_id = ?2";

pub(crate) const LIST_SKILL_REMOTE_SOURCES: &str = r#"
SELECT asset_id, provider, source_url, repo_url, branch, path, acquired_at,
       acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha,
       status, message
FROM skill_remote_sources
WHERE tenant_id = ?1
ORDER BY acquired_at DESC, asset_id ASC
"#;

pub(crate) const GET_SKILL_REMOTE_SOURCE: &str = r#"
SELECT asset_id, provider, source_url, repo_url, branch, path, acquired_at,
       acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha,
       status, message
FROM skill_remote_sources
WHERE tenant_id = ?1 AND asset_id = ?2
"#;

pub(crate) const UPSERT_SKILL_REMOTE_SOURCE: &str = r#"
INSERT INTO skill_remote_sources (
    tenant_id, asset_id, provider, source_url, repo_url, branch, path, acquired_at,
    acquired_tree_sha, local_content_hash, last_checked_at, latest_tree_sha,
    status, message
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
ON CONFLICT(tenant_id, asset_id) DO UPDATE SET
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
SET last_checked_at = ?3,
    latest_tree_sha = ?4,
    status = ?5,
    message = ?6
WHERE tenant_id = ?1 AND asset_id = ?2
"#;

pub(crate) const DELETE_ORPHAN_SKILL_REMOTE_SOURCES: &str = r#"
DELETE FROM skill_remote_sources
WHERE tenant_id = ?1 AND NOT EXISTS (
    SELECT 1 FROM assets
    WHERE assets.tenant_id = skill_remote_sources.tenant_id
    AND assets.id = skill_remote_sources.asset_id
)
"#;

pub(crate) const UPDATE_ASSET_DESCRIPTION: &str = r#"
UPDATE assets
SET description = ?1, updated_at = ?2
WHERE tenant_id = ?3 AND id = ?4
"#;

pub(crate) const INSERT_ASSET: &str = r#"
INSERT INTO assets (
    tenant_id, id, source_id, name, kind, format, relative_path, absolute_path,
    entry_file, description, content_hash, discovered_at, updated_at,
    detector_id, detector_version
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
"#;

pub(crate) const UPSERT_DEPLOYMENT_STATE: &str = r#"
INSERT INTO deployment_state (
    tenant_id, profile_id, asset_id, target_path, strategy, source_hash, deployed_at, managed_by
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(tenant_id, profile_id, asset_id, target_path) DO UPDATE SET
    strategy = excluded.strategy,
    source_hash = excluded.source_hash,
    deployed_at = excluded.deployed_at,
    managed_by = excluded.managed_by
"#;

pub(crate) const GET_MANAGED_DEPLOYMENT: &str = r#"
SELECT managed_by
FROM deployment_state
WHERE tenant_id = ?1 AND profile_id = ?2 AND asset_id = ?3 AND target_path = ?4
"#;

pub(crate) const LIST_MANAGED_DEPLOYMENT_TARGETS_BY_PROFILE: &str = r#"
SELECT asset_id, target_path
FROM deployment_state
WHERE tenant_id = ?1 AND profile_id = ?2 AND managed_by = 'assetiweave'
"#;

pub(crate) const DELETE_DEPLOYMENT_STATE: &str = r#"
DELETE FROM deployment_state
WHERE tenant_id = ?1 AND profile_id = ?2 AND asset_id = ?3 AND target_path = ?4
"#;

pub(crate) const DELETE_ORPHAN_DEPLOYMENT_STATE: &str = r#"
DELETE FROM deployment_state
WHERE tenant_id = ?1 AND NOT EXISTS (
    SELECT 1 FROM assets
    WHERE assets.tenant_id = deployment_state.tenant_id
    AND assets.id = deployment_state.asset_id
)
"#;
