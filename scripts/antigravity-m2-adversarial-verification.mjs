import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/antigravity/adapter.mjs");

function runAdapterUsage(location, params = {}) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({
      method: "read_usage",
      source: { location },
      params,
    }),
  });
  assert.equal(result.status, 0, `Adapter crashed: ${result.stderr}`);
  const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
  const events = messages
    .filter((message) => message.type === "item" && message.item?.kind === "usage_event")
    .map((message) => message.item.event);
  const complete = messages.find((message) => message.type === "complete")?.item ?? null;
  return { messages, events, complete };
}

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
    console.error(err);
    throw err;
  }
}

console.log("=== EMPIRICAL ADVERSARIAL TEST HARNESS: MILESTONE M2 ===");

// ---------------------------------------------------------------------------
// SUITE 1: Cursor Behavior & Incremental Skipping
// ---------------------------------------------------------------------------

runTest("Cursor: handles null, undefined, malformed JSON, and empty cursor without crashing", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-cursor-null-"));
  try {
    const sessionDir = path.join(fixtureRoot, "11111111-1111-1111-1111-111111111111", ".system_generated", "logs");
    mkdirSync(sessionDir, { recursive: true });
    writeFileSync(
      path.join(sessionDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "hi" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 5, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    // 1. null cursor
    const res1 = runAdapterUsage(fixtureRoot, { cursor: null });
    assert.equal(res1.events.length, 1);
    assert.ok(res1.complete.next_cursor);

    // 2. malformed string cursor
    const res2 = runAdapterUsage(fixtureRoot, { cursor: "{not: valid, json}" });
    assert.equal(res2.events.length, 1);
    assert.ok(res2.complete.next_cursor);

    // 3. empty string cursor
    const res3 = runAdapterUsage(fixtureRoot, { cursor: "" });
    assert.equal(res3.events.length, 1);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Cursor: backward compatibility with legacy processed_dbs in cursor", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-cursor-legacy-"));
  try {
    const sessionDir = path.join(fixtureRoot, "22222222-2222-2222-2222-222222222222", ".system_generated", "logs");
    mkdirSync(sessionDir, { recursive: true });
    const transcriptPath = path.join(sessionDir, "transcript.jsonl");
    writeFileSync(
      transcriptPath,
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "legacy" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 20, output_tokens: 10, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    const st = statSync(transcriptPath);
    const legacyCursor = JSON.stringify({
      last_mtime: st.mtimeMs + 1000,
      processed_dbs: [transcriptPath],
    });

    const res = runAdapterUsage(fixtureRoot, { cursor: legacyCursor });
    assert.equal(res.events.length, 0, "Expected file in processed_dbs to be skipped");
    assert.equal(res.complete.usage_event_count, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Cursor: multi-file directory with mixed state (unchanged, modified, and added with old mtime)", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-cursor-multi-"));
  try {
    const baseTime = Date.now() - 100000;
    // File A: session A
    const dirA = path.join(fixtureRoot, "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa", ".system_generated", "logs");
    mkdirSync(dirA, { recursive: true });
    const pathA = path.join(dirA, "transcript.jsonl");
    writeFileSync(
      pathA,
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "A" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 10, created_at: "2026-10-30T01:00:00Z" }),
      ].join("\n")
    );
    utimesSync(pathA, baseTime / 1000, baseTime / 1000);

    // File B: session B
    const dirB = path.join(fixtureRoot, "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb", ".system_generated", "logs");
    mkdirSync(dirB, { recursive: true });
    const pathB = path.join(dirB, "transcript.jsonl");
    writeFileSync(
      pathB,
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "B" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 20, output_tokens: 20, created_at: "2026-10-30T01:00:00Z" }),
      ].join("\n")
    );
    utimesSync(pathB, (baseTime + 5000) / 1000, (baseTime + 5000) / 1000);

    // Initial scan -> processes A and B
    const run1 = runAdapterUsage(fixtureRoot);
    assert.equal(run1.events.length, 2);
    const cursor1 = run1.complete.next_cursor;

    // Second scan with no changes -> skips both
    const run2 = runAdapterUsage(fixtureRoot, { cursor: cursor1 });
    assert.equal(run2.events.length, 0);

    // Now:
    // 1. Keep File A untouched (mtime = baseTime)
    // 2. Modify File B (append turn, update mtime to baseTime + 20000)
    writeFileSync(
      pathB,
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "B" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 20, output_tokens: 20, created_at: "2026-10-30T01:00:00Z" }),
        JSON.stringify({ step_index: 2, source: "USER_EXPLICIT", type: "USER_INPUT", content: "B2" }),
        JSON.stringify({ step_index: 3, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 30, output_tokens: 30, created_at: "2026-10-30T01:05:00Z" }),
      ].join("\n")
    );
    utimesSync(pathB, (baseTime + 20000) / 1000, (baseTime + 20000) / 1000);

    // 3. Add File C with an OLD mtime (baseTime - 10000) that was NOT in cursor1.processed_files!
    const dirC = path.join(fixtureRoot, "cccccccc-cccc-cccc-cccc-cccccccccccc", ".system_generated", "logs");
    mkdirSync(dirC, { recursive: true });
    const pathC = path.join(dirC, "transcript.jsonl");
    writeFileSync(
      pathC,
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "C" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 15, output_tokens: 15, created_at: "2026-10-30T01:00:00Z" }),
      ].join("\n")
    );
    utimesSync(pathC, (baseTime - 10000) / 1000, (baseTime - 10000) / 1000);

    // Run 3 with cursor1:
    // A should be skipped (0 events)
    // B should be rescanned (2 events: turn-0 and turn-1)
    // C was not in processed_files, so it MUST NOT be skipped despite old mtime! (1 event: turn-0)
    const run3 = runAdapterUsage(fixtureRoot, { cursor: cursor1 });
    assert.equal(run3.events.length, 3, `Expected 3 events (2 from B, 1 from C), got ${run3.events.length}`);
    const bEvents = run3.events.filter((e) => e.session_id.startsWith("bbbbbbbb"));
    const cEvents = run3.events.filter((e) => e.session_id.startsWith("cccccccc"));
    const aEvents = run3.events.filter((e) => e.session_id.startsWith("aaaaaaaa"));
    assert.equal(aEvents.length, 0, "A should have been skipped");
    assert.equal(bEvents.length, 2, "B should have emitted 2 turns");
    assert.equal(cEvents.length, 1, "C should have emitted 1 turn");

    // Next cursor must contain all 3 files
    const parsedCursor3 = JSON.parse(run3.complete.next_cursor);
    assert.ok(parsedCursor3.processed_files.some((f) => f.includes("aaaaaaaa")));
    assert.ok(parsedCursor3.processed_files.some((f) => f.includes("bbbbbbbb")));
    assert.ok(parsedCursor3.processed_files.some((f) => f.includes("cccccccc")));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Cursor: high volume session files list retention in processed_files", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-cursor-volume-"));
  try {
    const fileCount = 25;
    for (let i = 0; i < fileCount; i++) {
      const hex = i.toString(16).padStart(4, "0");
      const sid = `${hex}0000-0000-0000-0000-000000000000`;
      const logsDir = path.join(fixtureRoot, sid, ".system_generated", "logs");
      mkdirSync(logsDir, { recursive: true });
      writeFileSync(
        path.join(logsDir, "transcript.jsonl"),
        [
          JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: `Prompt ${i}` }),
          JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 5, created_at: "2026-10-30T00:00:00Z" }),
        ].join("\n")
      );
    }

    const run1 = runAdapterUsage(fixtureRoot);
    assert.equal(run1.events.length, fileCount);
    const cursor1 = JSON.parse(run1.complete.next_cursor);
    assert.equal(cursor1.processed_files.length, fileCount);

    // Second scan with cursor -> all 25 files skipped
    const run2 = runAdapterUsage(fixtureRoot, { cursor: run1.complete.next_cursor });
    assert.equal(run2.events.length, 0);
    const cursor2 = JSON.parse(run2.complete.next_cursor);
    assert.equal(cursor2.processed_files.length, fileCount);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 2: Deterministic ID Generation Across Directory Layouts
