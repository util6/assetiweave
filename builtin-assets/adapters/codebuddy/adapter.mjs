#!/usr/bin/env node
/**
 * @file Tencent CodeBuddy 统一会话解析适配器 (CodeBuddy & WorkBuddy Conversation Adapter)
 * @description 统一适配腾讯智能体生态，涵盖 CodeBuddy (CLI) 与 WorkBuddy (GUI)。
 *              自动多根目录探测 (~/.codebuddy 与 ~/.workbuddy)，
 *              读取 JSONL 交互流与 WorkBuddy SQLite 索引库，
 *              规范输出符合 Card Contract v1 的标准化会话。
 */
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { normalizeSessionPayload } from "./payload-policy.mjs";
import shellProjector from "./shell-projector.cjs";
const { projectCommandParts, SHELL_PROJECTOR_VERSION } = shellProjector;

const input = JSON.parse(readFileSync(0, "utf8") || "{}");
const ADAPTER_ID = "codebuddy";
const CONTENT_CARD_SCHEMA_VERSION = "codebuddy-content-cards-v1";

process.stdout.on("error", (err) => {
  if (err.code === "EPIPE") process.exit(0);
});

function emit(type, payload = {}) {
  process.stdout.write(`${JSON.stringify({ type, ...payload })}\n`);
}

function fail(message) {
  emit("error", { message: message instanceof Error ? message.message : String(message) });
  emit("complete", { item: {} });
}

function emitProgress(progress = {}) {
  emit("progress", {
    progress: {
      stage: progress.stage ?? "reading",
      operation: progress.operation ?? "scanning",
      worker: progress.worker ?? process.env.ASSETIWEAVE_WORKER_ID ?? undefined,
      path: progress.path,
      current: progress.current,
      total: progress.total,
    },
  });
}

function expandPath(value) {
  if (!value) return value;
  if (value === "~") return homedir();
  if (value.startsWith("~/")) return path.join(homedir(), value.slice(2));
  return value;
}

function sha256(text) {
  return createHash("sha256").update(text).digest("hex");
}

function fileVersionToken(filePath) {
  const stat = statSync(filePath);
  return sha256(`${CONTENT_CARD_SCHEMA_VERSION}\0${stat.size}\0${stat.mtimeMs}`);
}

function readStableFile(filePath) {
  for (let attempt = 0; attempt < 2; attempt++) {
    const before = fileVersionToken(filePath);
    const text = readFileSync(filePath, "utf8");
    const after = fileVersionToken(filePath);
    if (before === after) return { text, versionToken: after };
  }
  throw new Error(`session changed while being read: ${filePath}`);
}

function detectClientFlavor(filePath) {
  if (!filePath) return "codebuddy";
  const norm = filePath.toLowerCase();
  if (norm.includes(".workbuddy") || norm.includes("workbuddy")) return "workbuddy";
  return "codebuddy-cli";
}

function resolveCandidateRoots(startLocation) {
  const roots = new Set();
  if (startLocation) {
    const resolved = expandPath(startLocation);
    if (existsSync(resolved)) {
      roots.add(resolved);
      return Array.from(roots);
    }
  }
  const defaultCli = path.join(homedir(), ".codebuddy");
  const defaultGui = path.join(homedir(), ".workbuddy");
  if (existsSync(defaultCli)) roots.add(defaultCli);
  if (existsSync(defaultGui)) roots.add(defaultGui);
  return Array.from(roots);
}

function collectJsonlFiles(location) {
  const resolved = expandPath(location);
  if (!resolved || !existsSync(resolved)) return [];
  const stat = statSync(resolved);
  if (stat.isFile()) {
    return resolved.endsWith(".jsonl") && !resolved.endsWith(".ndjson") ? [resolved] : [];
  }
  const projectsDir = path.basename(resolved) === "projects"
    ? resolved
    : existsSync(path.join(resolved, "projects"))
      ? path.join(resolved, "projects")
      : resolved;

  const results = [];
  function walk(dir, depth = 0) {
    if (depth > 6 || !existsSync(dir)) return;
    try {
      const entries = readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(fullPath, depth + 1);
        } else if (entry.isFile() && entry.name.endsWith(".jsonl") && !entry.name.endsWith(".ndjson")) {
          results.push(fullPath);
        }
      }
    } catch {}
  }
  walk(projectsDir);
  return results;
}

