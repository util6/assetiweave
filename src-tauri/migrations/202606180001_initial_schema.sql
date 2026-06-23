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

UPDATE deployment_state
SET strategy = CASE strategy
    WHEN 'symlink' THEN 'symlink_to_source'
    WHEN 'copy' THEN 'copy_to_target'
    ELSE strategy
END
WHERE strategy IN ('symlink', 'copy');

UPDATE asset_mounts
SET strategy = CASE strategy
    WHEN 'symlink' THEN 'symlink_to_source'
    WHEN 'copy' THEN 'copy_to_target'
    ELSE strategy
END
WHERE strategy IN ('symlink', 'copy');
