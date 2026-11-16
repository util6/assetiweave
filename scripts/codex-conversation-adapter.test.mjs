import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import {
  normalizeSessionPayload,
  PAYLOAD_POLICY_VERSION,
} from "../builtin-assets/adapters/codex/payload-policy.mjs";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/codex/adapter.mjs");

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

test("Codex adapter keeps fenced code inside the assistant Markdown answer", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-markdown-code-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const answer = [
      "实现方式如下：",
      "",
      "```ts",
      "const result = await runTask();",
      "console.log(result);",
      "```",
      "",
      "代码块属于回答内容的一部分。",
    ].join("\n");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "解释实现方式"),
      event("2026-08-02T00:00:01Z", "assistant", answer),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:01Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        text: part.text,
      })),
      [{ kind: "codex.answer", renderer: "markdown", text: answer }],
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
        { command: skillPath, kind: "codex.command", renderer: "command", role: "tool", text: null },
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
          command: skillPath,
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
          output: [{ type: "input_text", text: "Error: tests failed" }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:04Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-typecheck",
          output: [{ type: "input_text", text: "Error: typecheck failed" }],
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

test("Codex adapter keeps one raw Shell Execution without persisting separator projections", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-command-separators-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const command = [
      "printf '%s\\n' '--- status ---'",
      "git status --short",
      "printf '%s\\n' '--- staged diff stat ---'",
      "git diff --cached --stat",
    ].join("; ");
    writeFileSync(rolloutPath, [
      event("2026-08-10T00:00:00Z", "user", "检查工作区"),
      JSON.stringify({
        timestamp: "2026-08-10T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-status",
          status: "completed",
          input: `const r = await tools.exec_command(${JSON.stringify({ cmd: command, workdir: "/tmp/project" })}); text(r.output);`,
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-10T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-status",
          output: "Error: failed",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-10T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].command, command);
    assert.equal(parts[0].source_execution_id, "call-status");
    assert.equal(parts[1].source_execution_id, "call-status");
    assert.equal(parts[1].text, "Error: failed");
    assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter keeps a pure printf separator as raw evidence without a persisted projection", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-pure-separator-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const command = "printf '%s\\n' '--- diagnostics ---'";
    writeFileSync(rolloutPath, [
      event("2026-08-10T00:00:00Z", "user", "显示诊断分隔符"),
      JSON.stringify({
        timestamp: "2026-08-10T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-pure-separator",
          input: `const r = await tools.exec_command(${JSON.stringify({ cmd: command })}); text(r.output);`,
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-10T00:00:01Z');`,
    ].join("\n"));

    const [part] = readFixtureSession(fixtureRoot).turns[0].parts;
    assert.equal(part.command, command);
    assert.equal(part.source_execution_id, "call-pure-separator");
    assert.equal(JSON.parse(part.metadata_json).shell_execution_projection, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter preserves complex raw shell syntax without a persisted projection", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-raw-shell-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const command = [
      "printf '%s\\n' '--- inspect ---'",
      "printf '%s' \"quoted && value\" | sed 's/quoted/quoted;still/'",
      "cat <<'EOF'",
      "a line with && ; | > and \\\"quotes\\\"",
      "EOF",
      "printf '%s\\n' '--- finish ---'",
      "git status --short > /tmp/status.txt",
    ].join("\n");
    const resultText = "Error: failed";
    writeFileSync(rolloutPath, [
      event("2026-08-10T00:00:00Z", "user", "检查复杂命令"),
      JSON.stringify({
        timestamp: "2026-08-10T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-raw-shell",
          input: `const r = await tools.exec_command(${JSON.stringify({ cmd: command, workdir: "/tmp/project" })}); text(r.output);`,
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-10T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-raw-shell",
          output: resultText,
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-10T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].command, command);
    assert.equal(parts[0].source_execution_id, "call-raw-shell");
    assert.equal(parts[1].source_execution_id, "call-raw-shell");
    assert.equal(parts[1].text, resultText);
    assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex historical 48d4ef52 fixture retains one raw execution for four legacy cards", () => {
  const fixturePath = path.join(
    repositoryRoot,
    "scripts/fixtures/codex/48d4ef52-shell-execution.json",
  );
  const fixture = JSON.parse(readFileSync(fixturePath, "utf8"));
  assert.equal(fixture.session_id, "48d4ef52");
  assert.deepEqual(fixture.historical_card_ids, [
    "e356e87a",
    "4ebc2319",
    "877ef979",
    "14d7ef7f",
  ]);

  const session = normalizeSessionPayload(structuredClone(fixture.session));
  const parts = session.turns[0].parts;
  assert.deepEqual(parts.map((part) => part.id), fixture.expected_persisted_part_ids);
  assert.equal(parts.filter((part) => part.kind === "command").length, 1);
  assert.equal(parts[0].command, fixture.session.turns[0].parts[0].command);
  assert.equal(parts[0].source_execution_id, "execution-48d4ef52");
  assert.equal(parts[1].source_execution_id, "execution-48d4ef52");
  assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
});

test("Codex adapter removes successful structured execution output", () => {
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
    const [command] = session.turns[0].parts;
    assert.equal(command.command, "printf '\\n'");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter omits successful command results whose payload is an empty object", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-empty-result-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "运行无输出检查"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-empty-result",
          input: "const r = await tools.exec_command({\"cmd\":\"true\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-empty-result",
          output: [
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: "{}" },
          ],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.map((part) => part.content_card?.kind),
      ["codex.command"],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter omits unpaired script results whose payload is an empty object", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-empty-script-result-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-02T00:00:00Z", "user", "更新执行计划"),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-empty-script-result",
          input: "const r = await tools.update_plan({plan: []}); text(r);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-02T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-empty-script-result",
          output: [
            { type: "input_text", text: "Script completed\nWall time 0.1 seconds\nOutput:\n" },
            { type: "input_text", text: "{}" },
          ],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-02T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    assert.deepEqual(
      session.turns[0].parts.filter((part) => part.content_card?.kind === "codex.result"),
      [],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter preserves and budgets unpaired structured output as unclassified", () => {
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
    assert.equal(JSON.parse(result.metadata_json).execution_kind, "unclassified");
    assert.equal(result.content_card?.renderer, "terminal_output");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter hides pending runner output and drops wait controls without empty DB rows", () => {
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
    assert.equal(session.turns[0].parts.length, 1);
    assert.equal(session.turns[0].parts[0].text, null);
    assert.equal(session.turns[0].parts[0].content_card?.kind, "codex.result");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter hides successful result cards while retaining command state", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-payload-policy-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-04T00:00:00Z", "user", "运行测试"),
      JSON.stringify({
        timestamp: "2026-08-04T00:00:01Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-tests",
          status: "completed",
          input: "const r = await tools.exec_command({\"cmd\":\"pnpm test\"}); text(r.output);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-04T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-tests",
          output: "Script completed\nWall time 0.2 seconds\nOutput:\n388 tests passed",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-04T00:00:02Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 1);
    assert.deepEqual(parts.map((part) => part.source_execution_id), ["call-tests"]);
    assert.equal(parts[0].status, "completed");
    assert.equal(parts[0].exit_code, 0);
    assert.equal(JSON.parse(parts[0].metadata_json).payload_policy_version, PAYLOAD_POLICY_VERSION);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter stores patch_apply_end changes as one file-change Part per file", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-patch-diff-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const projectPath = "/tmp/project";
    writeFileSync(rolloutPath, [
      JSON.stringify({
        timestamp: "2026-08-09T00:00:00Z",
        type: "session_meta",
        payload: { cwd: projectPath },
      }),
      event("2026-08-09T00:00:01Z", "user", "修改四个文件"),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "apply_patch",
          call_id: "call-patch",
          input: [
            "*** Begin Patch",
            "*** Update File: /tmp/project/src/main.ts",
            "@@ -1 +1 @@",
            "-const oldValue = 1;",
            "+const command = \"npm test\";",
            "*** End Patch",
          ].join("\n"),
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:03Z",
        type: "event_msg",
        payload: {
          type: "patch_apply_end",
          call_id: "call-patch",
          success: true,
          status: "completed",
          changes: {
            "/tmp/project/src/main.ts": {
              type: "update",
              unified_diff: "@@ -1 +1 @@\n-const oldValue = 1;\n+const command = \"npm test\";\n",
              move_path: null,
            },
            "/tmp/project/src/new.ts": {
              type: "add",
              content: "export const created = true;\n",
            },
            "/tmp/project/src/old.ts": {
              type: "delete",
              content: "export const removed = true;\n",
            },
            "/tmp/project/src/from.ts": {
              type: "update",
              unified_diff: "@@ -1 +1 @@\n-export const name = 'from';\n+export const name = 'to';\n",
              move_path: "/tmp/project/src/to.ts",
            },
          },
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:04Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-patch",
          output: "Exit code: 0\nOutput:\nSuccess. Updated four files.",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-09T00:00:04Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 5);
    assert.equal(parts[0].command, ["src/main.ts", "src/new.ts", "src/old.ts", "src/to.ts"].join("\n"));
    assert.equal(parts[0].command_label, "Edit");
    assert.equal(parts[0].source_execution_id, "call-patch");
    const fileParts = parts.slice(1);
    assert.deepEqual(fileParts.map((part) => part.source_execution_id), [
      "call-patch",
      "call-patch",
      "call-patch",
      "call-patch",
    ]);
    assert.deepEqual(fileParts.map((part) => JSON.parse(part.metadata_json).file_path), [
      "src/main.ts",
      "src/new.ts",
      "src/old.ts",
      "src/to.ts",
    ]);
    for (const [index, part] of fileParts.entries()) {
      assert.equal(part.kind, "file_change");
      assert.equal(part.content_card?.kind, "codex.file-change");
      assert.equal(part.content_card?.renderer, "diff");
      assert.equal(part.text.match(/^diff --git /gm)?.length, 1);
      const metadata = JSON.parse(part.metadata_json);
      assert.equal(metadata.file_change_index, index + 1);
      assert.equal(metadata.file_change_count, 4);
    }
    assert.match(fileParts[0].text, /diff --git a\/src\/main\.ts b\/src\/main\.ts/);
    assert.match(fileParts[1].text, /new file mode 100644[\s\S]*\+export const created = true;/);
    assert.match(fileParts[2].text, /deleted file mode 100644[\s\S]*-export const removed = true;/);
    assert.match(fileParts[3].text, /rename from src\/from\.ts[\s\S]*rename to src\/to\.ts/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter correlates a nested apply_patch event with its outer exec call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-nested-patch-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      JSON.stringify({
        timestamp: "2026-08-09T00:00:00Z",
        type: "session_meta",
        payload: { cwd: "/tmp/project" },
      }),
      event("2026-08-09T00:00:01Z", "user", "修改入口文件"),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-outer-exec",
          input: "const patch = `*** Begin Patch\\n*** Update File: src/main.ts\\n@@\\n-old\\n+new\\n*** End Patch`; text(await tools.apply_patch(patch));",
          internal_chat_message_metadata_passthrough: { turn_id: "turn-runtime" },
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:03Z",
        type: "event_msg",
        payload: {
          type: "patch_apply_end",
          call_id: "exec-inner-patch",
          turn_id: "turn-runtime",
          success: true,
          changes: {
            "/tmp/project/src/main.ts": {
              type: "update",
              unified_diff: "@@ -1 +1 @@\n-old\n+new\n",
            },
          },
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-09T00:00:04Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-outer-exec",
          output: [{ type: "input_text", text: "Script completed\nOutput:\n{}" }],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-09T00:00:04Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].command, "src/main.ts");
    assert.equal(parts[0].command_label, "Edit");
    assert.equal(parts[0].source_execution_id, "call-outer-exec");
    assert.equal(parts[1].source_execution_id, "call-outer-exec");
    assert.equal(parts[1].kind, "file_change");
    assert.equal(parts[1].content_card?.kind, "codex.file-change");
    assert.equal(parts[1].content_card?.renderer, "diff");
    assert.match(parts[1].text, /^diff --git a\/src\/main\.ts b\/src\/main\.ts/m);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter reconstructs a nested apply_patch when modern logs omit patch_apply_end", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-modern-nested-patch-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const patch = [
      "*** Begin Patch",
      "*** Update File: src/main.ts",
      "@@",
      "-export const value = 1;",
      "+export const value = 2;",
      "*** End Patch",
    ].join("\n");
    writeFileSync(rolloutPath, [
      JSON.stringify({
        timestamp: "2026-08-29T00:00:00Z",
        type: "session_meta",
        payload: { cwd: "/tmp/project" },
      }),
      event("2026-08-29T00:00:01Z", "user", "修改入口文件"),
      JSON.stringify({
        timestamp: "2026-08-29T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          status: "completed",
          name: "exec",
          call_id: "call-modern-outer-exec",
          input: `const patch = ${JSON.stringify(patch)};\ntext(await tools.apply_patch(patch));\n`,
          internal_chat_message_metadata_passthrough: { turn_id: "turn-modern-runtime" },
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-29T00:00:03Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-modern-outer-exec",
          output: [
            { type: "input_text", text: "Script completed\nWall time 0.0 seconds\nOutput:\n" },
            { type: "input_text", text: "{}" },
          ],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-29T00:00:03Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].command, "src/main.ts");
    assert.equal(parts[0].command_label, "Edit");
    assert.equal(parts[0].source_execution_id, "call-modern-outer-exec");
    assert.equal(parts[1].source_execution_id, "call-modern-outer-exec");
    assert.equal(parts[1].kind, "file_change");
    assert.equal(parts[1].content_card?.kind, "codex.file-change");
    assert.equal(parts[1].content_card?.renderer, "diff");
    assert.match(parts[1].text, /^diff --git a\/src\/main\.ts b\/src\/main\.ts/m);
    assert.match(parts[1].text, /-export const value = 1;\n\+export const value = 2;/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter does not emit a file change for a failed modern nested apply_patch", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-failed-modern-patch-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    const patch = [
      "*** Begin Patch",
      "*** Update File: src/main.ts",
      "@@",
      "-missing old line",
      "+replacement line",
      "*** End Patch",
    ].join("\n");
    writeFileSync(rolloutPath, [
      JSON.stringify({
        timestamp: "2026-08-29T00:00:00Z",
        type: "session_meta",
        payload: { cwd: "/tmp/project" },
      }),
      event("2026-08-29T00:00:01Z", "user", "修改入口文件"),
      JSON.stringify({
        timestamp: "2026-08-29T00:00:02Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          status: "completed",
          name: "exec",
          call_id: "call-failed-modern-patch",
          input: `const patch = ${JSON.stringify(patch)};\ntext(await tools.apply_patch(patch));\n`,
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-29T00:00:03Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-failed-modern-patch",
          output: [
            { type: "input_text", text: "Script failed\nWall time 0.0 seconds\nOutput:\n" },
            {
              type: "input_text",
              text: "Script error:\napply_patch verification failed: Failed to find expected lines in src/main.ts",
            },
          ],
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-08-29T00:00:03Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.some((part) => part.kind === "file_change"), false);
    assert.equal(parts.some((part) => part.text?.includes("apply_patch verification failed")), true);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter extracts update_plan tool calls into codex.plan Markdown cards and hides redundant ack results", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-plan-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-08-16T00:00:00Z", "user", "开始执行任务计划"),
      JSON.stringify({
        timestamp: "2026-08-16T00:00:01Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "update_plan",
          call_id: "call-plan-1",
          arguments: JSON.stringify({
            explanation: "重构定时任务与归档结构",
            plan: [
              { step: "检查现有配置", status: "completed" },
              { step: "编写核心解析逻辑", status: "in_progress" },
              { step: "运行完整测试套件", status: "pending" },
            ],
          }),
        },
      }),
      JSON.stringify({
        timestamp: "2026-08-16T00:00:02Z",
        type: "response_item",
        payload: {
          type: "function_call_output",
          call_id: "call-plan-1",
          output: "Plan updated",
        },
      }),
      event("2026-08-16T00:00:03Z", "assistant", "已更新计划并正在执行。"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Plan Fixture', '2026-08-16T00:00:03Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.equal(parts[0].content_card?.kind, "codex.plan");
    assert.equal(parts[0].content_card?.renderer, "markdown");
    assert.ok(parts[0].text.includes("📌 **阶段目标**：重构定时任务与归档结构"));
    assert.ok(parts[0].text.includes("- [x] ✅ 检查现有配置"));
    assert.ok(parts[0].text.includes("- [ ] 🔄 **编写核心解析逻辑** *(进行中)*"));
    assert.ok(parts[0].text.includes("- [ ] ⏳ 运行完整测试套件"));
    assert.equal(parts[1].content_card?.kind, "codex.answer");
    assert.equal(parts[1].content_card?.renderer, "markdown");
    assert.equal(parts[1].text, "已更新计划并正在执行。");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function readSessionMessages(fixtureRoot, sessionId) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({
      method: "read_session",
      source: { location: fixtureRoot },
      params: sessionId == null ? {} : { session_id: sessionId },
    }),
  });
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.trim().split("\n").map((line) => JSON.parse(line));
}

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

test("Codex adapter emits progress messages during list_sessions and read_session", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-progress-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        event("2026-07-27T00:00:00Z", "user", "Hello"),
        event("2026-07-27T00:00:01Z", "assistant", "World"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
        `INSERT INTO threads VALUES ('session-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:02Z');`,
      ].join("\n")
    );

    // 1. list_sessions
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "list_sessions",
        source: { location: fixtureRoot },
        params: {},
      }),
    });
    assert.equal(listResult.status, 0, listResult.stderr);
    const listMessages = listResult.stdout
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line));
    const listProgress = listMessages.filter((msg) => msg.type === "progress");
    assert.ok(listProgress.length > 0, "should emit progress during list_sessions");
    assert.equal(listProgress[0].progress.stage, "reading");
    assert.equal(listProgress[0].progress.operation, "list_sessions");

    // 2. read_session
    const readResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "read_session",
        source: { location: fixtureRoot },
        params: {},
      }),
    });
    assert.equal(readResult.status, 0, readResult.stderr);
    const readMessages = readResult.stdout
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line));
    const readProgress = readMessages.filter((msg) => msg.type === "progress");
    assert.ok(readProgress.length > 0, "should emit progress during read_session");
    assert.equal(readProgress[0].progress.stage, "reading");
    assert.equal(readProgress[0].progress.operation, "read_session");
    assert.equal(readProgress[readProgress.length - 1].progress.current, 1);
    assert.equal(readProgress[readProgress.length - 1].progress.total, 1);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter exposes subagent descriptors and anchors the dispatching parent turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-"));
  try {
    const userRollout = path.join(fixtureRoot, "rollout-user.jsonl");
    const subagentRollout = path.join(fixtureRoot, "rollout-subagent.jsonl");
    writeFileSync(
      userRollout,
      [
        event("2026-09-11T00:00:00Z", "user", "User question"),
        event("2026-09-11T00:00:01Z", "assistant", "User answer"),
      ].join("\n")
    );
    writeFileSync(
      subagentRollout,
      [
        event("2026-09-11T00:00:00Z", "user", "User question fork"),
        event("2026-09-11T00:00:02Z", "assistant", "Subagent answer"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('user-session-1', '${sqlString(userRollout)}', 'User Task', '2026-09-11T00:00:01Z', 'user', 'vscode');`,
        `INSERT INTO threads VALUES ('subagent-session-1', '${sqlString(subagentRollout)}', '', '2026-09-11T00:00:02Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"user-session-1"}}}');`,
      ].join("\n")
    );

    // 1. list_sessions must keep child descriptors: incremental sync uses this set
    //    for missing reconciliation; top-level hiding is the backend's job.
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "list_sessions",
        source: { location: fixtureRoot },
        params: {},
      }),
    });
    assert.equal(listResult.status, 0, listResult.stderr);
    const listItems = listResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .filter((m) => m.type === "item")
      .map((m) => m.item.external_id)
      .sort();
    assert.deepEqual(listItems, ["subagent-session-1", "user-session-1"]);

    // 2. read parent: one real question = one Turn; child dispatch is an anchor Part.
    const messages = readSessionMessages(fixtureRoot, "user-session-1");
    const sessions = messages
      .filter((m) => m.type === "item")
      .map((m) => m.item.session);
    const userSession = sessions.find((session) => session.external_id === "user-session-1");
    const childSession = sessions.find((session) => session.external_id === "subagent-session-1");
    assert.ok(userSession);
    assert.ok(childSession);

    assert.equal(userSession.turns.length, 1, "父会话一个真实提问只能有一个 Turn");
    assert.equal(userSession.turns[0].user_text, "User question");
    assert.ok(!userSession.turns.some((turn) => turn.turn_kind));

    const anchors = userSession.turns
      .flatMap((turn) => turn.parts)
      .filter((part) => part.kind === "subagent");
    assert.equal(anchors.length, 1);
    const anchorMeta = JSON.parse(anchors[0].metadata_json);
    assert.equal(anchorMeta.child_external_id, "subagent-session-1");
    assert.equal(anchorMeta.spawn_anchor, "subagent-session-1");
    assert.equal(anchorMeta.agent_role, null, "缺少子代理元数据时不得伪造角色");
    assert.equal(anchorMeta.status, "completed");
    assert.equal(anchorMeta.report_summary, "Subagent answer");
    assert.equal(anchors[0].source_execution_id, "subagent-session-1");
    assert.ok(!("content_card" in anchors[0]), "锚点不得挂 content_card");

    assert.equal(childSession.user_visible, false);
    assert.equal(childSession.execution_origin, "subagent");
    const purpose = JSON.parse(childSession.execution_purpose);
    assert.equal(purpose.parent_external_id, "user-session-1");
    assert.equal(purpose.parent_turn_index, 0);
    assert.equal(purpose.spawn_anchor, "subagent-session-1");
    assert.equal(childSession.turns.length, 1);
    assert.equal(childSession.turns[0].user_text, "User question fork");
    assert.equal(childSession.turns[0].turn_kind, undefined);

    // 3. reading the child directly returns the child session (anchor click must not 404)
    const readSubResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "read_session",
        source: { location: fixtureRoot },
        params: { session_id: "subagent-session-1" },
      }),
    });
    assert.equal(readSubResult.status, 0, readSubResult.stderr);
    const subItems = readSubResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .filter((m) => m.type === "item");
    assert.equal(subItems.length, 1);
    assert.equal(subItems[0].item.session.external_id, "subagent-session-1");
    assert.equal(subItems[0].item.session.user_visible, false);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter keeps each real question as its own Turn while subagents live in sessions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-multiturn-"));
  try {
    const userRollout = path.join(fixtureRoot, "rollout-user.jsonl");
    const subagentRollout = path.join(fixtureRoot, "rollout-subagent.jsonl");
    writeFileSync(
      userRollout,
      [
        event("2026-09-11T00:00:00Z", "user", "Question 1: Fix bug"),
        event("2026-09-11T00:00:01Z", "assistant", "Working on bug fix"),
        event("2026-09-11T00:05:00Z", "user", "Question 2: What is the weather today in Tokyo?"),
        event("2026-09-11T00:05:01Z", "assistant", "Tokyo is sunny."),
      ].join("\n")
    );
    writeFileSync(
      subagentRollout,
      [
        event("2026-09-11T00:00:01Z", "user", "Subagent task: analyze crash dump"),
        event("2026-09-11T00:00:02Z", "assistant", "Crash dump analyzed"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('user-session-multi', '${sqlString(userRollout)}', 'User Task', '2026-09-11T00:05:01Z', 'user', 'vscode');`,
        `INSERT INTO threads VALUES ('subagent-session-multi', '${sqlString(subagentRollout)}', '', '2026-09-11T00:00:02Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"user-session-multi"}}}');`,
      ].join("\n")
    );

    const sessions = readSessionMessages(fixtureRoot, "user-session-multi")
      .filter((m) => m.type === "item")
      .map((m) => m.item.session);
    const userSession = sessions.find((session) => session.external_id === "user-session-multi");
    const childSession = sessions.find((session) => session.external_id === "subagent-session-multi");
    assert.ok(userSession);
    assert.ok(childSession);

    // 父会话严格只有两个真实提问对应的 Turn，派发子代理不再切轮。
    assert.equal(userSession.turns.length, 2);
    assert.equal(userSession.turns[0].user_text, "Question 1: Fix bug");
    assert.equal(userSession.turns[0].turn_kind, undefined);
    assert.ok(
      userSession.turns[0].parts.some((part) => part.kind === "subagent"),
      "子代理锚点必须留在第 1 个父 Turn 内",
    );

    assert.equal(userSession.turns[1].user_text, "Question 2: What is the weather today in Tokyo?");
    assert.equal(userSession.turns[1].turn_kind, undefined, "Question 2 must remain a standard user turn");
    assert.ok(!userSession.turns[1].parts.some((part) => part.kind === "subagent"));

    assert.equal(childSession.user_visible, false);
    assert.equal(childSession.execution_origin, "subagent");
    assert.equal(childSession.turns.length, 1);
    assert.equal(childSession.turns[0].user_text, "Subagent task: analyze crash dump");
    assert.equal(childSession.turns[0].turn_kind, undefined);
    const purpose = JSON.parse(childSession.execution_purpose);
    assert.equal(purpose.parent_external_id, "user-session-multi");
    assert.equal(purpose.parent_turn_index, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter falls back to a valid prompt when subagent text is whitespace-only", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-ws-"));
  try {
    const userRollout = path.join(fixtureRoot, "rollout-user.jsonl");
    const subagentRollout = path.join(fixtureRoot, "rollout-subagent.jsonl");
    writeFileSync(
      userRollout,
      [
        event("2026-09-11T00:00:00Z", "user", "Main question"),
        event("2026-09-11T00:00:01Z", "assistant", "Main answer"),
      ].join("\n")
    );
    writeFileSync(
      subagentRollout,
      [
        event("2026-09-11T00:00:01Z", "user", "   \t\n  "),
        event("2026-09-11T00:00:02Z", "assistant", "Subagent answer"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('user-session-ws', '${sqlString(userRollout)}', 'Task', '2026-09-11T00:00:01Z', 'user', 'vscode');`,
        `INSERT INTO threads VALUES ('subagent-session-ws', '${sqlString(subagentRollout)}', '', '2026-09-11T00:00:02Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"user-session-ws"}}}');`,
      ].join("\n")
    );

    const messages = readSessionMessages(fixtureRoot, "user-session-ws");
    const sessions = messages.filter((m) => m.type === "item").map((m) => m.item.session);
    const userSession = sessions.find((session) => session.external_id === "user-session-ws");
    const childSession = sessions.find((session) => session.external_id === "subagent-session-ws");
    assert.ok(userSession);
    assert.ok(childSession);

    assert.equal(userSession.turns.length, 1, "父会话不得因空提示词增加伪轮次");
    const anchor = userSession.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor);
    assert.equal(JSON.parse(anchor.metadata_json).child_external_id, "subagent-session-ws");

    assert.equal(childSession.turns.length, 1);
    assert.equal(childSession.turns[0].user_text, "Subagent execution");
    assert.ok(childSession.turns[0].user_text.trim().length > 0);
    assert.ok(
      messages.some((message) => message.type === "warning" && /placeholder/.test(message.message ?? "")),
      "缺少可读派发提示词时必须发出告警事件"
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter folds each subagent thread into one Turn with real role and task prompt", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-merge-"));
  try {
    const mainRollout = path.join(fixtureRoot, "rollout-main.jsonl");
    const auditRollout = path.join(fixtureRoot, "rollout-audit.jsonl");
    const concurrencyRollout = path.join(fixtureRoot, "rollout-concurrency.jsonl");
    const injectedOnlyRollout = path.join(fixtureRoot, "rollout-injected-only.jsonl");

    writeFileSync(
      mainRollout,
      [
        event("2026-09-12T00:00:00Z", "user", "Main question"),
        event("2026-09-12T00:00:01Z", "assistant", "Main answer"),
      ].join("\n")
    );

    writeFileSync(
      auditRollout,
      [
        event("2026-09-12T00:00:10Z", "user", [
          "<recommended_plugins>",
          "- plugin-a: 一个很长的插件清单",
          "- plugin-b: 另一个插件条目",
          "</recommended_plugins>",
          "",
          "<environment_context>",
          "  <cwd>/Users/test/project</cwd>",
          "</environment_context>",
          "",
          "审查 SQLite 迁移脚本是否存在冗余索引",
        ].join("\n")),
        event("2026-09-12T00:00:11Z", "assistant", "开始审查迁移脚本。"),
        event("2026-09-12T00:00:12Z", "user", "继续核对索引命中率"),
        event("2026-09-12T00:00:13Z", "assistant", "审查完成：发现 2 个冗余索引。"),
      ].join("\n")
    );

    writeFileSync(
      concurrencyRollout,
      [
        event("2026-09-12T00:00:20Z", "user", [
          "<user_instructions>",
          "Always answer in Chinese.",
          "</user_instructions>",
          "",
          "检查并发锁的获取顺序",
        ].join("\n")),
        event("2026-09-12T00:00:21Z", "assistant", "并发审计完成：未发现锁顺序反转。"),
      ].join("\n")
    );

    writeFileSync(
      injectedOnlyRollout,
      [
        event("2026-09-12T00:00:30Z", "user", [
          "<recommended_plugins>",
          "- plugin-a: 只有注入块",
          "</recommended_plugins>",
        ].join("\n")),
        event("2026-09-12T00:00:31Z", "assistant", "注入块会话完成。"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('main-merge', '${sqlString(mainRollout)}', 'Main Task', '2026-09-12T00:00:01Z', 'user', 'vscode');`,
        `INSERT INTO threads VALUES ('child-audit', '${sqlString(auditRollout)}', '', '2026-09-12T00:00:13Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"main-merge","depth":1,"agent_path":"/root/guard_audit","agent_nickname":"Lagrange","agent_role":"default"}}}');`,
        `INSERT INTO threads VALUES ('child-concurrency', '${sqlString(concurrencyRollout)}', '', '2026-09-12T00:00:21Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"main-merge","depth":1,"agent_path":"/root/concurrency_audit","agent_nickname":"Noether","agent_role":"default"}}}');`,
        `INSERT INTO threads VALUES ('child-injected', '${sqlString(injectedOnlyRollout)}', '', '2026-09-12T00:00:31Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"main-merge","depth":1}}}');`,
      ].join("\n")
    );

    const firstRead = readSessionMessages(fixtureRoot, "main-merge");
    const sessions = firstRead.filter((m) => m.type === "item").map((m) => m.item.session);
    const session = sessions.find((entry) => entry.external_id === "main-merge");
    assert.ok(session);

    // 父会话严格只有一个真实提问 Turn，三个子代理只落三个锚点 Part。
    assert.equal(session.turns.length, 1);
    assert.ok(!session.turns.some((turn) => turn.turn_kind));
    const anchors = session.turns[0].parts.filter((part) => part.kind === "subagent");
    assert.equal(anchors.length, 3, "每个子代理线程只产出一个锚点 Part");
    assert.ok(anchors.every((part) => !("content_card" in part)), "锚点不得挂 content_card");

    const anchorByChild = new Map(
      anchors.map((part) => [JSON.parse(part.metadata_json).child_external_id, part]),
    );
    const auditAnchor = anchorByChild.get("child-audit");
    const concurrencyAnchor = anchorByChild.get("child-concurrency");
    const injectedAnchor = anchorByChild.get("child-injected");
    assert.ok(auditAnchor);
    assert.ok(concurrencyAnchor);
    assert.ok(injectedAnchor);

    const auditMeta = JSON.parse(auditAnchor.metadata_json);
    const concurrencyMeta = JSON.parse(concurrencyAnchor.metadata_json);
    const injectedMeta = JSON.parse(injectedAnchor.metadata_json);

    // 角色来自 thread_spawn.agent_path 末段，昵称进 title
    assert.equal(auditMeta.agent_role, "guard_audit");
    assert.equal(auditMeta.title, "Lagrange");
    assert.equal(concurrencyMeta.agent_role, "concurrency_audit");
    assert.equal(concurrencyMeta.title, "Noether");
    assert.equal(auditMeta.spawn_anchor, "child-audit");
    assert.equal(auditMeta.status, "completed");

    // 锚点汇报取子线程最终 assistant 正文，而不是父侧任何工具回执。
    assert.equal(auditMeta.report_summary, "审查完成：发现 2 个冗余索引。");
    assert.equal(concurrencyMeta.report_summary, "并发审计完成：未发现锁顺序反转。");
    assert.match(auditAnchor.text, /审查完成：发现 2 个冗余索引/);

    // 子会话 #T1 承载完整委派 Prompt + 工具链 + 最终汇报，绝不带 turn_kind。
    const auditChild = sessions.find((entry) => entry.external_id === "child-audit");
    const concurrencyChild = sessions.find((entry) => entry.external_id === "child-concurrency");
    const injectedChild = sessions.find((entry) => entry.external_id === "child-injected");
    assert.ok(auditChild);
    assert.ok(concurrencyChild);
    assert.ok(injectedChild);
    for (const child of [auditChild, concurrencyChild, injectedChild]) {
      assert.equal(child.user_visible, false);
      assert.equal(child.execution_origin, "subagent");
      assert.equal(child.turns.length, 1, "子会话 #T1 合并全部执行过程");
      assert.equal(child.turns[0].turn_kind, undefined);
      assert.equal(JSON.parse(child.execution_purpose).parent_external_id, "main-merge");
      assert.equal(JSON.parse(child.execution_purpose).parent_turn_index, 0);
    }

    assert.equal(auditChild.turns[0].user_text, "审查 SQLite 迁移脚本是否存在冗余索引");
    assert.ok(!auditChild.turns[0].user_text.includes("recommended_plugins"));
    assert.ok(!auditChild.turns[0].user_text.includes("environment_context"));
    assert.equal(concurrencyChild.turns[0].user_text, "检查并发锁的获取顺序");
    assert.ok(!concurrencyChild.turns[0].user_text.includes("user_instructions"));
    assert.notEqual(auditChild.turns[0].user_text, concurrencyChild.turns[0].user_text);

    assert.deepEqual(
      auditChild.turns[0].parts.map((part) => part.text),
      ["开始审查迁移脚本。", "审查完成：发现 2 个冗余索引。"]
    );
    assert.deepEqual(
      concurrencyChild.turns[0].parts.map((part) => part.text),
      ["并发审计完成：未发现锁顺序反转。"]
    );

    // 元数据缺失时角色留空、标题留空，但子会话仍以占位提示词成立。
    assert.equal(injectedMeta.agent_role, null, "缺少 thread_spawn 角色时必须留空");
    assert.equal(injectedMeta.title, null);
    assert.equal(injectedChild.turns[0].user_text, "Subagent execution");
    assert.equal(injectedMeta.report_summary, "注入块会话完成。");
    assert.ok(
      firstRead.some((message) => message.type === "warning" && /placeholder/.test(message.message ?? "")),
      "无可用任务文本时必须发出告警事件"
    );

    // 重复同步幂等：父锚点与子会话 external_id / 汇报都稳定。
    const secondRead = readSessionMessages(fixtureRoot, "main-merge");
    const secondSessions = secondRead.filter((m) => m.type === "item").map((m) => m.item.session);
    assert.deepEqual(
      secondSessions.map((entry) => entry.external_id).sort(),
      sessions.map((entry) => entry.external_id).sort()
    );
    const secondParent = secondSessions.find((entry) => entry.external_id === "main-merge");
    assert.deepEqual(
      secondParent.turns[0].parts
        .filter((part) => part.kind === "subagent")
        .map((part) => JSON.parse(part.metadata_json).child_external_id),
      anchors.map((part) => JSON.parse(part.metadata_json).child_external_id),
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter seamlessly merges multi-page paginated rollouts by history_base", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-multipage-"));
  try {
    const sessionId = "01a08e34-c343-7203-9e35-c23c39101192";
    const sessionsDir = path.join(fixtureRoot, "sessions", "2026", "09", "11");
    mkdirSync(sessionsDir, { recursive: true });

    const page1Path = path.join(sessionsDir, `rollout-2026-09-11T10-00-00-${sessionId}.jsonl`);
    const page2Path = path.join(sessionsDir, `rollout-2026-09-11T10-10-00-${sessionId}_page2.jsonl`);

    // Page 1: Turn 0 and partial Turn 1 that got rolled over
    writeFileSync(
      page1Path,
      [
        JSON.stringify({ timestamp: "2026-09-11T10:00:00Z", ordinal: 0, type: "session_meta", payload: { id: sessionId, session_id: sessionId } }),
        JSON.stringify({ timestamp: "2026-09-11T10:00:01Z", ordinal: 1, type: "response_item", payload: { type: "message", role: "user", content: [{ type: "input_text", text: "Turn 0 Question" }] } }),
        JSON.stringify({ timestamp: "2026-09-11T10:00:02Z", ordinal: 2, type: "response_item", payload: { type: "message", role: "assistant", content: [{ type: "output_text", text: "Turn 0 Answer" }] } }),
        JSON.stringify({ timestamp: "2026-09-11T10:00:03Z", ordinal: 3, type: "response_item", payload: { type: "message", role: "user", content: [{ type: "input_text", text: "Turn 1 Partial" }] } }),
      ].join("\n")
    );

    // Page 2: starts at ordinal 3 with history_base
    writeFileSync(
      page2Path,
      [
        JSON.stringify({ timestamp: "2026-09-11T10:10:00Z", ordinal: 3, type: "session_meta", payload: { id: sessionId, session_id: sessionId, history_mode: "paginated", history_base: { thread_id: sessionId, end_ordinal_exclusive: 3 } } }),
        JSON.stringify({ timestamp: "2026-09-11T10:10:01Z", ordinal: 3, type: "response_item", payload: { type: "message", role: "user", content: [{ type: "input_text", text: "Turn 1 Full Question" }] } }),
        JSON.stringify({ timestamp: "2026-09-11T10:10:02Z", ordinal: 4, type: "response_item", payload: { type: "message", role: "assistant", content: [{ type: "output_text", text: "Turn 1 Full Answer" }] } }),
      ].join("\n")
    );

    // threads.rollout_path only points to latest page (Page 2)
    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('${sessionId}', '${sqlString(page2Path)}', 'MultiPage Session', '2026-09-11T10:10:02Z', 'user', 'vscode');`,
      ].join("\n")
    );

    const readResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "read_session",
        source: { location: fixtureRoot },
        params: { session_id: sessionId },
      }),
    });
    assert.equal(readResult.status, 0, readResult.stderr);
    const session = readResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .find((m) => m.type === "item")?.item?.session;

    assert.ok(session);
    assert.equal(session.external_id, sessionId);
    assert.equal(session.turns.length, 2);
    assert.equal(session.turns[0].user_text, "Turn 0 Question");
    assert.equal(session.turns[1].user_text, "Turn 1 Full Question");
    assert.equal(session.turns[0].parts[0].text, "Turn 0 Answer");
    assert.equal(session.turns[1].parts[0].text, "Turn 1 Full Answer");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter lists and reads zstd-compressed .jsonl.zst session rollouts", () => {
  const zstdCheck = spawnSync("zstd", ["--version"], {
    env: { ...process.env, PATH: `${process.env.PATH || ""}:/opt/homebrew/bin:/usr/local/bin` },
  });
  if (zstdCheck.status !== 0) {
    // Skip if zstd binary is not available in test runner environment
    return;
  }

  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-zst-"));
  try {
    const sessionId = "019ce000-0000-7000-8000-000000000001";
    const sessionsDir = path.join(fixtureRoot, "sessions", "2026", "10", "10");
    mkdirSync(sessionsDir, { recursive: true });
    const rawJsonlPath = path.join(sessionsDir, `rollout-2026-10-10T12-00-00-${sessionId}.jsonl`);
    const zstPath = `${rawJsonlPath}.zst`;

    writeFileSync(
      rawJsonlPath,
      [
        event("2026-10-10T12:00:00Z", "user", "压缩会话测试问题"),
        event("2026-10-10T12:00:01Z", "assistant", "压缩会话测试回答。"),
      ].join("\n")
    );

    // Compress with zstd and remove raw jsonl
    const compressResult = spawnSync("zstd", ["-q", "--rm", rawJsonlPath, "-o", zstPath], {
      env: { ...process.env, PATH: `${process.env.PATH || ""}:/opt/homebrew/bin:/usr/local/bin` },
    });
    assert.equal(compressResult.status, 0);

    // threads table rollout_path retains original .jsonl path
    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, cwd TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('${sessionId}', '${sqlString(rawJsonlPath)}', 'Zstd Session', '2026-10-10T12:00:01Z', '/Users/test/project', 'user', 'vscode');`,
      ].join("\n")
    );

    // 1. Test list_sessions
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "list_sessions",
        source: { location: fixtureRoot },
      }),
    });
    assert.equal(listResult.status, 0, listResult.stderr);
    const listItems = listResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .filter((m) => m.type === "item");
    assert.equal(listItems.length, 1);
    assert.equal(listItems[0].item.external_id, sessionId);
    assert.equal(listItems[0].item.source_locator, zstPath);

    // 2. Test read_session
    const readResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "read_session",
        source: { location: fixtureRoot },
        params: { session_id: sessionId },
      }),
    });
    assert.equal(readResult.status, 0, readResult.stderr);
    const session = readResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .find((m) => m.type === "item")?.item?.session;

    assert.ok(session);
    assert.equal(session.external_id, sessionId);
    assert.equal(session.turns.length, 1);
    assert.equal(session.turns[0].user_text, "压缩会话测试问题");
    assert.equal(session.turns[0].parts[0].text, "压缩会话测试回答。");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});


