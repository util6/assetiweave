import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const claudeAdapterPath = path.join(repositoryRoot, "builtin-assets/adapters/claude-code/adapter.mjs");
const opencodeAdapterPath = path.join(repositoryRoot, "builtin-assets/adapters/opencode/adapter.mjs");
const codexAdapterPath = path.join(repositoryRoot, "builtin-assets/adapters/codex/adapter.mjs");
const zcodeAdapterPath = path.join(repositoryRoot, "builtin-assets/adapters/zcode/adapter.mjs");

function runAdapterUsage(adapterPath, location, params = {}, protocolVersion = null) {
  const payload = {
    method: "read_usage",
    source: { location },
    params,
  };
  if (protocolVersion !== null) {
    payload.protocol_version = protocolVersion;
  }
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify(payload),
  });
  assert.equal(result.status, 0, `Adapter (${path.basename(adapterPath)}) crashed: ${result.stderr}`);
  const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
  const events = messages
    .filter((m) => m.type === "usage_event" || (m.type === "item" && m.item?.kind === "usage_event"))
    .map((m) => m.usage_event || m.item?.event || m.item);
  const complete = messages.find((m) => m.type === "complete")?.item ?? null;
  return { messages, events, complete };
}

function runSqlite(dbPath, sql) {
  const target = dbPath.endsWith(".db") || dbPath.endsWith(".sqlite") ? dbPath : path.join(dbPath, "state_5.sqlite");
  const result = spawnSync("sqlite3", [target], { encoding: "utf8", input: sql });
  assert.equal(result.status, 0, `sqlite3 failed on ${target}: ${result.stderr}`);
}

function sqlString(value) {
  return String(value).replaceAll("'", "''");
}

const failures = [];
let passedTests = 0;
let totalTests = 0;

function runTest(name, fn) {
  totalTests++;
  process.stdout.write(`[TEST ${totalTests}] ${name} ... `);
  try {
    fn();
    passedTests++;
    console.log("PASS");
  } catch (err) {
    console.log("FAIL");
    console.error(err.message);
    failures.push({ name, error: err.message, stack: err.stack });
  }
}

console.log("=== EMPIRICAL ADVERSARIAL TEST HARNESS: MILESTONE M3 ===");

// ---------------------------------------------------------------------------
// SUITE 1: Claude Code Adversarial Stress Tests
// ---------------------------------------------------------------------------

