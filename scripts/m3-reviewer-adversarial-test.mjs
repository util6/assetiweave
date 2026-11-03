import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const migrationSqlPath = path.join(
  repositoryRoot,
  "backend/asset_core/migrations/20261031000001_unified_app_extensions_and_agent_market.sql"
);

function runSqlite(dbPath, sql) {
  const result = spawnSync("sqlite3", [dbPath], {
    encoding: "utf8",
    input: sql,
  });
  if (result.status !== 0) {
    throw new Error(`sqlite3 error: ${result.stderr}\nCommand: ${sql}`);
  }
  return result.stdout.trim();
}

function querySqliteJson(dbPath, query) {
  const result = spawnSync("sqlite3", ["-json", dbPath, query], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(`sqlite3 error: ${result.stderr}\nQuery: ${query}`);
  }
  return result.stdout.trim() ? JSON.parse(result.stdout.trim()) : [];
}

let passed = 0;
let total = 0;

function test(name, fn) {
  total++;
  process.stdout.write(`[M3 Reviewer Stress Test ${total}] ${name} ... `);
  try {
    fn();
    passed++;
    console.log("PASS");
  } catch (err) {
    console.log("FAIL");
    console.error(err);
  }
}

console.log("=== ADVERSARIAL STRESS TEST FOR MILESTONE 3 MIGRATION & SCHEMAS ===");

// -----------------------------------------------------------------------------
// Test 1: Capability kind has NO closed CHECK constraint (Open extensibility)
// -----------------------------------------------------------------------------
test("DDL: app_extensions allows arbitrary future capability_kind without CHECK failure", () => {
  const fixtureDir = mkdtempSync(path.join(tmpdir(), "m3-test-ddl-"));
  const dbPath = path.join(fixtureDir, "test.db");
  try {
    // Run migration on empty DB (table creation)
    const sqlContent = spawnSync("cat", [migrationSqlPath], { encoding: "utf8" }).stdout;
    // Extract only DDL (section 1)
    const ddl = sqlContent.split("-- 2. Migrate data")[0];
    runSqlite(dbPath, ddl);

    // Insert an agent
    runSqlite(
      dbPath,
      `INSERT INTO app_extensions (
        extension_id, capability_kind, target_id, display_name, active_version,
        distribution_type, install_dir, enabled, installation_status, runtime_status,
        installed_at, updated_at, capability_meta_json
      ) VALUES (
        'ext-1', 'agent', 'agent-1', 'Agent 1', '1.0.0',
        'binary', NULL, 1, 'ready', 'ready',
        '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '{}'
      );`
    );

    // Insert a conversation adapter
    runSqlite(
      dbPath,
      `INSERT INTO app_extensions (
        extension_id, capability_kind, target_id, display_name, active_version,
        distribution_type, install_dir, enabled, installation_status, runtime_status,
        installed_at, updated_at, capability_meta_json
      ) VALUES (
        'ext-2', 'conversation.adapter', 'adapter-1', 'Adapter 1', '1.0.0',
        'archive', NULL, 1, 'ready', 'ready',
        '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '{}'
      );`
    );

    // Insert a future capability (e.g., 'memory.provider' or 'custom.tool')
    runSqlite(
      dbPath,
      `INSERT INTO app_extensions (
        extension_id, capability_kind, target_id, display_name, active_version,
        distribution_type, install_dir, enabled, installation_status, runtime_status,
        installed_at, updated_at, capability_meta_json
      ) VALUES (
        'ext-3', 'memory.provider', 'mem-1', 'Memory Provider', '1.0.0',
        'binary', NULL, 1, 'ready', 'ready',
        '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '{}'
      );`
    );

    const rows = querySqliteJson(dbPath, "SELECT extension_id, capability_kind FROM app_extensions ORDER BY extension_id;");
    assert.equal(rows.length, 3);
    assert.equal(rows[2].capability_kind, "memory.provider");
  } finally {
    rmSync(fixtureDir, { force: true, recursive: true });
  }
});

