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

test("Codex adapter filters out subagent threads from list_sessions and read_session", () => {
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

    // 1. list_sessions should only return user-session-1
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
      .map((m) => m.item.external_id);
    assert.deepEqual(listItems, ["user-session-1"]);

    // 2. read_session for user session should succeed
    const readUserResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "read_session",
        source: { location: fixtureRoot },
        params: { session_id: "user-session-1" },
      }),
    });
    assert.equal(readUserResult.status, 0, readUserResult.stderr);
    const userSession = readUserResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .find((m) => m.type === "item")?.item?.session;
    assert.ok(userSession);
    assert.equal(userSession.external_id, "user-session-1");
    assert.equal(userSession.turns.length, 2);
    assert.equal(userSession.turns[0].user_text, "User question");
    assert.equal(userSession.turns[1].turn_kind, "subagent");
    assert.equal(userSession.turns[1].agent_role, null, "缺少子代理元数据时不得伪造角色");
    assert.equal(userSession.turns[1].parent_turn_id, userSession.turns[0].external_id);
    assert.equal(userSession.turns[1].user_text, "User question fork");

    // 3. read_session for subagent should return empty
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
    assert.equal(subItems.length, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter multi-turn session keeps subsequent question as standard user turn", () => {
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

    const readUserResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "read_session",
        source: { location: fixtureRoot },
        params: { session_id: "user-session-multi" },
      }),
    });
    assert.equal(readUserResult.status, 0, readUserResult.stderr);
    const userSession = readUserResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .find((m) => m.type === "item")?.item?.session;
    assert.ok(userSession);
    assert.equal(userSession.turns.length, 3);

    // Turn 0: Question 1
    assert.equal(userSession.turns[0].user_text, "Question 1: Fix bug");
    assert.equal(userSession.turns[0].turn_kind, undefined);

    // Turn 1: Subagent
    assert.equal(userSession.turns[1].turn_kind, "subagent");
    assert.equal(userSession.turns[1].agent_role, null, "缺少子代理元数据时不得伪造角色");
    assert.equal(userSession.turns[1].parent_turn_id, userSession.turns[0].external_id);
    assert.equal(userSession.turns[1].user_text, "Subagent task: analyze crash dump");

    // Turn 2: Question 2 (MUST remain standard user turn, NOT continuation!)
    assert.equal(userSession.turns[2].user_text, "Question 2: What is the weather today in Tokyo?");
    assert.equal(userSession.turns[2].turn_kind, undefined, "Question 2 turn_kind must be undefined");
    assert.equal(userSession.turns[2].parent_turn_id, undefined, "Question 2 parent_turn_id must be undefined");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Codex adapter falls back to valid string when subagent prompt is whitespace-only", () => {
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

    const readUserResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "read_session",
        source: { location: fixtureRoot },
        params: { session_id: "user-session-ws" },
      }),
    });
    assert.equal(readUserResult.status, 0, readUserResult.stderr);
    const userSession = readUserResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .find((m) => m.type === "item")?.item?.session;
    assert.ok(userSession);
    const fallbackTurn = userSession.turns[1];
    assert.equal(fallbackTurn.turn_kind, "subagent");
    assert.equal(fallbackTurn.user_text, "Subagent execution");
    assert.ok(fallbackTurn.user_text.trim().length > 0);
    assert.ok(
      readUserResult.stdout
        .trim()
        .split("\n")
        .map((l) => JSON.parse(l))
        .some((message) => message.type === "warning" && /placeholder/.test(message.message ?? "")),
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
    const session = firstRead.find((m) => m.type === "item")?.item?.session;
    assert.ok(session);

    const subagentTurns = session.turns.filter((turn) => turn.turn_kind === "subagent");
    assert.equal(subagentTurns.length, 3, "每个子代理线程只应产出一个 Turn");
    assert.equal(session.turns.length, 4);

    const [auditTurn, concurrencyTurn, injectedTurn] = subagentTurns;

    // 角色来自 thread_spawn.agent_path 末段，昵称进 title
    assert.equal(auditTurn.agent_role, "guard_audit");
    assert.equal(auditTurn.title, "Lagrange");
    assert.equal(concurrencyTurn.agent_role, "concurrency_audit");
    assert.equal(concurrencyTurn.title, "Noether");

    // 派发提示词跳过系统注入块，取真实任务文本
    assert.equal(auditTurn.user_text, "审查 SQLite 迁移脚本是否存在冗余索引");
    assert.ok(!auditTurn.user_text.includes("recommended_plugins"));
    assert.ok(!auditTurn.user_text.includes("environment_context"));
    assert.equal(concurrencyTurn.user_text, "检查并发锁的获取顺序");
    assert.ok(!concurrencyTurn.user_text.includes("user_instructions"));

    // 两个不同子代理不应产生相同的提示词
    assert.notEqual(auditTurn.user_text, concurrencyTurn.user_text);

    // 子代理内部逐轮执行内容合并为该 Turn 的内容事实，最终汇报在末尾
    assert.deepEqual(
      auditTurn.parts.map((part) => part.text),
      ["开始审查迁移脚本。", "审查完成：发现 2 个冗余索引。"]
    );
    assert.deepEqual(
      concurrencyTurn.parts.map((part) => part.text),
      ["并发审计完成：未发现锁顺序反转。"]
    );
    assert.ok(auditTurn.parts.length > 0);
    assert.equal(auditTurn.parent_turn_id, session.turns[0].external_id);

    // 元数据缺失时角色留空且不伪造
    assert.ok(!injectedTurn.agent_role, "缺少 thread_spawn 角色时必须留空");
    assert.equal(injectedTurn.title ?? null, null);
    assert.equal(injectedTurn.user_text, "Subagent execution");
    assert.ok(
      firstRead.some((message) => message.type === "warning" && /placeholder/.test(message.message ?? "")),
      "无可用任务文本时必须发出告警事件"
    );

    // 重复同步幂等：external_id 稳定
    const secondRead = readSessionMessages(fixtureRoot, "main-merge");
    const secondSession = secondRead.find((m) => m.type === "item")?.item?.session;
    assert.deepEqual(
      secondSession.turns.map((turn) => turn.external_id),
      session.turns.map((turn) => turn.external_id)
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
    const session = read.find((m) => m.type === "item")?.item?.session;
    assert.ok(session);
    assert.equal(session.turns.length, 3, "主 Turn + 两层子代理 Turn");

    const subagentTurns = session.turns.filter((turn) => turn.turn_kind === "subagent");
    assert.equal(subagentTurns.length, 2, "每个子线程只产出一个 Turn");

    const mid = subagentTurns.find((turn) => turn.agent_role === "orchestrator");
    const deep = subagentTurns.find((turn) => turn.agent_role === "reviewer");
    assert.ok(mid, "第一层子代理必须存在");
    assert.ok(deep, "第二层子代理必须存在");
    assert.equal(mid.parent_turn_id, session.turns[0].external_id);
    assert.equal(deep.parent_turn_id, mid.external_id, "第二层必须挂在其派发 Turn 之下");
    assert.ok(
      deep.parts.some((part) => /底层审查完成/.test(part.text ?? "")),
      "第二层必须带自己的执行内容"
    );
    assert.ok(
      mid.external_id.startsWith(`${session.turns[0].external_id}-subagent-`),
      "子代理 Turn ID 必须由父 Turn 派生"
    );
    assert.ok(
      deep.external_id.startsWith(`${mid.external_id}-subagent-`),
      "嵌套 Turn ID 沿父链稳定派生"
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
    const session = read.find((m) => m.type === "item")?.item?.session;
    assert.ok(session);

    const subagentTurns = session.turns.filter((turn) => turn.turn_kind === "subagent");
    assert.equal(
      subagentTurns.length,
      5,
      "深度上限 4 时只能展开 5 层子代理 Turn（超限层降级为叶子）"
    );
    assert.ok(
      !subagentTurns.some((turn) => /第 6 层/.test(turn.user_text ?? "")),
      "超限的更深层不得继续展开"
    );
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

test("Codex adapter degrades cyclic subagent thread references to leaf turns", () => {
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
    const session = read.find((m) => m.type === "item")?.item?.session;
    assert.ok(session);

    const subagentTurns = session.turns.filter((turn) => turn.turn_kind === "subagent");
    assert.equal(subagentTurns.length, 3, "环上的重复引用只降级为叶子 Turn，不得无限展开");
    assert.equal(
      new Set(subagentTurns.map((turn) => turn.parent_turn_id)).size,
      3,
      "每个叶子挂在各自的派发 Turn 之下"
    );
    assert.ok(
      read.some(
        (message) =>
          message.type === "warning" && /already expanded \(cycle\)/.test(message.message ?? "")
      ),
      "成环必须发出告警"
    );
    assert.ok(
      subagentTurns.some((turn) => turn.parent_turn_id?.includes("child-cycle-b")),
      "环上的重复节点必须以叶子形式挂在再次引用它的 Turn 之下"
    );
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
