import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, statSync, utimesSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repoRoot = path.resolve(import.meta.dirname, "..");
const claudeAdapter = path.join(repoRoot, "builtin-assets/adapters/claude-code/adapter.mjs");
const opencodeAdapter = path.join(repoRoot, "builtin-assets/adapters/opencode/adapter.mjs");
const codexAdapter = path.join(repoRoot, "builtin-assets/adapters/codex/adapter.mjs");
const zcodeAdapter = path.join(repoRoot, "builtin-assets/adapters/zcode/adapter.mjs");

function runAdapter(adapterPath, input) {
  const payload = {
    protocol_version: 1,
    ...input,
  };
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify(payload),
  });
  const lines = result.stdout.trim().split("\n").filter(Boolean);
  const messages = [];
  for (const line of lines) {
    try {
      messages.push(JSON.parse(line));
    } catch {}
  }
  const events = messages
    .filter((m) => m.type === "usage_event" || m.kind === "usage_event")
    .map((m) => m.usage_event || m.event || m.item);
  const complete = messages.find((m) => m.type === "complete" || m.kind === "complete")?.item ?? null;
  const errors = messages.filter((m) => m.type === "error" || m.kind === "error");
  return { status: result.status, stderr: result.stderr, messages, events, complete, errors };
}

function runSqlite(dbPath, sql) {
  const result = spawnSync("sqlite3", [dbPath], { encoding: "utf8", input: sql });
  assert.equal(result.status, 0, result.stderr);
}

function sqlString(value) {
  return String(value).replaceAll("'", "''");
}

// =========================================================================
// TEST SUITE 1: Subagent Sidechains & Turn Accounting Isolation
// =========================================================================

