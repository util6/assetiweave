import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const repositoryRoot = path.resolve(import.meta.dirname, "..");
const adapterPath = path.join(repositoryRoot, "builtin-assets/adapters/antigravity/adapter.mjs");

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
    assert.equal(session.turns[0].parts.length, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity omits successful command results with no useful payload", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-empty-command-result-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-02T00:00:00Z",
        content: "<USER_REQUEST>Run a silent check</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-08-02T00:00:01Z",
        content: "",
        tool_calls: [{
          name: "run_command",
          args: { CommandLine: "true" },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "RUN_COMMAND",
        status: "DONE",
        created_at: "2026-08-02T00:00:02Z",
        content: [
          "Script completed",
          "Process exited with code 0",
          "Output:",
        ].join("\n"),
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.deepEqual(
      session.turns[0].parts.map((part) => part.content_card?.kind),
      ["antigravity.command"],
    );
    assert.equal(session.turns[0].parts[0].source_execution_id, "turn-0:run-command:1");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity keeps a RUN_COMMAND execution boundary without persisted display projection", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-shell-projection-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const command = [
      "printf '%s\\n' '--- inspect ---'",
      "rg 'quoted && value' ./src | sed 's/;/|/'",
      "git status --short > /tmp/status.txt",
    ].join(" && ");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-03T00:00:00Z",
        content: "<USER_REQUEST>检查工作区</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-08-03T00:00:01Z",
        content: "",
        tool_calls: [{
          id: "antigravity-shell-projection",
          name: "run_command",
          args: { CommandLine: command },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "RUN_COMMAND",
        status: "ERROR",
        created_at: "2026-08-03T00:00:02Z",
        content: "Error: failed",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const parts = session.turns[0].parts;
    assert.equal(parts.length, 2);
    assert.deepEqual(parts.map((part) => part.source_execution_id), [
      "antigravity-shell-projection",
      "antigravity-shell-projection",
    ]);
    assert.equal(parts[0].command, command);
    assert.equal(parts[1].text, "Error: failed");
    assert.equal(JSON.parse(parts[0].metadata_json).shell_execution_projection, undefined);
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
    assert.equal(session.turns[0].parts.length, 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity reconstructs a concrete diff for a created file from the write tool call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-created-file-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const targetFile = "/tmp/project/created.py";
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-04T00:00:00Z",
        content: "<USER_REQUEST>Create a script</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-08-04T00:00:01Z",
        tool_calls: [{
          name: "write_to_file",
          args: {
            CodeContent: "print(\"hello\")\n",
            Overwrite: true,
            TargetFile: targetFile,
            toolSummary: "Create a Python script",
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "CODE_ACTION",
        status: "DONE",
        created_at: "2026-08-04T00:00:02Z",
        content: `Created file file://${targetFile} with requested content.`,
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const fileChange = session.turns[0].parts.find((part) => part.kind === "file_change");
    assert.ok(fileChange);
    assert.equal(fileChange.content_card.kind, "antigravity.file-change");
    assert.equal(fileChange.content_card.renderer, "diff");
    assert.match(fileChange.text, /^diff --git a\/tmp\/project\/created\.py b\/tmp\/project\/created\.py/m);
    assert.match(fileChange.text, /--- \/dev\/null\n\+\+\+ b\/tmp\/project\/created\.py/);
    assert.match(fileChange.text, /@@ -0,0 \+1,1 @@\n\+print\("hello"\)/);
    assert.doesNotMatch(fileChange.text, /Created file file:\/\//);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity reconstructs a concrete diff for a replaced file from the replace tool call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-replaced-file-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const targetFile = "/tmp/project/main.py";
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-04T00:00:00Z",
        content: "<USER_REQUEST>Update the script</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-08-04T00:00:01Z",
        tool_calls: [{
          name: "replace_file_content",
          args: {
            TargetFile: targetFile,
            TargetContent: "old\n",
            ReplacementContent: "new\n",
            toolSummary: "Update the Python script",
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "CODE_ACTION",
        status: "DONE",
        created_at: "2026-08-04T00:00:02Z",
        content: `Updated file file://${targetFile} with requested content.`,
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const fileChange = session.turns[0].parts.find((part) => part.kind === "file_change");
    assert.ok(fileChange);
    assert.match(fileChange.text, /^diff --git a\/tmp\/project\/main\.py b\/tmp\/project\/main\.py/m);
    assert.match(fileChange.text, /--- a\/tmp\/project\/main\.py\n\+\+\+ b\/tmp\/project\/main\.py/);
    assert.match(fileChange.text, /@@ -1,1 \+1,1 @@\n-old\n\+new/);
    assert.doesNotMatch(fileChange.text, /Updated file file:\/\//);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity treats a repeated write as an update instead of a new file", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-repeated-write-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const targetFile = "/tmp/project/repeated.py";
    const steps = [
      {
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-04T00:00:00Z",
        content: "<USER_REQUEST>Create and refine the script</USER_REQUEST>",
      },
      {
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-08-04T00:00:01Z",
        tool_calls: [{
          name: "write_to_file",
          args: { CodeContent: "print(1)\n", Overwrite: true, TargetFile: targetFile },
        }],
      },
      {
        source: "MODEL",
        type: "CODE_ACTION",
        status: "DONE",
        created_at: "2026-08-04T00:00:02Z",
        content: `Created file file://${targetFile} with requested content.`,
      },
      {
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-08-04T00:00:03Z",
        tool_calls: [{
          name: "write_to_file",
          args: { CodeContent: "print(2)\n", Overwrite: true, TargetFile: targetFile },
        }],
      },
      {
        source: "MODEL",
        type: "CODE_ACTION",
        status: "DONE",
        created_at: "2026-08-04T00:00:04Z",
        content: `Updated file file://${targetFile} with requested content.`,
      },
    ];
    writeFileSync(transcriptPath, steps.map((step) => JSON.stringify(step)).join("\n"));

    const session = readFixtureSession(transcriptPath);
    const fileChanges = session.turns[0].parts.filter((part) => part.kind === "file_change");
    assert.equal(fileChanges.length, 2);
    assert.match(fileChanges[0].text, /new file mode 100644/);
    assert.match(fileChanges[1].text, /--- a\/tmp\/project\/repeated\.py\n\+\+\+ b\/tmp\/project\/repeated\.py/);
    assert.match(fileChanges[1].text, /-print\(1\)\n\+print\(2\)/);
    assert.doesNotMatch(fileChanges[1].text, /new file mode 100644/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});


test("Antigravity automatically discovers sibling IDE and CLI brain directories", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-multibrain-"));
  try {
    const ideDir = path.join(fixtureRoot, "antigravity-ide", "brain", "11111111-1111-1111-1111-111111111111", ".system_generated", "logs");
    const cliDir = path.join(fixtureRoot, "antigravity-cli", "brain", "22222222-2222-2222-2222-222222222222", ".system_generated", "logs");
    mkdirSync(ideDir, { recursive: true });
    mkdirSync(cliDir, { recursive: true });

    writeFileSync(path.join(ideDir, "transcript_full.jsonl"), [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-05T00:00:00Z",
        content: "<USER_REQUEST>IDE session request</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-08-05T00:00:01Z",
        content: "IDE response",
      }),
    ].join("\n"));

    writeFileSync(path.join(cliDir, "transcript_full.jsonl"), [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-05T00:00:00Z",
        content: "<USER_REQUEST>CLI session request</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-08-05T00:00:01Z",
        content: "CLI response",
      }),
    ].join("\n"));

    // Pointing to antigravity-ide/brain should discover both ide and cli sessions
    const ideBrainPath = path.join(fixtureRoot, "antigravity-ide", "brain");
    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: ideBrainPath }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
    const items = messages.filter((message) => message.type === "item");
    assert.equal(items.length, 2);
    const titles = items.map((i) => i.item.session.title).sort();
    assert.deepEqual(titles, ["CLI session request", "IDE session request"]);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});


test("Antigravity parses SQLite conversation database (.db) from Antigravity ACP", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-acp-db-"));
  try {
    const dbPath = path.join(fixtureRoot, "33333333-3333-3333-3333-333333333333.db");
    const metaPath = path.join(fixtureRoot, "33333333-3333-3333-3333-333333333333.meta");
    writeFileSync(metaPath, JSON.stringify({ cwd: "/Users/test/projects/acp-demo", mode_id: "yolo" }));

    // User payload: "Hello from ACP!"
    const userHex = "9a0111120f48656c6c6f2066726f6d2041435021";
    // Assistant payload: "Response from ACP assistant"
    const asstHex = "a2011d0a1b526573706f6e73652066726f6d2041435020617373697374616e74";

    spawnSync("sqlite3", [
      dbPath,
      `CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER, step_payload BLOB);
       INSERT INTO steps VALUES (0, 14, X'${userHex}');
       INSERT INTO steps VALUES (1, 15, X'${asstHex}');`,
    ]);

    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: dbPath }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
    const items = messages.filter((m) => m.type === "item");
    assert.equal(items.length, 1);
    const session = items[0].item.session;
    assert.equal(session.external_id, "33333333-3333-3333-3333-333333333333");
    assert.equal(session.title, "Hello from ACP!");
    assert.equal(session.project_path, "/Users/test/projects/acp-demo");
    assert.equal(session.turns.length, 1);
    assert.equal(session.turns[0].user_text, "Hello from ACP!");
    assert.equal(session.turns[0].parts[0].text, "Response from ACP assistant");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity automatically discovers sibling ACP SQLite conversation databases alongside IDE and CLI", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-all-envs-"));
  try {
    const ideDir = path.join(fixtureRoot, "antigravity-ide", "brain", "11111111-1111-1111-1111-111111111111", ".system_generated", "logs");
    const cliDir = path.join(fixtureRoot, "antigravity-cli", "brain", "22222222-2222-2222-2222-222222222222", ".system_generated", "logs");
    const acpConvDir = path.join(fixtureRoot, "antigravity-acp", "conversations");
    mkdirSync(ideDir, { recursive: true });
    mkdirSync(cliDir, { recursive: true });
    mkdirSync(acpConvDir, { recursive: true });

    writeFileSync(path.join(ideDir, "transcript_full.jsonl"), [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-05T00:00:00Z",
        content: "<USER_REQUEST>IDE session request</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-08-05T00:00:01Z",
        content: "IDE response",
      }),
    ].join("\n"));

    writeFileSync(path.join(cliDir, "transcript_full.jsonl"), [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-08-05T00:00:00Z",
        content: "<USER_REQUEST>CLI session request</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-08-05T00:00:01Z",
        content: "CLI response",
      }),
    ].join("\n"));

    const acpDbPath = path.join(acpConvDir, "33333333-3333-3333-3333-333333333333.db");
    const userHex = "9a0111120f48656c6c6f2066726f6d2041435021";
    const asstHex = "a2011d0a1b526573706f6e73652066726f6d2041435020617373697374616e74";
    spawnSync("sqlite3", [
      acpDbPath,
      `CREATE TABLE steps (idx INTEGER PRIMARY KEY, step_type INTEGER, step_payload BLOB);
       INSERT INTO steps VALUES (0, 14, X'${userHex}');
       INSERT INTO steps VALUES (1, 15, X'${asstHex}');`,
    ]);

    // Pointing to antigravity-ide/brain should discover all 3 environments: IDE, CLI, and ACP
    const ideBrainPath = path.join(fixtureRoot, "antigravity-ide", "brain");
    const result = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({ method: "read_session", source: { location: ideBrainPath }, params: {} }),
    });
    assert.equal(result.status, 0, result.stderr);
    const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
    const items = messages.filter((message) => message.type === "item");
    assert.equal(items.length, 3);
    const ids = items.map((i) => i.item.session.external_id).sort();
    assert.deepEqual(ids, [
      "11111111-1111-1111-1111-111111111111",
      "22222222-2222-2222-2222-222222222222",
      "33333333-3333-3333-3333-333333333333",
    ]);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity correctly extracts exit code and status from real GENERIC run_command output", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-generic-cmd-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-10-03T16:00:00Z",
        content: "<USER_REQUEST>Run failing command</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-10-03T16:00:01Z",
        tool_calls: [{
          name: "run_command",
          args: { CommandLine: "cargo test --bad-arg", Cwd: "/app" },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-03T16:00:02Z",
        content: [
          "Created At: 2026-10-03T16:00:01Z",
          "Completed At: 2026-10-03T16:00:02Z",
          "",
          "The command exited with code 1.",
          "Output:",
          "error: unrecognized option '--bad-arg'",
        ].join("\n"),
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.equal(session.turns[0].parts.length, 2);
    const cmdPart = session.turns[0].parts[0];
    const resPart = session.turns[0].parts[1];

    assert.equal(cmdPart.content_card?.kind, "antigravity.command");
    assert.equal(resPart.content_card?.kind, "antigravity.result");
    assert.equal(resPart.content_card?.renderer, "terminal_output");
    assert.equal(resPart.exit_code, 1);
    assert.equal(resPart.status, "failed");
    assert.equal(resPart.source_execution_id, cmdPart.source_execution_id);
    assert.match(resPart.text, /unrecognized option/);
    assert.doesNotMatch(resPart.text, /Created At:/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity reconstructs concrete diff from real GENERIC replace_file_content output", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-generic-replace-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const targetFile = "/app/src/lib.rs";
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-10-03T16:10:00Z",
        content: "<USER_REQUEST>Fix a bug</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-10-03T16:10:01Z",
        tool_calls: [{
          name: "replace_file_content",
          args: {
            TargetFile: targetFile,
            TargetContent: "let x = 1;\n",
            ReplacementContent: "let x = 2;\n",
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-03T16:10:02Z",
        content: `Successfully replaced content in ${targetFile}`,
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const fileChange = session.turns[0].parts.find((part) => part.kind === "file_change");
    assert.ok(fileChange);
    assert.equal(fileChange.content_card?.kind, "antigravity.file-change");
    assert.equal(fileChange.content_card?.renderer, "diff");
    assert.match(fileChange.text, /^diff --git a\/app\/src\/lib\.rs b\/app\/src\/lib\.rs/m);
    assert.match(fileChange.text, /-let x = 1;/);
    assert.match(fileChange.text, /\+let x = 2;/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity maintains unique execution ids across multiple tool calls in a turn so success is not overwritten by failure", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "ag-turn-multicall-"));
  try {
    const transcriptPath = path.join(fixtureRoot, ".system_generated", "logs", "transcript.jsonl");
    mkdirSync(path.dirname(transcriptPath), { recursive: true });
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-03T16:00:00Z",
        content: "<USER_REQUEST>run checks</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-03T16:00:01Z",
        tool_calls: [{ name: "run_command", args: { CommandLine: "git status", Cwd: "/app" } }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-03T16:00:02Z",
        content: "The command exited with code 0.\nOutput:\non branch main\n",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-03T16:00:03Z",
        tool_calls: [{ name: "run_command", args: { CommandLine: "pnpm test", Cwd: "/app" } }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-03T16:00:04Z",
        content: "The command exited with code 1.\nOutput:\n1 test failed\n",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const commands = session.turns[0].parts.filter((part) => part.kind === "command");
    assert.equal(commands.length, 2);
    assert.notEqual(commands[0].source_execution_id, commands[1].source_execution_id);
    assert.equal(commands[0].exit_code, 0);
    assert.notEqual(commands[0].status, "failed");
    assert.equal(commands[1].exit_code, 1);
    assert.equal(commands[1].status, "failed");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity reconstructs concrete diff for modern edit_file tool call and extracts starting line", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-ag-edit-file-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const targetFile = "/tmp/repo/backend/cli/tests/cli_tests.rs";
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-10-21T01:00:00Z",
        content: "<USER_REQUEST>Update cli tests</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-21T01:00:01Z",
        tool_calls: [{
          name: "edit_file",
          args: {
            Path: targetFile,
            Target: "assert_eq!(old, 1);\nassert_eq!(count, 0);\n",
            Replacement: "assert_eq!(new, 2);\n",
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-21T01:00:02Z",
        content: `Made 1 replacement in ${targetFile} starting at line 301 (+1/-2 lines).`,
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const fileChange = session.turns[0].parts.find((part) => part.kind === "file_change");
    assert.ok(fileChange, "fileChange part should exist");
    assert.equal(fileChange.content_card?.kind, "antigravity.file-change");
    assert.equal(fileChange.content_card?.renderer, "diff");
    assert.match(fileChange.text, /^diff --git a\/tmp\/repo\/backend\/cli\/tests\/cli_tests\.rs b\/tmp\/repo\/backend\/cli\/tests\/cli_tests\.rs/m);
    assert.match(fileChange.text, /@@ -301,2 \+301,1 @@/);
    assert.match(fileChange.text, /-assert_eq!\(old, 1\);/);
    assert.match(fileChange.text, /\+assert_eq!\(new, 2\);/);
    assert.doesNotMatch(fileChange.text, /Made 1 replacement/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity reconstructs concrete diff for modern write_file tool call", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-ag-write-file-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const targetFile = "/tmp/repo/backend/new_feature.rs";
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-10-21T01:05:00Z",
        content: "<USER_REQUEST>Create new feature</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-21T01:05:01Z",
        tool_calls: [{
          name: "write_file",
          args: {
            Path: targetFile,
            Content: "pub fn test_hello() {}\n",
            Overwrite: true,
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-21T01:05:02Z",
        content: `Created file file://${targetFile} with requested content.`,
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    const fileChange = session.turns[0].parts.find((part) => part.kind === "file_change");
    assert.ok(fileChange, "fileChange part should exist");
    assert.equal(fileChange.content_card?.kind, "antigravity.file-change");
    assert.equal(fileChange.content_card?.renderer, "diff");
    assert.match(fileChange.text, /new file mode 100644/);
    assert.match(fileChange.text, /\+pub fn test_hello\(\) \{\}/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity reliably infers project path from modern Path parameter and resolves git root", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-ag-path-infer-"));
  try {
    // Create simulated git repo structure
    const gitDir = path.join(fixtureRoot, ".git");
    const subDir = path.join(fixtureRoot, "backend", "cli");
    mkdirSync(gitDir, { recursive: true });
    mkdirSync(subDir, { recursive: true });

    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    const targetFile = path.join(subDir, "main.rs");
    writeFileSync(targetFile, "// hello");

    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-10-21T01:10:00Z",
        content: "<USER_REQUEST>Read and run tests</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-21T01:10:01Z",
        tool_calls: [
          {
            name: "read_file",
            args: {
              Path: targetFile,
            },
          },
          {
            name: "run_command",
            args: {
              CommandLine: "cargo test",
              WaitMsBeforeAsync: "5000",
              toolSummary: "Run tests",
            },
          },
        ],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "VIEW_FILE",
        status: "DONE",
        created_at: "2026-10-21T01:10:02Z",
        content: `File Path: \`file://${targetFile}\`\n1: // hello`,
      }),
      JSON.stringify({
        source: "MODEL",
        type: "RUN_COMMAND",
        status: "DONE",
        created_at: "2026-10-21T01:10:03Z",
        content: "Process exited with code 0\nOutput:\ntest ok",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    // Git root should be resolved to fixtureRoot instead of subDir
    assert.equal(session.project_path, fixtureRoot);
    const cmdPart = session.turns[0].parts.find((part) => part.kind === "command");
    assert.ok(cmdPart);
    assert.equal(cmdPart.cwd, fixtureRoot);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity inlines invoke_subagent calls into specialized subagent turns and emits continuation turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript_full.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-22T10:00:00Z",
        content: "<USER_REQUEST>Audit dependencies and verify build</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:01Z",
        content: "I will dispatch a specialist subagent to audit dependencies.",
        tool_calls: [{
          name: "invoke_subagent",
          args: {
            Subagents: [{
              Role: "auditor",
              Prompt: "Audit npm packages for security vulnerabilities",
              Model: "gemini-2.5-pro",
            }],
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:02Z",
        content: "Checking packages with npm audit...",
        tool_calls: [{
          name: "run_command",
          args: {
            CommandLine: "npm audit --json",
            toolSummary: "Audit packages",
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "RUN_COMMAND",
        status: "DONE",
        created_at: "2026-10-22T10:00:03Z",
        content: "Process exited with code 0\nOutput:\n{\"vulnerabilities\": {}}",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-22T10:00:04Z",
        content: "Audit complete: found 0 vulnerabilities.",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:05Z",
        content: "Subagent completed successfully. All dependencies are secure and build verified.",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.equal(session.turns.length, 3);

    // Turn 0: Anchor turn (User prompt + main pre-dispatch response)
    const turn0 = session.turns[0];
    assert.equal(turn0.turn_index, 0);
    assert.equal(turn0.user_text, "Audit dependencies and verify build");
    assert.equal(turn0.external_id, "turn-0");
    assert.ok(turn0.parts.length > 0);

    // Turn 1: Specialized subagent turn
    const turn1 = session.turns[1];
    assert.equal(turn1.turn_index, 1);
    assert.equal(turn1.turn_kind, "subagent");
    assert.equal(turn1.agent_role, "auditor");
    assert.equal(turn1.parent_turn_id, "turn-0");
    assert.equal(turn1.user_text, "Audit npm packages for security vulnerabilities");
    assert.equal(turn1.model, "gemini-2.5-pro");
    assert.ok(turn1.parts.length >= 2);
    const hasCommand = turn1.parts.some((p) => p.kind === "command" && p.command.includes("npm audit"));
    assert.ok(hasCommand, "Subagent turn should contain command execution part");
    const lastPart = turn1.parts[turn1.parts.length - 1];
    assert.equal(lastPart.role, "assistant");
    assert.match(lastPart.text, /Audit complete/);

    // Turn 2: Continuation turn
    const turn2 = session.turns[2];
    assert.equal(turn2.turn_index, 2);
    assert.equal(turn2.turn_kind, "continuation");
    assert.equal(turn2.parent_turn_id, "turn-0");
    assert.equal(turn2.user_text, "Continue following subagent execution");
    assert.ok(turn2.parts.length > 0);
    assert.match(turn2.parts[0].text, /All dependencies are secure/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity excludes derivative subagent sessions from list_sessions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-listsub-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const parentId = "11111111-1111-1111-1111-111111111111";
    const subagentId = "22222222-2222-2222-2222-222222222222";

    // Create parent brain dir with transcript
    const parentLogs = path.join(brainDir, parentId, ".system_generated", "logs");
    mkdirSync(parentLogs, { recursive: true });
    writeFileSync(path.join(parentLogs, "transcript.jsonl"), [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        content: "<USER_REQUEST>Main task</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        content: "Main response",
      }),
    ].join("\n"));

    // Create subagent brain dir with meta.json indicating subagent
    const subLogs = path.join(brainDir, subagentId, ".system_generated", "logs");
    mkdirSync(subLogs, { recursive: true });
    writeFileSync(path.join(brainDir, subagentId, "meta.json"), JSON.stringify({
      is_subagent: true,
      parent_session_id: parentId,
      cwd: "/some/project",
    }));
    writeFileSync(path.join(subLogs, "transcript.jsonl"), [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        content: "<USER_REQUEST>Subagent delegated task</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        content: "Subagent response",
      }),
    ].join("\n"));

    const listResult = spawnSync(process.execPath, [adapterPath], {
      encoding: "utf8",
      input: JSON.stringify({
        method: "list_sessions",
        source: { location: fixtureRoot },
        params: {},
      }),
    });
    assert.equal(listResult.status, 0);
    const items = listResult.stdout
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l))
      .filter((m) => m.type === "item")
      .map((m) => m.item.external_id);
    assert.deepEqual(items, [parentId]);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity preserves anchor user turn when assistant dispatches subagent with zero preceding text", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-zerotext-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-22T10:00:00Z",
        content: "<USER_REQUEST>Fix login bug immediately</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:01Z",
        content: "",
        tool_calls: [{
          name: "invoke_subagent",
          args: { role: "debugger", prompt: "Investigate login crash" },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-22T10:00:02Z",
        content: "Fixed null pointer in auth handler",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.ok(session.turns.length >= 2, `Expected >= 2 turns, got ${session.turns.length}`);
    // Anchor user turn must be preserved as turn 0
    assert.equal(session.turns[0].turn_index, 0);
    assert.equal(session.turns[0].user_text, "Fix login bug immediately");
    assert.equal(session.turns[0].turn_kind, undefined);

    // Subagent turn
    assert.equal(session.turns[1].turn_index, 1);
    assert.equal(session.turns[1].turn_kind, "subagent");
    assert.equal(session.turns[1].parent_turn_id, session.turns[0].external_id);
    assert.equal(session.turns[1].agent_role, "debugger");
    assert.equal(session.turns[1].user_text, "Investigate login crash");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity falls back to valid string when subagent prompt is whitespace-only", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-ws-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-22T10:00:00Z",
        content: "<USER_REQUEST>Execute delegation</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:01Z",
        content: "",
        tool_calls: [{
          name: "invoke_subagent",
          args: { role: "worker", prompt: "   \t\n  " },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-22T10:00:02Z",
        content: "Done",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.equal(session.turns[1].turn_kind, "subagent");
    assert.equal(session.turns[1].user_text, "Subagent execution");
    assert.ok(session.turns[1].user_text.trim().length > 0);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity handles parallel subagents dispatching multiple distinct subagent turns", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-parallel-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-22T10:00:00Z",
        content: "<USER_REQUEST>Run parallel audits</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:01Z",
        content: "Spawning parallel workers",
        tool_calls: [{
          name: "invoke_subagent",
          args: {
            Subagents: [
              { Role: "linter", Prompt: "Lint source files" },
              { Role: "tester", Prompt: "Run test suite" },
            ],
          },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-22T10:00:02Z",
        content: "All parallel tasks completed",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    // Turns: 0 = User, 1 = Subagent linter, 2 = Subagent tester
    assert.ok(session.turns.length >= 3, `Expected >= 3 turns, got ${session.turns.length}`);
    assert.equal(session.turns[0].user_text, "Run parallel audits");
    assert.equal(session.turns[1].turn_kind, "subagent");
    assert.equal(session.turns[1].agent_role, "linter");
    assert.equal(session.turns[1].user_text, "Lint source files");
    assert.equal(session.turns[1].parent_turn_id, session.turns[0].external_id);

    assert.equal(session.turns[2].turn_kind, "subagent");
    assert.equal(session.turns[2].agent_role, "tester");
    assert.equal(session.turns[2].user_text, "Run test suite");
    assert.equal(session.turns[2].parent_turn_id, session.turns[0].external_id);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity multi-turn session keeps subsequent question as standard user turn", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-multiturn-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript.jsonl");
    writeFileSync(transcriptPath, [
      // Question 1
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-22T10:00:00Z",
        content: "<USER_REQUEST>Question 1: Fix bug</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:01Z",
        content: "",
        tool_calls: [{
          name: "invoke_subagent",
          args: { role: "worker", prompt: "Fix bug subtask" },
        }],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-22T10:00:02Z",
        content: "Subtask finished",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:03Z",
        content: "Question 1 resolved.",
      }),
      // Question 2
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-22T10:05:00Z",
        content: "<USER_REQUEST>Question 2: What is the weather in Tokyo?</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:05:01Z",
        content: "The weather in Tokyo is sunny.",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    // Find Question 2 turn
    const q2Turn = session.turns.find((t) => t.user_text && t.user_text.includes("Question 2: What is the weather in Tokyo?"));
    assert.ok(q2Turn, "Question 2 turn should exist in session");
    assert.equal(q2Turn.turn_kind, undefined, "Question 2 must NOT be marked as continuation");
    assert.equal(q2Turn.parent_turn_id, undefined, "Question 2 must NOT have parent_turn_id");
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