test("Codex adapter inlines nested subagent threads recursively under their dispatching turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-nested-"));
  try {
    const mainRollout = path.join(fixtureRoot, "rollout-main.jsonl");
    const midRollout = path.join(fixtureRoot, "rollout-mid.jsonl");
    const deepRollout = path.join(fixtureRoot, "rollout-deep.jsonl");

    writeFileSync(
      mainRollout,
      [
        event("2026-09-12T00:00:00Z", "user", "Main question"),
        event("2026-09-12T00:00:01Z", "assistant", "Main answer"),
      ].join("\n")
    );
    writeFileSync(
      midRollout,
      [
        event("2026-09-12T00:00:10Z", "user", "协调子任务"),
        event("2026-09-12T00:00:11Z", "assistant", "协调者已派发下一层。"),
      ].join("\n")
    );
    writeFileSync(
      deepRollout,
      [
        event("2026-09-12T00:00:20Z", "user", "执行底层审查"),
        event("2026-09-12T00:00:21Z", "assistant", "底层审查完成：无问题。"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('main-nested', '${sqlString(mainRollout)}', 'Main', '2026-09-12T00:00:01Z', 'user', 'vscode');`,
        `INSERT INTO threads VALUES ('child-mid', '${sqlString(midRollout)}', '', '2026-09-12T00:00:11Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"main-nested","depth":1,"agent_path":"/root/orchestrator"}}}');`,
        `INSERT INTO threads VALUES ('child-deep', '${sqlString(deepRollout)}', '', '2026-09-12T00:00:21Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"child-mid","depth":2,"agent_path":"/root/reviewer"}}}');`,
      ].join("\n")
    );

    const read = readSessionMessages(fixtureRoot, "main-nested");
    const sessions = read.filter((m) => m.type === "item").map((m) => m.item.session);
    assert.equal(sessions.length, 3, "主会话 + 两层子会话");

    const main = sessions.find((entry) => entry.external_id === "main-nested");
    const mid = sessions.find((entry) => entry.external_id === "child-mid");
    const deep = sessions.find((entry) => entry.external_id === "child-deep");
    assert.ok(main);
    assert.ok(mid, "第一层子代理必须存在");
    assert.ok(deep, "第二层子代理必须存在");
    assert.ok(!sessions.some((entry) => entry.turns.some((turn) => turn.turn_kind)));

    assert.equal(main.turns.length, 1);
    assert.equal(mid.turns.length, 1);
    assert.equal(deep.turns.length, 1);

    const mainAnchor = main.turns[0].parts.find((part) => part.kind === "subagent");
    const midAnchor = mid.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(mainAnchor);
    assert.ok(midAnchor);
    assert.equal(JSON.parse(mainAnchor.metadata_json).child_external_id, "child-mid");
    assert.equal(JSON.parse(mainAnchor.metadata_json).agent_role, "orchestrator");
    assert.equal(JSON.parse(midAnchor.metadata_json).child_external_id, "child-deep");
    assert.equal(JSON.parse(midAnchor.metadata_json).agent_role, "reviewer");
    assert.ok(!deep.turns[0].parts.some((part) => part.kind === "subagent"));

    const midPurpose = JSON.parse(mid.execution_purpose);
    const deepPurpose = JSON.parse(deep.execution_purpose);
    assert.equal(midPurpose.parent_external_id, "main-nested");
    assert.equal(midPurpose.spawn_anchor, "child-mid");
    assert.equal(deepPurpose.parent_external_id, "child-mid");
    assert.equal(deepPurpose.spawn_anchor, "child-deep");
    assert.equal(midPurpose.parent_turn_index, 0);
    assert.equal(deepPurpose.parent_turn_index, 0);
    assert.ok(
      deep.turns[0].parts.some((part) => /底层审查完成/.test(part.text ?? "")),
      "第二层必须带自己的执行内容"
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter stops expanding subagent threads beyond the depth limit", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-depth-"));
  try {
    const mainRollout = path.join(fixtureRoot, "rollout-main.jsonl");
    writeFileSync(
      mainRollout,
      [
        event("2026-09-12T00:00:00Z", "user", "Main question"),
        event("2026-09-12T00:00:01Z", "assistant", "Main answer"),
      ].join("\n")
    );

    const sql = [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
      `INSERT INTO threads VALUES ('main-depth', '${sqlString(mainRollout)}', 'Main', '2026-09-12T00:00:01Z', 'user', 'vscode');`,
    ];
    // 6 层链式派发，超过深度上限 4
    for (let level = 1; level <= 6; level += 1) {
      const rollout = path.join(fixtureRoot, `rollout-depth-${level}.jsonl`);
      writeFileSync(
        rollout,
        [
          event(`2026-09-12T00:01:${String(level).padStart(2, "0")}Z`, "user", `第 ${level} 层任务`),
          event(`2026-09-12T00:01:${String(level).padStart(2, "0")}Z`, "assistant", `第 ${level} 层汇报`),
        ].join("\n")
      );
      const parent = level === 1 ? "main-depth" : `child-depth-${level - 1}`;
      sql.push(
        `INSERT INTO threads VALUES ('child-depth-${level}', '${sqlString(rollout)}', '', '2026-09-12T00:01:00Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"${parent}","depth":${level},"agent_path":"/root/level_${level}"}}}');`
      );
    }
    runSqlite(fixtureRoot, sql.join("\n"));

    const read = readSessionMessages(fixtureRoot, "main-depth");
    const sessions = read.filter((m) => m.type === "item").map((m) => m.item.session);
    const session = sessions.find((entry) => entry.external_id === "main-depth");
    assert.ok(session);

    // 主会话 + 5 层子会话；第 6 层不再展开。
    assert.equal(sessions.length, 6, "深度上限 4 时只能展开到第 5 层子会话");
    assert.ok(!sessions.some((entry) => entry.external_id === "child-depth-6"));

    const anchorChildIds = sessions.flatMap((entry) => entry.turns.flatMap((turn) => turn.parts))
      .filter((part) => part.kind === "subagent")
      .map((part) => JSON.parse(part.metadata_json).child_external_id);
    assert.equal(anchorChildIds.length, 5);
    assert.ok(!anchorChildIds.includes("child-depth-6"), "超限的更深层不得继续展开");
    assert.ok(
      read.some(
        (message) =>
          message.type === "warning" &&
          /exceeds the depth limit of 4/.test(message.message ?? "")
      ),
      "超过深度上限必须发出告警"
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter keeps nested subagent sessions under their dispatching parent", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-cycle-"));
  try {
    const mainRollout = path.join(fixtureRoot, "rollout-main.jsonl");
    writeFileSync(
      mainRollout,
      [
        event("2026-09-12T00:00:00Z", "user", "Main question"),
        event("2026-09-12T00:00:01Z", "assistant", "Main answer"),
      ].join("\n")
    );

    const cycleARollout = path.join(fixtureRoot, "rollout-cycle-a.jsonl");
    const cycleBRollout = path.join(fixtureRoot, "rollout-cycle-b.jsonl");
    writeFileSync(
      cycleARollout,
      [
        event("2026-09-12T00:00:10Z", "user", "A 层任务"),
        event("2026-09-12T00:00:11Z", "assistant", "A 层汇报"),
      ].join("\n")
    );
    writeFileSync(
      cycleBRollout,
      [
        event("2026-09-12T00:00:20Z", "user", "B 层任务"),
        event("2026-09-12T00:00:21Z", "assistant", "B 层汇报"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        `INSERT INTO threads VALUES ('main-cycle', '${sqlString(mainRollout)}', 'Main', '2026-09-12T00:00:01Z', 'user', 'vscode');`,
        // A 的 spawn 元数据里出现 B 的线程 ID，B 又登记 A 为子代理，形成 A -> B -> A 的引用环
        `INSERT INTO threads VALUES ('child-cycle-a', '${sqlString(cycleARollout)}', '', '2026-09-12T00:00:11Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"main-cycle","depth":1,"agent_path":"/root/cycle_a","agent_nickname":"child-cycle-b"}}}');`,
        `INSERT INTO threads VALUES ('child-cycle-b', '${sqlString(cycleBRollout)}', '', '2026-09-12T00:00:21Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"child-cycle-a","depth":2,"agent_path":"/root/cycle_b"}}}');`,
      ].join("\n")
    );

    const read = readSessionMessages(fixtureRoot, "main-cycle");
    const sessions = read.filter((m) => m.type === "item").map((m) => m.item.session);
    assert.equal(sessions.length, 3, "环只能产生主会话 + 两个子会话，不得无限展开");

    const anchors = sessions.flatMap((entry) => entry.turns.flatMap((turn) => turn.parts))
      .filter((part) => part.kind === "subagent")
      .map((part) => JSON.parse(part.metadata_json));
    assert.equal(anchors.length, 2, "两条真实派发边各只保留一个锚点");
    assert.ok(
      anchors.some((meta) => meta.child_external_id === "child-cycle-a"),
      "第一层子代理锚点挂在父会话轮次里"
    );
    assert.ok(
      anchors.some((meta) => meta.child_external_id === "child-cycle-b"),
      "第二层子代理锚点挂在它自己的父会话轮次里"
    );

    const cycleA = sessions.find((entry) => entry.external_id === "child-cycle-a");
    const cycleB = sessions.find((entry) => entry.external_id === "child-cycle-b");
    assert.ok(cycleA);
    assert.ok(cycleB);
    assert.equal(JSON.parse(cycleA.execution_purpose).parent_external_id, "main-cycle");
    assert.equal(JSON.parse(cycleB.execution_purpose).parent_external_id, "child-cycle-a");
    assert.equal(cycleA.turns[0].parts.filter((part) => part.kind === "subagent").length, 1);
    assert.ok(!cycleB.turns[0].parts.some((part) => part.kind === "subagent"));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter stops cycling when orphan subagent rows reference each other", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-cycle-guard-"));
  try {
    const a = path.join(fixtureRoot, "rollout-a.jsonl");
    const b = path.join(fixtureRoot, "rollout-b.jsonl");
    writeFileSync(a, [
      event("2026-09-12T00:00:10Z", "user", "A 层任务"),
      event("2026-09-12T00:00:11Z", "assistant", "A 层汇报"),
    ].join("\n"));
    writeFileSync(b, [
      event("2026-09-12T00:00:20Z", "user", "B 层任务"),
      event("2026-09-12T00:00:21Z", "assistant", "B 层汇报"),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
      `INSERT INTO threads VALUES ('orphan-a', '${sqlString(a)}', '', '2026-09-12T00:00:11Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"orphan-b"}}}');`,
      `INSERT INTO threads VALUES ('orphan-b', '${sqlString(b)}', '', '2026-09-12T00:00:21Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"orphan-a"}}}');`,
    ].join("\n"));

    const read = readSessionMessages(fixtureRoot);
    const sessions = read.filter((m) => m.type === "item").map((m) => m.item.session);
    assert.equal(sessions.length, 2, "互相引用的两个子线程必须各自只产出一次，不得无限展开");
    assert.ok(
      read.some(
        (message) =>
          message.type === "warning" && /already expanded \(cycle\)/.test(message.message ?? "")
      ),
      "成环必须发出告警"
    );
    assert.ok(sessions.every((session) => session.user_visible === false));
    assert.ok(sessions.every((session) => session.execution_origin === "subagent"));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter prefers thread_spawn_edges for parent linkage", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-edges-"));
  try {
    const mainRollout = path.join(fixtureRoot, "rollout-main.jsonl");
    const childRollout = path.join(fixtureRoot, "rollout-child.jsonl");
    writeFileSync(
      mainRollout,
      [
        event("2026-09-13T00:00:00Z", "user", "Main question"),
        event("2026-09-13T00:00:01Z", "assistant", "Main answer"),
      ].join("\n")
    );
    writeFileSync(
      childRollout,
      [
        event("2026-09-13T00:00:02Z", "user", "Edge child task"),
        event("2026-09-13T00:00:03Z", "assistant", "Edge child report"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT);",
        "CREATE TABLE thread_spawn_edges (parent_thread_id TEXT, child_thread_id TEXT);",
        `INSERT INTO threads VALUES ('main-edge', '${sqlString(mainRollout)}', 'Main', '2026-09-13T00:00:01Z', 'user', 'vscode');`,
        // source 里故意留一个错误的 parent，验证 edge 表优先。
        `INSERT INTO threads VALUES ('child-edge', '${sqlString(childRollout)}', '', '2026-09-13T00:00:03Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"stale-parent","agent_path":"/root/edge_agent","agent_nickname":"Edge"}}}');`,
        "INSERT INTO thread_spawn_edges VALUES ('main-edge', 'child-edge');",
      ].join("\n")
    );

    const sessions = readSessionMessages(fixtureRoot, "main-edge")
      .filter((m) => m.type === "item")
      .map((m) => m.item.session);
    const main = sessions.find((entry) => entry.external_id === "main-edge");
    const child = sessions.find((entry) => entry.external_id === "child-edge");
    assert.ok(main);
    assert.ok(child);
    const anchor = main.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor);
    const meta = JSON.parse(anchor.metadata_json);
    assert.equal(meta.child_external_id, "child-edge");
    assert.equal(meta.title, "Edge");
    assert.equal(meta.agent_role, "edge_agent");
    assert.equal(JSON.parse(child.execution_purpose).parent_external_id, "main-edge");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter emits a shell subagent session when its rollout was cleaned up", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-subagent-shell-"));
  try {
    const mainRollout = path.join(fixtureRoot, "rollout-main.jsonl");
    const missingChildRollout = path.join(fixtureRoot, "rollout-child-gone.jsonl");
    writeFileSync(
      mainRollout,
      [
        event("2026-09-14T00:00:00Z", "user", "Main question"),
        event("2026-09-14T00:00:01Z", "assistant", "Main answer"),
      ].join("\n")
    );

    runSqlite(
      fixtureRoot,
      [
        "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT, thread_source TEXT, source TEXT, model TEXT);",
        `INSERT INTO threads VALUES ('main-shell', '${sqlString(mainRollout)}', 'Main', '2026-09-14T00:00:01Z', 'user', 'vscode', NULL);`,
        `INSERT INTO threads VALUES ('child-shell', '${sqlString(missingChildRollout)}', 'Shell Thread', '2026-09-14T00:00:09Z', 'subagent', '{"subagent":{"thread_spawn":{"parent_thread_id":"main-shell","agent_path":"/root/ghost","agent_nickname":"Ghost"}}}', 'gpt-5.4');`,
      ].join("\n")
    );

    const messages = readSessionMessages(fixtureRoot, "main-shell");
    const sessions = messages.filter((m) => m.type === "item").map((m) => m.item.session);
    const main = sessions.find((entry) => entry.external_id === "main-shell");
    const child = sessions.find((entry) => entry.external_id === "child-shell");
    assert.ok(main);
    assert.ok(child);

    const anchor = main.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor);
    const meta = JSON.parse(anchor.metadata_json);
    assert.equal(meta.child_external_id, "child-shell");
    assert.equal(meta.spawn_anchor, "child-shell");
    assert.equal(meta.title, "Ghost", "昵称优先于 threads.title");
    assert.match(meta.report_summary, /已清理/);

    assert.equal(child.user_visible, false);
    assert.equal(child.execution_origin, "subagent");
    assert.equal(child.source_locator, null);
    assert.equal(child.source_fingerprint, "2026-09-14T00:00:09Z", "壳会话指纹使用 updated_at");
    assert.equal(child.turns.length, 1, "壳会话只保留单轮摘要");
    assert.match(child.turns[0].parts[0].text, /已清理/);
    assert.equal(JSON.parse(child.execution_purpose).parent_external_id, "main-shell");

    // 点击父锚点直接读取壳子会话不得 404。
    const direct = readSessionMessages(fixtureRoot, "child-shell")
      .filter((m) => m.type === "item")
      .map((m) => m.item.session);
    assert.equal(direct.length, 1);
    assert.equal(direct[0].external_id, "child-shell");

    // 增量同步描述符必须保留壳子会话，避免被 missing 对账误删。
    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "list_sessions",
        source: { location: fixtureRoot },
        params: {},
      }),
    });
    assert.equal(listResult.status, 0, listResult.stderr);
    const descriptors = listResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .filter((m) => m.type === "item")
      .map((m) => m.item);
    const shellDescriptor = descriptors.find((descriptor) => descriptor.external_id === "child-shell");
    assert.ok(shellDescriptor);
    assert.equal(shellDescriptor.source_locator, null);
    assert.equal(shellDescriptor.updated_at, "2026-09-14T00:00:09Z");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter extracts single Turn usage and first event UTC timestamp", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-usage-single-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        JSON.stringify({ timestamp: "2026-07-27T00:00:00.100Z", type: "session_meta", payload: { model_provider: "openai" } }),
        JSON.stringify({ timestamp: "2026-07-27T00:00:00.200Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-07-27T00:00:00.300Z", type: "message", role: "user", content: "Implement feature" }),
        JSON.stringify({
          timestamp: "2026-07-27T00:00:01.456Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: {
              last_token_usage: {
                input_tokens: 100,
                cached_input_tokens: 30,
                output_tokens: 50,
                reasoning_output_tokens: 20,
                cache_write_input_tokens: 10,
                total_tokens: 210,
              },
            },
          },
        }),
      ].join("\n")
    );

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-codex-1', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:02Z');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(complete.decoder_profile, "codex-rollout-v1");
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, "sess-codex-1");
    assert.equal(event.turn_id, "turn-0");
    assert.equal(event.external_event_id, "sess-codex-1-turn-0");
    assert.equal(event.logical_request_id, "sess-codex-1-turn-0");
    assert.equal(event.attempt_index, 0);
    assert.equal(event.provider, "openai");
    assert.equal(event.model, "gpt-5.4");
    assert.equal(event.timestamp, "2026-07-27T00:00:00.200Z"); // First event timestamp in turn
    assert.equal(event.input_tokens, 100);
    assert.equal(event.cache_read_tokens, 30);
    assert.equal(event.output_tokens, 50);
    assert.equal(event.reasoning_tokens, 20);
    assert.equal(event.cache_write_tokens, 10);
    assert.equal(event.total_tokens, 210);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter accumulates multi-step tool calls / token_count events within single Turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-usage-multi-step-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        JSON.stringify({ timestamp: "2026-07-27T00:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-07-27T00:00:00.100Z", type: "message", role: "user", content: "Run multiple tools" }),
        JSON.stringify({
          timestamp: "2026-07-27T00:00:01.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: {
              last_token_usage: {
                input_tokens: 100,
                cached_input_tokens: 20,
                output_tokens: 30,
                total_tokens: 150,
              },
            },
          },
        }),
        JSON.stringify({
          timestamp: "2026-07-27T00:00:03.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: {
              last_token_usage: {
                input_tokens: 120,
                cached_input_tokens: 10,
                output_tokens: 40,
                total_tokens: 170,
              },
            },
          },
        }),
      ].join("\n")
    );

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-multi', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T00:00:05Z');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.turn_id, "turn-0");
    assert.equal(event.external_event_id, "sess-multi-turn-0");
    assert.equal(event.timestamp, "2026-07-27T00:00:00.000Z");
    assert.equal(event.input_tokens, 220);
    assert.equal(event.cache_read_tokens, 30);
    assert.equal(event.output_tokens, 70);
    assert.equal(event.total_tokens, 320);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter preserves separate historical UTC timestamps for cross-day multi-turn sessions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-usage-cross-day-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        // Turn 0 on Day 1
        JSON.stringify({ timestamp: "2026-07-27T10:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-07-27T10:00:00.100Z", type: "message", role: "user", content: "Day 1 query" }),
        JSON.stringify({
          timestamp: "2026-07-27T10:00:01.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: { last_token_usage: { input_tokens: 50, output_tokens: 25, total_tokens: 75 } },
          },
        }),
        // Turn 1 on Day 2
        JSON.stringify({ timestamp: "2026-07-28T11:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-07-28T11:00:00.200Z", type: "message", role: "user", content: "Day 2 query" }),
        JSON.stringify({
          timestamp: "2026-07-28T11:00:02.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: { last_token_usage: { input_tokens: 60, output_tokens: 30, total_tokens: 90 } },
          },
        }),
      ].join("\n")
    );

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-cross', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-28T12:00:00Z');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 2);
    assert.equal(events.length, 2);

    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, "sess-cross-turn-0");
    assert.equal(events[0].timestamp, "2026-07-27T10:00:00.000Z");
    assert.equal(events[0].input_tokens, 50);
    assert.equal(events[0].output_tokens, 25);

    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, "sess-cross-turn-1");
    assert.equal(events[1].timestamp, "2026-07-28T11:00:00.000Z");
    assert.equal(events[1].input_tokens, 60);
    assert.equal(events[1].output_tokens, 30);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter filters out empty zero-token turns from usage event stream", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-usage-zero-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(
      rolloutPath,
      [
        // Turn 0: empty, zero tokens
        JSON.stringify({ timestamp: "2026-07-27T10:00:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-07-27T10:00:00.100Z", type: "message", role: "user", content: "No tokens in this turn" }),
        // Turn 1: has tokens
        JSON.stringify({ timestamp: "2026-07-27T10:05:00.000Z", type: "turn_context", payload: { model: "gpt-5.4" } }),
        JSON.stringify({ timestamp: "2026-07-27T10:05:00.100Z", type: "message", role: "user", content: "Real query with tokens" }),
        JSON.stringify({
          timestamp: "2026-07-27T10:05:01.000Z",
          type: "event_msg",
          payload: {
            type: "token_count",
            info: { last_token_usage: { input_tokens: 80, output_tokens: 40, total_tokens: 120 } },
          },
        }),
      ].join("\n")
    );

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('sess-zero', '${sqlString(rolloutPath)}', 'Fixture', '2026-07-27T10:10:00Z');`,
    ].join("\n"));

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-1");
    assert.equal(events[0].external_event_id, "sess-zero-turn-1");
    assert.equal(events[0].total_tokens, 120);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter extracts single-quoted exec_command, batches web__run searches as codex.ambient, and drops empty/control tools", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-codex-exec-clean-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      event("2026-11-10T01:48:00Z", "user", "Search and check"),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:05Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-single-quote-cmd",
          input: "const r = await tools.exec_command({\n  cmd: 'git status && echo \\'ok\\'',\n  workdir: '/tmp/repo'\n});\ntext(r.output);\n",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:06Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-single-quote-cmd",
          output: "On branch main",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:10Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-web-1",
          input: "const r = await tools.web__run({search_query:[{q:\"AssetIWeave Tauri\"}],response_length:\"short\"}); text(r);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:11Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-web-1",
          output: "Search results JSON",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:12Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-web-2",
          input: "const r = await tools.web__run({search_query:[{q:\"Codex custom_tool_call\"}]}); text(r);",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:13Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-web-2",
          output: "More search results",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:15Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-empty-exec",
          input: "",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:16Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "write_stdin",
          call_id: "call-stdin",
          arguments: "{\"session_id\":1,\"chars\":\"\"}",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-10T01:48:17Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "create_goal",
          call_id: "call-goal",
          arguments: "{}",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('session-exec-clean', '${sqlString(rolloutPath)}', 'Fixture', '2026-11-10T01:48:20Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;
    const cmdPart = parts.find((p) => p.kind === "command");
    assert.ok(cmdPart, "single-quoted exec_command should become command part");
    assert.equal(cmdPart.command, "git status && echo 'ok'");
    assert.equal(cmdPart.cwd, "/tmp/repo");

    const ambientParts = parts.filter((p) => p.content_card?.kind === "codex.ambient");
    assert.equal(ambientParts.length, 1, "consecutive web__run calls should batch into one codex.ambient card");
    assert.equal(ambientParts[0].content_card?.semantic_role, "ambient");
    assert.equal(ambientParts[0].content_card?.renderer, "compact_action");
    assert.equal(ambientParts[0].text, 'Search "AssetIWeave Tauri"\nSearch "Codex custom_tool_call"');

    assert.equal(
      parts.some((p) => /^(?:custom_tool_call|function_call):/.test(String(p.text ?? ""))),
      false,
      "empty exec / bare control tool names must not persist",
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

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

test("Codex adapter collapses consecutive shell read/search exec_command calls and web__run into one codex.ambient card while keeping unsafe/skill/non-read commands separate", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "codex-ambient-shell-"));
  try {
    const rolloutPath = path.join(fixtureRoot, "rollout.jsonl");
    writeFileSync(rolloutPath, [
      JSON.stringify({
        timestamp: "2026-11-15T10:00:00Z",
        type: "session_meta",
        payload: { id: "thread-ambient-shell", cwd: "/tmp/project" },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:01Z",
        type: "response_item",
        payload: {
          type: "message",
          role: "user",
          content: [{ type: "input_text", text: "Explore project" }],
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:02Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "exec_command",
          call_id: "call-sed-1",
          arguments: JSON.stringify({ cmd: "sed -n '1,20p' ./src/a.ts", workdir: "/tmp/project" }),
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:03Z",
        type: "response_item",
        payload: {
          type: "function_call_output",
          call_id: "call-sed-1",
          output: "Chunk ID: 1\nProcess exited with code 0\nOutput:\nline1\nline2",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:04Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "exec_command",
          call_id: "call-rg-1",
          arguments: JSON.stringify({ cmd: 'rg -n "handler" src', workdir: "/tmp/project" }),
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:05Z",
        type: "response_item",
        payload: {
          type: "function_call_output",
          call_id: "call-rg-1",
          output: "Chunk ID: 2\nProcess exited with code 0\nOutput:\nsrc/a.ts:3:handler",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:06Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call",
          name: "exec",
          call_id: "call-web-ambient",
          input: 'const r = await tools.web__run({search_query:[{q:"compact action"}]}); text(r);',
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:07Z",
        type: "response_item",
        payload: {
          type: "custom_tool_call_output",
          call_id: "call-web-ambient",
          output: "ok",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:08Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "exec_command",
          call_id: "call-head-1",
          arguments: JSON.stringify({ cmd: "head -n 30 ./src/b.ts", workdir: "/tmp/project" }),
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:09Z",
        type: "response_item",
        payload: {
          type: "function_call_output",
          call_id: "call-head-1",
          output: "Chunk ID: 3\nProcess exited with code 0\nOutput:\nhead lines",
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:10Z",
        type: "response_item",
        payload: {
          type: "function_call",
          name: "exec_command",
          call_id: "call-git-status",
          arguments: JSON.stringify({ cmd: "git status", workdir: "/tmp/project" }),
        },
      }),
      JSON.stringify({
        timestamp: "2026-11-15T10:00:11Z",
        type: "response_item",
        payload: {
          type: "function_call_output",
          call_id: "call-git-status",
          output: "Chunk ID: 4\nProcess exited with code 0\nOutput:\nclean",
        },
      }),
    ].join("\n"));

    runSqlite(fixtureRoot, [
      "CREATE TABLE threads (id TEXT, rollout_path TEXT, title TEXT, updated_at TEXT);",
      `INSERT INTO threads VALUES ('thread-ambient-shell', '${sqlString(rolloutPath)}', 'Fixture', '2026-11-15T10:00:12Z');`,
    ].join("\n"));

    const session = readFixtureSession(fixtureRoot);
    const parts = session.turns[0].parts;

    const ambientCards = parts.filter((p) => p.content_card?.kind === "codex.ambient");
    assert.equal(ambientCards.length, 1);
    assert.deepEqual(ambientCards[0].text.split("\n"), [
      "Read /tmp/project/src/a.ts#L1-20",
      'Search "handler" @ /tmp/project/src',
      'Search "compact action"',
      "Read /tmp/project/src/b.ts#L1-30",
    ]);
    assert.equal(ambientCards[0].content_card?.renderer, "compact_action");
    assert.equal(JSON.parse(ambientCards[0].metadata_json).batch_count, 4);

    const commands = parts.filter((p) => p.kind === "command");
    assert.equal(commands.length, 1);
    assert.equal(commands[0].command, "git status");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});
