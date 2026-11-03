import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/zcode/adapter.mjs");

function runSqlite(dbPath, sql) {
  const result = spawnSync("sqlite3", [dbPath], { encoding: "utf8", input: sql });
  assert.equal(result.status, 0, result.stderr);
}

function sqlString(value) {
  return String(value).replaceAll("'", "''");
}

function runAdapter(dbPath, method, params = {}) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({
      protocol_version: 1,
      method,
      source: { location: dbPath },
      params,
    }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
  return messages;
}

function readFixtureUsage(location, params = {}) {
  const messages = runAdapter(location, "read_usage", params);
  const events = messages
    .filter((message) => message.type === "usage_event")
    .map((message) => message.usage_event);
  const complete = messages.find((message) => message.type === "complete")?.item ?? null;
  return { messages, events, complete };
}

test("ZCode adapter probe succeeds", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-zcode-probe-"));
  try {
    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        protocol_version: 1,
        method: "probe",
      }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").filter(Boolean).map((l) => JSON.parse(l));
    const complete = messages.find((m) => m.type === "complete");
    assert.ok(complete);
    assert.equal(complete.item.session_count, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("ZCode adapter lists session descriptors", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-zcode-list-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, time_updated INTEGER, directory TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
      "CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('zcode-sess-1', 'Build API', 1761800000000, '/tmp/project', NULL);",
      "INSERT INTO session VALUES ('zcode-sess-2', 'Refactor UI', 1761805000000, '/tmp/project', NULL);",
    ].join("\n"));

    const messages = runAdapter(dbPath, "list_sessions");
    const descriptors = messages
      .filter((m) => m.type === "item" && m.item?.kind === "session_descriptor")
      .map((m) => m.item);
    const complete = messages.find((m) => m.type === "complete");

    assert.equal(descriptors.length, 2);
    const ids = new Set(descriptors.map((d) => d.external_id));
    assert.ok(ids.has("zcode-sess-1"));
    assert.ok(ids.has("zcode-sess-2"));
    assert.ok(descriptors[0].version_token);
    assert.equal(complete?.item?.session_count, 2);
    assert.equal(complete?.item?.snapshot_complete, true);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("ZCode adapter reads session with user and assistant turns and parts", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-zcode-read-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const userMsg = JSON.stringify({ role: "user" });
    const userPart = JSON.stringify({ type: "text", text: "How does async/await work in JS?" });
    const assistantMsg = JSON.stringify({ role: "assistant", model: "zcode-code-v1" });
    const assistantPart = JSON.stringify({ type: "text", text: "Async/await is syntactic sugar over Promises." });

    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, time_updated INTEGER, directory TEXT, parent_id TEXT);",
      "CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);",
      "CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('sess-read-1', 'JS Async', 1761806000000, '/tmp/repo', NULL);",
      `INSERT INTO message VALUES ('msg-u1', 'sess-read-1', 1761806000000, '${sqlString(userMsg)}');`,
      `INSERT INTO part VALUES ('part-u1', 'msg-u1', 'sess-read-1', 1761806000000, '${sqlString(userPart)}');`,
      `INSERT INTO message VALUES ('msg-a1', 'sess-read-1', 1761806001000, '${sqlString(assistantMsg)}');`,
      `INSERT INTO part VALUES ('part-a1', 'msg-a1', 'sess-read-1', 1761806001000, '${sqlString(assistantPart)}');`,
    ].join("\n"));

    const messages = runAdapter(dbPath, "read_session", { session_id: "sess-read-1" });
    const session = messages.find((m) => m.type === "item" && m.item?.kind === "session")?.item?.session;
    assert.ok(session);
    assert.equal(session.external_id, "sess-read-1");
    assert.equal(session.title, "JS Async");
    assert.equal(session.project_path, "/tmp/repo");
    assert.equal(session.turns.length, 1);
    assert.equal(session.turns[0].user_text, "How does async/await work in JS?");
    assert.ok(session.turns[0].parts.length >= 1);
    assert.equal(session.turns[0].parts[0].text, "Async/await is syntactic sugar over Promises.");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("ZCode adapter extracts single message usage with turn_id, external_event_id and millisecond UTC timestamp", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-zcode-usage-col-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      modelID: "zcode-pro-preview",
      providerID: "zcode-cloud",
      cost: 0.005,
      tokens: {
        input: 150,
        output: 60,
        reasoning: 20,
        cache: { read: 30, write: 10 },
        total: 270,
      },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('z-sess-1', 'Test', 1761807000000);",
      `INSERT INTO message VALUES ('42', 'z-sess-1', 1761807000000, '${sqlString(msgData)}');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(dbPath);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(complete.decoder_profile, "zcode-sqlite-v1");
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, "z-sess-1");
    assert.equal(event.turn_id, "msg-42");
    assert.equal(event.external_event_id, "z-sess-1-msg-42");
    assert.equal(event.logical_request_id, "z-sess-1-msg-42");
    assert.equal(event.attempt_index, 0);
    assert.equal(event.provider, "zcode-cloud");
    assert.equal(event.model, "zcode-pro-preview");
    assert.equal(event.timestamp, new Date(1761807000000).toISOString());
    assert.equal(event.input_tokens, 150);
    assert.equal(event.output_tokens, 60);
    assert.equal(event.reasoning_tokens, 20);
    assert.equal(event.cache_read_tokens, 30);
    assert.equal(event.cache_write_tokens, 10);
    assert.equal(event.total_tokens, 270);
    assert.equal(event.cost, 0.005);
    assert.equal(event.status, "success");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("ZCode adapter safely queries usage when time_created column is missing by probing table_info and reading data.time.created", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-zcode-usage-nocol-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const msgData = JSON.stringify({
      role: "assistant",
      time: { created: "2026-10-30T04:45:00.789Z" },
      modelID: "zcode-lite",
      tokens: {
        input: 90,
        output: 30,
        total: 120,
      },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      // Schema without time_created column
      "CREATE TABLE message (id TEXT, session_id TEXT, data TEXT);",
      "INSERT INTO session VALUES ('z-sess-2', 'Test', 1761807000000);",
      `INSERT INTO message VALUES ('msg-77', 'z-sess-2', '${sqlString(msgData)}');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(dbPath);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, "z-sess-2");
    assert.equal(event.turn_id, "msg-77");
    assert.equal(event.external_event_id, "z-sess-2-msg-77");
    assert.equal(event.timestamp, "2026-10-30T04:45:00.789Z");
    assert.equal(event.input_tokens, 90);
    assert.equal(event.output_tokens, 30);
    assert.equal(event.total_tokens, 120);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("ZCode adapter suppresses messages where totalTokens <= 0", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-zcode-usage-zero-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const zeroMsg = JSON.stringify({
      role: "assistant",
      tokens: { input: 0, output: 0, total: 0 },
    });
    const validMsg = JSON.stringify({
      role: "assistant",
      tokens: { input: 40, output: 20, total: 60 },
    });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('z-sess-z', 'Test', 1761807000000);",
      `INSERT INTO message VALUES ('msg-z', 'z-sess-z', 1761808000000, '${sqlString(zeroMsg)}');`,
      `INSERT INTO message VALUES ('msg-v', 'z-sess-z', 1761808001000, '${sqlString(validMsg)}');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(dbPath);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "msg-v");
    assert.equal(events[0].external_event_id, "z-sess-z-msg-v");
    assert.equal(events[0].total_tokens, 60);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("ZCode adapter supports incremental cursor queries using last_time_created", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-zcode-usage-cursor-"));
  try {
    const dbPath = path.join(fixtureRoot, "zcode.db");
    const msg1 = JSON.stringify({ role: "assistant", tokens: { input: 15, output: 15, total: 30 } });
    const msg2 = JSON.stringify({ role: "assistant", tokens: { input: 25, output: 25, total: 50 } });
    runSqlite(dbPath, [
      "CREATE TABLE session (id TEXT, title TEXT, time_updated INTEGER);",
      "CREATE TABLE message (id TEXT, session_id TEXT, time_created INTEGER, data TEXT);",
      "INSERT INTO session VALUES ('z-sess-c', 'Test', 1761807000000);",
      `INSERT INTO message VALUES ('m-1', 'z-sess-c', 5000, '${sqlString(msg1)}');`,
    ].join("\n"));

    const firstRun = readFixtureUsage(dbPath);
    assert.equal(firstRun.events.length, 1);
    const cursor = JSON.parse(firstRun.complete.next_cursor);
    assert.equal(cursor.last_time_created, 5000);

    // Insert message 2 with time_created 6000
    runSqlite(dbPath, [
      `INSERT INTO message VALUES ('m-2', 'z-sess-c', 6000, '${sqlString(msg2)}');`,
    ].join("\n"));

    const secondRun = readFixtureUsage(dbPath, { cursor: firstRun.complete.next_cursor });
    assert.equal(secondRun.events.length, 1);
    assert.equal(secondRun.events[0].turn_id, "msg-m-2");
    const nextCursor = JSON.parse(secondRun.complete.next_cursor);
    assert.equal(nextCursor.last_time_created, 6000);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});