function createUnifiedNewFileDiff(targetPath, content) {
  const norm = String(targetPath || "file").replace(/^[/\\]+/, "");
  const lines = String(content ?? "").replace(/\r\n/g, "\n").split("\n");
  if (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return [
    `diff --git a/${norm} b/${norm}`,
    "new file mode 100644",
    "--- /dev/null",
    `+++ b/${norm}`,
    `@@ -0,0 +1,${lines.length} @@`,
    ...lines.map((l) => `+${l}`),
  ].join("\n");
}

function createUnifiedReplaceDiff(targetPath, oldStr, newStr) {
  const norm = String(targetPath || "file").replace(/^[/\\]+/, "");
  const oldLines = String(oldStr ?? "").replace(/\r\n/g, "\n").split("\n");
  const newLines = String(newStr ?? "").replace(/\r\n/g, "\n").split("\n");
  if (oldLines.length > 0 && oldLines[oldLines.length - 1] === "") oldLines.pop();
  if (newLines.length > 0 && newLines[newLines.length - 1] === "") newLines.pop();
  return [
    `diff --git a/${norm} b/${norm}`,
    `--- a/${norm}`,
    `+++ b/${norm}`,
    `@@ -1,${oldLines.length} +1,${newLines.length} @@`,
    ...oldLines.map((l) => `-${l}`),
    ...newLines.map((l) => `+${l}`),
  ].join("\n");
}

function extractBashOutput(text) {
  const stdoutM = text.match(/Stdout:\s*([\s\S]*?)(?:\nStderr:|$)/i);
  const stderrM = text.match(/Stderr:\s*([\s\S]*?)(?:\nExit Code:|$)/i);
  const out = (stdoutM ? stdoutM[1].trim() : "").replace(/^\(empty\)$/, "");
  const err = (stderrM ? stderrM[1].trim() : "").replace(/^\(empty\)$/, "");
  if (out && err) return `${out}\n${err}`;
  if (out) return out;
  if (err) return err;
  return text.replace(/^Command:.*?\n/i, "").replace(/Exit Code:.*$/i, "").trim();
}

function findSubagentsForSession(filePath, sessionId) {
  if (!filePath || !sessionId) return [];
  const subagentsDir = path.join(path.dirname(filePath), sessionId, "subagents");
  if (!existsSync(subagentsDir)) return [];
  const list = [];
  try {
    for (const entry of readdirSync(subagentsDir, { withFileTypes: true })) {
      if (entry.isFile() && entry.name.endsWith(".jsonl")) {
        const subPath = path.join(subagentsDir, entry.name);
        const id = path.basename(entry.name, ".jsonl");
        let promptSnippet = "";
        try {
          const firstLine = readFileSync(subPath, "utf8").split("\n")[0];
          if (firstLine) {
            const first = JSON.parse(firstLine);
            promptSnippet = first.content?.[0]?.text || "";
          }
        } catch {}
        list.push({ id, path: subPath, prompt: promptSnippet });
      }
    }
  } catch {}
  return list;
}

function isSubagentSessionPath(filePath) {
  if (!filePath) return false;
  return filePath.includes("/subagents/") || path.basename(path.dirname(filePath)) === "subagents";
}

function queryWorkbuddyDbTitles() {
  const dbPath = path.join(homedir(), ".workbuddy", "workbuddy.db");
  const map = new Map();
  if (!existsSync(dbPath)) return map;
  try {
    const sql = "SELECT id, title, custom_title, cwd FROM sessions WHERE deleted_at IS NULL;";
    const res = spawnSync("sqlite3", ["-json", dbPath, sql], {
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
    if (res.status === 0 && res.stdout) {
      const rows = JSON.parse(res.stdout);
      for (const row of rows) {
        if (row.id) {
          map.set(row.id, {
            title: row.custom_title || row.title || null,
            projectPath: row.cwd || null,
          });
        }
      }
    }
  } catch {}
  return map;
}

function extractToolOutputText(rawOutput) {
  if (rawOutput == null) return "";
  if (typeof rawOutput === "string") return rawOutput;
  if (Array.isArray(rawOutput)) {
    return rawOutput
      .map((item) => {
        if (!item) return "";
        if (typeof item === "string") return item;
        const text = item.text || item.content || item.output || "";
        if (typeof text === "string" && text.trim().startsWith("{")) {
          try {
            const parsed = JSON.parse(text);
            if (parsed.type === "present_files_result") {
              const expl = parsed.explanation || parsed.message || "";
              const files = Array.isArray(parsed.files) ? parsed.files.join("\n") : "";
              return [expl, files].filter(Boolean).join("\n");
            }
          } catch {}
        }
        return typeof text === "string" ? text : JSON.stringify(item);
      })
      .filter(Boolean)
      .join("\n");
  }
  if (typeof rawOutput === "object") {
    if (typeof rawOutput.text === "string") return rawOutput.text;
    if (typeof rawOutput.content === "string") return rawOutput.content;
    return JSON.stringify(rawOutput);
  }
  return String(rawOutput);
}

function extractReasoningText(entry) {
  if (!entry) return null;
  if (Array.isArray(entry.rawContent)) {
    const texts = entry.rawContent
      .filter((c) => c && (c.type === "reasoning_text" || c.type === "thinking" || c.text))
      .map((c) => c.text || c.thinking || "")
      .filter(Boolean);
    if (texts.length) return texts.join("\n\n");
  }
  if (Array.isArray(entry.content)) {
    const texts = entry.content
      .filter((c) => c && (c.type === "reasoning" || c.type === "thinking" || c.text))
      .map((c) => c.text || c.thinking || "")
      .filter(Boolean);
    if (texts.length) return texts.join("\n\n");
  }
  if (typeof entry.text === "string" && entry.text.trim()) return entry.text.trim();
  return null;
}

function extractUserPrompt(text) {
  if (!text || typeof text !== "string") return "";
  const queryMatch = text.match(/<user_query>([\s\S]*?)<\/user_query>/i);
  if (queryMatch) return queryMatch[1].trim();

  let cleaned = text
    .replace(/<system-reminder[\s\S]*?<\/system-reminder>/gi, "")
    .replace(/<user-context[\s\S]*?<\/user-context>/gi, "")
    .replace(/<command-name>[\s\S]*?<\/command-name>/gi, "")
    .replace(/<local-command-stdout>[\s\S]*?<\/local-command-stdout>/gi, "");
  return cleaned.trim();
}

function parseJsonl(text, sessionIdFallback, filePath) {
  const lines = text.trim().split("\n");
  let sessionId = sessionIdFallback;
  let projectPath = null;
  let sessionTitle = null;
  const turns = [];
  let currentTurn = null;
  const pendingToolCalls = new Map();
  const subagentFiles = findSubagentsForSession(filePath, sessionIdFallback);
  let subagentIndex = 0;

  for (const line of lines) {
    if (!line.trim()) continue;
    let entry;
    try {
      entry = JSON.parse(line);
    } catch {
      continue;
    }

    if (entry.sessionId && !sessionId) sessionId = entry.sessionId;
    if (entry.cwd && !projectPath) projectPath = entry.cwd;
    if (entry.type === "summary" && entry.summary && !sessionTitle) {
      sessionTitle = entry.summary;
    }

    if (entry.type === "message" && entry.role === "user") {
      if (entry.providerData?.skipRun === true) continue;
      const rawText = entry.content?.map((c) => c.text || "").join("") || "";
      const prompt = extractUserPrompt(rawText);
      if (!prompt) continue;

      if (currentTurn) {
        turns.push(currentTurn);
      }
      currentTurn = {
        external_id: entry.id || `turn-${turns.length}`,
        turn_index: turns.length,
        user_text: prompt,
        title: null,
        started_at: entry.timestamp ? new Date(entry.timestamp).toISOString() : null,
        ended_at: null,
        model: null,
        parts: [
          {
            role: "user",
            kind: "text",
            text: prompt,
            language: null,
            command: null,
            cwd: entry.cwd || null,
            status: null,
            exit_code: null,
            source_execution_id: null,
            metadata_json: null,
          },
        ],
      };
      continue;
    }

    if (!currentTurn) continue;

    if (entry.type === "reasoning") {
      const reasoningText = extractReasoningText(entry);
      if (reasoningText && reasoningText.trim()) {
        currentTurn.parts.push({
          role: "assistant",
          kind: "text",
          text: reasoningText.trim(),
          language: null,
          command: null,
          cwd: entry.cwd || null,
          status: null,
          exit_code: null,
          source_execution_id: null,
          metadata_json: JSON.stringify({ source_type: "reasoning" }),
          content_card: {
            schema_version: 1,
            kind: `${ADAPTER_ID}.reasoning`,
            semantic_role: "reasoning",
            renderer: "markdown",
          },
        });
      }
      if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
      continue;
    }

    if (
      entry.type === "function_call" ||
      (entry.type === "message" && entry.role === "assistant" && entry.content?.some((c) => c.type === "tool_use"))
    ) {
      const toolUseItem = entry.content?.find((c) => c.type === "tool_use");
      const toolName = entry.name || toolUseItem?.name || "tool";
      const callId = entry.callId || entry.id || toolUseItem?.id;
      let args = entry.arguments || toolUseItem?.input;
      if (typeof args === "string") {
        try { args = JSON.parse(args); } catch {}
      }
      pendingToolCalls.set(callId, { toolName, args });

      // Subagent dispatch (Agent tool)
      if (toolName === "Agent") {
        let matchedChildId = null;
        if (subagentFiles.length > 0) {
          const desc = args?.description || "";
          const p = args?.prompt || "";
          const found = subagentFiles.find((s) => (desc && s.prompt.includes(desc)) || (p && s.prompt.includes(p.slice(0, 30))));
          if (found) {
            matchedChildId = found.id;
          } else if (subagentIndex < subagentFiles.length) {
            matchedChildId = subagentFiles[subagentIndex].id;
            subagentIndex += 1;
          }
        }
        const agentData = {
          agent_role: args?.subagent_type || "Explore",
          description: args?.description || "Subagent Task",
          task: args?.prompt || args?.description || "",
          child_session_id: matchedChildId || undefined,
        };
        currentTurn.parts.push({
          role: "tool",
          kind: "subagent",
          text: JSON.stringify(agentData, null, 2),
          language: null,
          command: null,
          cwd: entry.cwd || null,
          status: "completed",
          exit_code: 0,
          source_execution_id: callId,
          metadata_json: JSON.stringify({
            tool_name: "Agent",
            agent_role: args?.subagent_type || "Explore",
            child_session_id: matchedChildId,
            description: args?.description,
          }),
          content_card: {
            schema_version: 1,
            kind: `${ADAPTER_ID}.subagent`,
            semantic_role: "subagent",
            renderer: "subagent_tree",
          },
        });
        if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
        continue;
      }

      // Write (Create file diff)
      if (toolName === "Write" && args?.path && args?.content != null) {
        const diffText = createUnifiedNewFileDiff(args.path, args.content);
        currentTurn.parts.push({
          role: "tool",
          kind: "file_change",
          text: diffText,
          language: null,
          command: null,
          cwd: entry.cwd || null,
          status: "success",
          exit_code: 0,
          source_execution_id: callId,
          metadata_json: JSON.stringify({ tool_name: toolName, execution_kind: "file_change", file_path: args.path }),
          content_card: {
            schema_version: 1,
            kind: `${ADAPTER_ID}.file-change`,
            semantic_role: "file-change",
            renderer: "diff",
          },
        });
        if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
        continue;
      }

      // Edit (Replace content diff)
      if (toolName === "Edit" && args?.path && args?.old_string != null && args?.new_string != null) {
        const diffText = createUnifiedReplaceDiff(args.path, args.old_string, args.new_string);
        currentTurn.parts.push({
          role: "tool",
          kind: "file_change",
          text: diffText,
          language: null,
          command: null,
          cwd: entry.cwd || null,
          status: "success",
          exit_code: 0,
          source_execution_id: callId,
          metadata_json: JSON.stringify({ tool_name: toolName, execution_kind: "file_change", file_path: args.path }),
          content_card: {
            schema_version: 1,
            kind: `${ADAPTER_ID}.file-change`,
            semantic_role: "file-change",
            renderer: "diff",
          },
        });
        if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
        continue;
      }

      // Ambient Read / Search tools
      const isRead = ["Read", "present_files", "show_widget", "widget_guidelines"].includes(toolName);
      const isSearch = ["Grep", "Glob", "WebSearch"].includes(toolName);
      const executionKind = isRead ? "read" : isSearch ? "search" : null;
      const signal = executionKind ? "ambient" : undefined;

      let command = null;
      let desc = null;
      let kind = "tool";
      if (toolName === "Bash" && args?.command) {
        kind = "command";
        command = args.command;
        desc = args.description || args.command;
      } else {
        desc = typeof args === "object" ? JSON.stringify(args) : String(args || "");
      }

      currentTurn.parts.push({
        role: "tool",
        kind,
        text: desc,
        language: null,
        command,
        cwd: entry.cwd || null,
        status: null,
        exit_code: null,
        source_execution_id: callId,
        metadata_json: JSON.stringify({ tool_name: toolName, execution_kind: executionKind, signal, file_path: args?.path }),
        content_card: {
          schema_version: 1,
          kind: kind === "command" ? `${ADAPTER_ID}.command` : `${ADAPTER_ID}.tool`,
          renderer: kind === "command" ? "command" : "plain",
        },
      });
      if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
      continue;
    }

    if (entry.type === "function_call_result" || (entry.type === "message" && entry.role === "tool")) {
      const callId = entry.callId || entry.id;
      const matchedCall = pendingToolCalls.get(callId);
      const matchedTool = matchedCall?.toolName;

      // If matchedCall was Write or Edit, the file change diff was already emitted
      if (matchedTool === "Write" || matchedTool === "Edit") {
        if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
        continue;
      }

      // If matchedCall was Agent, the subagent card was already emitted
      if (matchedTool === "Agent") {
        if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
        continue;
      }

      let outputText = extractToolOutputText(entry.output);

      let status = entry.status === "failed" ? "failed" : "success";
      let exitCode = entry.providerData?.toolResult?.rawResponse?.exitCode;

      const isRead = ["Read", "present_files", "show_widget", "widget_guidelines"].includes(matchedTool);
      const isSearch = ["Grep", "Glob", "WebSearch"].includes(matchedTool);
      const executionKind = isRead ? "read" : isSearch ? "search" : null;
      const signal = executionKind ? "ambient" : undefined;

      if (matchedTool === "Bash") {
        if (exitCode == null) {
          const m = outputText.match(/Exit Code:\s*(\d+)/i);
          if (m) exitCode = parseInt(m[1], 10);
        }
        if (exitCode == null) exitCode = (status === "failed" ? 1 : 0);
        status = exitCode === 0 ? "success" : "failed";
        outputText = extractBashOutput(outputText);
      }

      currentTurn.parts.push({
        role: "tool",
        kind: "tool",
        text: outputText,
        language: null,
        command: null,
        cwd: entry.cwd || null,
        status,
        exit_code: exitCode,
        source_execution_id: callId,
        metadata_json: JSON.stringify({
          status,
          exit_code: exitCode,
          execution_kind: executionKind,
          signal,
          file_path: matchedCall?.args?.path,
          tool_name: matchedTool,
        }),
        content_card: {
          schema_version: 1,
          kind: `${ADAPTER_ID}.result`,
          renderer: matchedTool === "Bash" ? "terminal_output" : "plain",
        },
      });
      if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
      continue;
    }

    if (entry.type === "message" && entry.role === "assistant") {
      const items = Array.isArray(entry.content)
        ? entry.content
        : (typeof entry.content === "string" && entry.content.trim()
            ? [{ type: "output_text", text: entry.content }]
            : (typeof entry.text === "string" && entry.text.trim()
                ? [{ type: "output_text", text: entry.text }]
                : []));

      for (const c of items) {
        if (!c) continue;
        if (typeof c === "string") {
          if (c.trim()) {
            currentTurn.parts.push({
              role: "assistant",
              kind: "text",
              text: c.trim(),
              language: null,
              command: null,
              cwd: entry.cwd || null,
              status: null,
              exit_code: null,
              source_execution_id: null,
              metadata_json: null,
              content_card: {
                schema_version: 1,
                kind: `${ADAPTER_ID}.answer`,
                semantic_role: "answer",
                renderer: "markdown",
              },
            });
          }
          continue;
        }

        if (c.type === "reasoning" || c.type === "thinking") {
          const reasoningText = c.text || c.thinking || "";
          if (reasoningText.trim()) {
            currentTurn.parts.push({
              role: "assistant",
              kind: "text",
              text: reasoningText.trim(),
              language: null,
              command: null,
              cwd: entry.cwd || null,
              status: null,
              exit_code: null,
              source_execution_id: null,
              metadata_json: JSON.stringify({ source_type: c.type }),
              content_card: {
                schema_version: 1,
                kind: `${ADAPTER_ID}.reasoning`,
                semantic_role: "reasoning",
                renderer: "markdown",
              },
            });
          }
        } else if (c.type === "text" || c.type === "output_text" || !c.type) {
          const answerText = typeof c.text === "string" ? c.text : (typeof c.content === "string" ? c.content : "");
          if (answerText.trim()) {
            currentTurn.parts.push({
              role: "assistant",
              kind: "text",
              text: answerText,
              language: null,
              command: null,
              cwd: entry.cwd || null,
              status: null,
              exit_code: null,
              source_execution_id: null,
              metadata_json: null,
              content_card: {
                schema_version: 1,
                kind: `${ADAPTER_ID}.answer`,
                semantic_role: "answer",
                renderer: "markdown",
              },
            });
          }
        }
      }
      if (entry.timestamp) currentTurn.ended_at = new Date(entry.timestamp).toISOString();
      if (!currentTurn.model && (entry.providerData?.model || entry.providerData?.requestModelId)) {
        currentTurn.model = entry.providerData.model || entry.providerData.requestModelId;
      }
      continue;
    }
  }

  if (currentTurn) {
    turns.push(currentTurn);
  }

  return {
    sessionId,
    projectPath,
    sessionTitle,
    turns,
  };
}

function resolveAllJsonlFiles() {
  const roots = resolveCandidateRoots(input.source?.location);
  const seenLocators = new Set();
  const allFiles = [];
  for (const r of roots) {
    for (const f of collectJsonlFiles(r)) {
      if (!seenLocators.has(f)) {
        seenLocators.add(f);
        allFiles.push(f);
      }
    }
  }
  return allFiles;
}

function listSessions() {
  const allFiles = resolveAllJsonlFiles();
  const descriptors = [];
  const dbTitles = queryWorkbuddyDbTitles();

  for (const filePath of allFiles) {
    try {
      const { text, versionToken } = readStableFile(filePath);
      const sid = path.basename(filePath, ".jsonl");
      const parsed = parseJsonl(text, sid, filePath);
      if (!parsed.turns.length) continue;

      const stat = statSync(filePath);
      const dbInfo = dbTitles.get(parsed.sessionId || sid);
      const isSubagent = isSubagentSessionPath(filePath);
      const title = isSubagent
        ? `[子代理] ${parsed.sessionTitle || parsed.turns[0]?.user_text?.slice(0, 40) || sid}`
        : (dbInfo?.title || parsed.sessionTitle || parsed.turns[0]?.user_text?.slice(0, 80) || "CodeBuddy 会话");
      descriptors.push({
        external_id: parsed.sessionId || sid,
        title,
        project_path: dbInfo?.projectPath || parsed.projectPath || null,
        started_at: parsed.turns[0]?.started_at ?? null,
        updated_at: parsed.turns.at(-1)?.ended_at ?? parsed.turns.at(-1)?.started_at ?? stat.mtime.toISOString(),
        source_locator: filePath,
        version_token: versionToken,
      });
    } catch {}
  }
  return descriptors;
}

function readSession() {
  const requestedSessionId = input.params?.session_id ?? null;
  const requestedLocator = input.params?.source_locator ? expandPath(input.params.source_locator) : null;
  const dbTitles = queryWorkbuddyDbTitles();

  const parseAndBuildSession = (filePath) => {
    const { text, versionToken } = readStableFile(filePath);
    const sid = path.basename(filePath, ".jsonl");
    const parsed = parseJsonl(text, sid, filePath);
    if (!parsed.turns.length) return null;

    const dbInfo = dbTitles.get(parsed.sessionId || sid);
    const isSubagent = isSubagentSessionPath(filePath);
    const origin = isSubagent ? "subagent" : detectClientFlavor(filePath);
    const title = isSubagent
      ? `[子代理] ${parsed.sessionTitle || parsed.turns[0]?.user_text?.slice(0, 40) || sid}`
      : (dbInfo?.title || parsed.sessionTitle || parsed.turns[0]?.user_text?.slice(0, 80) || "CodeBuddy 会话");
    const userVisible = !isSubagent;

    const session = {
      external_id: parsed.sessionId || sid,
      title,
      project_path: dbInfo?.projectPath || parsed.projectPath || null,
      started_at: parsed.turns[0]?.started_at ?? null,
      updated_at: parsed.turns.at(-1)?.ended_at ?? parsed.turns.at(-1)?.started_at ?? null,
      source_locator: filePath,
      source_fingerprint: versionToken,
      execution_origin: origin,
      execution_purpose: isSubagent ? "Explore" : origin,
      user_visible: userVisible,
      turns: parsed.turns,
    };
    return normalizeSessionPayload(session);
  };

  if (requestedLocator && existsSync(requestedLocator)) {
    const session = parseAndBuildSession(requestedLocator);
    return session ? [session] : [];
  }

  const allFiles = resolveAllJsonlFiles();
  const sessions = [];
  for (const filePath of allFiles) {
    if (requestedSessionId && path.basename(filePath, ".jsonl") !== String(requestedSessionId)) {
      continue;
    }
    const session = parseAndBuildSession(filePath);
    if (session) sessions.push(session);
  }
  return sessions;
}

function readUsage() {
  const allFiles = resolveAllJsonlFiles();
  const cursorRaw = input.params?.cursor;
  let cursor = null;
  if (cursorRaw) {
    try {
      cursor = typeof cursorRaw === "string" ? JSON.parse(cursorRaw) : cursorRaw;
    } catch {}
  }
  const cursorMtime = cursor?.last_mtime ? Number(cursor.last_mtime) : 0;
  const processedFiles = new Set(cursor?.processed_files || []);

  let maxMtime = cursorMtime;
  let eventCount = 0;
  const newProcessedFiles = [];

  for (const filePath of allFiles) {
    try {
      const stat = statSync(filePath);
      const mtimeMs = stat.mtimeMs;
      if (mtimeMs > maxMtime) maxMtime = mtimeMs;
      if (cursorMtime > 0 && mtimeMs <= cursorMtime && processedFiles.has(filePath)) {
        newProcessedFiles.push(filePath);
        continue;
      }

      const sessionId = path.basename(filePath, ".jsonl");
      const text = readFileSync(filePath, "utf8");
      const lines = text.split("\n");

      let lineIdx = 0;
      for (const line of lines) {
        lineIdx++;
        if (!line.trim()) continue;
        let p;
        try { p = JSON.parse(line); } catch { continue; }

        const usage = p.message?.usage || p.usage || p.providerData?.rawUsage;
        if (!usage) continue;

        const inputTokens = Number(usage.input_tokens || usage.prompt_tokens || 0);
        const outputTokens = Number(usage.output_tokens || usage.completion_tokens || 0);
        const cacheReadTokens = Number(usage.cache_read_input_tokens || usage.prompt_cache_hit_tokens || 0);
        const cacheWriteTokens = Number(usage.cache_creation_input_tokens || usage.cache_write_tokens || 0);
        const reasoningTokens = Number(usage.reasoning_tokens || usage.completion_thinking_tokens || 0);
        const totalTokens = Number(usage.total_tokens || (inputTokens + outputTokens));

        if (totalTokens <= 0) continue;

        const eventId = String(p.id || `${sessionId}-${lineIdx}`);
        const timestamp = p.timestamp ? new Date(p.timestamp).toISOString() : stat.mtime.toISOString();
        const modelStr = String(p.providerData?.model || p.providerData?.requestModelName || p.model || "codebuddy-model");

        const event = {
          external_event_id: eventId,
          session_id: sessionId,
          turn_id: null,
          logical_request_id: eventId,
          attempt_index: 0,
          timestamp,
          provider: "tencent",
          model: modelStr,
          input_tokens: inputTokens,
          output_tokens: outputTokens,
          cache_read_tokens: cacheReadTokens,
          cache_write_tokens: cacheWriteTokens,
          reasoning_tokens: reasoningTokens,
          total_tokens: totalTokens,
          status: "success",
          currency: null,
          cost: null,
          metadata: {
            line: lineIdx,
            file: path.basename(filePath),
          },
        };
        emit("usage_event", { usage_event: event });
        eventCount++;
      }
      newProcessedFiles.push(filePath);
    } catch {}
  }

  emit("complete", {
    item: {
      snapshot_complete: true,
      usage_event_count: eventCount,
      next_cursor: JSON.stringify({
        last_mtime: maxMtime,
        processed_files: newProcessedFiles.slice(-2000),
      }),
      decoder_profile: "codebuddy-transcript-v1",
      diagnostics: [],
    },
  });
}

try {
  if (input.method === "project_command_parts") {
    const projections = projectCommandParts(input.params?.parts ?? input.params?.command_parts);
    for (const projection of projections) emit("item", { item: { kind: "command_projection", ...projection } });
    emit("complete", { item: { projection_count: projections.length, projector_version: SHELL_PROJECTOR_VERSION } });
  } else if (input.method === "probe") {
    emit("complete", { item: { session_count: 0 } });
  } else if (input.method === "list_sessions") {
    emitProgress({ stage: "reading", operation: "list_sessions" });
    const descriptors = listSessions();
    for (let i = 0; i < descriptors.length; i += 1) {
      const descriptor = descriptors[i];
      if (i === 0 || i === descriptors.length - 1 || (i + 1) % 10 === 0) {
        emitProgress({
          stage: "reading",
          operation: "list_sessions",
          current: i + 1,
          total: descriptors.length,
          path: descriptor.external_id,
        });
      }
      emit("item", { item: { kind: "session_descriptor", ...descriptor } });
    }
    emit("complete", { item: { session_count: descriptors.length, snapshot_complete: true } });
  } else if (input.method === "read_session") {
    emitProgress({ stage: "reading", operation: "read_session" });
    const sessions = readSession();
    for (let i = 0; i < sessions.length; i += 1) {
      const session = sessions[i];
      emitProgress({
        stage: "reading",
        operation: "read_session",
        current: i + 1,
        total: sessions.length,
        path: session.external_id,
      });
      emit("item", { item: { kind: "session", session } });
    }
    emit("complete", { item: { session_count: sessions.length } });
  } else if (input.method === "read_usage") {
    emitProgress({ stage: "reading", operation: "read_usage" });
    readUsage();
  } else {
    fail(`unsupported method: ${input.method}`);
  }
} catch (error) {
  fail(error);
}