// -----------------------------------------------------------------------------
// Test 2: Foreign key CASCADE on app_extension_versions
// -----------------------------------------------------------------------------
test("DDL: app_extension_versions cascades delete on parent app_extensions removal", () => {
  const fixtureDir = mkdtempSync(path.join(tmpdir(), "m3-test-fk-"));
  const dbPath = path.join(fixtureDir, "test.db");
  try {
    const sqlContent = spawnSync("cat", [migrationSqlPath], { encoding: "utf8" }).stdout;
    const ddl = sqlContent.split("-- 2. Migrate data")[0];
    runSqlite(dbPath, "PRAGMA foreign_keys = ON;\n" + ddl);

    runSqlite(
      dbPath,
      `PRAGMA foreign_keys = ON;
      INSERT INTO app_extensions (
        extension_id, capability_kind, target_id, display_name, active_version,
        distribution_type, install_dir, enabled, installation_status, runtime_status,
        installed_at, updated_at, capability_meta_json
      ) VALUES (
        'ext-agent', 'agent', 'agent-target', 'Agent Test', '1.0.0',
        'binary', NULL, 1, 'ready', 'ready',
        '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '{}'
      );
      INSERT INTO app_extension_versions (
        extension_id, version, install_dir, content_hash, verification_status, installed_at
      ) VALUES
        ('ext-agent', '1.0.0', '/path/v1', 'hash1', 'tested', '2026-10-31T00:00:00Z'),
        ('ext-agent', '1.1.0', '/path/v2', 'hash2', 'tested', '2026-10-31T01:00:00Z');
      `
    );

    let vRows = querySqliteJson(dbPath, "SELECT count(*) as count FROM app_extension_versions;");
    assert.equal(vRows[0].count, 2);

    // Delete from parent
    runSqlite(dbPath, "PRAGMA foreign_keys = ON; DELETE FROM app_extensions WHERE extension_id = 'ext-agent';");

    vRows = querySqliteJson(dbPath, "SELECT count(*) as count FROM app_extension_versions;");
    assert.equal(vRows[0].count, 0, "Versions must be cascade deleted by SQLite foreign key");
  } finally {
    rmSync(fixtureDir, { force: true, recursive: true });
  }
});

