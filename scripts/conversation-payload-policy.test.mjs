import assert from "node:assert/strict";
import test from "node:test";
import { normalizeSessionPayload, PAYLOAD_POLICY_VERSION } from "../builtin-assets/adapters/opencode/payload-policy.mjs";

function card(type, renderer = type === "command" ? "command" : "terminal_output") {
  return { schema_version: 1, kind: `opencode.${type}`, renderer };
}

function part(input) {
  return {
    role: "tool",
    kind: input.kind ?? "tool",
    text: input.text ?? null,
    command: input.command ?? null,
    cwd: input.cwd ?? "/tmp/project",
    status: input.status ?? null,
    exit_code: input.exit_code ?? null,
    command_label: input.command_label ?? null,
    source_execution_id: input.source_execution_id ?? null,
    content_card: input.content_card ?? card(input.cardType ?? "result"),
    metadata_json: JSON.stringify(input.metadata ?? {}),
  };
}

test("payload policy keeps read/search pairs positional and removes low-value bodies", () => {
  const session = { turns: [{ parts: [
    part({ cardType: "command", command: "sed -n '1,20p' ./src/main.ts", source_execution_id: "read-1" }),
    part({ text: "1: const noisy = true", source_execution_id: "read-1", status: "completed", exit_code: 0 }),
    part({ cardType: "command", command: "rg TODO ./src", source_execution_id: "search-1" }),
    part({ text: "src/main.ts:1:TODO", source_execution_id: "search-1", status: "completed", exit_code: 0 }),
  ] }] };
  const before = session.turns[0].parts.length;

  normalizeSessionPayload(session);

  const parts = session.turns[0].parts;
  assert.equal(parts.length, before);
  assert.equal(parts[0].command, "/tmp/project/src/main.ts:1-20");
  assert.equal(parts[0].status, "completed");
  assert.equal(parts[0].exit_code, 0);
  assert.equal(parts[1].text, null);
  assert.equal(parts[3].text, null);
  assert.equal(JSON.parse(parts[0].metadata_json).payload_policy_version, PAYLOAD_POLICY_VERSION);
});

test("payload policy keeps only diagnostics for failed shell executions", () => {
  const session = { turns: [{ parts: [
    part({ cardType: "command", command: "npm test", source_execution_id: "shell-1" }),
    part({ text: "stdout that is not useful\nError: failed", source_execution_id: "shell-1", status: "failed", exit_code: 1, metadata: { stderr: "\u001b[31mENOENT\u001b[0m" } }),
  ] }] };

  normalizeSessionPayload(session);

  assert.equal(session.turns[0].parts[0].status, "failed");
  assert.equal(session.turns[0].parts[0].exit_code, 1);
  assert.equal(session.turns[0].parts[1].text, "ENOENT");
});

test("payload policy hides successful shell output while backfilling command status", () => {
  const session = { turns: [{ parts: [
    part({ cardType: "command", command: "pnpm test", source_execution_id: "shell-ok" }),
    part({ text: "388 tests passed", source_execution_id: "shell-ok", status: "completed", exit_code: 0 }),
  ] }] };

  normalizeSessionPayload(session);

  assert.equal(session.turns[0].parts[0].status, "completed");
  assert.equal(session.turns[0].parts[0].exit_code, 0);
  assert.equal(session.turns[0].parts[1].text, null);
});

