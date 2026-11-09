import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, statSync, utimesSync, writeFileSync } from "node:fs";
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

test("Antigravity keeps a single parent turn and records an anchor instead of a subagent pseudo-turn", () => {
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
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:05Z",
        content: "Subagent completed successfully. All dependencies are secure and build verified.",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);
    assert.equal(session.turns.length, 1, "一次人类提问派发子代理后必须仍然只有 1 个 Turn");

    const turn = session.turns[0];
    assert.equal(turn.turn_index, 0);
    assert.equal(turn.user_text, "Audit dependencies and verify build");
    assert.equal(turn.turn_kind, undefined);
    assert.equal(turn.parent_turn_id, undefined);

    const anchor = turn.parts.find((part) => part.kind === "subagent");
    assert.ok(anchor, "派发 invoke_subagent 必须留下委派锚点");
    assert.equal(anchor.role, "assistant");
    assert.match(anchor.text, /^\[委派任务\] /);
    const metadata = JSON.parse(anchor.metadata_json);
    assert.equal(metadata.agent_role, "auditor");
    assert.ok(metadata.child_external_id, "锚点必须记录子会话标识");
    assert.ok(metadata.spawn_anchor);
    assert.equal(metadata.status, "completed");
    assert.equal(anchor.content_card, undefined);
    assert.equal(metadata.content_card, undefined);

    // 子代理返回后的工具调用与最终回答必须留在同一个父轮次里
    assert.ok(
      turn.parts.some((part) => part.kind === "command" && part.command.includes("npm audit")),
      "主代理后续命令必须留在父轮次",
    );
    assert.match(turn.parts.at(-1).text, /All dependencies are secure/);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity keeps subagent sessions in list_sessions descriptors", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-listsub-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const parentId = "11111111-1111-1111-1111-111111111111";
    const subagentId = "22222222-2222-2222-2222-222222222222";

    // 父会话：派发一个登记在注册表里的子代理
    writeConversation(brainDir, parentId, {
      steps: [
        userStep(0, "Main task"),
        ...dispatchSteps(1, [{ Role: "Delegated Worker", TypeName: "teamwork_preview_worker", Prompt: "Do the subtask" }], [subagentId]),
        step(3, "MODEL", "PLANNER_RESPONSE", { content: "Main response" }),
      ],
      subagents: [{ conversationId: subagentId, role: "Delegated Worker", typeName: "teamwork_preview_worker", spawnStepIndex: 2 }],
    });

    // 子代理目录自身带 meta.json 的 is_subagent 标记
    writeConversation(brainDir, subagentId, {
      steps: [
        userStep(0, "Subagent delegated task"),
        step(1, "MODEL", "PLANNER_RESPONSE", { content: "Subagent response" }),
      ],
    });
    writeFileSync(path.join(brainDir, subagentId, "meta.json"), JSON.stringify({
      is_subagent: true,
      parent_session_id: parentId,
      cwd: "/some/project",
    }));

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
      .map((m) => m.item.external_id)
      .sort();
    assert.deepEqual(
      items,
      [parentId, subagentId].sort(),
      "子代理会话描述符必须保留，顶层隐藏由后端 user_visible 负责",
    );
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
    assert.equal(session.turns.length, 1, "派发子代理不得产生额外轮次");
    assert.equal(session.turns[0].turn_index, 0);
    assert.equal(session.turns[0].user_text, "Fix login bug immediately");
    assert.equal(session.turns[0].turn_kind, undefined);

    const anchor = session.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor, "零前置文本也必须留下委派锚点");
    const metadata = JSON.parse(anchor.metadata_json);
    assert.equal(metadata.agent_role, "debugger");
    assert.match(anchor.text, /^\[委派任务\] /);
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
    assert.equal(session.turns.length, 1, "空白 prompt 不得改变轮次数量");
    const anchor = session.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor);
    const metadata = JSON.parse(anchor.metadata_json);
    assert.equal(metadata.agent_role, "worker");
    assert.ok(String(metadata.title ?? "").trim().length > 0, "空白 prompt 必须回退到可读标题");
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
    assert.equal(session.turns.length, 1, "并行派发仍属于同一个父轮次");
    assert.equal(session.turns[0].user_text, "Run parallel audits");

    const anchors = session.turns[0].parts.filter((part) => part.kind === "subagent");
    assert.equal(anchors.length, 2);
    const roles = anchors.map((part) => JSON.parse(part.metadata_json).agent_role);
    assert.deepEqual(roles, ["linter", "tester"]);
    assert.equal(new Set(anchors.map((part) => JSON.parse(part.metadata_json).child_external_id)).size, 2);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity does not emit subagent JSON cards when several dispatch calls share one response", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-nocard-"));
  try {
    const transcriptPath = path.join(fixtureRoot, "transcript.jsonl");
    writeFileSync(transcriptPath, [
      JSON.stringify({
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        status: "DONE",
        created_at: "2026-10-22T10:00:00Z",
        content: "<USER_REQUEST>Run two audits</USER_REQUEST>",
      }),
      JSON.stringify({
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        status: "DONE",
        created_at: "2026-10-22T10:00:01Z",
        content: "Spawning two independent workers",
        tool_calls: [
          {
            name: "invoke_subagent",
            args: { Subagents: [{ Role: "linter", Prompt: "Lint source files" }] },
          },
          {
            name: "invoke_subagent",
            args: { Subagents: [{ Role: "tester", Prompt: "Run test suite" }] },
          },
        ],
      }),
      JSON.stringify({
        source: "MODEL",
        type: "GENERIC",
        status: "DONE",
        created_at: "2026-10-22T10:00:02Z",
        content: "Both tasks completed",
      }),
    ].join("\n"));

    const session = readFixtureSession(transcriptPath);

    assert.equal(session.turns.length, 1, "同一条响应里的多次派发仍只有一个父轮次");
    const anchors = session.turns[0].parts.filter((part) => part.kind === "subagent");
    assert.equal(anchors.length, 2, "每次派发都必须有独立锚点");
    assert.deepEqual(
      anchors.map((part) => JSON.parse(part.metadata_json).agent_role),
      ["linter", "tester"],
    );

    const cardParts = anchors.filter(
      (part) => part.content_card || JSON.parse(part.metadata_json ?? "{}").content_card,
    );
    assert.deepEqual(cardParts, [], "子代理锚点不得携带 content_card（其他普通零件仍可有自己的卡片）");
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
    assert.equal(session.turns.length, 2, "两次真实提问必须是两个物理轮次");
    const q2Turn = session.turns[1];
    assert.equal(q2Turn.turn_index, 1, "子代理派发不得让后续人类提问的轮次号偏移");
    assert.equal(q2Turn.user_text, "Question 2: What is the weather in Tokyo?");
    assert.equal(q2Turn.turn_kind, undefined, "Question 2 不得被标记为伪轮次");
    assert.equal(q2Turn.parent_turn_id, undefined);
    assert.ok(
      session.turns[0].parts.some((part) => part.kind === "subagent"),
      "Question 1 的派发锚点必须留在 Question 1 的轮次里",
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});


