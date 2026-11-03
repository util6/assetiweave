import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const m3MigrationSqlPath = path.join(
  repositoryRoot,
  "backend/asset_core/migrations/20261031000001_unified_app_extensions_and_agent_market.sql"
);
const m4MigrationSqlPath = path.join(
  repositoryRoot,
  "backend/asset_core/migrations/20261031000002_converge_conversation_adapters_and_drop_legacy_tables.sql"
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
  process.stdout.write(`[M4 Challenger Stress Test ${total}] ${name} ... `);
  try {
    fn();
    passed++;
    console.log("PASS");
  } catch (err) {
    console.log("FAIL");
    console.error(err);
  }
}

console.log("=== EMPIRICAL ADVERSARIAL STRESS TEST: MILESTONE M4 STORE RESILIENCE ===");

// -----------------------------------------------------------------------------
// Test 1: JSON Deserialization Torture & Database Constraints
// -----------------------------------------------------------------------------
test("DDL & SQLite CHECK constraints: json_valid rejects malformed JSON on capability_meta_json and release_meta_json", () => {
  const fixtureDir = mkdtempSync(path.join(tmpdir(), "m4-adv-json-"));
  const dbPath = path.join(fixtureDir, "test.db");
  try {
    const ddl = spawnSync("cat", [m3MigrationSqlPath], { encoding: "utf8" }).stdout.split("-- 2. Migrate data")[0];
    runSqlite(dbPath, ddl);

    // 1. Attempt malformed JSON on app_extensions
    const badExtInsert = spawnSync("sqlite3", [dbPath], {
      encoding: "utf8",
      input: `INSERT INTO app_extensions (
        extension_id, capability_kind, target_id, display_name, active_version,
        distribution_type, install_dir, enabled, installation_status, runtime_status,
        installed_at, updated_at, capability_meta_json
      ) VALUES (
        'ext-bad', 'conversation.adapter', 'target-bad', 'Bad Adapter', '1.0.0',
        'archive', NULL, 1, 'ready', 'ready',
        '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '{bad_json_not_valid'
      );`,
    });
    assert.notEqual(badExtInsert.status, 0, "sqlite3 must fail CHECK(json_valid(capability_meta_json))");
    assert.match(badExtInsert.stderr, /CHECK constraint failed/);

    // 2. Attempt malformed JSON on app_extension_catalog_releases
    const badReleaseInsert = spawnSync("sqlite3", [dbPath], {
      encoding: "utf8",
      input: `INSERT INTO app_extension_catalog_releases (
        catalog_source, extension_id, version, capability_kind, target_id,
        display_name, publisher, channel, released_at, artifact_url,
        artifact_size, artifact_sha256, release_meta_json, fetched_at
      ) VALUES (
        'https://catalog.example.com', 'ext-bad', '1.0.0', 'conversation.adapter', 'target-bad',
        'Bad Release', 'Publisher', 'stable', '2026-10-31T00:00:00Z', 'https://example.com/art.zip',
        1024, 'sha256', '{not json at all', '2026-10-31T00:00:00Z'
      );`,
    });
    assert.notEqual(badReleaseInsert.status, 0, "sqlite3 must fail CHECK(json_valid(release_meta_json))");
    assert.match(badReleaseInsert.stderr, /CHECK constraint failed/);
  } finally {
    rmSync(fixtureDir, { recursive: true, force: true });
  }
});

// -----------------------------------------------------------------------------
// Test 2: Foreign Key Cascades & Rollback Invariants
// -----------------------------------------------------------------------------
test("Foreign Key Invariants: Deleting extension cascade deletes all app_extension_versions", () => {
  const fixtureDir = mkdtempSync(path.join(tmpdir(), "m4-adv-fk-"));
  const dbPath = path.join(fixtureDir, "test.db");
  try {
    const ddl = spawnSync("cat", [m3MigrationSqlPath], { encoding: "utf8" }).stdout.split("-- 2. Migrate data")[0];
    runSqlite(dbPath, "PRAGMA foreign_keys = ON;\n" + ddl);

    // Insert extension
    runSqlite(
      dbPath,
      `PRAGMA foreign_keys = ON;
      INSERT INTO app_extensions (
        extension_id, capability_kind, target_id, display_name, active_version,
        distribution_type, install_dir, enabled, installation_status, runtime_status,
        installed_at, updated_at, capability_meta_json
      ) VALUES (
        'ext-cascade', 'conversation.adapter', 'target-cascade', 'Cascade Adapter', '1.0.0',
        'archive', '/tmp/install', 1, 'ready', 'ready',
        '2026-10-31T00:00:00Z', '2026-10-31T00:00:00Z', '{}'
      );
      INSERT INTO app_extension_versions (
        extension_id, version, install_dir, artifact_sha256, content_hash, verification_status, installed_at
      ) VALUES
        ('ext-cascade', '1.0.0', '/tmp/1', 'sha1', 'cnt1', 'ready', '2026-10-31T00:00:00Z'),
        ('ext-cascade', '1.1.0', '/tmp/2', 'sha2', 'cnt2', 'ready', '2026-10-31T00:00:00Z');`
    );

    let versions = querySqliteJson(dbPath, "SELECT version FROM app_extension_versions WHERE extension_id = 'ext-cascade'");
    assert.equal(versions.length, 2, "Must have 2 versions inserted");

    // Delete extension
    runSqlite(dbPath, "PRAGMA foreign_keys = ON; DELETE FROM app_extensions WHERE extension_id = 'ext-cascade';");

    // Verify cascade deletion
    versions = querySqliteJson(dbPath, "SELECT version FROM app_extension_versions WHERE extension_id = 'ext-cascade'");
    assert.equal(versions.length, 0, "Cascade delete must clear all versions on parent removal");
  } finally {
    rmSync(fixtureDir, { recursive: true, force: true });
  }
});

// -----------------------------------------------------------------------------
// Test 3: Rust Adversarial Store Resilience & Concurrency Suite
// -----------------------------------------------------------------------------
test("Rust Integration Suite: Execute adversarial_store_m4 test harness covering all 4 challenge areas", () => {
  const result = spawnSync("cargo", ["test", "-p", "asset_core", "--test", "adversarial_store_m4", "--", "--nocapture"], {
    cwd: repositoryRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(`cargo test failed:\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  }
  assert.match(result.stdout, /9 passed; 0 failed/);
  assert.match(
    result.stdout,
    /\[CONCURRENCY REMEDIATION\] Success: 12\/12, Lock error encountered: false/
  );
});

console.log(`\nAll ${total} empirical adversarial tests completed: ${passed}/${total} passed.`);
if (passed !== total) {
  process.exit(1);
}
