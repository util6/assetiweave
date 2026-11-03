import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawnSync } from "node:child_process";

const ADAPTER_PATH = path.resolve("builtin-assets/adapters/antigravity/adapter.mjs");

function runAdapter(payload) {
  const res = spawnSync(process.execPath, [ADAPTER_PATH], {
    input: JSON.stringify(payload),
    encoding: "utf8",
  });
  if (res.error) throw res.error;
  if (res.status !== 0) {
    throw new Error(`Adapter exited with code ${res.status}: ${res.stderr}`);
  }
  const lines = res.stdout.trim().split("\n").filter(Boolean);
  const items = [];
  let complete = null;
  for (const line of lines) {
    const msg = JSON.parse(line);
    if (msg.type === "item") items.push(msg.item);
    else if (msg.type === "complete") complete = msg.item;
  }
  return { items, complete, stderr: res.stderr };
}

test("Adversarial Stress: Multi-model intra-turn and cross-turn attribution", () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "ag-stress-model-"));
  try {
    const sId = "33333333-3333-3333-3333-333333333333";
    const logDir = path.join(tmpDir, sId, ".system_generated", "logs");
    fs.mkdirSync(logDir, { recursive: true });

    // Turn 0: Default model ("gemini-2.5-pro")
    // Turn 1: Setting change inside turn to "gemini-2.5-flash"
    // Turn 2: Starts with "gemini-2.5-flash", but step.model specifies "claude-3-7-sonnet"
    // Turn 3: Starts with "claude-3-7-sonnet" (from previous turn step.model), then step.model overrides to "gemini-1.5-pro"
    // Turn 4: No model override, inherits "gemini-1.5-pro"
    const transcriptLines = [
      // Turn 0
      JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", created_at: "2026-10-30T01:00:00Z" }),
      JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", created_at: "2026-10-30T01:00:01Z", input_tokens: 10, output_tokens: 20 }),

      // Turn 1
      JSON.stringify({ step_index: 2, source: "USER_EXPLICIT", type: "USER_INPUT", created_at: "2026-10-30T02:00:00Z" }),
      JSON.stringify({ step_index: 3, source: "SYSTEM", type: "SETTING_CHANGE", content: "The user changed setting Model Selection from gemini-2.5-pro to gemini-2.5-flash." }),
      JSON.stringify({ step_index: 4, source: "MODEL", type: "PLANNER_RESPONSE", created_at: "2026-10-30T02:00:02Z", input_tokens: 15, output_tokens: 25 }),

      // Turn 2
      JSON.stringify({ step_index: 5, source: "USER_EXPLICIT", type: "USER_INPUT", created_at: "2026-10-30T03:00:00Z" }),
      JSON.stringify({ step_index: 6, source: "MODEL", type: "PLANNER_RESPONSE", model: "claude-3-7-sonnet", created_at: "2026-10-30T03:00:01Z", input_tokens: 30, output_tokens: 40 }),

      // Turn 3 (multiple step.model within single turn)
      JSON.stringify({ step_index: 7, source: "USER_EXPLICIT", type: "USER_INPUT", created_at: "2026-10-30T04:00:00Z" }),
      JSON.stringify({ step_index: 8, source: "MODEL", type: "PLANNER_RESPONSE", model: "gemini-experimental", created_at: "2026-10-30T04:00:01Z", input_tokens: 5, output_tokens: 5 }),
      JSON.stringify({ step_index: 9, source: "MODEL", type: "PLANNER_RESPONSE", model: "gemini-1.5-pro", created_at: "2026-10-30T04:00:05Z", input_tokens: 50, output_tokens: 60 }),

      // Turn 4 (no override, should inherit gemini-1.5-pro)
      JSON.stringify({ step_index: 10, source: "USER_EXPLICIT", type: "USER_INPUT", created_at: "2026-10-30T05:00:00Z" }),
      JSON.stringify({ step_index: 11, source: "MODEL", type: "PLANNER_RESPONSE", created_at: "2026-10-30T05:00:01Z", input_tokens: 100, output_tokens: 200 }),
    ];

    fs.writeFileSync(path.join(logDir, "transcript.jsonl"), transcriptLines.join("\n"));

    const { items } = runAdapter({
      method: "read_usage",
      source: { location: tmpDir },
    });

    const events = items.filter(i => i.kind === "usage_event").map(i => i.event);
    assert.equal(events.length, 5, "Expected exactly 5 turns");

    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].model, "gemini-2.5-pro");
    assert.equal(events[0].total_tokens, 30);

    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].model, "gemini-2.5-flash");
    assert.equal(events[1].total_tokens, 40);

    assert.equal(events[2].turn_id, "turn-2");
    assert.equal(events[2].model, "claude-3-7-sonnet");
    assert.equal(events[2].total_tokens, 70);

    assert.equal(events[3].turn_id, "turn-3");
    assert.equal(events[3].model, "gemini-1.5-pro");
    assert.equal(events[3].total_tokens, 120);

    assert.equal(events[4].turn_id, "turn-4");
    assert.equal(events[4].model, "gemini-1.5-pro");
    assert.equal(events[4].total_tokens, 300);

  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test("Adversarial Stress: Session ID filtering edge cases", () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "ag-stress-filter-"));
  try {
    const sIds = [
      "11111111-1111-1111-1111-111111111111",
      "22222222-2222-2222-2222-222222222222",
      "33333333-3333-3333-3333-333333333333",
      "44444444-4444-4444-4444-444444444444",
    ];

    for (const sid of sIds) {
      const logDir = path.join(tmpDir, sid, ".system_generated", "logs");
      fs.mkdirSync(logDir, { recursive: true });
      fs.writeFileSync(path.join(logDir, "transcript.jsonl"), [
        JSON.stringify({ step_index: 0, source: "USER_EXPLICIT", type: "USER_INPUT", created_at: "2026-10-30T00:00:00Z" }),
        JSON.stringify({ step_index: 1, source: "MODEL", type: "PLANNER_RESPONSE", created_at: "2026-10-30T00:00:01Z", input_tokens: 10, output_tokens: 10 })
      ].join("\n"));
    }

    // 1. Query with existing session ID
    {
      const { items } = runAdapter({
        method: "read_usage",
        source: { location: tmpDir },
        params: { session_id: sIds[2] },
      });
      const events = items.filter(i => i.kind === "usage_event").map(i => i.event);
      assert.equal(events.length, 1, "Must return exactly 1 event for matching session");
      assert.equal(events[0].session_id, sIds[2]);
    }

    // 2. Query with non-existent session ID
    {
      const { items } = runAdapter({
        method: "read_usage",
        source: { location: tmpDir },
        params: { session_id: "00000000-0000-0000-0000-000000000000" },
      });
      const events = items.filter(i => i.kind === "usage_event").map(i => i.event);
      assert.equal(events.length, 0, "Must return 0 events for non-existent session ID");
    }

    // 3. Query without params.session_id (should return all 4 sessions)
    {
      const { items } = runAdapter({
        method: "read_usage",
        source: { location: tmpDir },
      });
      const events = items.filter(i => i.kind === "usage_event").map(i => i.event);
      assert.equal(events.length, 4, "Must return all 4 sessions when no filter specified");
    }

    // 4. Query with empty string session_id (should not filter, return all 4 sessions)
    {
      const { items } = runAdapter({
        method: "read_usage",
        source: { location: tmpDir },
        params: { session_id: "" },
      });
      const events = items.filter(i => i.kind === "usage_event").map(i => i.event);
      assert.equal(events.length, 4, "Empty string session_id is falsy and should return all sessions");
    }
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test("Adversarial Stress: Truncated transcript with PLANNER_RESPONSE and dynamic model", () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "ag-stress-headless-"));
  try {
    const sId = "55555555-5555-5555-5555-555555555555";
    const logDir = path.join(tmpDir, sId, ".system_generated", "logs");
    fs.mkdirSync(logDir, { recursive: true });

    // Headless transcript: No USER_INPUT at all, directly starts with PLANNER_RESPONSE
    // with model: "gemini-2.5-flash"
    const transcriptLines = [
      JSON.stringify({
        step_index: 0,
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        model: "gemini-2.5-flash",
        created_at: "2026-10-30T06:00:00Z",
        input_tokens: 50,
        output_tokens: 100,
      }),
    ];

    fs.writeFileSync(path.join(logDir, "transcript.jsonl"), transcriptLines.join("\n"));

    const { items } = runAdapter({
      method: "read_usage",
      source: { location: tmpDir },
    });

    const events = items.filter(i => i.kind === "usage_event").map(i => i.event);
    assert.equal(events.length, 1, "Headless turn should produce 1 event");
    assert.equal(events[0].model, "gemini-2.5-flash", "Headless turn should capture model from step.model");
    assert.equal(events[0].total_tokens, 150);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