runTest("Claude Code: Massive multi-step tool loop with interleaved thinking & token types", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-claude-multistep-"));
  try {
    const sessionId = "claude-adv-multistep";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    const lines = [
      // User message starting Turn 0
      JSON.stringify({
        timestamp: "2026-10-30T01:00:00.000Z",
        type: "user",
        message: { role: "user", content: [{ type: "text", text: "Refactor architecture and execute tools" }] },
      }),
      // Step 1: Tool call Glob
      JSON.stringify({
        timestamp: "2026-10-30T01:00:01.100Z",
        type: "assistant",
        message: {
          role: "assistant",
          model: "claude-3-7-sonnet-20250219",
          content: [{ type: "tool_use", id: "tool_glob_1", name: "Glob", input: { pattern: "**/*.rs" } }],
          usage: {
            input_tokens: 100,
            output_tokens: 30,
            cache_read_input_tokens: 50,
            cache_creation_input_tokens: 20,
            reasoning_tokens: 15,
          },
        },
      }),
      // Tool result 1
      JSON.stringify({
        timestamp: "2026-10-30T01:00:02.000Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "tool_glob_1", content: "main.rs\nlib.rs" }],
        },
      }),
      // Step 2: Tool call Read
      JSON.stringify({
        timestamp: "2026-10-30T01:00:03.200Z",
        type: "assistant",
        message: {
          role: "assistant",
          model: "claude-3-7-sonnet-20250219",
          content: [{ type: "tool_use", id: "tool_read_1", name: "Read", input: { file: "main.rs" } }],
          usage: {
            input_tokens: 150,
            output_tokens: 40,
            cache_read_input_tokens: 60,
            cache_creation_input_tokens: 0,
            reasoning_tokens: 25,
          },
        },
      }),
      // Tool result 2
      JSON.stringify({
        timestamp: "2026-10-30T01:00:04.000Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "tool_read_1", content: "fn main() {}" }],
        },
      }),
      // Step 3: Tool call Edit
      JSON.stringify({
        timestamp: "2026-10-30T01:00:05.300Z",
        type: "assistant",
        message: {
          role: "assistant",
          model: "claude-3-7-sonnet-20250219",
          content: [{ type: "tool_use", id: "tool_edit_1", name: "Edit", input: { file: "main.rs" } }],
          usage: {
            input_tokens: 200,
            output_tokens: 50,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 30,
            reasoning_tokens: 35,
          },
        },
      }),
      // Tool result 3
      JSON.stringify({
        timestamp: "2026-10-30T01:00:06.000Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "tool_edit_1", content: "Success" }],
        },
      }),
      // Step 4: Final Assistant answer
      JSON.stringify({
        timestamp: "2026-10-30T01:00:07.400Z",
        type: "assistant",
        message: {
          role: "assistant",
          model: "claude-3-7-sonnet-20250219",
          content: [{ type: "text", text: "Refactoring completed successfully." }],
          usage: {
            input_tokens: 250,
            output_tokens: 80,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_tokens: 40,
          },
        },
      }),
    ];
    writeFileSync(sessionFile, lines.join("\n"));

    const { events, complete } = runAdapterUsage(claudeAdapterPath, fixtureRoot);
    assert.equal(complete.usage_event_count, 1, "Should emit exactly 1 accumulated turn event");
    assert.equal(events.length, 1);

    const ev = events[0];
    assert.equal(ev.turn_id, "turn-0");
    assert.equal(ev.external_event_id, `${sessionId}-turn-0`);
    assert.equal(ev.logical_request_id, `${sessionId}-turn-0`);
    assert.equal(ev.timestamp, "2026-10-30T01:00:01.100Z", "Must preserve first assistant response timestamp");
    assert.equal(ev.input_tokens, 100 + 150 + 200 + 250); // 700
    assert.equal(ev.output_tokens, 30 + 40 + 50 + 80); // 200
    assert.equal(ev.cache_read_tokens, 50 + 60); // 110
    assert.equal(ev.cache_write_tokens, 20 + 30); // 50
    assert.equal(ev.reasoning_tokens, 15 + 25 + 35 + 40); // 115
    assert.equal(ev.total_tokens, 700 + 200 + 110 + 50 + 115); // 1175
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Claude Code: Cross-day continuation sessions retain separate historical UTC timestamps", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-claude-crossday-"));
  try {
    const sessionId = "claude-crossday-sess";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    const lines = [
      // Day 1
      JSON.stringify({ timestamp: "2026-08-01T10:00:00.000Z", type: "user", message: { role: "user", content: "Day 1 query" } }),
      JSON.stringify({
        timestamp: "2026-08-01T10:00:02.123Z",
        type: "assistant",
        message: { role: "assistant", content: "Day 1 answer", usage: { input_tokens: 50, output_tokens: 25 } },
      }),
      // Day 2
      JSON.stringify({ timestamp: "2026-08-02T11:00:00.000Z", type: "user", message: { role: "user", content: "Day 2 query" } }),
      JSON.stringify({
        timestamp: "2026-08-02T11:00:03.456Z",
        type: "assistant",
        message: { role: "assistant", content: "Day 2 answer", usage: { input_tokens: 60, output_tokens: 30 } },
      }),
      // Day 3
      JSON.stringify({ timestamp: "2026-08-03T12:00:00.000Z", type: "user", message: { role: "user", content: "Day 3 query" } }),
      JSON.stringify({
        timestamp: "2026-08-03T12:00:04.789Z",
        type: "assistant",
        message: { role: "assistant", content: "Day 3 answer", usage: { input_tokens: 70, output_tokens: 35 } },
      }),
    ];
    writeFileSync(sessionFile, lines.join("\n"));
    // Artificially change file mtime to months later
    const futureDate = new Date("2026-10-30T12:00:00.000Z");
    utimesSync(sessionFile, futureDate, futureDate);

    const { events } = runAdapterUsage(claudeAdapterPath, fixtureRoot);
    assert.equal(events.length, 3);

    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-0`);
    assert.equal(events[0].timestamp, "2026-08-01T10:00:02.123Z");

    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[1].timestamp, "2026-08-02T11:00:03.456Z");

    assert.equal(events[2].turn_id, "turn-2");
    assert.equal(events[2].external_event_id, `${sessionId}-turn-2`);
    assert.equal(events[2].timestamp, "2026-08-03T12:00:04.789Z");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Claude Code: Suppresses zero-token turns while preserving turn indices and deterministic IDs", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-claude-zero-"));
  try {
    const sessionId = "claude-zero-suppress";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    const lines = [
      // Turn 0: Zero tokens
      JSON.stringify({ timestamp: "2026-10-30T02:00:00Z", type: "user", message: { role: "user", content: "Turn 0 zero" } }),
      JSON.stringify({ timestamp: "2026-10-30T02:00:01Z", type: "assistant", message: { role: "assistant", content: "no usage" } }),
      // Turn 1: Valid tokens
      JSON.stringify({ timestamp: "2026-10-30T02:01:00Z", type: "user", message: { role: "user", content: "Turn 1 valid" } }),
      JSON.stringify({
        timestamp: "2026-10-30T02:01:02Z",
        type: "assistant",
        message: { role: "assistant", content: "answer 1", usage: { input_tokens: 80, output_tokens: 40 } },
      }),
      // Turn 2: Explicit 0 tokens
      JSON.stringify({ timestamp: "2026-10-30T02:02:00Z", type: "user", message: { role: "user", content: "Turn 2 zero" } }),
      JSON.stringify({
        timestamp: "2026-10-30T02:02:02Z",
        type: "assistant",
        message: { role: "assistant", content: "answer 2", usage: { input_tokens: 0, output_tokens: 0 } },
      }),
      // Turn 3: Valid tokens
      JSON.stringify({ timestamp: "2026-10-30T02:03:00Z", type: "user", message: { role: "user", content: "Turn 3 valid" } }),
      JSON.stringify({
        timestamp: "2026-10-30T02:03:02Z",
        type: "assistant",
        message: { role: "assistant", content: "answer 3", usage: { input_tokens: 90, output_tokens: 50 } },
      }),
    ];
    writeFileSync(sessionFile, lines.join("\n"));

    const { events } = runAdapterUsage(claudeAdapterPath, fixtureRoot);
    assert.equal(events.length, 2, "Turns 0 and 2 must be suppressed");

    assert.equal(events[0].turn_id, "turn-1");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[0].total_tokens, 120);

    assert.equal(events[1].turn_id, "turn-3");
    assert.equal(events[1].external_event_id, `${sessionId}-turn-3`);
    assert.equal(events[1].total_tokens, 140);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Claude Code: Handles corrupted lines, non-JSON lines, and sidechain subagents gracefully", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-claude-corrupt-"));
  try {
    const sessionId = "claude-corrupted";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    const lines = [
      "not a json line",
      "{ bad: json }",
      "",
      JSON.stringify({ timestamp: "2026-10-30T03:00:00Z", type: "user", message: { role: "user", content: "Hello" } }),
      // Sidechain message should NOT trigger turn flush or break current turn
      JSON.stringify({
        timestamp: "2026-10-30T03:00:01Z",
        type: "user",
        isSidechain: true,
        message: { role: "user", content: "subagent query" },
      }),
      JSON.stringify({
        timestamp: "2026-10-30T03:00:02Z",
        type: "assistant",
        message: { role: "assistant", content: "Hello there!", usage: { input_tokens: 45, output_tokens: 15 } },
      }),
      "{ incomplete json",
    ];
    writeFileSync(sessionFile, lines.join("\n"));

    const { events } = runAdapterUsage(claudeAdapterPath, fixtureRoot);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].total_tokens, 60);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 2: OpenCode Adversarial Stress Tests
// ---------------------------------------------------------------------------

runTest("OpenCode: Schema variability — Table without time_created column", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-opencode-notime-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msg1 = JSON.stringify({
      role: "assistant",
      modelID: "claude-3-5-sonnet",
      providerID: "anthropic",
      time: { created: "2026-10-25T14:30:00.123Z" },
      tokens: { input: 120, output: 60, reasoning: 30, cache: { read: 20, write: 10 } },
    });
    const msg2 = JSON.stringify({
      role: "assistant",
      modelID: "gpt-4o",
      providerID: "openai",
      time: { created: 1761400000000 },
      tokens: { input: 80, output: 40, total: 120 },
    });

    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      // Notice: NO time_created column in message table
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('opencode-sess-1', 'Test', '/tmp', '2026-10-25T14:30:00Z');",
      `INSERT INTO message VALUES ('msg-1', 'opencode-sess-1', '${sqlString(msg1)}');`,
      `INSERT INTO message VALUES ('msg-2', 'opencode-sess-1', '${sqlString(msg2)}');`,
    ].join("\n"));

    const { events, complete } = runAdapterUsage(opencodeAdapterPath, dbPath);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(events.length, 2);

    assert.equal(events[0].turn_id, "msg-1");
    assert.equal(events[0].external_event_id, "opencode-sess-1-msg-1");
    assert.equal(events[0].timestamp, "2026-10-25T14:30:00.123Z");
    assert.equal(events[0].input_tokens, 120);
    assert.equal(events[0].output_tokens, 60);
    assert.equal(events[0].reasoning_tokens, 30);
    assert.equal(events[0].cache_read_tokens, 20);
    assert.equal(events[0].cache_write_tokens, 10);
    assert.equal(events[0].total_tokens, 240);

    assert.equal(events[1].turn_id, "msg-2");
    assert.equal(events[1].external_event_id, "opencode-sess-1-msg-2");
    assert.equal(events[1].timestamp, new Date(1761400000000).toISOString());
    assert.equal(events[1].total_tokens, 120);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("OpenCode: Schema variability — Table with NULL time_created falling back to data.time.created", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-opencode-nulltime-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      time: { created: "2026-10-26T10:00:00.500Z" },
      tokens: { input: 50, output: 25, total: 75 },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('sess-null-ts', 'Test');",
      // time_created is explicitly NULL
      `INSERT INTO message VALUES ('msg-null-ts', 'sess-null-ts', NULL, '${sqlString(msgData)}');`,
    ].join("\n"));

    const { events } = runAdapterUsage(opencodeAdapterPath, dbPath);
    assert.equal(events.length, 1);
    assert.equal(events[0].timestamp, "2026-10-26T10:00:00.500Z");
    assert.equal(events[0].total_tokens, 75);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("OpenCode: Schema variability — Jumbled column order and extra columns", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-opencode-jumbled-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      tokens: { input: 100, output: 50, total: 150 },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT);",
      // Completely jumbled columns with extra columns
      "CREATE TABLE message (extra_a TEXT, data TEXT, session_id TEXT, tenant_id TEXT, id TEXT, time_created INTEGER, extra_b REAL);",
      "INSERT INTO session VALUES ('sess-jumbled', 'Test');",
      `INSERT INTO message VALUES ('extra1', '${sqlString(msgData)}', 'sess-jumbled', 'tenant-1', 'm-99', 1761800000000, 3.14);`,
    ].join("\n"));

    const { events } = runAdapterUsage(opencodeAdapterPath, dbPath);
    assert.equal(events.length, 1);
    assert.equal(events[0].session_id, "sess-jumbled");
    assert.equal(events[0].turn_id, "msg-m-99");
    assert.equal(events[0].external_event_id, "sess-jumbled-msg-m-99");
    assert.equal(events[0].total_tokens, 150);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("OpenCode: ID normalization idempotency (msg- prefix not duplicated)", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-opencode-id-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      tokens: { input: 10, output: 10, total: 20 },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('s-1', 'Test');",
      `INSERT INTO message VALUES ('raw-id-1', 's-1', 1000, '${sqlString(msgData)}');`,
      `INSERT INTO message VALUES ('msg-prefixed-2', 's-1', 2000, '${sqlString(msgData)}');`,
    ].join("\n"));

    const { events } = runAdapterUsage(opencodeAdapterPath, dbPath);
    assert.equal(events.length, 2);
    assert.equal(events[0].turn_id, "msg-raw-id-1");
    assert.equal(events[0].external_event_id, "s-1-msg-raw-id-1");
    assert.equal(events[1].turn_id, "msg-prefixed-2");
    assert.equal(events[1].external_event_id, "s-1-msg-prefixed-2");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("OpenCode: Incremental cursor filtering when table lacks time_created column", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-opencode-cursor-notime-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const msg1 = JSON.stringify({ role: "assistant", time: { created: 10000 }, tokens: { input: 10, output: 10, total: 20 } });
    const msg2 = JSON.stringify({ role: "assistant", time: { created: 20000 }, tokens: { input: 20, output: 20, total: 40 } });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('sess-c', 'Test');",
      `INSERT INTO message VALUES ('msg-1', 'sess-c', '${sqlString(msg1)}');`,
      `INSERT INTO message VALUES ('msg-2', 'sess-c', '${sqlString(msg2)}');`,
    ].join("\n"));

    const firstRun = runAdapterUsage(opencodeAdapterPath, dbPath);
    assert.equal(firstRun.events.length, 2);
    const cursor = JSON.parse(firstRun.complete.next_cursor);
    assert.equal(cursor.last_time_created, 20000);

    // Second run with cursor should filter out both msg-1 and msg-2
    const secondRun = runAdapterUsage(opencodeAdapterPath, dbPath, { cursor: firstRun.complete.next_cursor });
    assert.equal(secondRun.events.length, 0);

    // Insert msg-3 with later timestamp
    const msg3 = JSON.stringify({ role: "assistant", time: { created: 30000 }, tokens: { input: 30, output: 30, total: 60 } });
    runSqlite(dbPath, [`INSERT INTO message VALUES ('msg-3', 'sess-c', '${sqlString(msg3)}');`].join("\n"));

    const thirdRun = runAdapterUsage(opencodeAdapterPath, dbPath, { cursor: firstRun.complete.next_cursor });
    assert.equal(thirdRun.events.length, 1);
    assert.equal(thirdRun.events[0].turn_id, "msg-3");
    assert.equal(thirdRun.events[0].total_tokens, 60);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 3: Codex Adversarial Stress Tests
// ---------------------------------------------------------------------------

runTest("Codex: Multi-step tool loop with interleaved reasoning and token_count events", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-codex-multistep-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const lines = [
      JSON.stringify({ timestamp: "2026-07-27T00:00:00.000Z", type: "session_meta", payload: { model: "o3-mini", model_provider: "openai" } }),
      JSON.stringify({ timestamp: "2026-07-27T00:00:01.000Z", type: "turn_context", payload: { model: "o3-mini" } }),
      JSON.stringify({ timestamp: "2026-07-27T00:00:01.100Z", type: "message", role: "user", content: "Optimize SQL performance" }),
      // Tool 1
      JSON.stringify({
        timestamp: "2026-07-27T00:00:02.000Z",
        type: "response_item",
        payload: { type: "custom_tool_call", name: "exec", call_id: "c1" },
      }),
      JSON.stringify({
        timestamp: "2026-07-27T00:00:03.000Z",
        type: "event_msg",
        payload: {
          type: "token_count",
          info: {
            last_token_usage: {
              input_tokens: 100,
              cached_input_tokens: 40,
              output_tokens: 30,
              reasoning_output_tokens: 25,
              total_tokens: 195,
            },
          },
        },
      }),
      // Tool 2
      JSON.stringify({
        timestamp: "2026-07-27T00:00:04.000Z",
        type: "response_item",
        payload: { type: "custom_tool_call", name: "exec", call_id: "c2" },
      }),
      JSON.stringify({
        timestamp: "2026-07-27T00:00:05.000Z",
        type: "event_msg",
        payload: {
          type: "token_count",
          info: {
            last_token_usage: {
              input_tokens: 120,
              cached_input_tokens: 20,
              output_tokens: 40,
              reasoning_output_tokens: 35,
              total_tokens: 215,
            },
          },
        },
      }),
      // Tool 3 (final answer)
      JSON.stringify({
        timestamp: "2026-07-27T00:00:06.000Z",
        type: "event_msg",
        payload: {
          type: "token_count",
          info: {
            last_token_usage: {
              input_tokens: 150,
              cached_input_tokens: 10,
              output_tokens: 50,
              reasoning_output_tokens: 40,
              total_tokens: 250,
            },
          },
        },
      }),
    ];
    writeFileSync(rolloutPath, lines.join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-codex-multi', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:07Z');`,
    ].join("\n"));

    const { events, complete } = runAdapterUsage(codexAdapterPath, fixtureRoot);
    assert.equal(complete.usage_event_count, 1, "Must accumulate all 3 token_count events into 1 Turn");
    assert.equal(events.length, 1);

    const ev = events[0];
    assert.equal(ev.session_id, "sess-codex-multi");
    assert.equal(ev.turn_id, "turn-0");
    assert.equal(ev.external_event_id, "sess-codex-multi-turn-0");
    assert.equal(ev.timestamp, "2026-07-27T00:00:01.000Z");
    assert.equal(ev.input_tokens, 100 + 120 + 150); // 370
    assert.equal(ev.cache_read_tokens, 40 + 20 + 10); // 70
    assert.equal(ev.output_tokens, 30 + 40 + 50); // 120
    assert.equal(ev.reasoning_tokens, 25 + 35 + 40); // 100
    assert.equal(ev.total_tokens, 370 + 70 + 120 + 100); // 660
    assert.equal(ev.model, "o3-mini");
    assert.equal(ev.provider, "openai");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Codex: Cross-day multi-turn session preserves historical UTC timestamps", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-codex-crossday-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const lines = [
      // Day 1
      JSON.stringify({ timestamp: "2026-07-01T10:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-01T10:00:01.000Z", type: "message", role: "user", content: "Day 1 query" }),
      JSON.stringify({
        timestamp: "2026-07-01T10:00:02.000Z",
        type: "event_msg",
        payload: { type: "token_count", info: { last_token_usage: { input_tokens: 50, output_tokens: 25, total_tokens: 75 } } },
      }),
      // Day 2
      JSON.stringify({ timestamp: "2026-07-02T11:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-02T11:00:01.000Z", type: "message", role: "user", content: "Day 2 query" }),
      JSON.stringify({
        timestamp: "2026-07-02T11:00:02.000Z",
        type: "event_msg",
        payload: { type: "token_count", info: { last_token_usage: { input_tokens: 60, output_tokens: 30, total_tokens: 90 } } },
      }),
    ];
    writeFileSync(rolloutPath, lines.join("\n"));
    const futureDate = new Date("2026-10-30T12:00:00.000Z");
    utimesSync(rolloutPath, futureDate, futureDate);

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-codex-cd', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-02T12:00:00Z');`,
    ].join("\n"));

    const { events } = runAdapterUsage(codexAdapterPath, fixtureRoot);
    assert.equal(events.length, 2);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].timestamp, "2026-07-01T10:00:00.000Z");
    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].timestamp, "2026-07-02T11:00:00.000Z");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Codex: Empty turn 0 (zero tokens) followed by Turn 1 on next day with tokens", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-codex-empty0-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const lines = [
      // Turn 0 on Day 1: User message, but NO tokens produced (empty turn)
      JSON.stringify({ timestamp: "2026-07-01T10:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-01T10:00:01.000Z", type: "message", role: "user", content: "Day 1 query with no reply" }),
      // Turn 1 on Day 2: User message, has tokens
      JSON.stringify({ timestamp: "2026-07-02T11:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-02T11:00:01.000Z", type: "message", role: "user", content: "Day 2 query with reply" }),
      JSON.stringify({
        timestamp: "2026-07-02T11:00:02.000Z",
        type: "event_msg",
        payload: { type: "token_count", info: { last_token_usage: { input_tokens: 60, output_tokens: 30, total_tokens: 90 } } },
      }),
    ];
    writeFileSync(rolloutPath, lines.join("\n"));
    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-codex-empty0', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-02T12:00:00Z');`,
    ].join("\n"));

    const { events } = runAdapterUsage(codexAdapterPath, fixtureRoot);
    assert.equal(events.length, 1, "Only Turn 1 should produce an event");
    assert.equal(events[0].timestamp, "2026-07-02T11:00:00.000Z", "Event timestamp must be Day 2, not Day 1!");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Codex: Valid Turn 0, Empty Turn 1, Valid Turn 2 across 3 days", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-codex-empty1-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const lines = [
      // Turn 0 on Day 1: Has tokens
      JSON.stringify({ timestamp: "2026-07-01T10:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-01T10:00:01.000Z", type: "message", role: "user", content: "Day 1 query" }),
      JSON.stringify({
        timestamp: "2026-07-01T10:00:02.000Z",
        type: "event_msg",
        payload: { type: "token_count", info: { last_token_usage: { input_tokens: 50, output_tokens: 25, total_tokens: 75 } } },
      }),
      // Turn 1 on Day 2: User message, but NO tokens produced (empty turn)
      JSON.stringify({ timestamp: "2026-07-02T11:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-02T11:00:01.000Z", type: "message", role: "user", content: "Day 2 query with no reply" }),
      // Turn 2 on Day 3: Has tokens
      JSON.stringify({ timestamp: "2026-07-03T12:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-03T12:00:01.000Z", type: "message", role: "user", content: "Day 3 query with reply" }),
      JSON.stringify({
        timestamp: "2026-07-03T12:00:02.000Z",
        type: "event_msg",
        payload: { type: "token_count", info: { last_token_usage: { input_tokens: 80, output_tokens: 40, total_tokens: 120 } } },
      }),
    ];
    writeFileSync(rolloutPath, lines.join("\n"));
    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-codex-empty1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-03T13:00:00Z');`,
    ].join("\n"));

    const { events } = runAdapterUsage(codexAdapterPath, fixtureRoot);
    assert.equal(events.length, 2, "Turn 0 and Turn 2 should produce events (Turn 1 suppressed)");
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].timestamp, "2026-07-01T10:00:00.000Z");
    assert.equal(events[0].total_tokens, 75);

    assert.equal(events[1].turn_id, "turn-2", "Turn 2 ID must be turn-2, not turn-1");
    assert.equal(events[1].timestamp, "2026-07-03T12:00:00.000Z", "Turn 2 timestamp must be Day 3, not Day 2!");
    assert.equal(events[1].total_tokens, 120);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Codex: Turn boundary variations (with turn_context vs message user only)", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-codex-boundaries-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const lines = [
      // Turn 0: Started ONLY by message user (no preceding turn_context)
      JSON.stringify({ timestamp: "2026-07-10T08:00:00.000Z", type: "message", role: "user", content: "Only user message" }),
      JSON.stringify({
        timestamp: "2026-07-10T08:00:01.000Z",
        type: "event_msg",
        payload: { type: "token_count", info: { last_token_usage: { input_tokens: 30, output_tokens: 15, total_tokens: 45 } } },
      }),
      // Turn 1: Started by turn_context AND user message
      JSON.stringify({ timestamp: "2026-07-10T08:05:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
      JSON.stringify({ timestamp: "2026-07-10T08:05:00.100Z", type: "message", role: "user", content: "With context" }),
      JSON.stringify({
        timestamp: "2026-07-10T08:05:02.000Z",
        type: "event_msg",
        payload: { type: "token_count", info: { last_token_usage: { input_tokens: 40, output_tokens: 20, total_tokens: 60 } } },
      }),
    ];
    writeFileSync(rolloutPath, lines.join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-boundaries', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-10T08:10:00Z');`,
    ].join("\n"));

    const { events } = runAdapterUsage(codexAdapterPath, fixtureRoot);
    assert.equal(events.length, 2);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, "sess-boundaries-turn-0");
    assert.equal(events[0].total_tokens, 45);

    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, "sess-boundaries-turn-1");
    assert.equal(events[1].total_tokens, 60);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 4: ZCode Adversarial Stress Tests
// ---------------------------------------------------------------------------

runTest("ZCode: Schema variability — Table without time_created column", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-zcode-notime-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const msg1 = JSON.stringify({
      role: "assistant",
      modelID: "zcode-code-v1",
      providerID: "zcode-engine",
      time: { created: "2026-10-28T09:15:00.456Z" },
      tokens: { input: 110, output: 55, reasoning: 25, cache: { read: 15, write: 5 }, total: 210 },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      // No time_created column
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('z-adv-1', 'Test', 1761800000000);",
      `INSERT INTO message VALUES ('101', 'z-adv-1', '${sqlString(msg1)}');`,
    ].join("\n"));

    const { events, complete } = runAdapterUsage(zcodeAdapterPath, dbPath, {}, 1);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(events.length, 1);

    const ev = events[0];
    assert.equal(ev.session_id, "z-adv-1");
    assert.equal(ev.turn_id, "msg-101");
    assert.equal(ev.external_event_id, "z-adv-1-msg-101");
    assert.equal(ev.timestamp, "2026-10-28T09:15:00.456Z");
    assert.equal(ev.input_tokens, 110);
    assert.equal(ev.output_tokens, 55);
    assert.equal(ev.reasoning_tokens, 25);
    assert.equal(ev.cache_read_tokens, 15);
    assert.equal(ev.cache_write_tokens, 5);
    assert.equal(ev.total_tokens, 210);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("ZCode: Schema variability — NULL time_created and column order variations", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-zcode-nulltime-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      time: { created: "2026-10-29T16:00:00.000Z" },
      tokens: { input: 70, output: 35, total: 105 },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      // Reordered columns with NULL time_created
      "CREATE TABLE message (data TEXT, session_id TEXT, id TEXT, time_created INTEGER);",
      "INSERT INTO session VALUES ('z-adv-2', 'Test', 1761800000000);",
      `INSERT INTO message VALUES ('${sqlString(msgData)}', 'z-adv-2', 'msg-202', NULL);`,
    ].join("\n"));

    const { events } = runAdapterUsage(zcodeAdapterPath, dbPath, {}, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "msg-202", "Must not duplicate msg- prefix");
    assert.equal(events[0].external_event_id, "z-adv-2-msg-202");
    assert.equal(events[0].timestamp, "2026-10-29T16:00:00.000Z");
    assert.equal(events[0].total_tokens, 105);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("ZCode: Incremental cursor filtering when table lacks time_created column", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-zcode-cursor-notime-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const msg1 = JSON.stringify({ role: "assistant", time: { created: 1000 }, tokens: { input: 10, output: 10, total: 20 } });
    const msg2 = JSON.stringify({ role: "assistant", time: { created: 2000 }, tokens: { input: 20, output: 20, total: 40 } });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('z-sess-c', 'Test', 1761800000000);",
      `INSERT INTO message VALUES ('m-1', 'z-sess-c', '${sqlString(msg1)}');`,
      `INSERT INTO message VALUES ('m-2', 'z-sess-c', '${sqlString(msg2)}');`,
    ].join("\n"));

    const firstRun = runAdapterUsage(zcodeAdapterPath, dbPath, {}, 1);
    assert.equal(firstRun.events.length, 2);
    const cursor = JSON.parse(firstRun.complete.next_cursor);
    assert.equal(cursor.last_time_created, 2000);

    const secondRun = runAdapterUsage(zcodeAdapterPath, dbPath, { cursor: firstRun.complete.next_cursor }, 1);
    assert.equal(secondRun.events.length, 0);

    const msg3 = JSON.stringify({ role: "assistant", time: { created: 3000 }, tokens: { input: 30, output: 30, total: 60 } });
    runSqlite(dbPath, [`INSERT INTO message VALUES ('m-3', 'z-sess-c', '${sqlString(msg3)}');`].join("\n"));

    const thirdRun = runAdapterUsage(zcodeAdapterPath, dbPath, { cursor: firstRun.complete.next_cursor }, 1);
    assert.equal(thirdRun.events.length, 1);
    assert.equal(thirdRun.events[0].turn_id, "msg-m-3");
    assert.equal(thirdRun.events[0].total_tokens, 60);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 5: Cross-Adapter Contract & Invalidation Invariant Verification
// ---------------------------------------------------------------------------

runTest("Cross-Adapter Contract Invariants: Verify all events satisfy R1, R2, R3 invariants", () => {
  const isoUtcPattern = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/;
  const eventIdPattern = /^[a-zA-Z0-9_\-\.]+$/;

  // Run all four adapters with standard valid multi-turn sessions
  const claudeRoot = mkdtempSync(path.join(tmpdir(), "inv-claude-"));
  const opencodeRoot = mkdtempSync(path.join(tmpdir(), "inv-opencode-"));
  const codexRoot = mkdtempSync(path.join(tmpdir(), "inv-codex-"));
  const zcodeRoot = mkdtempSync(path.join(tmpdir(), "inv-zcode-"));

  try {
    // Claude fixture
    writeFileSync(
      path.join(claudeRoot, "inv-sess.jsonl"),
      [
        JSON.stringify({ timestamp: "2026-10-30T04:00:00Z", type: "user", message: { role: "user", content: "Hi" } }),
        JSON.stringify({
          timestamp: "2026-10-30T04:00:01.123Z",
          type: "assistant",
          message: { role: "assistant", content: "Hello", usage: { input_tokens: 20, output_tokens: 10 } },
        }),
      ].join("\n")
    );
    // OpenCode fixture
    const openDb = path.join(opencodeRoot, "opencode.db");
    runSqlite(openDb, [
      "CREATE TABLE session (id TEXT, title TEXT);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('s-inv', 'T');",
      `INSERT INTO message VALUES ('msg-1', 's-inv', 1761800000000, '${sqlString(JSON.stringify({ role: "assistant", tokens: { input: 10, output: 5, total: 15 } }))}');`,
    ].join("\n"));
    // Codex fixture
    const rolloutPath = path.join(codexRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        JSON.stringify({ timestamp: "2026-10-30T04:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-10-30T04:00:01.000Z", type: "message", role: "user", content: "Hi" }),
        JSON.stringify({
          timestamp: "2026-10-30T04:00:02.000Z",
          type: "event_msg",
          payload: { type: "token_count", info: { last_token_usage: { input_tokens: 30, output_tokens: 15, total_tokens: 45 } } },
        }),
      ].join("\n")
    );
    runSqlite(codexRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-inv', '${sqlString(rolloutPath)}', 'Fixture', '2026-10-30T04:00:05Z');`,
    ].join("\n"));
    // ZCode fixture
    const zDb = path.join(zcodeRoot, "zcode.db");
    runSqlite(zDb, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('z-inv', 'T', 1761800000000);",
      `INSERT INTO message VALUES ('msg-z1', 'z-inv', 1761800000000, '${sqlString(JSON.stringify({ role: "assistant", tokens: { input: 25, output: 10, total: 35 } }))}');`,
    ].join("\n"));

    const allEvents = [
      ...runAdapterUsage(claudeAdapterPath, claudeRoot).events,
      ...runAdapterUsage(opencodeAdapterPath, openDb).events,
      ...runAdapterUsage(codexAdapterPath, codexRoot).events,
      ...runAdapterUsage(zcodeAdapterPath, zDb, {}, 1).events,
    ];

    assert.equal(allEvents.length, 4);

    for (const ev of allEvents) {
      assert.ok(ev.turn_id, `turn_id must not be empty or null: ${JSON.stringify(ev)}`);
      assert.notEqual(ev.turn_id, "null");
      assert.notEqual(ev.turn_id, "");
      assert.ok(eventIdPattern.test(ev.turn_id), `turn_id must be safe identifier: ${ev.turn_id}`);

      assert.ok(ev.external_event_id, `external_event_id must not be empty or null: ${JSON.stringify(ev)}`);
      assert.ok(eventIdPattern.test(ev.external_event_id), `external_event_id must be safe identifier: ${ev.external_event_id}`);
      assert.equal(ev.logical_request_id, ev.external_event_id, `logical_request_id must match external_event_id`);

      assert.ok(isoUtcPattern.test(ev.timestamp), `timestamp must be valid ISO-8601 UTC string: ${ev.timestamp}`);
      assert.ok(ev.total_tokens > 0, `total_tokens must be positive: ${ev.total_tokens}`);
      assert.equal(ev.status, "success");
      assert.equal(ev.attempt_index, 0);
    }
  } finally {
    rmSync(claudeRoot, { force: true, recursive: true });
    rmSync(opencodeRoot, { force: true, recursive: true });
    rmSync(codexRoot, { force: true, recursive: true });
    rmSync(zcodeRoot, { force: true, recursive: true });
  }
});

console.log(`\n========================================`);
console.log(`TEST RESULTS: ${passedTests}/${totalTests} PASSED, ${failures.length} FAILED`);
if (failures.length > 0) {
  console.log(`\nFAILURES SUMMARY:`);
  for (const f of failures) {
    console.log(`- ${f.name}: ${f.error}`);
  }
  console.log(`========================================`);
  process.exit(1);
} else {
  console.log(`ALL ADVERSARIAL STRESS TESTS PASSED EMPIRICALLY!`);
  console.log(`========================================`);
}