// -----------------------------------------------------------------------------
// Test 3: Data migration & downgrade logic from app_agent_installations
// -----------------------------------------------------------------------------
test("Migration: Legacy app_agent_installations data migration & downgrade logic", () => {
  const fixtureDir = mkdtempSync(path.join(tmpdir(), "m3-test-mig-"));
  const dbPath = path.join(fixtureDir, "test.db");
  try {
    // 1. Setup legacy table schemas & dummy un-prefixed tables
    runSqlite(
      dbPath,
      `
      CREATE TABLE conversation_adapter_package_versions (id TEXT);
      CREATE TABLE conversation_adapter_catalog_releases (id TEXT);
      CREATE TABLE conversation_adapter_packages (id TEXT);
      CREATE TABLE agent_installations (id TEXT);
      CREATE TABLE app_conversation_adapter_packages (
        package_id TEXT NOT NULL PRIMARY KEY,
        adapter_id TEXT NOT NULL,
        name TEXT NOT NULL,
        version TEXT NOT NULL,
        origin TEXT NOT NULL,
        install_dir TEXT,
        runtime_ready INTEGER NOT NULL,
        runtime_gate_status TEXT NOT NULL,
        error_message TEXT,
        last_checked_at TEXT,
        runtime_validated_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        record_kind TEXT,
        manifest_path TEXT,
        adapter_manifest_path TEXT,
        runtime_protocol TEXT,
        installed_content_hash TEXT,
        trusted_package_hash TEXT,
        source_url TEXT,
        git_ref TEXT,
        git_commit TEXT,
        catalog_url TEXT,
        update_policy TEXT,
        latest_version TEXT
      );
      CREATE TABLE app_conversation_adapter_package_versions (
        package_id TEXT NOT NULL,
        version TEXT NOT NULL,
        install_dir TEXT NOT NULL,
        artifact_hash TEXT,
        content_hash TEXT NOT NULL,
        runtime_gate_status TEXT NOT NULL,
        installed_at TEXT NOT NULL
      );
      CREATE TABLE app_conversation_adapter_catalog_releases (
        catalog_url TEXT NOT NULL,
        package_id TEXT NOT NULL,
        version TEXT NOT NULL,
        adapter_id TEXT NOT NULL,
        name TEXT NOT NULL,
        publisher TEXT NOT NULL,
        channel TEXT NOT NULL,
        released_at TEXT,
        artifact_url TEXT NOT NULL,
        artifact_size INTEGER,
        artifact_sha256 TEXT,
        core_compatibility TEXT,
        changelog_markdown TEXT,
        breaking_change INTEGER,
        runtime_protocol TEXT,
        record_kind TEXT,
        package_manifest_file TEXT,
        adapter_manifest_file TEXT,
        adapter_manifest_json TEXT,
        source_json TEXT,
        etag TEXT,
        fetched_at TEXT NOT NULL
      );
      CREATE TABLE app_agent_installations (
        agent_id TEXT NOT NULL PRIMARY KEY,
        installation_id TEXT NOT NULL,
        display_name TEXT NOT NULL,
        catalog_item_version TEXT NOT NULL,
        agent_version TEXT NOT NULL,
        protocol TEXT NOT NULL,
        distribution_id TEXT NOT NULL,
        distribution_type TEXT NOT NULL,
        ownership TEXT NOT NULL,
        install_dir TEXT,
        resolved_program TEXT NOT NULL,
        args_json TEXT NOT NULL,
        definition_json TEXT NOT NULL,
        integrity_json TEXT,
        source_registry TEXT NOT NULL,
        catalog_version TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        installation_status TEXT NOT NULL,
        runtime_status TEXT NOT NULL,
        runtime_error_code TEXT,
        runtime_error_message TEXT,
        runtime_checked_at TEXT,
        protocol_status TEXT NOT NULL,
        protocol_error_code TEXT,
        protocol_error_message TEXT,
        protocol_checked_at TEXT,
        model_status TEXT,
        model_error_code TEXT,
        model_checked_at TEXT,
        installed_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
      );
      `
    );

    // Insert test records:
    // Row 1: Modern ACP binary (ready)
    runSqlite(
      dbPath,
      `INSERT INTO app_agent_installations VALUES (
        'antigravity', 'inst-agy-1', 'AntiGravity', '1.0.0', '1.0.0',
        'acp', 'dist-bin', 'binary', 'managed', '/apps/agy',
        '/apps/agy/bin', '[]', '{"acp":true}', '{"sha256":"hash-agy","content_hash":"chash-agy"}',
        'curated', '2026.10.1', 1, 'ready', 'ready', NULL, NULL, '2026-10-31T00:00:00Z',
        'ready', NULL, NULL, '2026-10-31T00:00:00Z', 'ready', NULL, '2026-10-31T00:00:00Z',
        '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z'
      );`
    );

    // Row 2: Legacy native protocol (must be downgraded to incompatible + agent_reinstall_required)
    runSqlite(
      dbPath,
      `INSERT INTO app_agent_installations VALUES (
        'legacy-native-agent', 'inst-nat-1', 'Legacy Native', '0.9.0', '0.9.0',
        'native', 'dist-sys', 'system', 'system', NULL,
        '/usr/bin/legacy', '[]', '{"native":true}', NULL,
        'curated', '2026.08.1', 1, 'ready', 'ready', NULL, NULL, '2026-08-01T00:00:00Z',
        'ready', NULL, NULL, '2026-08-01T00:00:00Z', NULL, NULL, NULL,
        '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z'
      );`
    );

    // Row 3: Legacy uvx distribution (must be downgraded to incompatible + agent_reinstall_required)
    runSqlite(
      dbPath,
      `INSERT INTO app_agent_installations VALUES (
        'legacy-uvx-agent', 'inst-uvx-1', 'Legacy UVX', '0.8.0', '0.8.0',
        'acp', 'dist-uvx', 'uvx', 'managed', '/apps/uvx',
        '/apps/uvx/bin', '[]', '{}', NULL,
        'curated', '2026.08.1', 1, 'ready', 'ready', NULL, NULL, '2026-08-01T00:00:00Z',
        'ready', NULL, NULL, '2026-08-01T00:00:00Z', NULL, NULL, NULL,
        '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z'
      );`
    );

    // 2. Execute full migration script
    const sqlContent = spawnSync("cat", [migrationSqlPath], { encoding: "utf8" }).stdout;
    runSqlite(dbPath, sqlContent);

    // 3. Verify migrated rows in app_extensions
    const extensions = querySqliteJson(
      dbPath,
      "SELECT extension_id, capability_kind, distribution_type, installation_status, runtime_status, error_code FROM app_extensions WHERE capability_kind = 'agent' ORDER BY extension_id;"
    );
    assert.equal(extensions.length, 3);

    // antigravity: ready
    assert.equal(extensions[0].extension_id, "antigravity");
    assert.equal(extensions[0].distribution_type, "binary");
    assert.equal(extensions[0].installation_status, "ready");
    assert.equal(extensions[0].runtime_status, "ready");
    assert.equal(extensions[0].error_code, null);

    // legacy-native-agent: downgraded
    assert.equal(extensions[1].extension_id, "legacy-native-agent");
    assert.equal(extensions[1].installation_status, "incompatible");
    assert.equal(extensions[1].runtime_status, "failed");
    assert.equal(extensions[1].error_code, "agent_reinstall_required");
    // distribution_type in app_extensions has CHECK constraint ('binary', 'npx', 'archive', 'workspace')
    assert.equal(extensions[1].distribution_type, "binary");

    // legacy-uvx-agent: downgraded
    assert.equal(extensions[2].extension_id, "legacy-uvx-agent");
    assert.equal(extensions[2].installation_status, "incompatible");
    assert.equal(extensions[2].runtime_status, "failed");
    assert.equal(extensions[2].error_code, "agent_reinstall_required");
    assert.equal(extensions[2].distribution_type, "binary");

    // 4. Verify app_extension_versions populated
    const versions = querySqliteJson(
      dbPath,
      "SELECT extension_id, version, artifact_sha256, verification_status FROM app_extension_versions WHERE extension_id = 'antigravity';"
    );
    assert.equal(versions.length, 1);
    assert.equal(versions[0].version, "1.0.0");
    assert.equal(versions[0].artifact_sha256, "hash-agy");
    assert.equal(versions[0].verification_status, "ready");

    const legacyVersions = querySqliteJson(
      dbPath,
      "SELECT extension_id, version, verification_status FROM app_extension_versions WHERE extension_id = 'legacy-native-agent';"
    );
    assert.equal(legacyVersions[0].verification_status, "incompatible");

    // 5. Verify the 5 legacy tables are DROPPED
    const droppedTables = [
      "conversation_adapter_package_versions",
      "conversation_adapter_catalog_releases",
      "conversation_adapter_packages",
      "agent_installations",
      "app_agent_installations",
    ];
    for (const table of droppedTables) {
      const exists = querySqliteJson(
        dbPath,
        `SELECT COUNT(*) as count FROM sqlite_master WHERE type = 'table' AND name = '${table}';`
      );
      assert.equal(exists[0].count, 0, `Table ${table} must be dropped`);
    }

    // 6. Verify remaining 3 app_conversation_adapter_* tables still exist (reserved for M4)
    const m4Tables = [
      "app_conversation_adapter_packages",
      "app_conversation_adapter_package_versions",
      "app_conversation_adapter_catalog_releases",
    ];
    for (const table of m4Tables) {
      const exists = querySqliteJson(
        dbPath,
        `SELECT COUNT(*) as count FROM sqlite_master WHERE type = 'table' AND name = '${table}';`
      );
      assert.equal(exists[0].count, 1, `Table ${table} must still exist until M4`);
    }
  } finally {
    rmSync(fixtureDir, { force: true, recursive: true });
  }
});