test("Antigravity inlines every registered subagent as its own turn with a resolved parent and deep content", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-tree-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "a0000000-0000-4000-8000-000000000001";
    const orchestratorId = "a0000000-0000-4000-8000-000000000002";
    const projectId = "a0000000-0000-4000-8000-000000000003";
    const leafIds = Array.from({ length: 16 }, (_, index) => `b0000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`);

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "Orchestrate the multi-agent review"),
        ...dispatchSteps(1, [
          { Role: "Teamwork Orchestrator", TypeName: "teamwork_preview", Prompt: "Coordinate the multi-agent review", Model: "inherit" },
        ], [orchestratorId]),
        step(3, "MODEL", "PLANNER_RESPONSE", { content: "Orchestrator dispatched." }),
      ],
      subagents: [{ conversationId: orchestratorId, role: "Teamwork Orchestrator", typeName: "teamwork_preview", spawnStepIndex: 2 }],
    });

    writeConversation(brainDir, orchestratorId, {
      steps: [
        step(0, "SYSTEM", "SYSTEM_MESSAGE", { content: "<SYSTEM_MESSAGE>You are the teamwork orchestrator.</SYSTEM_MESSAGE>" }),
        ...dispatchSteps(1, [
          { Role: "Project Orchestrator", TypeName: "teamwork_preview_orchestrator", Prompt: "Plan the review work" },
        ], [projectId]),
        step(3, "MODEL", "PLANNER_RESPONSE", { content: "Project orchestrator spawned." }),
      ],
      subagents: [{ conversationId: projectId, role: "Project Orchestrator", typeName: "teamwork_preview_orchestrator", spawnStepIndex: 2 }],
    });

    const leafRoles = leafIds.map((_, index) => `Rust Core Reviewer ${index + 1}`);
    const projectSteps = [];
    const projectSubagents = [];
    for (let batch = 0; batch < 4; batch += 1) {
      const startIndex = 1 + batch * 2;
      const batchRoles = leafRoles.slice(batch * 4, batch * 4 + 4);
      const batchIds = leafIds.slice(batch * 4, batch * 4 + 4);
      projectSteps.push(
        ...dispatchSteps(
          startIndex,
          batchRoles.map((role) => ({ Role: role, TypeName: "teamwork_preview_reviewer", Prompt: `Review module for ${role}` })),
          batchIds,
        ),
      );
      batchIds.forEach((conversationId, index) => {
        projectSubagents.push({
          conversationId,
          role: batchRoles[index],
          typeName: "teamwork_preview_reviewer",
          spawnStepIndex: startIndex + 1,
        });
      });
    }
    projectSteps.push(step(9, "MODEL", "PLANNER_RESPONSE", { content: "All reviewers are running." }));

    writeConversation(brainDir, projectId, { steps: projectSteps, subagents: projectSubagents });

    leafIds.forEach((conversationId, index) => {
      writeConversation(brainDir, conversationId, {
        steps: [
          step(0, "SYSTEM", "SYSTEM_MESSAGE", { content: "<SYSTEM_MESSAGE>You are a reviewer subagent.</SYSTEM_MESSAGE>" }),
          step(1, "MODEL", "PLANNER_RESPONSE", {
            content: "",
            tool_calls: [{ name: "run_command", args: { CommandLine: `rg "module ${index + 1}" src`, Cwd: "/tmp/project" } }],
          }),
          step(2, "MODEL", "GENERIC", { content: "Process exited with code 0\nOutput:\n3 matches" }),
          step(3, "MODEL", "PLANNER_RESPONSE", { content: `Module ${index + 1} review complete: 3 matches.` }),
        ],
      });
    });

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { session, sessions, warnings } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    assert.deepEqual(warnings, []);
    assert.equal(sessions.length, 19, "1 个父会话 + 18 个隐藏子代理会话");
    const byId = new Map(sessions.map((entry) => [entry.external_id, entry]));

    // 父会话：一次人类提问 = 1 个 Turn，派发处只有锚点
    const parent = byId.get(mainId);
    assert.ok(parent);
    assert.equal(parent.user_visible, true);
    assert.equal(parent.turns.length, 1, "父会话只能有 1 个真实提问轮次");
    const parentAnchor = parent.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(parentAnchor, "父轮次必须包含第一层委派锚点");
    const parentAnchorMeta = JSON.parse(parentAnchor.metadata_json);
    assert.equal(parentAnchorMeta.child_external_id, orchestratorId);
    assert.equal(parentAnchorMeta.agent_role, "Teamwork Orchestrator");
    assert.equal(parentAnchorMeta.status, "completed");
    assert.equal(parentAnchorMeta.spawn_anchor, orchestratorId);
    assert.match(parentAnchor.text, /^\[委派任务\] /);
    assert.match(parentAnchor.text, /\[汇报结论\] /);

    // 第一层子会话
    const orchestrator = byId.get(orchestratorId);
    assert.ok(orchestrator);
    assert.equal(orchestrator.user_visible, false);
    assert.equal(orchestrator.execution_origin, "subagent");
    const orchestratorPurpose = JSON.parse(orchestrator.execution_purpose);
    assert.equal(orchestratorPurpose.parent_external_id, mainId);
    assert.equal(orchestratorPurpose.parent_turn_index, 0);
    assert.equal(orchestratorPurpose.agent_role, "Teamwork Orchestrator");
    assert.equal(orchestratorPurpose.spawn_anchor, orchestratorId);
    assert.equal(orchestrator.turns.length, 1, "子会话 #T1 承载全量委派 Prompt 与执行链");
    assert.equal(orchestrator.turns[0].user_text, "Coordinate the multi-agent review");
    assert.equal(orchestrator.turns[0].turn_kind, undefined);
    const orchestratorAnchor = orchestrator.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(orchestratorAnchor, "嵌套子代理的锚点必须写在子会话自己的轮次里");
    assert.equal(JSON.parse(orchestratorAnchor.metadata_json).child_external_id, projectId);

    // 第二层子会话：16 个叶子锚点都挂在 project 自己的轮次里
    const project = byId.get(projectId);
    assert.ok(project);
    assert.equal(project.turns.length, 1);
    assert.equal(project.turns[0].user_text, "Plan the review work");
    const leafAnchors = project.turns[0].parts.filter((part) => part.kind === "subagent");
    assert.equal(leafAnchors.length, 16);
    const firstLeafAnchorMeta = JSON.parse(leafAnchors[0].metadata_json);
    assert.equal(firstLeafAnchorMeta.agent_role, "Rust Core Reviewer 1");
    assert.equal(firstLeafAnchorMeta.status, "completed");
    assert.match(leafAnchors[0].text, /\[汇报结论\] Module 1 review complete/);
    assert.ok(firstLeafAnchorMeta.step_count >= 1, "锚点必须记录子代理执行的步骤数");

    // 最深一层子会话：全量 Prompt + 工具链 + 最终汇报
    const leaf = byId.get(leafIds[0]);
    assert.ok(leaf);
    assert.equal(leaf.user_visible, false);
    assert.equal(leaf.turns.length, 1);
    assert.equal(leaf.turns[0].user_text, "Review module for Rust Core Reviewer 1");
    const commandPart = leaf.turns[0].parts.find((part) => part.kind === "command");
    assert.ok(commandPart, "子会话必须暴露它执行的命令");
    assert.match(commandPart.command, /rg "module 1" src/);
    const lastPart = leaf.turns[0].parts.at(-1);
    assert.equal(lastPart.role, "assistant");
    assert.match(lastPart.text, /Module 1 review complete/);
    assert.equal(
      lastPart.content_card?.kind,
      "antigravity.answer",
      "子代理汇报必须以 answer 卡片收尾，才能落进该 Turn 的 response",
    );

    for (const entry of sessions) {
      for (const turn of entry.turns) {
        assert.equal(turn.turn_kind, undefined, "不得再产出 subagent / continuation 伪轮次");
        assert.equal(turn.parent_turn_id, undefined);
      }
    }

    const repeated = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });
    assert.deepEqual(
      repeated.sessions.map((entry) => [entry.external_id, entry.turns.map((turn) => turn.external_id)]),
      sessions.map((entry) => [entry.external_id, entry.turns.map((turn) => turn.external_id)]),
      "会话与轮次身份必须在重复同步时保持稳定",
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity keeps every subagent dispatched inside the same execution step as one anchor and one child session", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-parallel-step-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "c0000000-0000-4000-8000-000000000001";
    const childIds = [
      "c0000000-0000-4000-8000-000000000011",
      "c0000000-0000-4000-8000-000000000012",
      "c0000000-0000-4000-8000-000000000013",
      "c0000000-0000-4000-8000-000000000014",
    ];
    const roles = ["Scope Explorer", "Adapter Explorer", "Storage Explorer", "UI Explorer"];

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "Explore the repository"),
        step(1, "MODEL", "PLANNER_RESPONSE", {
          content: "",
          tool_calls: [
            {
              name: "invoke_subagent",
              args: { Subagents: [
                { Role: roles[0], TypeName: "teamwork_preview_explorer", Prompt: "Explore the scope" },
                { Role: roles[1], TypeName: "teamwork_preview_explorer", Prompt: "Explore the adapters" },
              ] },
            },
            {
              name: "invoke_subagent",
              args: { Subagents: [
                { Role: roles[2], TypeName: "teamwork_preview_explorer", Prompt: "Explore storage" },
                { Role: roles[3], TypeName: "teamwork_preview_explorer", Prompt: "Explore the UI" },
              ] },
            },
          ],
        }),
        step(2, "MODEL", "GENERIC", { content: "Created the following subagents:\n{}" }),
      ],
      subagents: childIds.map((conversationId, index) => ({
        conversationId,
        role: roles[index],
        typeName: "teamwork_preview_explorer",
        spawnStepIndex: 2,
      })),
    });

    childIds.forEach((conversationId, index) => {
      writeConversation(brainDir, conversationId, {
        steps: [
          step(0, "MODEL", "PLANNER_RESPONSE", { content: "", tool_calls: [{ name: "list_dir", args: { DirectoryPath: `/tmp/project/${index}` } }] }),
          step(1, "MODEL", "GENERIC", { content: `{"name":"src","isDir":true}` }),
          step(2, "MODEL", "PLANNER_RESPONSE", { content: `${roles[index]} is done.` }),
        ],
      });
    });

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    const parent = sessions.find((entry) => entry.external_id === mainId);
    assert.ok(parent);
    assert.equal(parent.turns.length, 1, "同一步骤里的 4 次派发仍只属于 1 个父轮次");
    const anchors = parent.turns[0].parts.filter((part) => part.kind === "subagent");
    assert.equal(anchors.length, 4, "每次派发都必须有独立锚点");
    assert.deepEqual(
      anchors.map((part) => JSON.parse(part.metadata_json).agent_role),
      roles,
    );
    assert.deepEqual(
      anchors.map((part) => JSON.parse(part.metadata_json).child_external_id),
      childIds,
      "锚点必须指向确定性的子会话 external_id",
    );
    assert.equal(sessions.length, 5, "1 个父会话 + 4 个隐藏子会话");
    for (const childId of childIds) {
      const child = sessions.find((entry) => entry.external_id === childId);
      assert.ok(child, `子会话 ${childId} 必须独立产出`);
      assert.equal(child.user_visible, false);
      assert.equal(child.execution_origin, "subagent");
      assert.equal(child.turns.length, 1);
      assert.ok(child.turns[0].parts.length > 0);
      assert.equal(JSON.parse(child.execution_purpose).parent_external_id, mainId);
    }
    assert.ok(sessions.every((entry) => entry.turns.every((turn) => turn.turn_kind === undefined)));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity hangs each subagent anchor off the turn that dispatched it", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-per-question-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "14000000-0000-4000-8000-000000000001";
    const firstChildId = "14000000-0000-4000-8000-000000000011";
    const secondChildId = "14000000-0000-4000-8000-000000000012";

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "First question"),
        ...dispatchSteps(1, [{ Role: "First Worker", TypeName: "teamwork_preview_worker", Prompt: "Handle the first question" }], [firstChildId]),
        userStep(3, "Second question"),
        ...dispatchSteps(4, [{ Role: "Second Worker", TypeName: "teamwork_preview_worker", Prompt: "Handle the second question" }], [secondChildId]),
      ],
      subagents: [
        { conversationId: firstChildId, role: "First Worker", typeName: "teamwork_preview_worker", spawnStepIndex: 2 },
        { conversationId: secondChildId, role: "Second Worker", typeName: "teamwork_preview_worker", spawnStepIndex: 5 },
      ],
    });

    for (const [conversationId, text] of [[firstChildId, "first done"], [secondChildId, "second done"]]) {
      writeConversation(brainDir, conversationId, {
        steps: [step(0, "MODEL", "PLANNER_RESPONSE", { content: text })],
      });
    }

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    const parent = sessions.find((entry) => entry.external_id === mainId);
    assert.ok(parent);
    assert.equal(parent.turns.length, 2, "两次人类提问必须对应两个完整父轮次");
    assert.deepEqual(parent.turns.map((turn) => turn.user_text), ["First question", "Second question"]);
    const firstAnchors = parent.turns[0].parts.filter((part) => part.kind === "subagent");
    const secondAnchors = parent.turns[1].parts.filter((part) => part.kind === "subagent");
    assert.deepEqual(firstAnchors.map((part) => JSON.parse(part.metadata_json).child_external_id), [firstChildId]);
    assert.deepEqual(secondAnchors.map((part) => JSON.parse(part.metadata_json).child_external_id), [secondChildId]);
    assert.equal(JSON.parse(firstAnchors[0].metadata_json).agent_role, "First Worker");
    assert.equal(JSON.parse(secondAnchors[0].metadata_json).agent_role, "Second Worker");
    assert.equal(sessions.length, 3);
    for (const childId of [firstChildId, secondChildId]) {
      const child = sessions.find((entry) => entry.external_id === childId);
      assert.ok(child);
      assert.equal(JSON.parse(child.execution_purpose).parent_external_id, mainId);
      assert.equal(child.turns[0].turn_kind, undefined);
    }
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity materializes a deeply nested subagent chain as one session per level", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-depth-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const chainIds = Array.from({ length: 6 }, (_, index) => `d0000000-0000-4000-8000-${String(index + 1).padStart(12, "0")}`);

    chainIds.forEach((conversationId, index) => {
      const nextId = chainIds[index + 1];
      const steps = [userStep(0, `Level ${index + 1} task`)];
      const subagents = [];
      if (nextId) {
        steps.push(...dispatchSteps(1, [{ Role: `Level ${index + 2} Orchestrator`, TypeName: "teamwork_preview_orchestrator", Prompt: `Delegate level ${index + 2}` }], [nextId]));
        subagents.push({
          conversationId: nextId,
          role: `Level ${index + 2} Orchestrator`,
          typeName: "teamwork_preview_orchestrator",
          spawnStepIndex: 2,
        });
      }
      steps.push(step(3, "MODEL", "PLANNER_RESPONSE", { content: `Level ${index + 1} finished.` }));
      writeConversation(brainDir, conversationId, { steps, subagents });
    });

    const transcriptPath = path.join(brainDir, chainIds[0], ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    assert.equal(sessions.length, chainIds.length, "每一层子代理都产出独立会话，且不得无限展开");
    const byId = new Map(sessions.map((entry) => [entry.external_id, entry]));
    for (let index = 0; index < chainIds.length - 1; index += 1) {
      const parent = byId.get(chainIds[index]);
      assert.ok(parent, `第 ${index + 1} 层会话必须存在`);
      assert.equal(parent.turns.length, 1);
      const anchors = parent.turns[0].parts.filter((part) => part.kind === "subagent");
      assert.deepEqual(
        anchors.map((part) => JSON.parse(part.metadata_json).child_external_id),
        [chainIds[index + 1]],
        "每层的锚点只指向它的直接子会话",
      );
      assert.ok(parent.turns.every((turn) => turn.turn_kind === undefined));
    }
    const deepest = byId.get(chainIds.at(-1));
    assert.ok(deepest);
    assert.equal(deepest.turns.length, 1);
    assert.equal(deepest.turns[0].parts.filter((part) => part.kind === "subagent").length, 0, "链尾不得继续派生");
    assert.equal(byId.get(chainIds[1]).user_visible, false);
    assert.equal(byId.get(chainIds[1]).execution_origin, "subagent");
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity stops expanding a cyclic subagent reference and warns", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-cycle-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "e0000000-0000-4000-8000-000000000001";
    const childId = "e0000000-0000-4000-8000-000000000002";

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "Delegate a cyclic task"),
        ...dispatchSteps(1, [{ Role: "Loop Orchestrator", TypeName: "teamwork_preview_orchestrator", Prompt: "Delegate back to the root" }], [childId]),
      ],
      subagents: [{ conversationId: childId, role: "Loop Orchestrator", typeName: "teamwork_preview_orchestrator", spawnStepIndex: 2 }],
    });

    writeConversation(brainDir, childId, {
      steps: [
        step(0, "MODEL", "PLANNER_RESPONSE", { content: "", tool_calls: [{ name: "run_command", args: { CommandLine: "pwd" } }] }),
        step(1, "MODEL", "GENERIC", { content: "Process exited with code 0\nOutput:\n/tmp/project" }),
        ...dispatchSteps(2, [{ Role: "Root Again", TypeName: "teamwork_preview", Prompt: "Return to the root" }], [mainId]),
      ],
      subagents: [{ conversationId: mainId, role: "Root Again", typeName: "teamwork_preview", spawnStepIndex: 3 }],
    });

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions, warnings } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    assert.equal(sessions.length, 2, "成环的两个会话各自只产出一次，不得无限展开");
    const byId = new Map(sessions.map((entry) => [entry.external_id, entry]));
    const root = byId.get(mainId);
    const child = byId.get(childId);
    assert.ok(root);
    assert.ok(child);
    assert.equal(root.turns.length, 1);
    assert.equal(child.turns.length, 1);
    assert.deepEqual(
      root.turns[0].parts.filter((part) => part.kind === "subagent").map((part) => JSON.parse(part.metadata_json).child_external_id),
      [childId],
    );
    assert.deepEqual(
      child.turns[0].parts.filter((part) => part.kind === "subagent").map((part) => JSON.parse(part.metadata_json).child_external_id),
      [mainId],
      "回指父会话的派发仍以锚点形式出现一次",
    );
    assert.ok(sessions.every((entry) => entry.turns.every((turn) => turn.turn_kind === undefined)));
    assert.ok(warnings.some((warning) => /cycle/i.test(warning)), `expected a cycle warning, got ${JSON.stringify(warnings)}`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity keeps the dispatch anchor and warns when the child session record is missing", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-missing-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "f0000000-0000-4000-8000-000000000001";
    const missingId = "f0000000-0000-4000-8000-000000000002";

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "Delegate a task with no transcript"),
        ...dispatchSteps(1, [{ Role: "Ghost Worker", TypeName: "teamwork_preview_worker", Prompt: "Audit the migration" }], [missingId]),
        step(3, "MODEL", "PLANNER_RESPONSE", { content: "Waiting for the worker." }),
      ],
      subagents: [{ conversationId: missingId, role: "Ghost Worker", typeName: "teamwork_preview_worker", spawnStepIndex: 2 }],
    });

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions, warnings } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    assert.equal(sessions.length, 1, "子会话目录不存在时不得凭空造会话");
    const parent = sessions[0];
    assert.equal(parent.external_id, mainId);
    assert.equal(parent.turns.length, 1);
    const anchor = parent.turns[0].parts.find((part) => part.kind === "subagent");
    assert.ok(anchor, "父会话必须仍然导入，且保留派发锚点");
    const meta = JSON.parse(anchor.metadata_json);
    assert.equal(meta.agent_role, "Ghost Worker");
    assert.equal(meta.child_external_id, missingId);
    assert.match(anchor.text, /\[委派任务\] Audit the migration/);
    assert.ok(
      warnings.some((warning) => /child session|missing/i.test(warning)),
      `expected a missing-session warning, got ${JSON.stringify(warnings)}`,
    );
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity takes the dispatch task text instead of system injected prompt blocks", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-prompt-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "11000000-0000-4000-8000-000000000001";
    const childId = "11000000-0000-4000-8000-000000000002";

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "Audit the adapters"),
        ...dispatchSteps(1, [{ Role: "Adapter Auditor", TypeName: "teamwork_preview_auditor", Prompt: "Audit the antigravity adapter" }], [childId]),
      ],
      subagents: [{ conversationId: childId, role: "Adapter Auditor", typeName: "teamwork_preview_auditor", spawnStepIndex: 2 }],
    });

    writeConversation(brainDir, childId, {
      steps: [
        userStep(0, "<environment_context><cwd>/tmp/project</cwd><plugins>300 plugins</plugins></environment_context>"),
        userStep(1, "Investigate the flaky test"),
        step(2, "MODEL", "PLANNER_RESPONSE", { content: "Investigation done." }),
      ],
    });

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions, warnings } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    const child = sessions.find((entry) => entry.external_id === childId);
    assert.ok(child, "子代理必须作为独立会话承载全量委派 Prompt");
    assert.equal(child.turns.length, 1);
    assert.equal(child.turns[0].user_text, "Audit the antigravity adapter");
    assert.equal(child.turns[0].turn_kind, undefined);
    assert.deepEqual(warnings, []);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity falls back to the child user input when no dispatch task text exists", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-fallback-prompt-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "12000000-0000-4000-8000-000000000001";
    const childId = "12000000-0000-4000-8000-000000000002";

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "Dispatch without a task prompt"),
        ...dispatchSteps(1, [{ Role: "Flaky Test Hunter", TypeName: "teamwork_preview_worker" }], [childId]),
      ],
      subagents: [{ conversationId: childId, role: "Flaky Test Hunter", typeName: "teamwork_preview_worker", spawnStepIndex: 2 }],
    });

    writeConversation(brainDir, childId, {
      steps: [
        userStep(0, "<environment_context><cwd>/tmp/project</cwd></environment_context>"),
        userStep(1, "Investigate the flaky test"),
        step(2, "MODEL", "PLANNER_RESPONSE", { content: "The flaky test is fixed." }),
      ],
    });

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions, warnings } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    const child = sessions.find((entry) => entry.external_id === childId);
    assert.ok(child);
    assert.equal(child.turns.length, 1);
    assert.equal(child.turns[0].user_text, "Investigate the flaky test");
    assert.deepEqual(warnings, []);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity uses a placeholder prompt and warns when no task text can be recovered", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-subagent-placeholder-"));
  try {
    const brainDir = path.join(fixtureRoot, "brain");
    const mainId = "13000000-0000-4000-8000-000000000001";
    const childId = "13000000-0000-4000-8000-000000000002";

    writeConversation(brainDir, mainId, {
      steps: [
        userStep(0, "Dispatch an anonymous worker"),
        ...dispatchSteps(1, [{ Role: "Anonymous Worker", TypeName: "teamwork_preview_worker" }], [childId]),
      ],
      subagents: [{ conversationId: childId, role: "Anonymous Worker", typeName: "teamwork_preview_worker", spawnStepIndex: 2 }],
    });

    writeConversation(brainDir, childId, {
      steps: [step(0, "SYSTEM", "SYSTEM_MESSAGE", { content: "<SYSTEM_MESSAGE>no task text here</SYSTEM_MESSAGE>" })],
    });

    const transcriptPath = path.join(brainDir, mainId, ".system_generated", "logs", "transcript_full.jsonl");
    const { sessions, warnings } = runFixtureAdapter({ method: "read_session", source: { location: transcriptPath }, params: {} });

    const child = sessions.find((entry) => entry.external_id === childId);
    assert.ok(child);
    assert.equal(child.turns.length, 1);
    assert.equal(child.turns[0].user_text, "Subagent execution");
    assert.ok(warnings.some((warning) => /prompt/i.test(warning)), `expected a prompt warning, got ${JSON.stringify(warnings)}`);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

function step(index, source, type, extra = {}) {
  return {
    step_index: index,
    source,
    type,
    status: "DONE",
    created_at: `2026-10-28T10:00:${String(index).padStart(2, "0")}Z`,
    ...extra,
  };
}

function userStep(index, text) {
  return step(index, "USER_EXPLICIT", "USER_INPUT", { content: `<USER_REQUEST>${text}</USER_REQUEST>` });
}

function dispatchSteps(startIndex, subagents, conversationIds = []) {
  const created = conversationIds.map((conversationId) => JSON.stringify({
    conversationId,
    logAbsoluteUri: `file:///tmp/brain/${conversationId}/.system_generated/logs/transcript.jsonl`,
  }));
  return [
    step(startIndex, "MODEL", "PLANNER_RESPONSE", {
      content: "",
      tool_calls: [{ name: "invoke_subagent", args: { Subagents: subagents } }],
    }),
    step(startIndex + 1, "MODEL", "GENERIC", {
      content: `Created the following subagents:\n${created.join("\n")}`,
    }),
  ];
}

function writeConversation(brainDir, conversationId, { steps = [], subagents = [] }) {
  const conversationDir = path.join(brainDir, conversationId);
  const logsDir = path.join(conversationDir, ".system_generated", "logs");
  mkdirSync(logsDir, { recursive: true });
  writeFileSync(path.join(logsDir, "transcript_full.jsonl"), steps.map((entry) => JSON.stringify(entry)).join("\n"));
  if (subagents.length === 0) return conversationDir;
  const registryDir = path.join(conversationDir, ".system_generated", "subagents");
  mkdirSync(registryDir, { recursive: true });
  for (const subagent of subagents) {
    writeFileSync(
      path.join(registryDir, `${subagent.conversationId}.json`),
      JSON.stringify({
        conversationId: subagent.conversationId,
        subagentDescriptor: { typeName: subagent.typeName, role: subagent.role },
        state: "SUBAGENT_STATE_ALIVE",
        spawnStepIndex: subagent.spawnStepIndex,
        workspaceUris: ["file:///tmp/project"],
      }),
    );
  }
  return conversationDir;
}

function runFixtureAdapter(input) {
  const result = spawnSync(process.execPath, [adapterPath], {
    encoding: "utf8",
    input: JSON.stringify(input),
  });
  assert.equal(result.status, 0, result.stderr);
  const messages = result.stdout.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
  const sessions = messages
    .filter((message) => message.type === "item" && message.item?.session)
    .map((message) => message.item.session);
  return {
    messages,
    sessions,
    session: sessions[0] ?? null,
    warnings: messages.filter((message) => message.type === "warning").map((message) => message.message),
  };
}


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
    .filter((message) => message.type === "item" && message.item?.kind === "usage_event")
    .map((message) => message.item.event);
  const complete = messages.find((message) => message.type === "complete")?.item ?? null;
  return { messages, events, complete };
}

