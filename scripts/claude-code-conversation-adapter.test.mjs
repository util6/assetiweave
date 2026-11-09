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

test("Claude Code turns inline sidechain events into a hidden child session without splitting the parent turn", () => {
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
        agentId: "agent-side-1",
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
        agentId: "agent-side-1",
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
        agentId: "agent-side-1",
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
        agentId: "agent-side-1",
        type: "assistant",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "Found 1 missing index on email column." }],
        },
      }),
      // Main agent continues inside the same turn
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
    assert.equal(session.turns.length, 1, "一次人类提问必须严格保持为 1 个 Turn");
    assert.equal(session.user_visible, true);
    assert.equal(session.execution_origin, "user");

    const turn = session.turns[0];
    assert.equal(turn.turn_index, 0);
    assert.equal(turn.user_text, "Refactor database queries");
    assert.equal(turn.turn_kind, undefined);
    assert.ok(
      turn.parts.some((part) => part.text && part.text.includes("Schema review complete")),
      "子代理返回后的主代理输出必须留在同一个父轮次里",
    );

    const anchor = turn.parts.find((part) => part.kind === "subagent");
    assert.ok(anchor, "父轮次必须包含子代理委派锚点");
    const anchorMeta = JSON.parse(anchor.metadata_json);
    assert.equal(anchorMeta.child_external_id, "session:subagent:agent-side-1");
    assert.match(anchor.text, /^\[委派任务\] /);
    assert.match(anchor.text, /\[汇报结论\] Found 1 missing index/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code emits a dispatch anchor for Agent tool calls and keeps the parent turn intact", () => {
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
    assert.equal(session.turns.length, 1, "派发子代理不得切分父轮次");

    const turn = session.turns[0];
    const kinds = turn.parts.map((part) => part.kind);
    assert.deepEqual(kinds, ["text", "subagent", "text"]);
    assert.equal(turn.parts[0].text, "Invoking explorer subagent.");

    const anchor = turn.parts[1];
    assert.equal(anchor.role, "assistant");
    assert.equal(anchor.source_execution_id, "toolu_agent_1");
    assert.match(anchor.text, /\[委派任务\] Locate authentication middleware implementations|\[委派任务\] Explore/);
    assert.match(anchor.text, /\[汇报结论\] Auth middleware found/);
    const anchorMeta = JSON.parse(anchor.metadata_json);
    assert.equal(anchorMeta.agent_role, "Explore");
    assert.equal(anchorMeta.spawn_anchor, "toolu_agent_1");
    assert.equal(anchorMeta.child_external_id, "session:subagent:toolu_agent_1");

    // 派发结果不再作为独立 result 零件重复出现
    assert.ok(
      !turn.parts.some((part) => part.source_execution_id === "toolu_agent_1" && part.kind !== "subagent"),
      "子代理派发的结果已被锚点吸收，不得再落一份重复零件",
    );
    assert.match(turn.parts[2].text, /Found the auth middleware/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code materializes subagents/agent-*.jsonl as deterministic hidden child sessions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-subagentfile-"));
  try {
    const parentSessionId = "parent-session";
    const mainFile = path.join(fixtureRoot, `${parentSessionId}.jsonl`);
    const subagentsDir = path.join(fixtureRoot, parentSessionId, "subagents");
    mkdirSync(subagentsDir, { recursive: true });
    const agentFile = path.join(subagentsDir, "agent-a434.jsonl");

    writeFileSync(mainFile, [
      JSON.stringify({
        timestamp: "2026-08-08T00:00:00Z",
        sessionId: parentSessionId,
        type: "user",
        message: { role: "user", content: [{ type: "text", text: "Main session task" }] },
      }),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:01Z",
        sessionId: parentSessionId,
        type: "assistant",
        message: {
          role: "assistant",
          content: [{
            type: "tool_use",
            id: "toolu_file_1",
            name: "Agent",
            input: { subagent_type: "Explore", description: "审计路径处理", prompt: "Audit path handling" },
          }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:02Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_file_1", content: "Async agent launched successfully." }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:03Z",
        sessionId: parentSessionId,
        type: "assistant",
        message: { role: "assistant", content: [{ type: "text", text: "Main session answer" }] },
      }),
    ].join("\n"));

    writeFileSync(agentFile, [
      JSON.stringify({
        timestamp: "2026-08-08T00:00:00Z",
        sessionId: parentSessionId,
        agentId: "a434",
        isSidechain: true,
        type: "user",
        message: { role: "user", content: [{ type: "text", text: "Audit path handling" }] },
      }),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:01Z",
        sessionId: parentSessionId,
        agentId: "a434",
        isSidechain: true,
        type: "assistant",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "发现 3 处硬编码路径分隔符。" }],
        },
      }),
    ].join("\n"));
    writeFileSync(
      path.join(subagentsDir, "agent-a434.meta.json"),
      JSON.stringify({
        agentType: "Explore",
        description: "审计路径处理",
        toolUseId: "toolu_file_1",
        spawnDepth: 1,
      }),
    );

    // 1. 父会话读取必须同时产出父子两个会话
    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: fixtureRoot }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const sessions = result.stdout
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line))
      .filter((entry) => entry.type === "item")
      .map((entry) => entry.item.session);
    assert.equal(sessions.length, 2, "一个父会话 + 一个子代理会话");

    const parent = sessions.find((entry) => entry.external_id === parentSessionId);
    const child = sessions.find((entry) => entry.external_id === `${parentSessionId}:subagent:a434`);
    assert.ok(parent, "父会话必须存在");
    assert.ok(child, "子会话 external_id 必须是 <parent>:subagent:<agentId>");

    assert.equal(parent.turns.length, 1);
    const anchor = parent.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor);
    const anchorMeta = JSON.parse(anchor.metadata_json);
    assert.equal(anchorMeta.child_external_id, `${parentSessionId}:subagent:a434`);
    assert.equal(anchorMeta.agent_role, "Explore");
    assert.equal(anchorMeta.title, "审计路径处理");
    assert.equal(anchorMeta.spawn_anchor, "toolu_file_1");
    assert.equal(anchorMeta.step_count, 0);
    assert.match(anchor.text, /\[汇报结论\] 发现 3 处硬编码路径分隔符/);

    assert.equal(child.user_visible, false);
    assert.equal(child.execution_origin, "subagent");
    const purpose = JSON.parse(child.execution_purpose);
    assert.equal(purpose.parent_external_id, parentSessionId);
    assert.equal(purpose.parent_turn_index, 0);
    assert.equal(purpose.agent_role, "Explore");
    assert.equal(purpose.spawn_anchor, "toolu_file_1");
    assert.equal(child.turns.length, 1);
    assert.equal(child.turns[0].user_text, "Audit path handling");

    // 2. 单独读取子代理文件同样产出隐藏子会话
    const childReadResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: agentFile }, params: {} }),
    });
    assert.equal(childReadResult.status, 0, childReadResult.stderr);
    const standalone = childReadResult.stdout
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line))
      .find((entry) => entry.type === "item")?.item?.session;
    assert.equal(standalone.external_id, `${parentSessionId}:subagent:a434`);
    assert.equal(standalone.user_visible, false);
    assert.equal(standalone.execution_origin, "subagent");

    // 3. list_sessions 必须包含子代理描述符，否则增量同步会把它误判为源已消失
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "list_sessions", source: { location: fixtureRoot }, params: {} }),
    });
    assert.equal(listResult.status, 0);
    const descriptors = listResult.stdout
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line))
      .filter((entry) => entry.type === "item")
      .map((entry) => entry.item);
    const ids = descriptors.map((entry) => entry.external_id).sort();
    assert.deepEqual(ids, [parentSessionId, `${parentSessionId}:subagent:a434`].sort());
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
    assert.equal(session.turns.length, 1, "派发子代理不得产生额外轮次");
    assert.equal(session.turns[0].turn_index, 0);
    assert.equal(session.turns[0].user_text, "Fix database pool leak");
    assert.equal(session.turns[0].turn_kind, undefined);
    const anchor = session.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor, "零前置文本也必须留下委派锚点");
    assert.match(anchor.text, /\[汇报结论\] Leak traced to unclosed connection/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code keeps subagent anchors distinct for sequential dispatches without ghost turns", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-sequential-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session.jsonl");
    writeFileSync(sessionFile, [
      message("2026-08-08T00:00:00Z", "user", [{ type: "text", text: "Execute two tasks in sequence" }], { id: "msg-user-1" }),
      message("2026-08-08T00:00:01Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_seq_1",
          name: "Agent",
          input: { subagent_type: "Explore", prompt: "Task 1 prompt" },
        },
      ]),
      JSON.stringify({
        timestamp: "2026-08-08T00:00:02Z",
        type: "user",
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_agent_seq_1", content: "Result 1" }],
        },
      }),
      message("2026-08-08T00:00:03Z", "assistant", [
        {
          type: "tool_use",
          id: "toolu_agent_seq_2",
          name: "Agent",
          input: { subagent_type: "Plan", prompt: "Task 2 prompt" },
        },
      ]),
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
    assert.equal(session.turns.length, 1, "连续两次派发仍然只有 1 个父轮次");
    assert.equal(session.turns.filter((turn) => turn.turn_kind).length, 0, "不得再产出伪轮次");

    const anchors = session.turns[0].parts.filter((part) => part.kind === "subagent");
    assert.equal(anchors.length, 2);
    const childIds = anchors.map((part) => JSON.parse(part.metadata_json).child_external_id);
    assert.deepEqual(childIds, [
      "session:subagent:toolu_agent_seq_1",
      "session:subagent:toolu_agent_seq_2",
    ]);
    assert.match(anchors[0].text, /\[汇报结论\] Result 1/);
    assert.match(anchors[1].text, /\[汇报结论\] Result 2/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code keeps an interrupted dispatch anchor inside its own parent turn", () => {
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
      // 用户在工具结果返回前打断
      message("2026-08-08T00:00:05Z", "user", [{ type: "text", text: "Stop, do this instead" }], { id: "msg-user-2" }),
      message("2026-08-08T00:00:06Z", "assistant", [{ type: "text", text: "Understood, stopped." }]),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    assert.equal(session.turns.length, 2);
    assert.equal(session.turns[0].user_text, "Start long subagent");
    assert.equal(session.turns[1].user_text, "Stop, do this instead");
    assert.equal(session.turns[1].turn_kind, undefined, "打断后的新提问是正常轮次，不是续接伪轮次");
    assert.equal(session.turns[1].parent_turn_id, undefined);

    const anchor = session.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor, "被打断的派发仍要留下锚点");
    assert.equal(JSON.parse(anchor.metadata_json).child_external_id, "session:subagent:toolu_agent_interrupted");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code multi-turn session keeps the second question at turn index 1", () => {
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
    assert.equal(session.turns.length, 2, "两次真实提问必须是两个物理轮次");
    const q2Turn = session.turns[1];
    assert.equal(q2Turn.turn_index, 1, "子代理派发不得让后续人类提问的轮次号偏移");
    assert.equal(q2Turn.user_text, "Question 2: What is the weather in Tokyo?");
    assert.equal(q2Turn.turn_kind, undefined);
    assert.equal(q2Turn.parent_turn_id, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code extracts single Turn usage and first response UTC timestamp", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-usage-single-"));
  try {
    const sessionId = "claude-session-1";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    writeFileSync(
      sessionFile,
      [
        message("2026-10-30T04:00:00Z", "user", [{ type: "text", text: "Explain rust ownership" }]),
        JSON.stringify({
          timestamp: "2026-10-30T04:00:02.123Z",
          type: "assistant",
          message: {
            role: "assistant",
            model: "claude-3-7-sonnet-20250219",
            content: [{ type: "text", text: "Rust ownership ensures memory safety..." }],
            usage: {
              input_tokens: 120,
              output_tokens: 65,
              cache_read_input_tokens: 40,
              cache_creation_input_tokens: 15,
              reasoning_tokens: 25,
            },
          },
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(complete.decoder_profile, "claude-code-transcript-v1");
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, sessionId);
    assert.equal(event.turn_id, "turn-0");
    assert.equal(event.external_event_id, `${sessionId}-turn-0`);
    assert.equal(event.logical_request_id, `${sessionId}-turn-0`);
    assert.equal(event.attempt_index, 0);
    assert.equal(event.provider, "anthropic");
    assert.equal(event.model, "claude-3-7-sonnet-20250219");
    assert.equal(event.timestamp, "2026-10-30T04:00:02.123Z");
    assert.equal(event.input_tokens, 120);
    assert.equal(event.output_tokens, 65);
    assert.equal(event.cache_read_tokens, 40);
    assert.equal(event.cache_write_tokens, 15);
    assert.equal(event.reasoning_tokens, 25);
    assert.equal(event.total_tokens, 265);
    assert.equal(event.status, "success");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code accumulates multi-step tool calls / thinking tokens within single Turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-usage-multi-step-"));
  try {
    const sessionId = "claude-multi-step";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    writeFileSync(
      sessionFile,
      [
        message("2026-10-30T04:10:00Z", "user", [{ type: "text", text: "Refactor database query" }]),
        JSON.stringify({
          timestamp: "2026-10-30T04:10:01.500Z",
          type: "assistant",
          message: {
            role: "assistant",
            model: "claude-3-7-sonnet-20250219",
            content: [{ type: "tool_use", id: "tool-read", name: "Read", input: { file_path: "db.ts" } }],
            usage: {
              input_tokens: 100,
              output_tokens: 30,
              cache_read_input_tokens: 10,
              cache_creation_input_tokens: 5,
              reasoning_tokens: 15,
            },
          },
        }),
        JSON.stringify({
          timestamp: "2026-10-30T04:10:02.000Z",
          type: "user",
          message: {
            role: "user",
            content: [{ type: "tool_result", tool_use_id: "tool-read", content: "export function query() {}" }],
          },
        }),
        JSON.stringify({
          timestamp: "2026-10-30T04:10:05.800Z",
          type: "assistant",
          message: {
            role: "assistant",
            model: "claude-3-7-sonnet-20250219",
            content: [{ type: "text", text: "Refactoring complete." }],
            usage: {
              input_tokens: 150,
              output_tokens: 40,
              cache_read_input_tokens: 20,
              cache_creation_input_tokens: 0,
              reasoning_tokens: 10,
            },
          },
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.turn_id, "turn-0");
    assert.equal(event.external_event_id, `${sessionId}-turn-0`);
    assert.equal(event.timestamp, "2026-10-30T04:10:01.500Z"); // First response timestamp preserved
    assert.equal(event.input_tokens, 250);
    assert.equal(event.output_tokens, 70);
    assert.equal(event.cache_read_tokens, 30);
    assert.equal(event.cache_write_tokens, 5);
    assert.equal(event.reasoning_tokens, 25);
    assert.equal(event.total_tokens, 380);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code preserves separate historical UTC timestamps for cross-day multi-turn sessions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-usage-cross-day-"));
  try {
    const sessionId = "claude-cross-day";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    writeFileSync(
      sessionFile,
      [
        message("2026-10-28T10:00:00Z", "user", [{ type: "text", text: "Day 1 query" }]),
        JSON.stringify({
          timestamp: "2026-10-28T10:00:01.100Z",
          type: "assistant",
          message: {
            role: "assistant",
            content: [{ type: "text", text: "Day 1 answer" }],
            usage: { input_tokens: 50, output_tokens: 25 },
          },
        }),
        message("2026-10-29T11:00:00Z", "user", [{ type: "text", text: "Day 2 query" }]),
        JSON.stringify({
          timestamp: "2026-10-29T11:00:03.200Z",
          type: "assistant",
          message: {
            role: "assistant",
            content: [{ type: "text", text: "Day 2 answer" }],
            usage: { input_tokens: 60, output_tokens: 30 },
          },
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 2);
    assert.equal(events.length, 2);

    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-0`);
    assert.equal(events[0].timestamp, "2026-10-28T10:00:01.100Z");
    assert.equal(events[0].input_tokens, 50);
    assert.equal(events[0].output_tokens, 25);

    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[1].timestamp, "2026-10-29T11:00:03.200Z");
    assert.equal(events[1].input_tokens, 60);
    assert.equal(events[1].output_tokens, 30);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code filters out empty zero-token turns from usage event stream", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-usage-zero-token-"));
  try {
    const sessionId = "claude-zero-token";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    writeFileSync(
      sessionFile,
      [
        message("2026-10-30T05:00:00Z", "user", [{ type: "text", text: "Turn 0 has no tokens" }]),
        message("2026-10-30T05:00:01Z", "assistant", [{ type: "text", text: "No usage info" }]),
        message("2026-10-30T05:01:00Z", "user", [{ type: "text", text: "Turn 1 has real tokens" }]),
        JSON.stringify({
          timestamp: "2026-10-30T05:01:02.500Z",
          type: "assistant",
          message: {
            role: "assistant",
            content: [{ type: "text", text: "Here are tokens" }],
            usage: { input_tokens: 80, output_tokens: 40 },
          },
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-1");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[0].total_tokens, 120);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Claude Code supports incremental cursor skipping for unchanged files", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-claude-usage-cursor-"));
  try {
    const sessionId = "claude-cursor-sess";
    const sessionFile = path.join(fixtureRoot, `${sessionId}.jsonl`);
    writeFileSync(
      sessionFile,
      [
        message("2026-10-30T05:20:00Z", "user", [{ type: "text", text: "Initial question" }]),
        JSON.stringify({
          timestamp: "2026-10-30T05:20:01.000Z",
          type: "assistant",
          message: {
            role: "assistant",
            content: [{ type: "text", text: "Initial answer" }],
            usage: { input_tokens: 40, output_tokens: 20 },
          },
        }),
      ].join("\n")
    );

    const firstRun = readFixtureUsage(fixtureRoot);
    assert.equal(firstRun.events.length, 1);
    const cursor = JSON.parse(firstRun.complete.next_cursor);
    assert.ok(cursor.last_mtime > 0);
    assert.ok(cursor.processed_files.includes(sessionFile));

    // Second run with cursor should skip file
    const secondRun = readFixtureUsage(fixtureRoot, { cursor: firstRun.complete.next_cursor });
    assert.equal(secondRun.events.length, 0);
    assert.equal(secondRun.complete.usage_event_count, 0);
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

function readFixtureUsage(location, params = {}) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({
      method: "read_usage",
      source: { location },
      params,
    }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
  const events = messages
    .filter((message) => message.type === "usage_event")
    .map((message) => message.usage_event);
  const complete = messages.find((message) => message.type === "complete")?.item ?? null;
  return { messages, events, complete };
}
