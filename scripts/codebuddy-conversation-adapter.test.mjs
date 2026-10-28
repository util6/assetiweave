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

    // 1. Read main session
    const mainSession = readFixtureSession(mainSessionFile);
    assert.equal(mainSession.turns.length, 2, "main session should contain anchor turn and inlined subagent turn");
    const agentPart = mainSession.turns[0].parts.find((p) => p.kind === "subagent");
    assert.ok(agentPart, "subagent part should exist in main session");
    assert.equal(agentPart.content_card?.kind, "codebuddy.subagent");
    assert.equal(agentPart.content_card?.renderer, "subagent_tree");

    const meta = JSON.parse(agentPart.metadata_json);
    assert.equal(meta.child_session_id, subagentId);
    assert.equal(meta.agent_role, "Explore");

    // Verify inlined subagent turn
    const subTurn = mainSession.turns[1];
    assert.equal(subTurn.turn_kind, "subagent");
    assert.equal(subTurn.agent_role, "Explore");
    assert.equal(subTurn.parent_turn_id, mainSession.turns[0].external_id);
    assert.equal(subTurn.user_text, "摸底架构设计与实现");
    assert.ok(subTurn.parts.some((p) => p.role === "assistant" && p.text?.includes("已完成架构摸底")));

    // 2. Read subagent session directly
    const subSession = readFixtureSession(subagentFile);
    assert.equal(subSession.execution_origin, "subagent");
    assert.equal(subSession.user_visible, false);
    assert.match(subSession.title, /子代理/);

    // 3. List sessions in fixtureRoot
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "list_sessions", source: { location: fixtureRoot }, params: {} }),
    });
    assert.equal(listResult.status, 0);
    const items = listResult.stdout.trim().split("\n").map((l) => JSON.parse(l)).filter((m) => m.type === "item");
    assert.equal(items.length, 1); // Subagent session is filtered out from top-level list_sessions
    assert.equal(items[0].item.external_id, "session-main");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("CodeBuddy inlines Agent subagent child turns and emits continuation turn when main agent resumes", () => {
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

    const session = readFixtureSession(mainSessionFile);
    assert.equal(session.turns.length, 3, "expected anchor, subagent, and continuation turns");

    const [anchorTurn, subTurn, contTurn] = session.turns;

    // Anchor turn
    assert.equal(anchorTurn.external_id, "msg-1");
    assert.equal(anchorTurn.user_text, "请协助设计微服务架构");
    assert.equal(anchorTurn.turn_kind, undefined);

    // Subagent turn
    assert.equal(subTurn.turn_kind, "subagent");
    assert.equal(subTurn.agent_role, "ArchitectureExpert");
    assert.equal(subTurn.parent_turn_id, anchorTurn.external_id);
    assert.equal(subTurn.user_text, "调研微服务划分");
    assert.ok(subTurn.parts.some((p) => p.text?.includes("微服务划分方案")));

    // Continuation turn
    assert.equal(contTurn.turn_kind, "continuation");
    assert.equal(contTurn.parent_turn_id, anchorTurn.external_id);
    assert.equal(contTurn.user_text, "Continue following subagent execution");
    assert.ok(contTurn.parts.some((p) => p.text?.includes("综合子代理调研结论")));
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

test("CodeBuddy falls back to valid string when subagent prompt is whitespace-only", () => {
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
    const subTurn = session.turns.find((t) => t.turn_kind === "subagent");
    assert.ok(subTurn, "Subagent turn must exist");
    assert.equal(subTurn.user_text, "Subagent execution");
    assert.ok(subTurn.user_text.trim().length > 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});
