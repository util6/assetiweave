import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "parser-catalog/adapters/codex/adapter.mjs");

test("Codex adapter keeps Skill injection out of the user question and emits one Skill path card", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-skill-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const skillPath = "/Users/test/.codex/skills/session-exporter/SKILL.md";
    writeFileSync(rolloutPath, [
      event("2026-07-27T00:00:00Z", "user", `[$session-exporter](${skillPath}) 归档 SESSION`),
      event("2026-07-27T00:00:01Z", "user", [
        "<skill>",
        "<name>session-exporter</name>",
        `<path>${skillPath}</path>`,
        "---",
        "name: session-exporter",
        "description: Export a session.",
        "</skill>",
      ].join("\n")),
      event("2026-07-27T00:00:02Z", "assistant", "归档完成。"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:02Z');`,
    ].join("\n"));

    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: fixtureRoot }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
    const session = messages.find((message) => message.type === "item")?.item?.session;

    assert.ok(session);
    assert.equal(session.turns.length, 1);
    assert.equal(session.turns[0].user_text, "归档 SESSION");
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        role: part.role,
        text: part.text,
      })),
      [
        { kind: "codex.skill", renderer: "path", role: "system", text: skillPath },
        { kind: "codex.answer", renderer: "markdown", role: "assistant", text: "归档完成。" },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter separates a SKILL.md read from the command result", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-skill-read-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const skillPath = "/Users/test/.codex/skills/session-exporter/SKILL.md";
    const skillContent = [
      "---",
      "name: session-exporter",
      "description: Export a session.",
      "---",
      "",
      "# Session Exporter",
      "",
      "Export a local Codex session.",
    ].join("\n");
    writeFileSync(rolloutPath, [
      event("2026-07-27T00:00:00Z", "user", "导出当前会话"),
      JSON.stringify({
        timestamp: "2026-07-27T00:00:01Z",
        type: "response_item",
        payload: { type: "exec", command: `cat ${skillPath}` },
      }),
      JSON.stringify({
        timestamp: "2026-07-27T00:00:02Z",
        type: "response_item",
        payload: { type: "tool_result", name: "exec", output: skillContent },
      }),
      event("2026-07-27T00:00:03Z", "assistant", "已读取 Skill。"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:03Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.equal(session.turns[0].user_text, "导出当前会话");
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        command: part.command,
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        role: part.role,
        text: part.text,
      })),
      [
        { command: `cat ${skillPath}`, kind: "codex.command", renderer: "command", role: "tool", text: null },
        { command: null, kind: "codex.skill", renderer: "path", role: "system", text: skillPath },
        { command: null, kind: "codex.answer", renderer: "markdown", role: "assistant", text: "已读取 Skill。" },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter recognizes a SKILL.md read from a modern custom exec call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-custom-skill-read-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const skillPath = "/Users/test/.codex/skills/using-agent-skills/SKILL.md";
    const workdir = "/Users/test/code-space/assetiweave";
    const skillContent = [
      "---",
      "name: using-agent-skills",
      "description: Discover and invoke agent skills.",
      "---",
      "",
      "# Using Agent Skills",
    ].join("\n");
    writeFileSync(rolloutPath, [
      event("2026-07-31T00:00:00Z", "user", "读取 Skill"),
      JSON.stringify({
        timestamp: "2026-07-31T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-skill-read",
          input: [
            `const r = await tools.exec_command(${JSON.stringify({ cmd: `cat ${skillPath}`, workdir })});`,
            "text(r.output);",
          ].join("\n"),
        },
      }),
      JSON.stringify({
        timestamp: "2026-07-31T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-skill-read",
          output: [
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: skillContent },
          ],
        },
      }),
      event("2026-07-31T00:00:03Z", "assistant", "已读取 Skill。"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-31T00:00:03Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        command: part.command,
        cwd: part.cwd,
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        role: part.role,
        text: part.text,
      })),
      [
        {
          command: `cat ${skillPath}`,
          cwd: workdir,
          kind: "codex.command",
          renderer: "command",
          role: "tool",
          text: null,
        },
        {
          command: null,
          cwd: null,
          kind: "codex.skill",
          renderer: "path",
          role: "system",
          text: skillPath,
        },
        {
          command: null,
          cwd: null,
          kind: "codex.answer",
          renderer: "markdown",
          role: "assistant",
          text: "已读取 Skill。",
        },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter preserves source execution IDs across interleaved command results", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-executions-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "并行运行检查"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-typecheck",
          input: "const r = await tools.exec_command({\"cmd\":\"pnpm typecheck\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-test",
          input: "const r = await tools.exec_command({\"cmd\":\"pnpm test\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:03Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-test",
          output: [{ type: "input_text", text: "tests passed" }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:04Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-typecheck",
          output: [{ type: "input_text", text: "typecheck passed" }],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:04Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        kind: part.content_card?.kind,
        sourceExecutionId: part.source_execution_id,
      })),
      [
        { kind: "codex.command", sourceExecutionId: "call-typecheck" },
        { kind: "codex.command", sourceExecutionId: "call-test" },
        { kind: "codex.result", sourceExecutionId: "call-test" },
        { kind: "codex.result", sourceExecutionId: "call-typecheck" },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter decodes structured execution output and removes runner noise", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-result-text-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "运行检查"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-check",
          input: "const r = await tools.exec_command({\"cmd\":\"printf '\\\\n'\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-check",
          output: JSON.stringify([
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: "\u001b[32mcheck passed\u001b[0m" },
          ]),
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const [command, result] = session.turns[0].parts;
    assert.equal(command.command, "printf '\\n'");
    assert.equal(result.text, "check passed");
    assert.equal(result.content_card?.renderer, "terminal_output");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter decodes structured execution output before applying text budgets", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-large-result-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const terminalOutput = `visible output line\n${"detail line\n".repeat(3_000)}`;
    writeFileSync(rolloutPath, [
      event("2026-08-03T00:00:00Z", "user", "运行大型检查"),
      JSON.stringify({
        timestamp: "2026-08-03T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-large-check",
          output: JSON.stringify([
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: terminalOutput },
          ]),
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-03T00:00:01Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const [result] = session.turns[0].parts;
    assert.ok(result.text.startsWith("visible output line\n"));
    assert.equal(result.text.includes("input_text"), false);
    assert.equal(result.text.startsWith("[{"), false);
    assert.equal(result.content_card?.renderer, "terminal_output");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter hides pending runner output and wait controls without shifting Part positions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-control-noise-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-03T00:00:00Z", "user", "等待脚本完成"),
      JSON.stringify({
        timestamp: "2026-08-03T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-running-check",
          output: [{
            type: "input_text",
            text: "Script running with cell ID 7\nWall time 11.0 seconds\nOutput:\n",
          }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-03T00:00:02Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "wait",
          arguments: JSON.stringify({ cell_id: "7", yield_time_ms: 30_000 }),
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-03T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.equal(session.turns[0].parts.length, 2);
    assert.equal(session.turns[0].parts[0].text, null);
    assert.equal(session.turns[0].parts[0].content_card?.kind, "codex.result");
    assert.equal(session.turns[0].parts[1].text, null);
    assert.equal(session.turns[0].parts[1].content_card, undefined);
    assert.equal(session.turns[0].parts[1].metadata_json, null);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function readFixtureSession(fixtureRoot) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: fixtureRoot }, params: {} }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  const session = messages.find((message) => message.type === "item")?.item?.session;
  assert.ok(session);
  return session;
}

function event(timestamp, role, text) {
  return JSON.stringify({
    timestamp,
    type: "response_item",
    payload: {
      type: "message",
      role,
      content: [{ type: role === "assistant" ? "output_text" : "input_text", text }],
    },
  });
}

function runSqlite(directory, sql) {
  const result = spawnSync("sqlite3", [path.join(directory, "state_5.sqlite")], {
    encoding: "utf8",
    input: sql,
  });
  assert.equal(result.status, 0, result.stderr);
}

function sqlString(value) {
  return value.replaceAll("'", "''");
}
