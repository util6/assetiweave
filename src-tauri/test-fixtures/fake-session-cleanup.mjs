import fs from "node:fs";

const recordPath = process.env.ASSETIWEAVE_FAKE_ACP_RECORD_PATH;
const cleanupMode = process.env.ASSETIWEAVE_FAKE_SESSION_CLEANUP_MODE ?? "success";
const sessionId = process.argv[2];

const priorRecords = recordPath && fs.existsSync(recordPath)
  ? fs.readFileSync(recordPath, "utf8")
  : "";

if (recordPath) {
  fs.appendFileSync(
    recordPath,
    `${JSON.stringify({
      event: "fallback_delete",
      sessionId,
      originalProcessReaped: priorRecords.includes('"event":"sigterm"'),
      workspaceExists: fs.existsSync(process.cwd()),
    })}\n`,
  );
}

if (cleanupMode === "not_found") {
  process.stderr.write(`Session not found: ${sessionId}\n`);
  process.exit(1);
}

if (cleanupMode === "timeout") {
  setInterval(() => {}, 1_000);
} else {
  process.exit(cleanupMode === "failure" ? 23 : 0);
}