test("payload policy omits decorative print separators when splitting chained shell commands", () => {
  const session = { turns: [{ parts: [
    part({
      cardType: "command",
      command: [
        "printf '%s\\n' '--- lint ---'",
        "pnpm lint",
        "printf '\\n--- TESTS ---\\n'",
        "pnpm test",
        "echo '=== done ==='",
      ].join(" && "),
      command_label: "exec",
      source_execution_id: "shell-chain",
    }),
    part({ text: "done", source_execution_id: "shell-chain", status: "completed", exit_code: 0 }),
  ] }] };

  normalizeSessionPayload(session);

  const parts = session.turns[0].parts;
  assert.equal(parts.length, 3);
  assert.deepEqual(parts.map((item) => item.command), [
    "pnpm lint",
    "pnpm test",
    null,
  ]);
  assert.deepEqual(parts.map((item) => item.command_label), ["lint", "TESTS", null]);
  assert.deepEqual(parts.map((item) => item.source_execution_id), [
    "shell-chain:command:1",
    "shell-chain:command:2",
    "shell-chain:command:2",
  ]);
  assert.equal(parts[2].text, null);
  for (const [index, commandPart] of parts.slice(0, 2).entries()) {
    const metadata = JSON.parse(commandPart.metadata_json);
    assert.equal(metadata.source_execution_parent_id, "shell-chain");
    assert.equal(metadata.command_index, index + 1);
    assert.equal(metadata.command_count, 2);
  }
});

test("payload policy preserves the original execution ID when one real command remains after separator filtering", () => {
  const session = { turns: [{ parts: [
    part({
      cardType: "command",
      command: "printf '%s\\n' '--- status ---'; git status --short",
      command_label: "exec",
      source_execution_id: "shell-single",
    }),
    part({ text: "clean", source_execution_id: "shell-single", status: "completed", exit_code: 0 }),
  ] }] };

  normalizeSessionPayload(session);

  const parts = session.turns[0].parts;
  assert.equal(parts.length, 2);
  assert.equal(parts[0].command, "git status --short");
  assert.equal(parts[0].command_label, "status");
  assert.equal(parts[0].source_execution_id, "shell-single");
  assert.equal(parts[1].source_execution_id, "shell-single");
  assert.equal(parts[1].text, null);
});

test("payload policy retains printf and echo commands that carry non-decorative values", () => {
  const session = { turns: [{ parts: [part({
    cardType: "command",
    command: [
      "printf '%s\\n' '--- paths ---'",
      "printf '%s\\n' \"$tmpdir\"",
      "echo 'build completed'",
    ].join("; "),
    command_label: "exec",
    source_execution_id: "shell-output",
  })] }] };

  normalizeSessionPayload(session);

  assert.deepEqual(session.turns[0].parts.map((item) => item.command), [
    "printf '%s\\n' \"$tmpdir\"",
    "echo 'build completed'",
  ]);
  assert.deepEqual(session.turns[0].parts.map((item) => item.command_label), ["paths", "exec"]);
});

test("payload policy filters an unlabeled divider without replacing the original command label", () => {
  const session = { turns: [{ parts: [part({
    cardType: "command",
    command: "echo '---'; pnpm test",
    command_label: "exec",
    source_execution_id: "shell-divider",
  })] }] };

  normalizeSessionPayload(session);

  assert.equal(session.turns[0].parts.length, 1);
  assert.equal(session.turns[0].parts[0].command, "pnpm test");
  assert.equal(session.turns[0].parts[0].command_label, "exec");
});

test("payload policy splits only top-level separators and preserves quoted operators and pipelines", () => {
  const session = { turns: [{ parts: [
    part({
      cardType: "command",
      command: `node -e "console.log('a && b')" && printf 'a;b' |\n  sed 's/;/|/'\npnpm test`,
      source_execution_id: "shell-quoted",
    }),
  ] }] };

  normalizeSessionPayload(session);

  const parts = session.turns[0].parts;
  assert.deepEqual(parts.map((item) => item.command), [
    `node -e "console.log('a && b')"`,
    "printf 'a;b' |\n  sed 's/;/|/'",
    "pnpm test",
  ]);
});

test("payload policy keeps heredoc and shell control-flow scripts as one command", () => {
  const scripts = [
    "python3 - <<'PY'\nprint('first; second && third')\nPY",
    "for file in a b; do echo \"$file\"; done",
  ];

  for (const [index, command] of scripts.entries()) {
    const session = { turns: [{ parts: [part({
      cardType: "command",
      command,
      source_execution_id: `shell-script-${index}`,
    })] }] };

    normalizeSessionPayload(session);

    assert.equal(session.turns[0].parts.length, 1);
    assert.equal(session.turns[0].parts[0].command, command);
  }
});