// ---------------------------------------------------------------------------

runTest("Deterministic ID: Standard layout (<uuid>/.system_generated/logs/transcript.jsonl)", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-id-standard-"));
  try {
    const uuid = "33333333-3333-3333-3333-333333333333";
    const dir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      path.join(dir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "t0" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 5, created_at: "2026-10-30T00:00:00Z" }),
        JSON.stringify({ step_index: 2, source: "USER_EXPLICIT", type: "USER_INPUT", content: "t1" }),
        JSON.stringify({ step_index: 3, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 5, created_at: "2026-10-30T00:01:00Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 2);
    assert.equal(events[0].session_id, uuid);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${uuid}-turn-0`);
    assert.equal(events[0].logical_request_id, `${uuid}-turn-0`);

    assert.equal(events[1].session_id, uuid);
    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, `${uuid}-turn-1`);
    assert.equal(events[1].logical_request_id, `${uuid}-turn-1`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Deterministic ID: Flat layout (<uuid>/transcript.jsonl and transcript_full.jsonl)", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-id-flat-"));
  try {
    const uuid1 = "44444444-4444-4444-4444-444444444444";
    const dir1 = path.join(fixtureRoot, uuid1);
    mkdirSync(dir1, { recursive: true });
    writeFileSync(
      path.join(dir1, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "flat short" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 5, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    const uuid2 = "55555555-5555-5555-5555-555555555555";
    const dir2 = path.join(fixtureRoot, uuid2);
    mkdirSync(dir2, { recursive: true });
    writeFileSync(
      path.join(dir2, "transcript_full.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "flat full" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 20, output_tokens: 15, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 2);
    const ev1 = events.find((e) => e.session_id === uuid1);
    const ev2 = events.find((e) => e.session_id === uuid2);
    assert.ok(ev1);
    assert.ok(ev2);
    assert.equal(ev1.external_event_id, `${uuid1}-turn-0`);
    assert.equal(ev2.external_event_id, `${uuid2}-turn-0`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Deterministic ID: File priority prefers transcript_full.jsonl over transcript.jsonl", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-id-prio-"));
  try {
    const uuid = "98989898-9898-9898-9898-989898989898";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });

    // transcript.jsonl has 1 turn
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Short version" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 5, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    // transcript_full.jsonl has 2 turns
    writeFileSync(
      path.join(logsDir, "transcript_full.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Full turn 0" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 50, output_tokens: 20, created_at: "2026-10-30T00:00:00Z" }),
        JSON.stringify({ step_index: 2, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Full turn 1" }),
        JSON.stringify({ step_index: 3, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 70, output_tokens: 30, created_at: "2026-10-30T00:01:00Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    // Must pick transcript_full.jsonl, emitting 2 events
    assert.equal(events.length, 2, "Should prioritize transcript_full.jsonl over transcript.jsonl");
    assert.equal(events[0].total_tokens, 70);
    assert.equal(events[1].total_tokens, 100);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Deterministic ID: Custom folder name with meta.json or .meta", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-id-meta-"));
  try {
    const metaSessionId = "66666666-6666-6666-6666-666666666666";
    const customDir = path.join(fixtureRoot, "my-custom-task-dir");
    const logsDir = path.join(customDir, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(path.join(customDir, "meta.json"), JSON.stringify({ session_id: metaSessionId }));
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "meta" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 10, output_tokens: 5, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    // Scan customDir directly
    const { events } = runAdapterUsage(customDir);
    assert.equal(events.length, 1);
    assert.equal(events[0].session_id, metaSessionId);
    assert.equal(events[0].external_event_id, `${metaSessionId}-turn-0`);
    assert.equal(events[0].turn_id, "turn-0");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Deterministic ID: Subagent internal session filtered by meta.json is_subagent", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-id-subagent-"));
  try {
    const mainSessionId = "aaaa1111-2222-3333-4444-555566667777";
    const subSessionId = "bbbb1111-2222-3333-4444-555566667777";

    // Main session
    const mainDir = path.join(fixtureRoot, mainSessionId, ".system_generated", "logs");
    mkdirSync(mainDir, { recursive: true });
    writeFileSync(
      path.join(mainDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Main prompt" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 100, output_tokens: 50, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    // Subagent session marked in meta.json
    const subDir = path.join(fixtureRoot, subSessionId);
    const subLogsDir = path.join(subDir, ".system_generated", "logs");
    mkdirSync(subLogsDir, { recursive: true });
    writeFileSync(path.join(subDir, "meta.json"), JSON.stringify({ session_id: subSessionId, is_subagent: true }));
    writeFileSync(
      path.join(subLogsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Subagent prompt" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 200, output_tokens: 100, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    // Only main session should be ingested; subagent session MUST be filtered out
    assert.equal(events.length, 1);
    assert.equal(events[0].session_id, mainSessionId);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Deterministic ID: Direct transcript file path specified as location", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-id-direct-file-"));
  try {
    const uuid = "77777777-7777-7777-7777-777777777777";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    const directFile = path.join(logsDir, "transcript.jsonl");
    writeFileSync(
      directFile,
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "direct" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 30, output_tokens: 15, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(directFile);
    assert.equal(events.length, 1);
    assert.equal(events[0].session_id, uuid);
    assert.equal(events[0].external_event_id, `${uuid}-turn-0`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Deterministic ID: Explicit session_id parameter override in params", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-id-override-"));
  try {
    const origUuid = "88888888-8888-8888-8888-888888888888";
    const overrideUuid = "99999999-9999-9999-9999-999999999999";
    const logsDir = path.join(fixtureRoot, origUuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "override" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 40, output_tokens: 20, created_at: "2026-10-30T00:00:00Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot, { session_id: overrideUuid });
    assert.equal(events.length, 1);
    assert.equal(events[0].session_id, overrideUuid);
    assert.equal(events[0].external_event_id, `${overrideUuid}-turn-0`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 3: Zero-Token Turn Filtering & Empty Transcript Edge Cases
// ---------------------------------------------------------------------------

runTest("Zero-token & Empty: Completely empty file (0 bytes)", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-empty-zero-"));
  try {
    const uuid = "aaaaaaaa-1111-2222-3333-444444444444";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(path.join(logsDir, "transcript.jsonl"), "");

    const { events, complete } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 0);
    assert.equal(complete.usage_event_count, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Zero-token & Empty: Whitespace, comments, and only non-user step types", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-whitespace-"));
  try {
    const uuid = "bbbbbbbb-1111-2222-3333-444444444444";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        "   \n\n\t",
        JSON.stringify({ step_index: 0, type: "CONVERSATION_HISTORY", content: "history" }),
        JSON.stringify({ step_index: 1, type: "CHECKPOINT", content: "cp" }),
        JSON.stringify({ step_index: 2, type: "SYSTEM_MESSAGE", content: "sys" }),
        "",
      ].join("\n")
    );

    const { events, complete } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 0);
    assert.equal(complete.usage_event_count, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Zero-token Filtering: Interspersed zero-token and non-zero-token turns preserve deterministic monotonic turn index", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-zero-interspersed-"));
  try {
    const uuid = "cccccccc-1111-2222-3333-444444444444";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        // Turn 0: User input, but model response has 0 tokens (e.g. unfulfilled / error / empty)
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Turn 0 prompt" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 0, output_tokens: 0, created_at: "2026-10-30T01:00:00Z" }),

        // Turn 1: User input with non-zero tokens
        JSON.stringify({ step_index: 2, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Turn 1 prompt" }),
        JSON.stringify({ step_index: 3, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 150, output_tokens: 50, created_at: "2026-10-30T01:01:00Z" }),

        // Turn 2: User input, no responses followed at all (e.g. aborted conversation)
        JSON.stringify({ step_index: 4, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Turn 2 prompt" }),

        // Turn 3: User input with non-zero tokens
        JSON.stringify({ step_index: 5, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Turn 3 prompt" }),
        JSON.stringify({ step_index: 6, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 200, output_tokens: 80, created_at: "2026-10-30T01:03:00Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    // Turn 0 and Turn 2 had 0 tokens, so they must be filtered out!
    assert.equal(events.length, 2, "Expected exactly 2 events emitted (Turn 1 and Turn 3)");

    // Turn 1 must have turn_id 'turn-1' and external_event_id '${uuid}-turn-1'
    assert.equal(events[0].turn_id, "turn-1");
    assert.equal(events[0].external_event_id, `${uuid}-turn-1`);
    assert.equal(events[0].total_tokens, 200);

    // Turn 3 must have turn_id 'turn-3' and external_event_id '${uuid}-turn-3'
    assert.equal(events[1].turn_id, "turn-3");
    assert.equal(events[1].external_event_id, `${uuid}-turn-3`);
    assert.equal(events[1].total_tokens, 280);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Zero-token & Resilience: Malformed JSON lines and missing/string token numbers", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-malformed-tokens-"));
  try {
    const uuid = "dddddddd-1111-2222-3333-444444444444";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Prompt" }),
        "CORRUPTED_NON_JSON_LINE {{{{{",
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          input_tokens: "123", // string representation
          cache_read_tokens: null, // null field
          output_tokens: 45,
          created_at: "2026-10-30T02:00:00Z",
        }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].input_tokens, 123);
    assert.equal(events[0].cache_read_tokens, 0);
    assert.equal(events[0].output_tokens, 45);
    assert.equal(events[0].total_tokens, 168);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Zero-token & Resilience: Turn with only cache_read_tokens or reasoning_tokens is NOT suppressed", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-cache-reasoning-"));
  try {
    const uuid = "12121212-1212-1212-1212-121212121212";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        // Turn 0: input_tokens = 0, output_tokens = 0, but cache_read_tokens = 500
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Cached query" }),
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          input_tokens: 0,
          cache_read_tokens: 500,
          output_tokens: 0,
          created_at: "2026-10-30T02:00:00Z",
        }),
        // Turn 1: only reasoning_tokens = 250
        JSON.stringify({ step_index: 2, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Thinking query" }),
        JSON.stringify({
          step_index: 3,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          input_tokens: 0,
          cache_read_tokens: 0,
          reasoning_tokens: 250,
          output_tokens: 0,
          created_at: "2026-10-30T02:05:00Z",
        }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 2, "Both cache-only and reasoning-only turns must be emitted");
    assert.equal(events[0].total_tokens, 500);
    assert.equal(events[0].cache_read_tokens, 500);
    assert.equal(events[1].total_tokens, 250);
    assert.equal(events[1].reasoning_tokens, 250);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Resilience: Transcript starting with PLANNER_RESPONSE without USER_INPUT (truncated transcript)", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-no-user-input-"));
  try {
    const uuid = "eeeeeeee-1111-2222-3333-444444444444";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({
          step_index: 0,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          input_tokens: 88,
          output_tokens: 33,
          created_at: "2026-10-30T02:30:00Z",
        }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${uuid}-turn-0`);
    assert.equal(events[0].total_tokens, 121);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 4: Multi-Step Tool Loops & Historical Timestamp Retention
// ---------------------------------------------------------------------------

runTest("Tool Loop: Multiple PLANNER_RESPONSE steps retain FIRST response timestamp and accumulate tokens", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-tool-loop-"));
  try {
    const uuid = "ffffffff-1111-2222-3333-444444444444";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Complex task", created_at: "2026-10-25T10:00:00Z" }),
        // Response 1 (model plans tool call)
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-25T10:00:02.123Z",
          input_tokens: 100,
          cache_read_tokens: 50,
          reasoning_tokens: 20,
          output_tokens: 30,
        }),
        // Tool call
        JSON.stringify({ step_index: 2, source: "MODEL", type: "TOOL_CALL", content: "run command" }),
        // Response 2 (model plans second tool call)
        JSON.stringify({
          step_index: 3,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-25T10:00:10.500Z",
          input_tokens: 150,
          cache_read_tokens: 60,
          reasoning_tokens: 10,
          output_tokens: 40,
        }),
        // Response 3 (final response)
        JSON.stringify({
          step_index: 4,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-25T10:00:20.900Z",
          input_tokens: 200,
          cache_read_tokens: 70,
          reasoning_tokens: 30,
          output_tokens: 100,
        }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 1);
    const ev = events[0];
    assert.equal(ev.timestamp, "2026-10-25T10:00:02.123Z", "Must retain the FIRST response timestamp");
    assert.equal(ev.input_tokens, 450); // 100 + 150 + 200
    assert.equal(ev.cache_read_tokens, 180); // 50 + 60 + 70
    assert.equal(ev.reasoning_tokens, 60); // 20 + 10 + 30
    assert.equal(ev.output_tokens, 170); // 30 + 40 + 100
    assert.equal(ev.total_tokens, 860);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Cross-day Multi-turn: Historical turns retain separate UTC dates across days", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-cross-day-"));
  try {
    const uuid = "10101010-1010-1010-1010-101010101010";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        // Day 1
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Day 1 prompt", created_at: "2026-10-01T08:00:00Z" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", created_at: "2026-10-01T08:00:05.100Z", input_tokens: 100, output_tokens: 50 }),
        // Day 2
        JSON.stringify({ step_index: 2, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Day 2 prompt", created_at: "2026-10-02T14:30:00Z" }),
        JSON.stringify({ step_index: 3, source: "MODEL", type: "PLANNER_RESPONSE", created_at: "2026-10-02T14:30:10.200Z", input_tokens: 200, output_tokens: 80 }),
        // Day 3
        JSON.stringify({ step_index: 4, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Day 3 prompt", created_at: "2026-10-03T19:45:00Z" }),
        JSON.stringify({ step_index: 5, source: "MODEL", type: "PLANNER_RESPONSE", created_at: "2026-10-03T19:45:15.300Z", input_tokens: 300, output_tokens: 120 }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 3);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].timestamp, "2026-10-01T08:00:05.100Z");
    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].timestamp, "2026-10-02T14:30:10.200Z");
    assert.equal(events[2].turn_id, "turn-2");
    assert.equal(events[2].timestamp, "2026-10-03T19:45:15.300Z");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

