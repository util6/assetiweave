import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/codebuddy/adapter.mjs");

function readFixtureSession(filePath) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: filePath }, params: {} }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  const session = messages.find((message) => message.type === "item")?.item?.session;
  assert.ok(session, `session not found in messages: ${result.stdout}`);
  return session;
}

function readFixtureSessions(filePath, sourceLocation = filePath) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: sourceLocation }, params: { source_locator: filePath } }),
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .filter((message) => message.type === "item" && message.item?.session)
    .map((message) => message.item.session);
}

function anchorPartsOf(session) {
  return session.turns.flatMap((turn) => (turn.parts ?? []).filter((part) => part.kind === "subagent"));
}

test("CodeBuddy parses Bash command and extracts clean output with exitCode", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-bash-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-1.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Run cargo build" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-1",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Bash",
        callId: "call_bash_1",
        arguments: { command: "cargo build --bin demo", description: "Build demo binary" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-1",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Bash",
        callId: "call_bash_1",
        status: "completed",
        output: {
          type: "text",
          text: "Command: cargo build --bin demo\nStdout: (empty)\nStderr: error[E0425]: cannot find value `x` in this scope\nExit Code: 1",
        },
        providerData: {
          toolResult: {
            rawResponse: {
              exitCode: 1,
            },
          },
        },
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    assert.equal(session.turns[0].parts.length, 3); // User, Command, Result

    const cmdPart = session.turns[0].parts[1];
    const resPart = session.turns[0].parts[2];

    assert.equal(cmdPart.kind, "command");
    assert.equal(cmdPart.content_card?.kind, "codebuddy.command");
    assert.equal(cmdPart.command, "cargo build --bin demo");

    assert.equal(resPart.kind, "tool");
    assert.equal(resPart.content_card?.kind, "codebuddy.result");
    assert.equal(resPart.content_card?.renderer, "terminal_output");
    assert.equal(resPart.exit_code, 1);
    assert.equal(resPart.status, "failed");
    assert.match(resPart.text, /cannot find value `x`/);
    assert.doesNotMatch(resPart.text, /^Command:/);
    assert.equal(resPart.source_execution_id, "call_bash_1");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy reconstructs concrete unified diff from Write tool call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-write-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-write.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Create config file" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-w",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Write",
        callId: "call_write_1",
        arguments: {
          path: "/project/config.json",
          content: '{\n  "mode": "dev"\n}',
        },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-w",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Write",
        callId: "call_write_1",
        status: "completed",
        output: { type: "text", text: "File created successfully" },
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    const fileChange = session.turns[0].parts.find((p) => p.kind === "file_change");
    assert.ok(fileChange, "file_change part should exist");
    assert.equal(fileChange.content_card?.kind, "codebuddy.file-change");
    assert.equal(fileChange.content_card?.renderer, "diff");
    assert.match(fileChange.text, /^diff --git a\/project\/config\.json b\/project\/config\.json/m);
    assert.match(fileChange.text, /\+\s*"mode": "dev"/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy reconstructs concrete unified diff from Edit tool call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-edit-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-edit.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Edit config" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-e",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Edit",
        callId: "call_edit_1",
        arguments: {
          path: "/project/config.json",
          old_string: '"mode": "dev"',
          new_string: '"mode": "prod"',
        },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-e",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Edit",
        callId: "call_edit_1",
        status: "completed",
        output: { type: "text", text: "File updated successfully" },
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    const fileChange = session.turns[0].parts.find((p) => p.kind === "file_change");
    assert.ok(fileChange, "file_change part should exist");
    assert.equal(fileChange.content_card?.kind, "codebuddy.file-change");
    assert.equal(fileChange.content_card?.renderer, "diff");
    assert.match(fileChange.text, /^diff --git a\/project\/config\.json b\/project\/config\.json/m);
    assert.match(fileChange.text, /-"mode": "dev"/);
    assert.match(fileChange.text, /\+"mode": "prod"/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy marks Read, Grep, Glob with ambient signal", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-ambient-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-ambient.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Inspect codebase" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-read",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Read",
        callId: "call_read_1",
        arguments: { path: "/project/README.md" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-read",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Read",
        callId: "call_read_1",
        status: "completed",
        output: { type: "text", text: "# Project Readme\nline 2" },
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    const callPart = session.turns[0].parts[1];
    const resPart = session.turns[0].parts[2];

    const callMeta = JSON.parse(callPart.metadata_json);
    const resMeta = JSON.parse(resPart.metadata_json);

    assert.equal(callMeta.signal, "ambient");
    assert.equal(callMeta.execution_kind, "read");
    assert.equal(callMeta.file_path, "/project/README.md");

    assert.equal(resMeta.signal, "ambient");
    assert.equal(resMeta.execution_kind, "read");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy links Agent tool call to subagent child session and discovers subagent", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-subagent-"));
  try {
    const parentSessionId = "session-main";
    const subagentId = "agent-explore-123";

    // Create session directories
    const mainSessionFile = path.join(fixtureRoot, `${parentSessionId}.jsonl`);
    const subagentsDir = path.join(fixtureRoot, parentSessionId, "subagents");
    mkdirSync(subagentsDir, { recursive: true });
    const subagentFile = path.join(subagentsDir, `${subagentId}.jsonl`);

    // Write subagent session
    writeFileSync(subagentFile, [
      JSON.stringify({
        id: "sub-msg-1",
        timestamp: 1791000001500,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "摸底架构设计与实现" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "sub-msg-2",
        timestamp: 1791000002000,
        type: "message",
        role: "assistant",
        content: [{ type: "text", text: "已完成架构摸底。" }],
        cwd: "/project",
      }),
    ].join("\n"));

    // Write main session
    writeFileSync(mainSessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "派发子代理探索" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-agent",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Agent",
        callId: "call_agent_1",
        arguments: {
          description: "摸底架构设计与实现",
          prompt: "摸底架构设计与实现",
          subagent_type: "Explore",
        },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-agent",
        timestamp: 1791000003000,
        type: "function_call_result",
        name: "Agent",
        callId: "call_agent_1",
        status: "completed",
        output: { type: "text", text: "已完成架构摸底报告" },
        cwd: "/project",
      }),
    ].join("\n"));

    // 1. 父会话：一次人类提问 = 1 个完整 Turn，派发处只有锚点 Part
    const sessions = readFixtureSessions(mainSessionFile);
    assert.equal(sessions.length, 2, "一个父会话 + 一个独立子代理会话");
    const parent = sessions.find((entry) => entry.external_id === parentSessionId);
    assert.ok(parent);
    assert.equal(parent.user_visible, true);
    assert.equal(parent.turns.length, 1, "派发子代理不得切分父轮次");
    assert.ok(parent.turns.every((turn) => turn.turn_kind === undefined));

    const anchors = anchorPartsOf(parent);
    assert.equal(anchors.length, 1);
    assert.equal(anchors[0].source_execution_id, "call_agent_1");
    assert.ok(!("content_card" in anchors[0]), "锚点不得携带 content_card");
    assert.match(anchors[0].text, /^\[委派任务\] /);
    assert.match(anchors[0].text, /\[汇报结论\] 已完成架构摸底/);
    const meta = JSON.parse(anchors[0].metadata_json);
    assert.equal(meta.child_external_id, `${parentSessionId}:subagent:${subagentId}`);
    assert.equal(meta.agent_role, "Explore");
    assert.equal(meta.title, "摸底架构设计与实现");
    assert.equal(meta.status, "completed");
    assert.equal(meta.spawn_anchor, "call_agent_1");
    assert.equal(meta.report_summary, "已完成架构摸底。");

    // 2. 子会话：独立、隐藏、带父子溯源元数据
    const child = sessions.find((entry) => entry.external_id === `${parentSessionId}:subagent:${subagentId}`);
    assert.ok(child);
    assert.equal(child.user_visible, false);
    assert.equal(child.execution_origin, "subagent");
    assert.match(child.title, /子代理/);
    assert.equal(child.turns.length, 1, "子会话 #T1 承载全量委派 Prompt 与最终汇报");
    assert.equal(child.turns[0].user_text, "摸底架构设计与实现");
    assert.ok(child.turns[0].parts.some((part) => part.role === "assistant" && part.text?.includes("已完成架构摸底")));
    const purpose = JSON.parse(child.execution_purpose);
    assert.equal(purpose.parent_external_id, parentSessionId);
    assert.equal(purpose.parent_turn_index, 0);
    assert.equal(purpose.agent_role, "Explore");
    assert.equal(purpose.spawn_anchor, "call_agent_1");

    // 3. 单独读取子代理文件同样产出隐藏子会话
    const standalone = readFixtureSession(subagentFile);
    assert.equal(standalone.external_id, `${parentSessionId}:subagent:${subagentId}`);
    assert.equal(standalone.execution_origin, "subagent");
    assert.equal(standalone.user_visible, false);

    // 4. list_sessions 必须列出子代理描述符，否则增量同步会把它误判为源已消失
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "list_sessions", source: { location: fixtureRoot }, params: {} }),
    });
    assert.equal(listResult.status, 0);
    const ids = listResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .filter((m) => m.type === "item")
      .map((m) => m.item.external_id)
      .sort();
    assert.deepEqual(ids, [parentSessionId, `${parentSessionId}:subagent:${subagentId}`].sort());
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy keeps the main agent continuation inside the same parent turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-cont-"));
  try {
    const parentSessionId = "session-cont-main";
    const subagentId = "agent-arch-explore";

    const mainSessionFile = path.join(fixtureRoot, `${parentSessionId}.jsonl`);
    const subagentsDir = path.join(fixtureRoot, parentSessionId, "subagents");
    mkdirSync(subagentsDir, { recursive: true });
    const subagentFile = path.join(subagentsDir, `${subagentId}.jsonl`);

    writeFileSync(subagentFile, [
      JSON.stringify({
        id: "sub-1",
        timestamp: 1791000001200,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "调研微服务划分" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "sub-2",
        timestamp: 1791000001800,
        type: "message",
        role: "assistant",
        content: [{ type: "text", text: "微服务划分方案：按DDD领域拆分。" }],
        cwd: "/project",
      }),
    ].join("\n"));

    writeFileSync(mainSessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "请协助设计微服务架构" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-agent",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Agent",
        callId: "call_agent_sub_1",
        arguments: {
          description: "调研微服务划分",
          prompt: "调研微服务划分",
          subagent_type: "ArchitectureExpert",
        },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-agent",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Agent",
        callId: "call_agent_sub_1",
        status: "completed",
        output: { type: "text", text: "子代理微服务调研完成" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "msg-2",
        timestamp: 1791000003000,
        type: "message",
        role: "assistant",
        content: [{ type: "text", text: "综合子代理调研结论，建议采用4个核心领域服务。" }],
        cwd: "/project",
      }),
    ].join("\n"));

    const sessions = readFixtureSessions(mainSessionFile);
    assert.equal(sessions.length, 2, "父会话 + 一个独立子会话");
    const parent = sessions.find((entry) => entry.external_id === parentSessionId);
    const child = sessions.find((entry) => entry.external_id === `${parentSessionId}:subagent:${subagentId}`);
    assert.ok(parent);
    assert.ok(child);

    assert.equal(parent.turns.length, 1, "子代理返回后主代理的动作必须留在同一个父轮次");
    const turn = parent.turns[0];
    assert.equal(turn.external_id, "msg-1");
    assert.equal(turn.user_text, "请协助设计微服务架构");
    assert.equal(turn.turn_kind, undefined);
    assert.equal(turn.parent_turn_id, undefined);
    assert.ok(
      turn.parts.some((part) => part.text?.includes("综合子代理调研结论")),
      "主代理的最终回答不能再被切到续接伪轮次里",
    );

    const anchors = anchorPartsOf(parent);
    assert.equal(anchors.length, 1);
    const meta = JSON.parse(anchors[0].metadata_json);
    assert.equal(meta.child_external_id, `${parentSessionId}:subagent:${subagentId}`);
    assert.equal(meta.agent_role, "ArchitectureExpert");
    assert.match(anchors[0].text, /\[汇报结论\] 微服务划分方案/);

    assert.equal(child.turns.length, 1);
    assert.equal(child.turns[0].user_text, "调研微服务划分");
    assert.ok(child.turns[0].parts.some((part) => part.text?.includes("微服务划分方案")));
    assert.equal(JSON.parse(child.execution_purpose).parent_turn_index, 0);
    assert.ok(sessions.every((entry) => entry.turns.every((item) => item.turn_kind === undefined)));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy parses assistant message with output_text as Markdown answer card", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-output-text-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-output-text.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "如何设计架构？" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-1",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Read",
        callId: "call_read_1",
        arguments: { path: "/project/ARCHITECTURE.md" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-1",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Read",
        callId: "call_read_1",
        status: "completed",
        output: { type: "text", text: "# Architecture" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "asst-1",
        timestamp: 1791000003000,
        type: "message",
        role: "assistant",
        status: "completed",
        content: [
          {
            type: "output_text",
            text: "## 架构设计建议\n\n1. 采用分层解耦架构；\n2. 保持领域层纯粹。\n\n**总结**：零入侵收口。",
          },
        ],
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    assert.equal(session.turns.length, 1);
    const turn = session.turns[0];

    // Verify last part is assistant Markdown answer, NOT trapped in tool result
    const lastPart = turn.parts.at(-1);
    assert.equal(lastPart.role, "assistant");
    assert.equal(lastPart.kind, "text");
    assert.equal(lastPart.content_card?.kind, "codebuddy.answer");
    assert.equal(lastPart.content_card?.renderer, "markdown");
    assert.match(lastPart.text, /## 架构设计建议/);
    assert.match(lastPart.text, /\*\*总结\*\*：零入侵收口。/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy parses standalone reasoning entry and extracts thinking markdown", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-reasoning-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-reasoning.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "分析一下性能瓶颈" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "reason-1",
        timestamp: 1791000001000,
        type: "reasoning",
        content: [],
        rawContent: [{ type: "reasoning_text", text: "首先观察日志中的耗时瓶颈点，确定是否由大IO引起。" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "asst-1",
        timestamp: 1791000002000,
        type: "message",
        role: "assistant",
        content: [{ type: "output_text", text: "已完成性能分析。" }],
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    const turn = session.turns[0];

    const reasoningPart = turn.parts.find((p) => p.content_card?.kind === "codebuddy.reasoning");
    assert.ok(reasoningPart, "reasoning part must exist");
    assert.equal(reasoningPart.role, "assistant");
    assert.equal(reasoningPart.content_card?.renderer, "markdown");
    assert.match(reasoningPart.text, /首先观察日志中的耗时瓶颈点/);

    const answerPart = turn.parts.find((p) => p.content_card?.kind === "codebuddy.answer");
    assert.ok(answerPart, "answer part must exist");
    assert.equal(answerPart.text, "已完成性能分析。");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy cleans present_files array output and tags ambient signal", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-present-files-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-present-files.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "生成报告" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-1",
        timestamp: 1791000001000,
        type: "function_call",
        name: "present_files",
        callId: "call_pf_1",
        arguments: { files: ["/project/report.md"] },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-1",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "present_files",
        callId: "call_pf_1",
        status: "completed",
        output: [
          {
            type: "input_text",
            text: JSON.stringify({
              type: "present_files_result",
              files: ["/project/report.md"],
              explanation: "完整报告已生成",
            }),
          },
        ],
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    const turn = session.turns[0];
    const callPart = turn.parts[1];
    const resPart = turn.parts[2];

    const callMeta = JSON.parse(callPart.metadata_json);
    assert.equal(callMeta.signal, "ambient");
    assert.equal(callMeta.execution_kind, "read");

    const resMeta = JSON.parse(resPart.metadata_json);
    assert.equal(resMeta.signal, "ambient");
    assert.equal(resMeta.execution_kind, "read");
    assert.equal(resPart.content_card?.renderer, "plain");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy multi-turn session keeps subsequent question as standard user turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-multiturn-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-multi.jsonl");
    writeFileSync(sessionFile, [
      // Question 1
      JSON.stringify({
        id: "msg-q1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Question 1: Fix bug" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-agent-q1",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Agent",
        callId: "call_agent_q1",
        arguments: { prompt: "Subtask prompt", subagent_type: "Explore" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-agent-q1",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Agent",
        callId: "call_agent_q1",
        status: "completed",
        output: { type: "text", text: "Subtask complete" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "msg-ans-q1",
        timestamp: 1791000003000,
        type: "message",
        role: "assistant",
        content: [{ type: "text", text: "Question 1 is fixed." }],
        cwd: "/project",
      }),
      // Question 2
      JSON.stringify({
        id: "msg-q2",
        timestamp: 1791000010000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Question 2: What is the weather today in Tokyo?" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "msg-ans-q2",
        timestamp: 1791000011000,
        type: "message",
        role: "assistant",
        content: [{ type: "text", text: "Tokyo is sunny." }],
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    const q2Turn = session.turns.find((t) => t.user_text && t.user_text.includes("Question 2: What is the weather today in Tokyo?"));
    assert.ok(q2Turn, "Question 2 turn must exist");
    assert.equal(q2Turn.turn_kind, undefined, "Question 2 must NOT be marked as continuation");
    assert.equal(q2Turn.parent_turn_id, undefined, "Question 2 must NOT have parent_turn_id");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy keeps a non-empty anchor title when the subagent prompt is whitespace-only", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-ws-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-ws.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "Run delegator" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-agent-ws",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Agent",
        callId: "call_agent_ws",
        arguments: { prompt: "   \t\n  ", subagent_type: "Explore" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-agent-ws",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Agent",
        callId: "call_agent_ws",
        status: "completed",
        output: { type: "text", text: "Subtask complete" },
        cwd: "/project",
      }),
    ].join("\n"));

    const session = readFixtureSession(sessionFile);
    assert.equal(session.turns.length, 1, "空提示词不得让父轮次被切分");
    const anchors = anchorPartsOf(session);
    assert.equal(anchors.length, 1);
    assert.ok(anchors[0].text.trim().length > 0);
    assert.match(anchors[0].text, /^\[委派任务\] /);
    assert.match(anchors[0].text, /\[汇报结论\] Subtask complete/);
    const meta = JSON.parse(anchors[0].metadata_json);
    assert.equal(meta.child_external_id, "session-ws:subagent:call_agent_ws");
    assert.ok(!meta.title, "全空白的 description/prompt 不得成为锚点标题");
    assert.ok(session.turns.every((turn) => turn.turn_kind === undefined));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy folds every child round into the child session #T1", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-merge-"));
  try {
    const parentSessionId = "session-merge-main";
    const subagentId = "agent-merge-child";
    const mainSessionFile = path.join(fixtureRoot, `${parentSessionId}.jsonl`);
    const subagentsDir = path.join(fixtureRoot, parentSessionId, "subagents");
    mkdirSync(subagentsDir, { recursive: true });
    const subagentFile = path.join(subagentsDir, `${subagentId}.jsonl`);

    writeFileSync(subagentFile, [
      JSON.stringify({
        id: "child-1",
        timestamp: 1791000001100,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "先核对索引" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "child-2",
        timestamp: 1791000001200,
        type: "message",
        role: "assistant",
        content: [{ type: "text", text: "索引核对完成。" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "child-3",
        timestamp: 1791000001300,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "再核对外键" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "child-4",
        timestamp: 1791000001400,
        type: "message",
        role: "assistant",
        content: [{ type: "text", text: "外键核对完成：无违规。" }],
        cwd: "/project",
      }),
    ].join("\n"));

    writeFileSync(mainSessionFile, [
      JSON.stringify({
        id: "main-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "审查数据库迁移" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "main-call-agent",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Agent",
        callId: "call_merge_agent",
        arguments: {
          description: "审查数据库迁移",
          prompt: "先核对索引，再核对外键",
          subagent_type: "DatabaseReviewer",
        },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "main-res-agent",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Agent",
        callId: "call_merge_agent",
        status: "completed",
        output: { type: "text", text: "子代理审查完成" },
        cwd: "/project",
      }),
    ].join("\n"));

    const sessions = readFixtureSessions(mainSessionFile);
    assert.equal(sessions.length, 2);
    const parent = sessions.find((entry) => entry.external_id === parentSessionId);
    const child = sessions.find((entry) => entry.external_id === `${parentSessionId}:subagent:${subagentId}`);
    assert.ok(parent);
    assert.ok(child);

    assert.equal(parent.turns.length, 1, "子代理的多轮内容不得裂成父会话的伪轮次");
    assert.equal(anchorPartsOf(parent).length, 1);
    assert.equal(JSON.parse(anchorPartsOf(parent)[0].metadata_json).agent_role, "DatabaseReviewer");

    assert.equal(child.turns.length, 1, "子代理逐轮内容折进独立子会话的 #T1");
    assert.equal(child.turns[0].user_text, "先核对索引");
    assert.equal(child.turns[0].turn_kind, undefined);
    assert.deepEqual(
      child.turns[0].parts.filter((part) => part.role === "assistant").map((part) => part.text),
      ["索引核对完成。", "外键核对完成：无违规。"],
      "子代理逐轮内容按原顺序串成 #T1 的 parts"
    );
    assert.equal(JSON.parse(child.execution_purpose).parent_turn_index, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy leaves anchor agent_role empty when the dispatch declares no subagent_type", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codebuddy-roleless-"));
  try {
    const sessionFile = path.join(fixtureRoot, "session-roleless.jsonl");
    writeFileSync(sessionFile, [
      JSON.stringify({
        id: "msg-1",
        timestamp: 1791000000000,
        type: "message",
        role: "user",
        content: [{ type: "input_text", text: "派发一个未声明类型的子代理" }],
        cwd: "/project",
      }),
      JSON.stringify({
        id: "call-agent-roleless",
        timestamp: 1791000001000,
        type: "function_call",
        name: "Agent",
        callId: "call_agent_roleless",
        arguments: { prompt: "未声明类型的任务" },
        cwd: "/project",
      }),
      JSON.stringify({
        id: "res-agent-roleless",
        timestamp: 1791000002000,
        type: "function_call_result",
        name: "Agent",
        callId: "call_agent_roleless",
        status: "completed",
        output: { type: "text", text: "任务完成" },
        cwd: "/project",
      }),
    ].join("\n"));

    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: sessionFile }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
    const session = messages.find((message) => message.type === "item")?.item?.session;
    assert.ok(session);

    const anchors = anchorPartsOf(session);
    assert.equal(anchors.length, 1, "派发仍必须留下锚点");
    const meta = JSON.parse(anchors[0].metadata_json);
    assert.ok(!meta.agent_role, "缺少 subagent_type 时角色必须留空，不得伪造 Explore");
    assert.equal(session.turns.length, 1);
    assert.ok(
      messages.some(
        (message) =>
          message.type === "warning" && /has no subagent_type/.test(message.message ?? "")
      ),
      "角色缺失必须发出告警"
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});
