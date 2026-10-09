CREATE TEMP TABLE conversation_adapter_package_id_map (
    old_id TEXT PRIMARY KEY,
    new_id TEXT NOT NULL UNIQUE
);

INSERT INTO conversation_adapter_package_id_map (old_id, new_id) VALUES
('codex-session', 'io.github.util6.codex-session'),
('opencode-session', 'io.github.util6.opencode-session'),
('claude-code-session', 'io.github.util6.claude-code-session'),
('zcode-session', 'io.github.util6.zcode-session'),
('chatgpt-web', 'io.github.util6.chatgpt-web'),
('qwen-web', 'io.github.util6.qwen-web'),
('gemini-web', 'io.github.util6.gemini-web');

INSERT OR IGNORE INTO conversation_adapter_packages (
    tenant_id, package_id, adapter_id, name, version, record_kind, install_dir,
    manifest_path, adapter_manifest_path, runtime_protocol, runtime_ready,
    installed_content_hash, trusted_package_hash, error_message, created_at, updated_at,
    origin, source_url, git_ref, git_commit, catalog_url, update_policy,
    latest_version, last_checked_at, runtime_gate_status, runtime_validated_at
)
SELECT p.tenant_id, m.new_id, p.adapter_id, p.name, p.version, p.record_kind, p.install_dir,
       p.manifest_path, p.adapter_manifest_path, p.runtime_protocol, p.runtime_ready,
       p.installed_content_hash, p.trusted_package_hash, p.error_message, p.created_at, p.updated_at,
       p.origin, p.source_url, p.git_ref, p.git_commit, p.catalog_url, p.update_policy,
       p.latest_version, p.last_checked_at, p.runtime_gate_status, p.runtime_validated_at
FROM conversation_adapter_packages p
JOIN conversation_adapter_package_id_map m ON m.old_id = p.package_id;

UPDATE conversation_adapter_package_versions
SET package_id = (
    SELECT new_id FROM conversation_adapter_package_id_map
    WHERE old_id = conversation_adapter_package_versions.package_id
)
WHERE package_id IN (SELECT old_id FROM conversation_adapter_package_id_map);

DELETE FROM conversation_adapter_packages
WHERE package_id IN (SELECT old_id FROM conversation_adapter_package_id_map);

DROP TABLE conversation_adapter_package_id_map;