runTest("Timestamp Fallback: Handles invalid date string gracefully without crashing", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-invalid-date-"));
  try {
    const uuid = "13131313-1313-1313-1313-131313131313";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", content: "bad date" }),
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          input_tokens: 50,
          output_tokens: 25,
          created_at: "NOT_AN_ISO_DATE_STRING_AT_ALL",
        }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 1);
    assert.ok(events[0].timestamp);
    // Should fallback to valid ISO date (mtime)
    assert.doesNotThrow(() => new Date(events[0].timestamp).toISOString());
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

// ---------------------------------------------------------------------------
// SUITE 5: Zero-SQL & Zero-Protobuf Isolation Verification
// ---------------------------------------------------------------------------

runTest("Decoupling: readUsage execution path does not invoke sqlite3, DatabaseSync, or decodeProto", () => {
  import("node:fs").then(({ readFileSync }) => {
    const adapterCode = readFileSync(adapterPath, "utf8");
    const readUsageStart = adapterCode.indexOf("function readUsage()");
    assert.ok(readUsageStart > 0, "readUsage function not found");
    const readUsageEnd = adapterCode.indexOf("\ntry {\n  if (input.method ===", readUsageStart);
    assert.ok(readUsageEnd > readUsageStart, "readUsage boundary not found");
    const readUsageBody = adapterCode.slice(readUsageStart, readUsageEnd);

    assert.ok(!readUsageBody.includes("sqlite3"), "readUsage must not contain sqlite3");
    assert.ok(!readUsageBody.includes("DatabaseSync"), "readUsage must not contain DatabaseSync");
    assert.ok(!readUsageBody.includes("decodeProto"), "readUsage must not contain decodeProto");
    assert.ok(!readUsageBody.includes("gen_metadata"), "readUsage must not contain gen_metadata");
    assert.ok(!readUsageBody.includes("discoverConversationDbFiles"), "readUsage must not call discoverConversationDbFiles");
  });
});