test("Antigravity extracts single Turn usage and first response UTC timestamp from transcript.jsonl", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-usage-single-"));
  try {
    const sessionId = "2a30f556-a539-413d-961c-304ed5dc529b";
    const sessionDir = path.join(fixtureRoot, sessionId);
    const logsDir = path.join(sessionDir, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    const transcriptPath = path.join(logsDir, "transcript.jsonl");

    writeFileSync(
      transcriptPath,
      [
        JSON.stringify({
          step_index: 0,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          status: "DONE",
          created_at: "2026-10-30T04:00:00.000Z",
          content: "<USER_REQUEST>Explain quantum computing</USER_REQUEST>",
        }),
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          status: "DONE",
          created_at: "2026-10-30T04:00:03.456Z",
          input_tokens: 150,
          cache_read_tokens: 45,
          output_tokens: 80,
          content: "Quantum computing harnesses quantum mechanics...",
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(complete.decoder_profile, "antigravity-transcript-v1");
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, sessionId);
    assert.equal(event.turn_id, "turn-0");
    assert.equal(event.external_event_id, `${sessionId}-turn-0`);
    assert.equal(event.logical_request_id, `${sessionId}-turn-0`);
    assert.equal(event.attempt_index, 0);
    assert.equal(event.provider, "google");
    assert.equal(event.model, "gemini-2.5-pro");
    assert.equal(event.status, "completed");
    assert.equal(event.timestamp, "2026-10-30T04:00:03.456Z");
    assert.equal(event.input_tokens, 150);
    assert.equal(event.cache_read_tokens, 45);
    assert.equal(event.cache_write_tokens, 0);
    assert.equal(event.reasoning_tokens, 0);
    assert.equal(event.output_tokens, 80);
    assert.equal(event.total_tokens, 275);
    assert.equal(event.host_reported_cost, null);
    assert.equal(event.currency, null);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity accumulates multi-step tool loop tokens within a single Turn and retains first response timestamp", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-usage-loop-"));
  try {
    const sessionId = "f5f26120-1514-41e7-ac3c-909f65378606";
    const sessionDir = path.join(fixtureRoot, sessionId);
    const logsDir = path.join(sessionDir, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    const transcriptPath = path.join(logsDir, "transcript.jsonl");

    writeFileSync(
      transcriptPath,
      [
        JSON.stringify({
          step_index: 0,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          created_at: "2026-10-27T10:00:00.000Z",
          content: "<USER_REQUEST>Search and fix code</USER_REQUEST>",
        }),
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-27T10:00:01.500Z",
          input_tokens: 100,
          cache_read_tokens: 20,
          output_tokens: 50,
          tool_calls: [{ name: "grep_search", args: { query: "todo" } }],
        }),
        JSON.stringify({
          step_index: 2,
          source: "MODEL",
          type: "GENERIC",
          created_at: "2026-10-27T10:00:03.000Z",
          content: "file.js:10: TODO",
        }),
        JSON.stringify({
          step_index: 3,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-27T10:00:05.200Z",
          input_tokens: 200,
          cache_read_tokens: 40,
          output_tokens: 80,
          tool_calls: [{ name: "replace_file_content", args: {} }],
        }),
        JSON.stringify({
          step_index: 4,
          source: "MODEL",
          type: "GENERIC",
          created_at: "2026-10-27T10:00:07.000Z",
          content: "replaced",
        }),
        JSON.stringify({
          step_index: 5,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-27T10:00:10.800Z",
          input_tokens: 300,
          cache_read_tokens: 60,
          output_tokens: 120,
          content: "All done!",
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);

    const event = events[0];
    assert.equal(event.session_id, sessionId);
    assert.equal(event.turn_id, "turn-0");
    assert.equal(event.external_event_id, `${sessionId}-turn-0`);
    assert.equal(event.timestamp, "2026-10-27T10:00:01.500Z"); // Strictly first response timestamp
    assert.equal(event.input_tokens, 600); // 100 + 200 + 300
    assert.equal(event.cache_read_tokens, 120); // 20 + 40 + 60
    assert.equal(event.output_tokens, 250); // 50 + 80 + 120
    assert.equal(event.total_tokens, 970); // 600 + 120 + 250
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity preserves separate historical UTC timestamps for cross-day multi-turn sessions", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-usage-crossday-"));
  try {
    const sessionId = "104c0143-cfd7-4ff0-a270-3859ac2826d2";
    const sessionDir = path.join(fixtureRoot, sessionId);
    const logsDir = path.join(sessionDir, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    const transcriptPath = path.join(logsDir, "transcript.jsonl");

    writeFileSync(
      transcriptPath,
      [
        // Turn 0: Day 1 (2026-10-27)
        JSON.stringify({
          step_index: 0,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          created_at: "2026-10-27T16:14:00.000Z",
          content: "<USER_REQUEST>Day 1 question</USER_REQUEST>",
        }),
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-27T16:14:51.100Z",
          input_tokens: 120,
          cache_read_tokens: 10,
          output_tokens: 60,
          content: "Day 1 answer",
        }),
        // Turn 1: Day 2 (2026-10-28)
        JSON.stringify({
          step_index: 2,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          created_at: "2026-10-28T03:37:00.000Z",
          content: "<USER_REQUEST>Day 2 continuation</USER_REQUEST>",
        }),
        JSON.stringify({
          step_index: 3,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-28T03:37:20.250Z",
          input_tokens: 240,
          cache_read_tokens: 50,
          output_tokens: 90,
          content: "Day 2 answer",
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 2);
    assert.equal(events.length, 2);

    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-0`);
    assert.equal(events[0].timestamp, "2026-10-27T16:14:51.100Z");
    assert.equal(events[0].total_tokens, 190);

    assert.equal(events[1].turn_id, "turn-1");
    assert.equal(events[1].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[1].timestamp, "2026-10-28T03:37:20.250Z");
    assert.equal(events[1].total_tokens, 380);

    assert.ok(events[0].timestamp.startsWith("2026-10-27"));
    assert.ok(events[1].timestamp.startsWith("2026-10-28"));
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity filters out empty zero-token turns from usage event stream", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-usage-zerotoken-"));
  try {
    const sessionId = "33333333-3333-3333-3333-333333333333";
    const sessionDir = path.join(fixtureRoot, sessionId);
    const logsDir = path.join(sessionDir, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    const transcriptPath = path.join(logsDir, "transcript.jsonl");

    writeFileSync(
      transcriptPath,
      [
        // Turn 0: Zero token response
        JSON.stringify({
          step_index: 0,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          created_at: "2026-10-30T01:00:00.000Z",
          content: "<USER_REQUEST>Empty turn</USER_REQUEST>",
        }),
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-30T01:00:01.000Z",
          input_tokens: 0,
          cache_read_tokens: 0,
          output_tokens: 0,
          content: "",
        }),
        // Turn 1: Valid non-zero tokens
        JSON.stringify({
          step_index: 2,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          created_at: "2026-10-30T01:05:00.000Z",
          content: "<USER_REQUEST>Valid turn</USER_REQUEST>",
        }),
        JSON.stringify({
          step_index: 3,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-30T01:05:02.000Z",
          input_tokens: 80,
          cache_read_tokens: 10,
          output_tokens: 40,
          content: "Valid response",
        }),
        // Turn 2: User input interrupted, no model response
        JSON.stringify({
          step_index: 4,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          created_at: "2026-10-30T01:10:00.000Z",
          content: "<USER_REQUEST>Interrupted</USER_REQUEST>",
        }),
      ].join("\n")
    );

    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.usage_event_count, 1);
    assert.equal(events.length, 1);
    assert.equal(events[0].turn_id, "turn-1");
    assert.equal(events[0].external_event_id, `${sessionId}-turn-1`);
    assert.equal(events[0].total_tokens, 130);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity extracts usage in pure transcript environment without SQLite or Protobuf dependencies", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-usage-nosql-"));
  try {
    const sessionId = "44444444-4444-4444-4444-444444444444";
    const sessionDir = path.join(fixtureRoot, sessionId);
    const logsDir = path.join(sessionDir, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    const transcriptPath = path.join(logsDir, "transcript.jsonl");

    writeFileSync(
      transcriptPath,
      [
        JSON.stringify({
          step_index: 0,
          source: "USER_EXPLICIT",
          type: "USER_INPUT",
          created_at: "2026-10-30T02:00:00.000Z",
          content: "<USER_REQUEST>Pure transcript test</USER_REQUEST>",
        }),
        JSON.stringify({
          step_index: 1,
          source: "MODEL",
          type: "PLANNER_RESPONSE",
          created_at: "2026-10-30T02:00:01.000Z",
          input_tokens: 50,
          cache_read_tokens: 0,
          output_tokens: 25,
          content: "No SQL needed",
        }),
      ].join("\n")
    );

    // Verify absolutely no .db file exists in the directory tree
    const { events, complete } = readFixtureUsage(fixtureRoot);
    assert.equal(complete.snapshot_complete, true);
    assert.equal(complete.decoder_profile, "antigravity-transcript-v1");
    assert.equal(events.length, 1);
    assert.equal(events[0].session_id, sessionId);
    assert.equal(events[0].turn_id, "turn-0");
    assert.equal(events[0].total_tokens, 75);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

test("Antigravity supports incremental cursor skipping for unchanged transcript files and rescanning on update", () => {
  const fixtureRoot = mkdtempSync(path.join(tmpdir(), "assetiweave-antigravity-usage-cursor-"));
  try {
    const sessionId = "55555555-5555-5555-5555-555555555555";
    const sessionDir = path.join(fixtureRoot, sessionId);
    const logsDir = path.join(sessionDir, ".system_generated", "logs");
    mkdirSync(logsDir, { recursive: true });
    const transcriptPath = path.join(logsDir, "transcript.jsonl");

    const turn0Lines = [
      JSON.stringify({
        step_index: 0,
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-10-30T03:00:00.000Z",
        content: "<USER_REQUEST>Initial prompt</USER_REQUEST>",
      }),
      JSON.stringify({
        step_index: 1,
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-10-30T03:00:01.000Z",
        input_tokens: 100,
        output_tokens: 50,
        content: "Initial answer",
      }),
    ];

    writeFileSync(transcriptPath, turn0Lines.join("\n"));

    // 1. Initial scan
    const run1 = readFixtureUsage(fixtureRoot);
    assert.equal(run1.complete.usage_event_count, 1);
    assert.equal(run1.events.length, 1);
    assert.ok(run1.complete.next_cursor);
    const cursor1 = JSON.parse(run1.complete.next_cursor);
    assert.ok(cursor1.last_mtime > 0);
    assert.ok(cursor1.processed_files.some((f) => f.includes("transcript.jsonl")));

    // 2. Second scan with cursor and no file changes -> must skip
    const run2 = readFixtureUsage(fixtureRoot, { cursor: run1.complete.next_cursor });
    assert.equal(run2.complete.usage_event_count, 0);
    assert.equal(run2.events.length, 0);

    // 3. Append new turn to file with updated mtime
    const stat1 = statSync(transcriptPath);
    const turn1Lines = [
      ...turn0Lines,
      JSON.stringify({
        step_index: 2,
        source: "USER_EXPLICIT",
        type: "USER_INPUT",
        created_at: "2026-10-30T03:10:00.000Z",
        content: "<USER_REQUEST>Follow-up</USER_REQUEST>",
      }),
      JSON.stringify({
        step_index: 3,
        source: "MODEL",
        type: "PLANNER_RESPONSE",
        created_at: "2026-10-30T03:10:02.000Z",
        input_tokens: 200,
        output_tokens: 80,
        content: "Follow-up answer",
      }),
    ];
    writeFileSync(transcriptPath, turn1Lines.join("\n"));
    utimesSync(transcriptPath, (stat1.mtimeMs + 2000) / 1000, (stat1.mtimeMs + 2000) / 1000);

    // 4. Scan again with cursor -> modified file must be rescanned
    const run3 = readFixtureUsage(fixtureRoot, { cursor: run1.complete.next_cursor });
    assert.equal(run3.complete.usage_event_count, 2);
    assert.equal(run3.events.length, 2);
    assert.equal(run3.events[0].turn_id, "turn-0");
    assert.equal(run3.events[1].turn_id, "turn-1");
    const cursor3 = JSON.parse(run3.complete.next_cursor);
    assert.ok(cursor3.last_mtime > cursor1.last_mtime);
  } finally {
    rmSync(fixtureRoot, { force: true, recursive: true });
  }
});