test("payload policy marks ambiguous execution results as unclassified", () => {
  const session = { turns: [{ parts: [
    part({ text: "opaque tool payload", source_execution_id: "opaque-1", metadata: {} }),
  ] }] };

  normalizeSessionPayload(session);

  const metadata = JSON.parse(session.turns[0].parts[0].metadata_json);
  assert.equal(metadata.execution_kind, "unclassified");
  assert.equal(session.turns[0].parts[0].text, "opaque tool payload");
});

test("payload policy does not mistake todowrite results for file changes", () => {
  const session = { turns: [{ parts: [part({
    text: "更新任务列表\n验证测试结果",
    source_execution_id: "todo-1",
    content_card: card("result", "plain"),
    metadata: { source_type: "tool", tool: "todowrite" },
  })] }] };

  normalizeSessionPayload(session);

  const [normalizedPart] = session.turns[0].parts;
  assert.equal(normalizedPart.kind, "tool");
  assert.equal(normalizedPart.text, "更新任务列表\n验证测试结果");
  assert.equal(normalizedPart.content_card.renderer, "plain");
  assert.equal(JSON.parse(normalizedPart.metadata_json).execution_kind, "unclassified");
});

test("payload policy preserves ambiguous completed results without an exact command ID", () => {
  const session = { turns: [{ parts: [
    part({
      text: "completed payload whose semantics are unknown",
      source_execution_id: "opaque-completed",
      status: "completed",
      exit_code: 0,
      metadata: {},
    }),
  ] }] };

  normalizeSessionPayload(session);

  assert.equal(session.turns[0].parts[0].text, "completed payload whose semantics are unknown");
  assert.equal(JSON.parse(session.turns[0].parts[0].metadata_json).execution_kind, "unclassified");
});

test("payload policy splits one multi-file diff into one file-change Part per file", () => {
  const diff = [
    "Script completed",
    "diff --git a/a.txt b/a.txt",
    "--- a/a.txt",
    "+++ b/a.txt",
    "@@ -1 +1 @@",
    "-old",
    "+new",
    "diff --git a/image.png b/image.png",
    "Binary files a/image.png and b/image.png differ",
    "Wall time 0.1 seconds",
  ].join("\n");
  const session = { turns: [{ parts: [
    part({ kind: "file_change", text: diff, source_execution_id: "patch-1", content_card: card("result") }),
  ] }] };

  normalizeSessionPayload(session);

  const parts = session.turns[0].parts;
  assert.equal(parts.length, 2);
  assert.match(parts[0].text, /^diff --git a\/a\.txt b\/a\.txt/m);
  assert.doesNotMatch(parts[0].text, /Binary files a\/image\.png and b\/image\.png differ/);
  assert.match(parts[1].text, /^diff --git a\/image\.png b\/image\.png/m);
  assert.match(parts[1].text, /Binary files a\/image\.png and b\/image\.png differ/);
  for (const [index, part] of parts.entries()) {
    assert.equal(part.content_card.kind, "opencode.file-change");
    assert.equal(part.content_card.renderer, "diff");
    assert.equal(part.source_execution_id, "patch-1");
    const metadata = JSON.parse(part.metadata_json);
    assert.equal(metadata.file_change_index, index + 1);
    assert.equal(metadata.file_change_count, 2);
    assert.equal(metadata.file_path, index === 0 ? "a.txt" : "image.png");
    assert.equal(part.text.includes("Script completed"), false);
    assert.equal(part.text.includes("Wall time"), false);
  }
});

