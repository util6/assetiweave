import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/claude-code/adapter.mjs");

test("Claude Code normalizes a Read execution in-place and removes its successful body", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-read-"));
  try {
    const sessionPath = path.join(fixtureRoot, "session-read.jsonl");
    writeFileSync(sessionPath, [
      message("2026-08-05T00:00:00Z", "user", [{ type: "text", text: "读取入口文件" }]),
      message("2026-08-05T00:00:01Z", "assistant", [{
        type: "tool_use",
        id: "tool-read",
        name: "Read",
        input: { file_path: "/tmp/project/src/main.ts", offset: 10, limit: 20 },
      }]),
      message("2026-08-05T00:00:02Z", "user", [{
        type: "tool_result",
        tool_use_id: "tool-read",
        content: "10→const value = true;",
      }], { toolUseResult: { type: "text", file: { filePath: "/tmp/project/src/main.ts" } } }),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.deepEqual(parts.map((part) => part.source_execution_id), ["tool-read", "tool-read"]);
    assert.equal(parts[0].kind, "command");
    assert.equal(parts[0].content_card.kind, "claude-code.command");
    assert.equal(parts[0].command, "/tmp/project/src/main.ts:10-29");
    assert.equal(parts[0].status, "completed");
    assert.equal(parts[0].exit_code, 0);
    assert.equal(parts[1].text, null);
    assert.equal(JSON.parse(parts[1].metadata_json).execution_kind, "read");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code stores one canonical diff in the original Edit result Part", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-edit-"));
  try {
    const sessionPath = path.join(fixtureRoot, "session-edit.jsonl");
    writeFileSync(sessionPath, [
      message("2026-08-05T00:00:00Z", "user", [{ type: "text", text: "修改入口文件" }]),
      message("2026-08-05T00:00:01Z", "assistant", [{
        type: "tool_use",
        id: "tool-edit",
        name: "Edit",
        input: { file_path: "/tmp/project/src/main.ts", old_string: "old", new_string: "new" },
      }]),
      message("2026-08-05T00:00:02Z", "user", [{
        type: "tool_result",
        tool_use_id: "tool-edit",
        content: "The file /tmp/project/src/main.ts has been updated successfully.",
      }], {
        toolUseResult: {
          filePath: "/tmp/project/src/main.ts",
          oldString: "old",
          newString: "new",
          structuredPatch: [{
            oldStart: 4,
            oldLines: 1,
            newStart: 4,
            newLines: 1,
            lines: ["-old", "+new"],
          }],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.deepEqual(parts.map((part) => part.source_execution_id), ["tool-edit", "tool-edit"]);
    assert.equal(parts[1].kind, "file_change");
    assert.equal(parts[1].content_card.kind, "claude-code.file-change");
    assert.equal(parts[1].content_card.renderer, "diff");
    assert.match(parts[1].text, /^diff --git a\/tmp\/project\/src\/main\.ts b\/tmp\/project\/src\/main\.ts/m);
    assert.match(parts[1].text, /@@ -4,1 \+4,1 @@\n-old\n\+new/);
    assert.equal(parts[1].text.includes("updated successfully"), false);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code keeps an aggregated Bash execution raw without persisted display projection", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-shell-projection-"));
  try {
    const sessionPath = path.join(fixtureRoot, "session-shell.jsonl");
    const command = [
      "printf '%s\\n' '--- inspect ---'",
      "rg 'quoted && value' ./src | sed 's/;/|/'",
      "git status --short > /tmp/status.txt",
    ].join(" && ");
    writeFileSync(sessionPath, [
      message("2026-08-05T00:00:00Z", "user", [{ type: "text", text: "检查工作区" }]),
      message("2026-08-05T00:00:01Z", "assistant", [{
        type: "tool_use",
        id: "tool-shell-projection",
        name: "Bash",
        input: { command, description: "inspect" },
      }]),
      message("2026-08-05T00:00:02Z", "user", [{
        type: "tool_result",
        tool_use_id: "tool-shell-projection",
        content: "Error: failed",
        is_error: true,
      }]),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.deepEqual(parts.map((part) => part.source_execution_id), [
      "tool-shell-projection",
      "tool-shell-projection",
    ]);
    assert.equal(parts[0].command, command);
    assert.equal(parts[1].text, "Error: failed");
    assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code keeps a simple Bash execution boundary and omits persisted display projection", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-simple-shell-"));
  try {
    const sessionPath = path.join(fixtureRoot, "session-simple-shell.jsonl");
    writeFileSync(sessionPath, [
      message("2026-08-05T00:00:00Z", "user", [{ type: "text", text: "查看状态" }]),
      message("2026-08-05T00:00:01Z", "assistant", [{
        type: "tool_use",
        id: "tool-simple-shell",
        name: "Bash",
        input: { command: "git status --short" },
      }]),
      message("2026-08-05T00:00:02Z", "user", [{
        type: "tool_result",
        tool_use_id: "tool-simple-shell",
        content: "clean",
      }]),
    ].join("\n"));

    const parts = readFixtureSession(fixtureRoot).turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].command, "git status --short");
    assert.deepEqual(parts.map((part) => part.source_execution_id), [
      "tool-simple-shell",
      "tool-simple-shell",
    ]);
    assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code extracts model identifier onto the turn object", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-model-"));
  try {
    const sessionPath = path.join(fixtureRoot, "session-model.jsonl");
    writeFileSync(sessionPath, [
      message("2026-08-05T00:00:00Z", "user", [{ type: "text", text: "测试模型提取" }]),
      JSON.stringify({
        timestamp: "2026-08-05T00:00:01Z",
        type: "assistant",
        message: {
          role: "assistant",
          model: "claude-3-7-sonnet-20250219",
          content: [{ type: "text", text: "模型回答测试" }],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.equal(session.turns[0].model, "claude-3-7-sonnet-20250219");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code inlines sidechain events into specialized subagent turn and emits continuation turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-sidechain-"));
  try {
    writeFileSync(path.join(fixtureRoot, "session.jsonl"), [
      message("2026-08-06T00:00:00Z", "user", [{ type: "text", text: "Refactor database queries" }]),
      JSON.stringify({
        timestamp: "2026-08-06T00:00:01Z",
        type: "assistant",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "Dispatching sidechain agent to review schemas" }],
        },
      }),
      // Sidechain task prompt
      JSON.stringify({
        timestamp: "2026-08-06T00:00:02Z",
        isSidechain: true,
        type: "user",
        message: {
          role: "user",
          content: [{ type: "text", text: "Review schema migration files for index usage" }],
        },
      }),
      // Sidechain tool use
      JSON.stringify({
        timestamp: "2026-08-06T00:00:03Z",
        isSidechain: true,
        type: "assistant",
        message: {
          role: "assistant",
          content: [{
            type: "tool_use",
            id: "toolu_side_1",
            name: "Bash",
            input: { command: "cat migrations/001.sql" },
          }],
        },
      }),
      // Sidechain tool result
      JSON.stringify({
        timestamp: "2026-08-06T00:00:04Z",
        isSidechain: true,
        type: "user",
        message: {
          role: "user",
          content: [{
            type: "tool_result",
            tool_use_id: "toolu_side_1",
            content: "CREATE TABLE users (id TEXT PRIMARY KEY, email TEXT);",
          }],
        },
      }),
      // Sidechain assistant answer
      JSON.stringify({
        timestamp: "2026-08-06T00:00:05Z",
        isSidechain: true,
        type: "assistant",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "Found 1 missing index on email column." }],
        },
      }),
      // Main agent continuation
      JSON.stringify({
        timestamp: "2026-08-06T00:00:06Z",
        type: "assistant",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "Schema review complete. Adding index on email." }],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.equal(session.turns.length, 3);

    // Turn 0: Main Anchor turn
    const turn0 = session.turns[0];
    assert.equal(turn0.turn_index, 0);
    assert.equal(turn0.user_text, "Refactor database queries");
    assert.equal(turn0.external_id, "turn-0");

    // Turn 1: Specialized subagent turn
    const turn1 = session.turns[1];
    assert.equal(turn1.turn_index, 1);
    assert.equal(turn1.turn_kind, "subagent");
    assert.equal(turn1.agent_role, "Sidechain Subagent");
    assert.equal(turn1.parent_turn_id, "turn-0");
    assert.equal(turn1.user_text, "Review schema migration files for index usage");
    assert.ok(turn1.parts.length >= 2);
    const hasCmd = turn1.parts.some((p) => p.kind === "command" && p.command.includes("cat migrations"));
    assert.ok(hasCmd, "Sidechain turn should contain command part");
    const lastAnswer = turn1.parts[turn1.parts.length - 1];
    assert.equal(lastAnswer.role, "assistant");
    assert.match(lastAnswer.text, /Found 1 missing index/);

    // Turn 2: Continuation turn
    const turn2 = session.turns[2];
    assert.equal(turn2.turn_index, 2);
    assert.equal(turn2.turn_kind, "continuation");
    assert.equal(turn2.parent_turn_id, "turn-0");
    assert.equal(turn2.user_text, "Continue following subagent execution");
    assert.ok(turn2.parts.length > 0);
    assert.match(turn2.parts[0].text, /Schema review complete/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code inlines Agent tool call into subagent turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-agenttool-"));
  try {
    writeFileSync(path.join(fixtureRoot, "session.jsonl"), [
      message("2026-08-07T00:00:00Z", "user", [{ type: "text", text: "Explore the codebase" }]),
      JSON.stringify({
        timestamp: "2026-08-07T00:00:01Z",
        type: "assistant",
        message: {
          role: "assistant",
          content: [
            { type: "text", text: "Invoking explorer subagent." },
            {
              type: "tool_use",
              id: "toolu_agent_1",
              name: "Agent",
              input: {
                prompt: "Locate authentication middleware implementations",
                subagent_type: "Explore",
              },
            },
          ],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-07T00:00:02Z",
        type: "user",
        message: {
          role: "user",
          content: [{
            type: "tool_result",
            tool_use_id: "toolu_agent_1",
            content: "Auth middleware found in src/middleware/auth.ts",
          }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-07T00:00:03Z",
        type: "assistant",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "Found the auth middleware, now inspecting." }],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.equal(session.turns.length, 3);

    // Turn 1 should be subagent turn
    const turn1 = session.turns[1];
    assert.equal(turn1.turn_kind, "subagent");
    assert.equal(turn1.agent_role, "Explore");
    assert.equal(turn1.parent_turn_id, "turn-0");
    assert.equal(turn1.user_text, "Locate authentication middleware implementations");
    assert.ok(turn1.parts.length > 0);
    assert.match(turn1.parts[turn1.parts.length - 1].text, /Auth middleware found/);

    // Turn 2 should be continuation turn
    const turn2 = session.turns[2];
    assert.equal(turn2.turn_kind, "continuation");
    assert.equal(turn2.parent_turn_id, "turn-0");
    assert.match(turn2.parts[0].text, /Found the auth middleware/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code filters subagent sessions from list_sessions and sets user_visible false on standalone read", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-filter-"));
  try {
    const mainFile = path.join(fixtureRoot, "main-session.jsonl");
    const subagentsDir = path.join(fixtureRoot, "subagents");
    mkdirSync(subagentsDir, { recursive: true });
    const subFile = path.join(subagentsDir, "subagent-1.jsonl");

    writeFileSync(mainFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Main session task" }]),
      message("2026-08-08T00:00:01Z", "assistant", [{ type: "text", text: "Main session answer" }]),
    ].join("\n"));

    writeFileSync(subFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Subagent internal task" }]),
      message("2026-08-08T00:00:01Z", "assistant", [{ type: "text", text: "Subagent answer" }]),
    ].join("\n"));

    // 1. list_sessions should exclude subagents/
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "list_sessions", source: { location: fixtureRoot }, params: {} }),
    });
    assert.equal(listResult.status, 0);
    const items = listResult.stdout.trim().split("\n").map((l) => JSON.parse(l)).filter((m) => m.type === "item");
    assert.equal(items.length, 1);
    assert.equal(items[0].item.external_id, "main-session");

    // 2. standalone read of subFile should set user_visible: false
    const readSubResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: subFile }, params: {} }),
    });
    assert.equal(readSubResult.status, 0);
    const subSession = readSubResult.stdout.trim().split("\n").map((l) => JSON.parse(l)).find((m) => m.type === "item")?.item?.session;
    assert.ok(subSession);
    assert.equal(subSession.user_visible, false);
    assert.equal(subSession.execution_origin, "subagent");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code preserves anchor user turn when assistant dispatches Agent with zero preceding text", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-zerotext-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session.jsonl");
    writeFileSync(sessionFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Fix database pool leak" }], { id: "msg-user-1" }),
      message("2026-08-08T00:00:01Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_1",
          name: "Agent",
          input: { subagent_type: "Explore", prompt: "Trace connection pool leak" },
        },
      ]),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:02Z",
        type: "user",
        message: {
          role: "user",
          content: [
            {
              type: "tool_result",
              tool_use_id: "toolu_agent_1",
              content: "Leak traced to unclosed connection in healthcheck",
            },
          ],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    assert.ok(session.turns.length >= 2, `Expected >= 2 turns, got ${session.turns.length}`);
    // Anchor turn must be preserved as turn 0
    assert.equal(session.turns[0].turn_index, 0);
    assert.equal(session.turns[0].user_text, "Fix database pool leak");
    assert.equal(session.turns[0].turn_kind, undefined);

    // Subagent turn
    assert.equal(session.turns[1].turn_index, 1);
    assert.equal(session.turns[1].turn_kind, "subagent");
    assert.equal(session.turns[1].parent_turn_id, session.turns[0].external_id);
    assert.equal(session.turns[1].user_text, "Trace connection pool leak");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code falls back to valid string when subagent prompt is whitespace-only", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-ws-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session.jsonl");
    writeFileSync(sessionFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Run delegator" }], { id: "msg-user-1" }),
      message("2026-08-08T00:00:01Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_ws",
          name: "Agent",
          input: { subagent_type: "Explore", prompt: "   \t\n  " },
        },
      ]),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:02Z",
        type: "user",
        message: {
          role: "user",
          content: [
            {
              type: "tool_result",
              tool_use_id: "toolu_agent_ws",
              content: "Done",
            },
          ],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    assert.equal(session.turns[1].turn_kind, "subagent");
    assert.equal(session.turns[1].user_text, "Subagent execution");
    assert.ok(session.turns[1].user_text.trim().length > 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code does not emit empty ghost continuation turns on sequential subagent calls", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-sequential-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session.jsonl");
    writeFileSync(sessionFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Execute two tasks in sequence" }], { id: "msg-user-1" }),
      // Subagent 1 dispatch
      message("2026-08-08T00:00:01Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_seq_1",
          name: "Agent",
          input: { subagent_type: "Explore", prompt: "Task 1 prompt" },
        },
      ]),
      // Subagent 1 result
      JSON.stringify({
        timestamp: "2026-08-08T00:00:02Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_agent_seq_1", content: "Result 1" }],
        },
      }),
      // Immediate Subagent 2 dispatch with zero pre-text
      message("2026-08-08T00:00:03Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_seq_2",
          name: "Agent",
          input: { subagent_type: "Plan", prompt: "Task 2 prompt" },
        },
      ]),
      // Subagent 2 result
      JSON.stringify({
        timestamp: "2026-08-08T00:00:04Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_agent_seq_2", content: "Result 2" }],
        },
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    // Must NOT contain any continuation turn with parts: []
    const emptyContTurns = session.turns.filter((t) => t.turn_kind === "continuation" && (!t.parts || t.parts.length === 0));
    assert.equal(emptyContTurns.length, 0, "No empty ghost continuation turns should be present");

    // Both subagents should be parented to msg-user-1
    const subagents = session.turns.filter((t) => t.turn_kind === "subagent");
    assert.equal(subagents.length, 2);
    assert.equal(subagents[0].parent_turn_id, "msg-user-1");
    assert.equal(subagents[1].parent_turn_id, "msg-user-1");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code preserves subagent before user interruption in chronological order", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-interrupt-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session.jsonl");
    writeFileSync(sessionFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Start long subagent" }], { id: "msg-user-1" }),
      message("2026-08-08T00:00:01Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_interrupted",
          name: "Agent",
          input: { subagent_type: "Explore", prompt: "Long running search" },
        },
      ]),
      // User interrupts with a new message before tool result
      message("2026-08-08T00:00:05Z", "user", [{ type: "text", text: "Stop, do this instead" }], { id: "msg-user-2" }),
      message("2026-08-08T00:00:06Z", "assistant", [{ type: "text", text: "Understood, stopped." }]),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    // Subagent should be emitted BEFORE msg-user-2 turn
    const subagentIdx = session.turns.findIndex((t) => t.turn_kind === "subagent");
    const user2Idx = session.turns.findIndex((t) => t.user_text === "Stop, do this instead");
    assert.ok(subagentIdx !== -1, "Subagent turn must be preserved");
    assert.ok(user2Idx !== -1, "User interruption turn must be preserved");
    assert.ok(subagentIdx < user2Idx, `Subagent index (${subagentIdx}) must precede interrupted user turn (${user2Idx})`);
    assert.equal(session.turns[user2Idx].turn_kind, undefined, "Interruption user turn must NOT be continuation");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code multi-turn session keeps subsequent question as standard user turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-multiturn-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session.jsonl");
    writeFileSync(sessionFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Question 1: Fix bug" }], { id: "msg-user-1" }),
      message("2026-08-08T00:00:01Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_q1",
          name: "Agent",
          input: { subagent_type: "Explore", prompt: "Subtask prompt" },
        },
      ]),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:02Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_agent_q1", content: "Done" }],
        },
      }),
      message("2026-08-08T00:00:03Z", "assistant", [{ type: "text", text: "Question 1 complete" }]),
      message("2026-08-08T00:05:00Z", "user", [{ type: "text", text: "Question 2: What is the weather in Tokyo?" }], { id: "msg-user-2" }),
      message("2026-08-08T00:05:01Z", "assistant", [{ type: "text", text: "Tokyo is sunny." }]),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    const q2Turn = session.turns.find((t) => t.user_text && t.user_text.includes("Question 2: What is the weather in Tokyo?"));
    assert.ok(q2Turn, "Question 2 turn must exist");
    assert.equal(q2Turn.turn_kind, undefined, "Question 2 must NOT be marked as continuation");
    assert.equal(q2Turn.parent_turn_id, undefined, "Question 2 must NOT have parent_turn_id");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function message(timestamp, role, content, extra = {}) {
  return JSON.stringify({ timestamp, type: role, message: { role, content }, ...extra });
}

function readFixtureSession(fixtureRoot) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: fixtureRoot }, params: {} }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  const session = messages.find((entry) => entry.type === "item")?.item?.session;
  assert.ok(session);
  return session;
}