test("Adversarial: Claude Code sidechain with model change does not pollute primary turn accounting", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-claude-sidechain-"));
  try {
    const sessionId = "claude-sidechain-session";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    writeFileSync(
      sessionFile,
      [
        // Turn 0: User message
        JSON.stringify({
          timestamp: "2026-10-30T01:00:00.000Z",
          type: "user",
          message: { role: "user", content: [{ type: "text", text: "Refactor database query" }] },
        }),
        // Primary assistant step 1: calls Agent tool
        JSON.stringify({
          timestamp: "2026-10-30T01:00:01.000Z",
          type: "assistant",
          message: {
            role: "assistant",
            model: "claude-3-7-sonnet-20250219",
            content: [{ type: "tool_use", id: "tool-agent-1", name: "Agent", input: { prompt: "Analyze queries" } }],
            usage: { input_tokens: 100, output_tokens: 20 },
          },
        }),
        // Sidechain user prompt (isSidechain: true)
        JSON.stringify({
          timestamp: "2026-10-30T01:00:02.000Z",
          isSidechain: true,
          type: "user",
          message: { role: "user", content: [{ type: "text", text: "Subagent internal prompt: analyze index usage" }] },
        }),
        // Sidechain assistant step with different model (Haiku) and tokens
        JSON.stringify({
          timestamp: "2026-10-30T01:00:03.000Z",
          isSidechain: true,
          type: "assistant",
          message: {
            role: "assistant",
            model: "claude-3-5-haiku-20241022",
            content: [{ type: "text", text: "Index analysis complete" }],
            usage: { input_tokens: 200, output_tokens: 50 },
          },
        }),
        // Sidechain tool result returned to main
        JSON.stringify({
          timestamp: "2026-10-30T01:00:04.000Z",
          type: "user",
          message: {
            role: "user",
            content: [{ type: "tool_result", tool_use_id: "tool-agent-1", content: "Analysis done" }],
          },
        }),
        // Primary assistant completion
        JSON.stringify({
          timestamp: "2026-10-30T01:00:05.000Z",
          type: "assistant",
          message: {
            role: "assistant",
            model: "claude-3-7-sonnet-20250219",
            content: [{ type: "text", text: "I have refactored the database query." }],
            usage: { input_tokens: 300, output_tokens: 80 },
          },
        }),
        // Turn 1: Second real user prompt
        JSON.stringify({
          timestamp: "2026-10-30T01:05:00.000Z",
          type: "user",
          message: { role: "user", content: [{ type: "text", text: "Now run performance benchmark" }] },
        }),
        // Primary assistant answer for Turn 1
        JSON.stringify({
          timestamp: "2026-10-30T01:05:02.000Z",
          type: "assistant",
          message: {
            role: "assistant",
            model: "claude-3-7-sonnet-20250219",
            content: [{ type: "text", text: "Benchmark is 10x faster." }],
            usage: { input_tokens: 150, output_tokens: 40 },
          },
        }),
      ].join("\n")
    );

    const { events, complete } = runAdapter(claudeAdapter, {
      method: "read_usage",
      source: { location: fixtureRoot },
    });

    // Adversarial verification:
    // Exactly 2 primary turns must be emitted (Turn 0 and Turn 1).
    // The sidechain user prompt MUST NOT create a spurious Turn 1 or disrupt turnIndex!
    assert.equal(complete.usage_event_count, 2, `Expected 2 events, got ${complete.usage_event_count}`);
    assert.equal(events.length, 2);

    // Turn 0:
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-0`);
    assert.equal(events[0].timestamp, "2026-10-30T01:00:01.000Z"); // First response timestamp preserved
    assert.equal(events[0].model, "claude-3-7-sonnet-20250219"); // Primary model preserved, not clobbered by Haiku
    // Tokens in Turn 0: (100+20) + (200+50) + (300+80) = 750 total tokens
    assert.equal(events[0].input_tokens, 600);
    assert.equal(events[0].output_tokens, 150);
    assert.equal(events[0].total_tokens, 750);

    // Turn 1:
    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[1].timestamp, "2026-10-30T01:05:02.000Z");
    assert.equal(events[1].input_tokens, 150);
    assert.equal(events[1].output_tokens, 40);
    assert.equal(events[1].total_tokens, 190);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Adversarial: Codex sidechains and mid-turn turn_context do not pollute primary turn accounting", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-codex-sidechain-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        // Session meta
        JSON.stringify({ type: "session_meta", payload: { model: "gpt-5.4", model_provider: "openai" } }),
        // Turn 0: User turn context & message
        JSON.stringify({
          timestamp: "2026-10-30T02:00:00.000Z",
          type: "turn_context",
          payload: { model: "gpt-5.4" },
        }),
        JSON.stringify({
          timestamp: "2026-10-30T02:00:00.100Z",
          type: "message",
          payload: { role: "user", content: "Optimize SQL index" },
        }),
        // First token event
        JSON.stringify({
          timestamp: "2026-10-30T02:00:01.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: { last_token_usage: { input_tokens: 120, output_tokens: 30 } },
          },
        }),
        // Mid-turn subagent invocation with different model
        JSON.stringify({
          timestamp: "2026-10-30T02:00:02.000Z",
          type: "turn_context",
          payload: { model: "o3-mini" },
        }),
        // Second token event within same turn
        JSON.stringify({
          timestamp: "2026-10-30T02:00:03.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: { last_token_usage: { input_tokens: 180, output_tokens: 40 } },
          },
        }),
        // Turn 1: Second user message
        JSON.stringify({
          timestamp: "2026-10-30T02:10:00.000Z",
          type: "turn_context",
          payload: { model: "gpt-5.4" },
        }),
        JSON.stringify({
          timestamp: "2026-10-30T02:10:00.100Z",
          type: "message",
          payload: { role: "user", content: "Write migration script" },
        }),
        // Token event for Turn 1
        JSON.stringify({
          timestamp: "2026-10-30T02:10:02.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: { last_token_usage: { input_tokens: 200, output_tokens: 50 } },
          },
        }),
      ].join("\n")
    );

    const sessionId = "codex-sidechain-sess";
    runSqlite(path.join(fixtureRoot, "state_5.sqlite"), [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('${sessionId}', '${sqlString(rolloutPath)}', 'Fixture', '2026-10-30T02:10:02Z');`,
    ].join("\n"));

    const { events, complete } = runAdapter(codexAdapter, {
      method: "read_usage",
      source: { location: fixtureRoot },
    });

    assert.equal(complete.usage_event_count, 2);
    assert.equal(events.length, 2);

    // Turn 0
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-0`);
    assert.equal(events[0].timestamp, "2026-10-30T02:00:00.000Z");
    assert.equal(events[0].model, "gpt-5.4");
    assert.equal(events[0].input_tokens, 300);
    assert.equal(events[0].output_tokens, 70);
    assert.equal(events[0].total_tokens, 370);

    // Turn 1
    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[1].timestamp, "2026-10-30T02:10:00.000Z");
    assert.equal(events[1].input_tokens, 200);
    assert.equal(events[1].output_tokens, 50);
    assert.equal(events[1].total_tokens, 250);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// =========================================================================
// TEST SUITE 2: High Volume Sessions & Incremental Cursor Skipping
// =========================================================================

test("Adversarial: Claude Code high-volume sessions (200 sessions) cursor skipping & incremental rescan", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-claude-highvol-"));
  try {
    const sessionCount = 200;
    const baseTime = 1730000000000;

    for (let i = 0; i < sessionCount; i++) {
      const sessId = `session-${String(i).padStart(4, "0")}`;
      const filePath = path.join(fixtureRoot, `${sessId}.jsonl`);
      const fileTime = baseTime + i * 1000;
      const isoTime = new Date(fileTime).toISOString();
      writeFileSync(
        filePath,
        [
          JSON.stringify({ timestamp: isoTime, type: "user", message: { role: "user", content: `Prompt ${i}` } }),
          JSON.stringify({
            timestamp: new Date(fileTime + 500).toISOString(),
            type: "assistant",
            message: { role: "assistant", content: `Answer ${i}`, usage: { input_tokens: 10 + i, output_tokens: 5 } },
          }),
        ].join("\n")
      );
      // Explicitly set mtime
      const mtimeSec = (fileTime + 500) / 1000;
      utimesSync(filePath, mtimeSec, mtimeSec);
    }

    // 1. Initial scan: all 200 sessions scanned
    const run1 = runAdapter(claudeAdapter, {
      method: "read_usage",
      source: { location: fixtureRoot },
    });
    assert.equal(run1.complete.usage_event_count, sessionCount);
    assert.equal(run1.events.length, sessionCount);
    const cursor1 = run1.complete.next_cursor;
    assert.ok(cursor1);

    // 2. Incremental scan without changes: 0 events emitted!
    const run2 = runAdapter(claudeAdapter, {
      method: "read_usage",
      source: { location: fixtureRoot },
      params: { cursor: cursor1 },
    });
    assert.equal(run2.complete.usage_event_count, 0);
    assert.equal(run2.events.length, 0);

    // 3. Mutate 1 session (append turn 1 with newer mtime)
    const targetSess = `session-0050`;
    const targetFile = path.join(fixtureRoot, `${targetSess}.jsonl`);
    const newTime = baseTime + (sessionCount + 10) * 1000;
    const newIso = new Date(newTime).toISOString();
    writeFileSync(
      targetFile,
      [
        JSON.stringify({ timestamp: new Date(baseTime + 50 * 1000).toISOString(), type: "user", message: { role: "user", content: "Prompt 50" } }),
        JSON.stringify({
          timestamp: new Date(baseTime + 50 * 1000 + 500).toISOString(),
          type: "assistant",
          message: { role: "assistant", content: "Answer 50", usage: { input_tokens: 60, output_tokens: 5 } },
        }),
        JSON.stringify({ timestamp: newIso, type: "user", message: { role: "user", content: "Prompt 50續聊" } }),
        JSON.stringify({
          timestamp: new Date(newTime + 500).toISOString(),
          type: "assistant",
          message: { role: "assistant", content: "Answer 50续聊", usage: { input_tokens: 80, output_tokens: 15 } },
        }),
      ].join("\n")
    );
    const updatedMtimeSec = (newTime + 500) / 1000;
    utimesSync(targetFile, updatedMtimeSec, updatedMtimeSec);

    // 4. Incremental scan after mutation: ONLY mutated session is scanned (emits 2 turns)
    const run3 = runAdapter(claudeAdapter, {
      method: "read_usage",
      source: { location: fixtureRoot },
      params: { cursor: cursor1 },
    });
    assert.equal(run3.complete.usage_event_count, 2, `Expected 2 events for updated session, got ${run3.complete.usage_event_count}`);
    assert.equal(run3.events.length, 2);
    assert.equal(run3.events[0].turn_id, "turn-0");
    assert.equal(run3.events[0].external_event_id, `${targetSess}-turn-0`);
    assert.equal(run3.events[1].turn_id, "turn-1");
    assert.equal(run3.events[1].external_event_id, `${targetSess}-turn-1`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Adversarial: OpenCode incremental cursor queries and pagination boundary", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-opencode-cursor-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const count = 50;
    const statements = [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
    ];

    for (let i = 1; i <= count; i++) {
      const timeMs = 1730000000000 + i * 1000;
      const data = JSON.stringify({
        modelID: "claude-3-7-sonnet",
        providerID: "anthropic",
        tokens: { input: 10 + i, output: 5, total: 15 + i },
        time: { created: timeMs },
      });
      statements.push(
        `INSERT INTO message VALUES ('msg-${i}', 'sess-1', ${timeMs}, '${sqlString(data)}');`
      );
    }
    runSqlite(dbPath, statements.join("\n"));

    // First scan: all 50 messages
    const run1 = runAdapter(opencodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
    });
    assert.equal(run1.complete.usage_event_count, count);
    assert.equal(run1.events.length, count);
    const cursor = JSON.parse(run1.complete.next_cursor);
    assert.equal(cursor.last_time_created, 1730000000000 + count * 1000);

    // Second scan with cursor: 0 events emitted
    const run2 = runAdapter(opencodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
      params: { cursor: run1.complete.next_cursor },
    });
    assert.equal(run2.complete.usage_event_count, 0);
    assert.equal(run2.events.length, 0);

    // Append 5 new messages
    const newStatements = [];
    for (let i = count + 1; i <= count + 5; i++) {
      const timeMs = 1730000000000 + i * 1000;
      const data = JSON.stringify({
        modelID: "claude-3-7-sonnet",
        providerID: "anthropic",
        tokens: { input: 20 + i, output: 10, total: 30 + i },
        time: { created: timeMs },
      });
      newStatements.push(
        `INSERT INTO message VALUES ('msg-${i}', 'sess-1', ${timeMs}, '${sqlString(data)}');`
      );
    }
    runSqlite(dbPath, newStatements.join("\n"));

    // Third scan with cursor: exactly the 5 new messages emitted
    const run3 = runAdapter(opencodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
      params: { cursor: run1.complete.next_cursor },
    });
    assert.equal(run3.complete.usage_event_count, 5);
    assert.equal(run3.events.length, 5);
    assert.equal(run3.events[0].turn_id, `msg-${count + 1}`);
    assert.equal(run3.events[4].turn_id, `msg-${count + 5}`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Adversarial: ZCode incremental cursor queries and pagination boundary", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-zcode-cursor-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const count = 50;
    const statements = [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
    ];

    for (let i = 1; i <= count; i++) {
      const timeMs = 1730000000000 + i * 1000;
      const data = JSON.stringify({
        modelID: "gpt-5.4",
        providerID: "openai",
        tokens: { input: 15 + i, output: 10, total: 25 + i },
        time: { created: timeMs },
      });
      statements.push(
        `INSERT INTO message VALUES ('msg-${i}', 'sess-z', ${timeMs}, '${sqlString(data)}');`
      );
    }
    runSqlite(dbPath, statements.join("\n"));

    // First scan: all 50 messages
    const run1 = runAdapter(zcodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
    });
    assert.equal(run1.complete.usage_event_count, count);
    assert.equal(run1.events.length, count);
    const cursor = JSON.parse(run1.complete.next_cursor);
    assert.equal(cursor.last_time_created, 1730000000000 + count * 1000);

    // Second scan with cursor: 0 events emitted
    const run2 = runAdapter(zcodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
      params: { cursor: run1.complete.next_cursor },
    });
    assert.equal(run2.complete.usage_event_count, 0);
    assert.equal(run2.events.length, 0);

    // Append 3 new messages
    const newStatements = [];
    for (let i = count + 1; i <= count + 3; i++) {
      const timeMs = 1730000000000 + i * 1000;
      const data = JSON.stringify({
        modelID: "gpt-5.4",
        providerID: "openai",
        tokens: { input: 20 + i, output: 10, total: 30 + i },
        time: { created: timeMs },
      });
      newStatements.push(
        `INSERT INTO message VALUES ('msg-${i}', 'sess-z', ${timeMs}, '${sqlString(data)}');`
      );
    }
    runSqlite(dbPath, newStatements.join("\n"));

    // Third scan: exactly 3 new messages
    const run3 = runAdapter(zcodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
      params: { cursor: run1.complete.next_cursor },
    });
    assert.equal(run3.complete.usage_event_count, 3);
    assert.equal(run3.events.length, 3);
    assert.equal(run3.events[0].turn_id, `msg-${count + 1}`);
    assert.equal(run3.events[2].turn_id, `msg-${count + 3}`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// =========================================================================
// TEST SUITE 3: Database Locks & Corrupted Lines Handling
// =========================================================================

test("Adversarial: OpenCode handles corrupt JSON in message.data and missing fields gracefully", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-opencode-corrupt-"));
  try {
    const dbPath = path.join(fixtureRoot, "opencode.db");
    const statements = [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
      // Row 1: Corrupted data (truncated JSON)
      "INSERT INTO message VALUES ('corrupt-1', 'sess-1', 1730000001000, '{\"tokens\": {\"input\": 10, ');",
      // Row 2: Corrupted data (random text)
      "INSERT INTO message VALUES ('corrupt-2', 'sess-1', 1730000002000, 'not-json-at-all');",
      // Row 3: NULL data
      "INSERT INTO message VALUES ('corrupt-3', 'sess-1', 1730000003000, NULL);",
      // Row 4: Valid data with tokens
      `INSERT INTO message VALUES ('valid-4', 'sess-1', 1730000004000, '${sqlString(
        JSON.stringify({
          modelID: "claude-3-7-sonnet",
          providerID: "anthropic",
          tokens: { input: 100, output: 50, total: 150 },
        })
      )}');`,
      // Row 5: Data missing tokens object
      `INSERT INTO message VALUES ('notokens-5', 'sess-1', 1730000005000, '${sqlString(
        JSON.stringify({ modelID: "claude-3-7-sonnet", text: "No tokens here" })
      )}');`,
      // Row 6: Zero tokens (should be filtered)
      `INSERT INTO message VALUES ('zero-6', 'sess-1', 1730000006000, '${sqlString(
        JSON.stringify({
          modelID: "claude-3-7-sonnet",
          tokens: { input: 0, output: 0, total: 0 },
        })
      )}');`,
    ];
    runSqlite(dbPath, statements.join("\n"));

    const { events, complete, status } = runAdapter(opencodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
    });

    assert.equal(status, 0);
    // Only valid-4 should produce an event! Corrupted/null/zero-token rows must be skipped cleanly.
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "msg-valid-4");
    assert.equal(events[0].total_tokens, 150);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Adversarial: ZCode handles corrupt JSON in message.data and missing fields gracefully", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-zcode-corrupt-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const statements = [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
      // Corrupt row 1
      "INSERT INTO message VALUES ('zcorrupt-1', 'sess-z', 1730000001000, '{broken-json');",
      // Corrupt row 2 (NULL)
      "INSERT INTO message VALUES ('zcorrupt-2', 'sess-z', 1730000002000, NULL);",
      // Valid row 3
      `INSERT INTO message VALUES ('zvalid-3', 'sess-z', 1730000003000, '${sqlString(
        JSON.stringify({
          modelID: "gpt-5.4",
          providerID: "openai",
          tokens: { input: 120, output: 60, total: 180 },
        })
      )}');`,
    ];
    runSqlite(dbPath, statements.join("\n"));

    const { events, complete, status } = runAdapter(zcodeAdapter, {
      method: "read_usage",
      source: { location: dbPath },
    });

    assert.equal(status, 0);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "msg-zvalid-3");
    assert.equal(events[0].total_tokens, 180);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Adversarial: Claude Code handles corrupted JSON lines and empty lines gracefully", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-claude-corrupt-"));
  try {
    const sessionId = "claude-corrupt-lines";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    writeFileSync(
      sessionFile,
      [
        "", // Leading empty line
        "   \t  ", // Whitespace line
        "{not valid json at all", // Syntax error
        JSON.stringify({
          timestamp: "2026-10-30T03:00:00.000Z",
          type: "user",
          message: { role: "user", content: "Valid prompt after syntax error" },
        }),
        "{\"truncated\": true, \"unclosed", // Truncated line
        JSON.stringify({
          timestamp: "2026-10-30T03:00:01.500Z",
          type: "assistant",
          message: {
            role: "assistant",
            content: "Valid answer",
            usage: { input_tokens: 50, output_tokens: 25 },
          },
        }),
        "\r\n\r\n", // Mixed newlines
      ].join("\n")
    );

    const { events, complete, status } = runAdapter(claudeAdapter, {
      method: "read_usage",
      source: { location: fixtureRoot },
    });

    assert.equal(status, 0);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-0`);
    assert.equal(events[0].timestamp, "2026-10-30T03:00:01.500Z");
    assert.equal(events[0].total_tokens, 75);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Adversarial: Codex handles corrupted JSON lines and empty lines gracefully", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-codex-corrupt-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        "", // Empty line
        "{broken-json", // Malformed
        JSON.stringify({ type: "session_meta", payload: { model: "gpt-5.4" } }),
        "another malformed line !!!",
        JSON.stringify({
          timestamp: "2026-10-30T03:10:00.000Z",
          type: "turn_context",
          payload: { model: "gpt-5.4" },
        }),
        JSON.stringify({
          timestamp: "2026-10-30T03:10:00.100Z",
          type: "message",
          payload: { role: "user", content: "Valid message after corruptions" },
        }),
        JSON.stringify({
          timestamp: "2026-10-30T03:10:01.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: { last_token_usage: { input_tokens: 70, output_tokens: 35 } },
          },
        }),
      ].join("\n")
    );

    const sessionId = "codex-corrupt-sess";
    runSqlite(path.join(fixtureRoot, "state_5.sqlite"), [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('${sessionId}', '${sqlString(rolloutPath)}', 'Fixture', '2026-10-30T03:10:01Z');`,
    ].join("\n"));

    const { events, complete, status } = runAdapter(codexAdapter, {
      method: "read_usage",
      source: { location: fixtureRoot },
    });

    assert.equal(status, 0);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-0`);
    assert.equal(events[0].total_tokens, 105);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// =========================================================================
// TEST SUITE 4: Zero Token Suppression & Missing Timestamps Guardrails
// =========================================================================

test("Adversarial: All 4 adapters suppress zero-token turns completely", () => {
  // 1. Claude Code
  const claudeRoot = mkdtempSync(path.join(tmpdir(), "adv-zero-claude-"));
  try {
    writeFileSync(
      path.join(claudeRoot, "sess-zero.jsonl"),
      [
        JSON.stringify({ timestamp: "2026-10-30T04:00:00Z", type: "user", message: { role: "user", content: "hi" } }),
        JSON.stringify({
          timestamp: "2026-10-30T04:00:01Z",
          type: "assistant",
          message: { role: "assistant", content: "hello", usage: { input_tokens: 0, output_tokens: 0 } },
        }),
      ].join("\n")
    );
    const res = runAdapter(claudeAdapter, { method: "read_usage", source: { location: claudeRoot } });
    assert.equal(res.events.length, 0);
    assert.equal(res.complete.usage_event_count, 0);
  } finally {
    rmSync(claudeRoot, { force: true, recursive: true });
  }

  // 2. OpenCode
  const opencodeRoot = mkdtempSync(path.join(tmpdir(), "adv-zero-opencode-"));
  try {
    const dbPath = path.join(opencodeRoot, "opencode.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, project TEXT, updated_at TEXT);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
      `INSERT INTO message VALUES ('zero-1', 's1', 1730000000000, '${sqlString(
        JSON.stringify({ tokens: { input: 0, output: 0, total: 0 } })
      )}');`,
    ].join("\n"));
    const res = runAdapter(opencodeAdapter, { method: "read_usage", source: { location: dbPath } });
    assert.equal(res.events.length, 0);
    assert.equal(res.complete.usage_event_count, 0);
  } finally {
    rmSync(opencodeRoot, { force: true, recursive: true });
  }

  // 3. ZCode
  const zcodeRoot = mkdtempSync(path.join(tmpdir(), "adv-zero-zcode-"));
  try {
    const dbPath = path.join(zcodeRoot, "zcode.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
      `INSERT INTO message VALUES ('zero-1', 's1', 1730000000000, '${sqlString(
        JSON.stringify({ tokens: { input: 0, output: 0, total: 0 } })
      )}');`,
    ].join("\n"));
    const res = runAdapter(zcodeAdapter, { method: "read_usage", source: { location: dbPath } });
    assert.equal(res.events.length, 0);
    assert.equal(res.complete.usage_event_count, 0);
  } finally {
    rmSync(zcodeRoot, { force: true, recursive: true });
  }

  // 4. Codex
  const codexRoot = mkdtempSync(path.join(tmpdir(), "adv-zero-codex-"));
  try {
    const rolloutPath = path.join(codexRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        JSON.stringify({ type: "session_meta", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-10-30T04:00:00Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-10-30T04:00:00.100Z", type: "message", payload: { role: "user", content: "hi" } }),
        JSON.stringify({
          timestamp: "2026-10-30T04:00:01Z",
          type: "event_msg",
          payload: { type: "token_count", info: { last_token_usage: { input_tokens: 0, output_tokens: 0 } } },
        }),
      ].join("\n")
    );
    runSqlite(path.join(codexRoot, "state_5.sqlite"), [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-zero', '${sqlString(rolloutPath)}', 'Fixture', '2026-10-30T04:00:01Z');`,
    ].join("\n"));
    const res = runAdapter(codexAdapter, { method: "read_usage", source: { location: codexRoot } });
    assert.equal(res.events.length, 0);
    assert.equal(res.complete.usage_event_count, 0);
  } finally {
    rmSync(codexRoot, { force: true, recursive: true });
  }
});