test("payload policy splits headerless files without treating hunk content as file headers", () => {
  const session = { turns: [{ parts: [part({
    kind: "file_change",
    source_execution_id: "patch-headerless",
    content_card: card("result"),
    text: [
      "--- a/first.txt",
      "+++ b/first.txt",
      "@@ -1,2 +1,2 @@",
      "--- old content",
      "+++ new content",
      "--- a/second.txt",
      "+++ b/second.txt",
      "@@ -1 +1 @@",
      "-before",
      "+after",
    ].join("\n"),
  })] }] };

  normalizeSessionPayload(session);

  assert.equal(session.turns[0].parts.length, 2);
  assert.match(session.turns[0].parts[0].text, /--- old content\n\+\+\+ new content/);
  assert.doesNotMatch(session.turns[0].parts[0].text, /second\.txt/);
  assert.match(session.turns[0].parts[1].text, /second\.txt/);
});

test("payload policy keeps an empty file change hidden behind a valid Result descriptor", () => {
  const session = { turns: [{ parts: [
    part({
      kind: "file_change",
      text: "   ",
      source_execution_id: "patch-empty",
      content_card: card("result", "diff"),
    }),
  ] }] };

  normalizeSessionPayload(session);

  const result = session.turns[0].parts[0];
  assert.equal(result.text, null);
  assert.equal(result.content_card.kind, "opencode.result");
  assert.equal(result.content_card.renderer, "terminal_output");
});

test("payload policy reconstructs file headers for Antigravity diff blocks", () => {
  const session = { turns: [{ parts: [
    part({
      kind: "file_change",
      text: [
        "Created At: 2026-08-09T00:00:00Z",
        "The following changes were made by the edit tool to: /tmp/project/src/main.ts. If relevant, run checks.",
        "[diff_block_start]",
        "@@ -1 +1 @@",
        "-old",
        "+new",
        "[diff_block_end]",
        "surrounding log",
      ].join("\n"),
      content_card: card("result"),
      metadata: { source_type: "CODE_ACTION" },
    }),
  ] }] };

  normalizeSessionPayload(session);

  assert.match(session.turns[0].parts[0].text, /^diff --git a\/tmp\/project\/src\/main\.ts b\/tmp\/project\/src\/main\.ts/);
  assert.match(session.turns[0].parts[0].text, /@@ -1 \+1 @@\n-old\n\+new/);
  assert.equal(session.turns[0].parts[0].content_card.kind, "opencode.file-change");
  assert.equal(session.turns[0].parts[0].text.includes("surrounding log"), false);
});

test("payload policy reduces one-megabyte read and successful stdout payloads by at least 95 percent", () => {
  const oneMegabyte = "x".repeat(1024 * 1024);
  const diff = [
    "diff --git a/src/main.ts b/src/main.ts",
    "--- a/src/main.ts",
    "+++ b/src/main.ts",
    "@@ -1 +1 @@",
    "-old",
    "+new",
  ].join("\n");
  const session = { turns: [{ parts: [
    part({ cardType: "command", command: "cat src/main.ts", source_execution_id: "read-large" }),
    part({ text: oneMegabyte, source_execution_id: "read-large", status: "completed", exit_code: 0 }),
    part({ cardType: "command", command: "pnpm test", source_execution_id: "shell-large" }),
    part({ text: oneMegabyte, source_execution_id: "shell-large", status: "completed", exit_code: 0 }),
    part({ kind: "file_change", text: diff, source_execution_id: "patch-large", content_card: card("result") }),
  ] }] };
  const beforeBytes = session.turns[0].parts.reduce((total, candidate) => total + Buffer.byteLength(candidate.text ?? ""), 0);
  const beforeCount = session.turns[0].parts.length;

  normalizeSessionPayload(session);

  const afterBytes = session.turns[0].parts.reduce((total, candidate) => total + Buffer.byteLength(candidate.text ?? ""), 0);
  assert.equal(session.turns[0].parts.length, beforeCount);
  assert.ok(afterBytes <= beforeBytes * 0.05, `${afterBytes} bytes remained from ${beforeBytes}`);
  assert.equal(session.turns[0].parts[1].text, null);
  assert.equal(session.turns[0].parts[3].text, null);
  assert.equal(session.turns[0].parts[4].text, diff);
  assert.equal(session.turns[0].parts[4].content_card.kind, "opencode.file-change");
});
