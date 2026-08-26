import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import path from "node:path";
import shellProjector from "../builtin-assets/adapters/common/shell-projector.cjs";

const {
  SHELL_PROJECTOR_SCHEMA_VERSION,
  SHELL_PROJECTOR_VERSION,
  projectCommandParts,
} = shellProjector;

test("projects a batch without mutating raw command Parts", () => {
  const parts = [{ part_id: "part-1", command: "printf '%s\\n' '--- tests ---' && pnpm test" }];
  const before = structuredClone(parts);
  const result = projectCommandParts(parts);
  assert.deepEqual(parts, before);
  assert.deepEqual(result, [{
    part_id: "part-1",
    schema_version: SHELL_PROJECTOR_SCHEMA_VERSION,
    projector_version: SHELL_PROJECTOR_VERSION,
    nodes: [{ display_order: 0, command: "pnpm test", command_label: "tests" }],
  }]);
});

test("associates labels in source order before, between, and after commands", () => {
  const result = projectCommandParts([
    { id: "before", command: "printf '%s\\n' '--- first ---'; git status --short" },
    { id: "middle", command: "git diff; echo '=== second ==='; git log -1" },
    { id: "end", command: "git status --short; printf '%s\\n' '--- done ---'" },
  ]);
  assert.deepEqual(result.map(({ nodes }) => nodes), [
    [{ display_order: 0, command: "git status --short", command_label: "first" }],
    [
      { display_order: 0, command: "git diff" },
      { display_order: 1, command: "git log -1", command_label: "second" },
    ],
    [{ display_order: 0, command: "git status --short" }],
  ]);
});

test("an unlabeled divider does not clear a pending label", () => {
  const [{ nodes }] = projectCommandParts([{
    part_id: "nested-divider",
    command: "printf '%s\\n' '--- inspect ---'; echo '===='; git status --short",
  }]);
  assert.deepEqual(nodes, [{
    display_order: 0,
    command: "git status --short",
    command_label: "inspect",
  }]);
});

test("preserves quoted operators, pipelines, and redirections", () => {
  const command = [
    `node -e "console.log('a && b')"`,
    `printf '%s' '--- inspect ---'`,
    `rg 'quoted && value' ./src | sed 's/;/|/'`,
    `git status --short > /tmp/status.txt`,
  ].join(" && ");
  const [{ nodes }] = projectCommandParts([{ part_id: "quoted", command }]);
  assert.deepEqual(nodes, [
    { display_order: 0, command: `node -e "console.log('a && b')"` },
    { display_order: 1, command: `rg 'quoted && value' ./src | sed 's/;/|/'`, command_label: "inspect" },
    { display_order: 2, command: "git status --short > /tmp/status.txt" },
  ]);
});

test("exposes the projector through the adapter NDJSON protocol", () => {
  const adapterPath = path.resolve(import.meta.dirname, "../builtin-assets/adapters/codex/adapter.mjs");
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({
      method: "project_command_parts",
      params: { parts: [{ part_id: "protocol-1", command: "echo '--- inspect ---'; git status --short" }] },
    }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  assert.deepEqual(messages[0], {
    type: "item",
    item: {
      kind: "command_projection",
      part_id: "protocol-1",
      schema_version: SHELL_PROJECTOR_SCHEMA_VERSION,
      projector_version: SHELL_PROJECTOR_VERSION,
      nodes: [{ display_order: 0, command: "git status --short", command_label: "inspect" }],
    },
  });
  assert.deepEqual(messages.at(-1), {
    type: "complete",
    item: { projection_count: 1, projector_version: SHELL_PROJECTOR_VERSION },
  });
});

test("all built-in adapters expose the same projection protocol", () => {
  const adapters = [
    "antigravity", "chatgpt-web", "claude-code", "codex", "gemini-web", "opencode", "qwen-web", "zcode",
  ];
  for (const adapter of adapters) {
    const extension = ["chatgpt-web", "gemini-web", "qwen-web"].includes(adapter) ? "js" : "mjs";
    const adapterPath = path.resolve(import.meta.dirname, `../builtin-assets/adapters/${adapter}/adapter.${extension}`);
    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "project_command_parts", params: { parts: [{ id: adapter, command: "git status --short" }] } }),
    });
    assert.equal(result.status, 0, `${adapter}: ${result.stderr}`);
    const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
    assert.equal(messages[0].item.kind, "command_projection", adapter);
    assert.equal(messages[0].item.part_id, adapter, adapter);
    assert.deepEqual(messages[0].item.nodes, [{ display_order: 0, command: "git status --short" }], adapter);
  }
});
