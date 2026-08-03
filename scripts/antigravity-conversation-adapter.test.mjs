import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "src-tauri/bundled/conversation-adapters/antigravity/adapter.mjs");

test("Antigravity classifies a VIEW_FILE result for SKILL.md as skill content", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-skill-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const skillPath = "/Users/test/.gemini/config/skills/sample/SKILL.md";
    writeFileSync(transcriptPath, [
      JSON.stringify({
        step_index: 0,
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-07-31T00:00:00Z",
        content: "<USER_REQUEST>Read the sample skill</USER_REQUEST>",
      }),
      JSON.stringify({
        step_index: 1,
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-07-31T00:00:01Z",
        content: "",
        tool_calls: [{
          name: "view_file",
          args: {
            toolSummary: "View sample skill",
            AbsolutePath: skillPath,
            IsSkillFile: true,
          },
        }],
      }),
      JSON.stringify({
        step_index: 2,
        source: "MODEL",
        type: "VIEW_FILE",
        status: "DONE",
        created_at: "2026-07-31T00:00:02Z",
        content: [
          "Created At: 2026-07-31T00:00:02Z",
          "Completed At: 2026-07-31T00:00:02Z",
          `File Path: \`file://${skillPath}\``,
          "Total Lines: 5",
          "The following code has been modified to include a line number before every line, in the format: <line_number>: <original_line>.",
          "1: ---",
          "2: name: sample-skill",
          "3: description: A sample skill.",
          "4: ---",
          "5: # Sample Skill",
          "The above content shows the entire, complete file contents of the requested file.",
        ].join("\n"),
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.deepEqual(
      session.turns[0].parts.map((part) => ({
        partKind: part.kind,
        kind: part.content_card?.kind,
        renderer: part.content_card?.renderer,
        text: part.text,
      })),
      [
        {
          partKind: "tool",
          kind: "antigravity.tool",
          renderer: "plain",
          text: "Tool: View sample skill\n\nAbsolutePath: /Users/test/.gemini/config/skills/sample/SKILL.md\nIsSkillFile: true",
        },
        {
          partKind: "metadata",
          kind: "antigravity.skill-content",
          renderer: "markdown",
          text: "---\nname: sample-skill\ndescription: A sample skill.\n---\n# Sample Skill",
        },
      ],
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity keeps a non-SKILL.md VIEW_FILE result as a result card", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-file-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-07-31T00:00:00Z",
        content: "<USER_REQUEST>Read a text file</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "VIEW_FILE",
        status: "DONE",
        created_at: "2026-07-31T00:00:01Z",
        content: "File Path: `file:///tmp/README.md`\n1: hello",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.equal(session.turns[0].parts[0].content_card.kind, "antigravity.result");
    assert.equal(session.turns[0].parts[0].content_card.renderer, "terminal_output");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity removes command runner headers and terminal control sequences", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-result-text-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-02T00:00:00Z",
        content: "<USER_REQUEST>Run a check</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "RUN_COMMAND",
        status: "DONE",
        created_at: "2026-08-02T00:00:01Z",
        content: [
          "Created At: 2026-08-02T00:00:01Z",
          "Completed At: 2026-08-02T00:00:02Z",
          "",
          "The command completed successfully.",
          "Output:",
          JSON.stringify([
            { type: "input_text", text: "\u001b[32mcheck passed\u001b[0m" },
          ]),
        ].join("\n"),
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const result = session.turns[0].parts[0];
    assert.equal(result.text, "check passed");
    assert.equal(result.content_card.renderer, "terminal_output");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity normalizes structured command output before applying text budgets", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-large-result-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const terminalOutput = `visible output line\n${"detail line\n".repeat(3_000)}`;
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-03T00:00:00Z",
        content: "<USER_REQUEST>Run a large check</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "RUN_COMMAND",
        status: "DONE",
        created_at: "2026-08-03T00:00:01Z",
        content: [
          "Script completed",
          "Wall time 0.1 seconds",
          "Output:",
          JSON.stringify([
            { type: "input_text", text: terminalOutput },
          ]),
        ].join("\n"),
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const result = session.turns[0].parts[0];
    assert.ok(result.text.startsWith("visible output line\n"));
    assert.equal(result.text.includes("input_text"), false);
    assert.equal(result.text.startsWith("[{"), false);
    assert.equal(result.content_card.renderer, "terminal_output");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function readFixtureSession(transcriptPath) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify({ method: "read_session", source: { location: transcriptPath }, params: {} }),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").map((line) => JSON.parse(line));
  const session = messages.find((message) => message.type === "item")?.item?.session;
  assert.ok(session);
  return session;
}