runTest("Adversarial: Dynamic model switching via step.model attribute within turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "adv-model-switch-"));
  try {
    const uuid = "20202020-2020-2020-2020-202020202020";
    const logsDir = path.join(fixtureRoot, uuid, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    writeFileSync(
      path.join(logsDir, "transcript.jsonl"),
      [
        JSON.stringify({ step_index: 0, content: "The user changed setting `Model Selection` from gemini-2.5-pro to gemini-2.5-flash." }),
        JSON.stringify({ step_index: 1, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Fast prompt", created_at: "2026-10-30T06:00:00.000Z" }),
        JSON.stringify({ step_index: 2, source: "MODEL", type: "PLANNER_RESPONSE", input_tokens: 50, output_tokens: 20, created_at: "2026-10-30T06:00:01.000Z" }),
        JSON.stringify({ step_index: 3, source: "USER_EXPLICIT", type: "USER_INPUT", content: "Pro prompt", created_at: "2026-10-30T06:10:00.000Z" }),
        JSON.stringify({ step_index: 4, source: "MODEL", type: "PLANNER_RESPONSE", model: "gemini-2.5-pro", input_tokens: 1000, output_tokens: 500, created_at: "2026-10-30T06:10:05.000Z" }),
      ].join("\n")
    );

    const { events } = runAdapterUsage(fixtureRoot);
    assert.equal(events.length, 2);
    assert.equal(events[0].model, "gemini-2.5-flash");
    assert.equal(events[1].model, "gemini-2.5-pro", `Expected Turn 1 model to be 'gemini-2.5-pro', but got '${events[1].model}'`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

console.log(`\n========================================`);
console.log(`ALL ${totalTests} ADVERSARIAL STRESS TESTS COMPLETED`);
console.log(`========================================\n`);
