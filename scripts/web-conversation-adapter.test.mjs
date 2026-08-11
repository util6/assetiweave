import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");

for (const adapterId of ["chatgpt-web", "gemini-web", "qwen-web"]) {
  test(`${adapterId} emits file changes as independent Diff cards`, () => {
    const fixtureRoot = mkdtempSync(path.join(tmpdir(), `assetiweave-${adapterId}-file-change-`));
    try {
      writeFileSync(path.join(fixtureRoot, "sessions.json"), JSON.stringify({
        sessions: [{
          external_id: "session-1",
          title: "File change fixture",
          turns: [{
            external_id: "turn-1",
            turn_index: 0,
            user_text: "修改入口文件",
            parts: [
              {
                role: "tool",
                kind: "file_change",
                text: [
                  "diff --git a/src/main.ts b/src/main.ts",
                  "--- a/src/main.ts",
                  "+++ b/src/main.ts",
                  "@@ -1 +1 @@",
                  "-old",
                  "+new",
                  "diff --git a/src/other.ts b/src/other.ts",
                  "--- a/src/other.ts",
                  "+++ b/src/other.ts",
                  "@@ -1 +1 @@",
                  "-before",
                  "+after",
                ].join("\n"),
                source_execution_id: "call-1",
                metadata_json: JSON.stringify({
                  content_card: { type: "result", format: "plain" },
                }),
              },
              {
                role: "tool",
                kind: "tool",
                text: "{\"ok\":true}",
                content_card: {
                  schema_version: 1,
                  kind: `${adapterId}.result`,
                  renderer: "json",
                },
              },
            ],
          }],
        }],
      }));

      const adapterPath = path.join(
        repositoryRoot,
        "builtin-assets",
        "adapters",
        adapterId,
        "adapter.js",
      );
      const result = spawnSync(process.execPath, [adapterPath], {
        encoding: "utf8",
        input: JSON.stringify({
          method: "read_session",
          source: { location: fixtureRoot },
          params: { session_id: "session-1" },
        }),
      });
      assert.equal(result.status, 0, result.stderr);
      const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
      const session = messages.find((message) => message.type === "item")?.item?.session;
      assert.ok(session);
      assert.equal(session.turns[0].parts.length, 3);
      assert.deepEqual(session.turns[0].parts[0].content_card, {
        schema_version: 1,
        kind: `${adapterId}.file-change`,
        renderer: "diff",
      });
      assert.equal(session.turns[0].parts[0].source_execution_id, "call-1");
      assert.match(session.turns[0].parts[0].body ?? session.turns[0].parts[0].text, /src\/main\.ts/);
      assert.equal(session.turns[0].parts[1].content_card.kind, `${adapterId}.file-change`);
      assert.match(session.turns[0].parts[1].text, /src\/other\.ts/);
      assert.deepEqual(session.turns[0].parts[2].content_card, {
        schema_version: 1,
        kind: `${adapterId}.result`,
        renderer: "json",
      });
    } finally {
      rmSync(fixtureRoot, { force: true, recursive: true });
    }
  });
}