// -----------------------------------------------------------------------------
// Test 4: Integrity check on source code for hardcoding / facade tricks
// -----------------------------------------------------------------------------
test("Integrity: Verify no fake/dummy implementations or hardcoded shortcuts", () => {
  const extRepoPath = path.join(repositoryRoot, "backend/asset_core/src/store/system/extension_catalog_releases.rs");
  const agentRepoPath = path.join(repositoryRoot, "backend/asset_core/src/store/system/agent_installations.rs");
  const installPath = path.join(repositoryRoot, "backend/asset_core/src/application/agents/lifecycle/install.rs");

  const extRepoSrc = spawnSync("cat", [extRepoPath], { encoding: "utf8" }).stdout;
  const agentRepoSrc = spawnSync("cat", [agentRepoPath], { encoding: "utf8" }).stdout;
  const installSrc = spawnSync("cat", [installPath], { encoding: "utf8" }).stdout;

  // Verify real SQL queries exist in repositories
  assert(extRepoSrc.includes("INSERT INTO app_extension_catalog_releases"), "Must have real batch_upsert SQL");
  assert(extRepoSrc.includes("SELECT\n                catalog_source, extension_id"), "Must have real list query");
  assert(extRepoSrc.includes("SELECT count(*) FROM app_extension_catalog_releases"), "Must have real count query");

  assert(agentRepoSrc.includes("INSERT INTO app_extensions"), "Must have real upsert SQL");
  assert(agentRepoSrc.includes("INSERT INTO app_extension_versions"), "Must have version sync SQL");
  assert(agentRepoSrc.includes("DELETE FROM app_extension_versions"), "Must have real delete SQL");

  assert(!installSrc.includes("target/download"), "Obsolete heuristic URL string replacement must be removed");
  assert(installSrc.includes("upstream_target_unavailable"), "Must fail closed with upstream_target_unavailable");
});

console.log(`\n========================================`);
console.log(`REVIEWER ADVERSARIAL RESULTS: ${passed}/${total} PASSED`);
console.log(`========================================`);
