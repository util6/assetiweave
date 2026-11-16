import path from "node:path";

export const PAYLOAD_POLICY_VERSION = 13;

const SUCCESS_STATUSES = new Set(["success", "succeeded", "completed", "complete", "done", "ok"]);
const FAILURE_STATUS = /^(error|failed|failure|cancelled|canceled|interrupted|timeout|timed_out)$/i;

const EXACT_READ_TOOLS = new Set([
  "view_file", "read_file", "read_url_content", "list_dir", "list_directory", "read_dir", "read_resource", "fetch_web_page",
]);

const EXACT_SEARCH_TOOLS = new Set([
  "grep_search", "search_web", "search_docs", "find_files", "file_search", "glob", "locate_files",
]);

const EXACT_FILE_CHANGE_TOOLS = new Set([
  "write_to_file", "write_file", "replace_file_content", "edit_file", "apply_patch", "create_file", "patch", "str_replace_editor", "code_action",
]);

const EXACT_PLAN_TOOLS = new Set([
  "update_plan", "todo_list", "task_plan",
]);

const EXACT_SUBAGENT_TOOLS = new Set([
  "invoke_subagent", "subagent", "agent_task", "spawn_agent",
]);

function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== ""),
  );
}

/**
 * Classifies and trims execution payloads. A shell or tool execution remains one persisted command/invocation Part;
 * display-only command fragments are recorded as a deterministic projection in metadata. Result/
 * command association is performed with an exact source_execution_id.
 */
export function normalizeSessionPayload(session) {
  for (const turn of Array.isArray(session?.turns) ? session.turns : []) {
    const originalParts = Array.isArray(turn?.parts) ? turn.parts : [];
    for (const part of originalParts) removePersistedShellProjection(part);
    const parts = originalParts;
    turn.parts = parts;
    const commands = new Map();
    const invocations = new Map();
    const results = new Map();

    for (const part of parts) {
      const executionId = executionIdOf(part);
      if (!executionId) continue;
      if (isCommandPart(part)) commands.set(executionId, part);
      if (!isResultPart(part)) invocations.set(executionId, part);
      else results.set(executionId, part);
    }

    for (const part of parts) {
      if (!part || typeof part !== "object" || !isCommandPart(part)) continue;
      const metadata = parseMetadata(part.metadata_json);
      const result = results.get(executionIdOf(part)) ?? null;
      const resultMetadata = parseMetadata(result?.metadata_json);
      const executionKind = classifyExecution(part, metadata, result, resultMetadata);
      const outcome = inferOutcome(part, result, executionKind);
      applyOutcome(part, outcome);

      if (executionKind === "read" || executionKind === "search") {
        const location = extractLocation(part.command ?? part.text, part.cwd, metadata);
        if (location) {
          part.command = formatLocation(location);
          part.text = null;
        }
        if (isSuccessful(part) || outcome.status === "completed") {
          metadata.signal = "ambient";
        }
        setExecutionMetadata(part, metadata, executionKind, location);
        continue;
      }
      if (executionKind === "unclassified") markUnclassified(part, metadata);
      else setExecutionMetadata(part, metadata, executionKind, null);
    }

    for (const part of parts) {
      if (!part || typeof part !== "object" || !isResultPart(part) || isCommandPart(part)) continue;
      const metadata = parseMetadata(part.metadata_json);
      const invocation = invocations.get(executionIdOf(part)) ?? null;
      const invocationMetadata = parseMetadata(invocation?.metadata_json);
      const executionKind = classifyExecution(part, metadata, invocation, invocationMetadata);
      const outcome = inferOutcome(invocation, part, executionKind);
      applyOutcome(part, outcome);

      if (isFailure(part)) {
        part.text = diagnosticText(part, metadata);
        metadata.signal = "focus";
        setExecutionMetadata(part, metadata, executionKind, null);
      } else if (part.kind === "file_change" || executionKind === "file_change") {
        part.kind = "file_change";
        if (!metadata.content_reduced) {
          part.text = canonicalUnifiedDiff(part.text);
        }
        if (part.text) setFileChangeCard(part, metadata);
        else setEmptyFileChangeResultCard(part, metadata);
      } else if (executionKind === "read" || executionKind === "search" || (
        executionKind !== "unclassified" && isSuccessful(part)
      )) {
        part.text = null;
        clearPayloadBudgetMetadata(metadata);
        if (executionKind === "read" || executionKind === "search") {
          metadata.signal = "ambient";
        }
        setExecutionMetadata(part, metadata, executionKind, null);
      } else if (executionKind === "unclassified") {
        markUnclassified(part, metadata);
      } else {
        setExecutionMetadata(part, metadata, executionKind, null);
      }
    }

    const shouldFilterEmpty = parts.some((p) => {
      const k = p?.content_card?.kind ?? "";
      return typeof k === "string" && (k.startsWith("codex.") || k.startsWith("antigravity."));
    });
    const splitParts = splitFileChangeParts(parts);
    turn.parts = shouldFilterEmpty ? splitParts.filter((part) => !isEmptyResultPayload(part)) : splitParts;
  }
  return session;
}

function splitFileChangeParts(parts) {
  return parts.flatMap((part) => {
    if (!part || typeof part !== "object" || part.kind !== "file_change" || !part.text) {
      return [part];
    }
    const files = splitUnifiedDiffFiles(part.text);
    if (files.length < 2) return [part];

    const metadata = parseMetadata(part.metadata_json);
    return files.map(({ filePath, text }, index) => {
      const filePart = {
        ...part,
        text,
        content_card: part.content_card ? { ...part.content_card } : part.content_card,
      };
      writeMetadata(filePart, {
        ...metadata,
        ...(filePath ? { file_path: filePath } : {}),
        file_change_index: index + 1,
        file_change_count: files.length,
      });
      return filePart;
    });
  });
}

function splitUnifiedDiffFiles(value) {
  const text = String(value ?? "").trimEnd();
  if (!text) return [];
  const lines = text.split("\n");
  let starts = lines.flatMap((line, index) => line.startsWith("diff --git ") ? [index] : []);
  if (starts.length < 2) {
    starts = lines.flatMap((line, index) => (
      isUnifiedFileHeaderPair(line, lines[index + 1]) ? [index] : []
    ));
  }
  if (starts.length < 2) return [{ filePath: diffFilePath(text), text }];
  if (starts[0] > 0) starts[0] = 0;

  return starts.map((start, index) => {
    const fileText = lines.slice(start, starts[index + 1] ?? lines.length).join("\n").trimEnd();
    return { filePath: diffFilePath(fileText), text: fileText };
  });
}

function isUnifiedFileHeaderPair(oldLine, newLine) {
  if (!oldLine?.startsWith("--- ") || !newLine?.startsWith("+++ ")) return false;
  const oldPath = oldLine.slice(4).split("\t", 1)[0].trim();
  const newPath = newLine.slice(4).split("\t", 1)[0].trim();
  if (oldPath === "/dev/null" || newPath === "/dev/null") return true;
  const decodedOld = decodeDiffPath(oldPath);
  const decodedNew = decodeDiffPath(newPath);
  return decodedOld === decodedNew
    || (oldPath.startsWith("a/") && newPath.startsWith("b/"));
}

function diffFilePath(value) {
  const lines = String(value ?? "").split("\n");
  const renamed = lines.find((line) => line.startsWith("rename to "))?.slice("rename to ".length);
  if (renamed) return decodeDiffPath(renamed);

  const newPath = lines.find((line) => line.startsWith("+++ "))?.slice(4);
  const oldPath = lines.find((line) => line.startsWith("--- "))?.slice(4);
  for (const candidate of [newPath, oldPath]) {
    const decoded = decodeDiffPath(candidate);
    if (decoded && decoded !== "/dev/null") return decoded;
  }

  const binary = lines.find((line) => /^Binary files .+ and .+ differ$/.test(line));
  if (binary) {
    const separator = binary.lastIndexOf(" and ");
    const decoded = decodeDiffPath(binary.slice(separator + 5, -" differ".length));
    if (decoded) return decoded;
  }

  const gitHeader = lines.find((line) => line.startsWith("diff --git "));
  const quoted = gitHeader?.match(/^diff --git "a\/(.*)" "b\/(.*)"$/);
  if (quoted) return decodeDiffPath(`"b/${quoted[2]}"`);
  const unquoted = gitHeader?.match(/^diff --git a\/(\S+) b\/(\S+)$/);
  return decodeDiffPath(unquoted?.[2]);
}

function decodeDiffPath(value) {
  let candidate = String(value ?? "").split("\t", 1)[0].trim();
  if (!candidate) return null;
  if (candidate.startsWith('"') && candidate.endsWith('"')) {
    try {
      candidate = JSON.parse(candidate);
    } catch {
      candidate = candidate.slice(1, -1);
    }
  }
  return candidate.replace(/^[ab]\//, "") || null;
}

function removePersistedShellProjection(part) {
  if (!part || typeof part !== "object") return;
  const metadata = parseMetadata(part.metadata_json);
  if (!("shell_execution_projection" in metadata)) return;
  delete metadata.shell_execution_projection;
  writeMetadata(part, metadata);
}

function executionIdOf(part) {
  return typeof part?.source_execution_id === "string" && part.source_execution_id.trim()
    ? part.source_execution_id.trim()
    : null;
}

function isCommandPart(part) {
  return part?.kind === "command" || part?.command != null || cardTypeOf(part) === "command";
}

function isResultPart(part) {
  if (part?.kind === "file_change" || cardTypeOf(part) === "result" || part?.kind === "result") {
    return true;
  }
  const meta = parseMetadata(part?.metadata_json);
  const type = meta.type ?? meta.source_type;
  return type === "result" || type === "RUN_COMMAND" || type === "TOOL_RESULT";
}

function cardTypeOf(part, metadata = parseMetadata(part?.metadata_json)) {
  const card = part?.content_card ?? metadata.content_card ?? metadata.contentCard;
  const kind = typeof card?.kind === "string" ? card.kind : card?.type;
  return typeof kind === "string" ? kind.slice(kind.lastIndexOf(".") + 1) : null;
}

/**
 * Classifies an execution part or pair into a semantic execution kind:
 * "read" | "search" | "file_change" | "shell" | "plan" | "subagent" | "unclassified"
 * Accepts either traditional (part, metadata, peer, peerMetadata) or a single options object { command, toolName, toolInput }.
 */
export function classifyExecution(part, metadata = {}, peer = null, peerMetadata = {}) {
  // Support options object form: classifyExecution({ command, toolName, toolInput })
  if (part && typeof part === "object" && !("metadata_json" in part) && !("command" in part && "role" in part)) {
    const opts = part;
    const toolName = String(opts.toolName ?? opts.tool_name ?? opts.name ?? "").toLowerCase().trim();
    const command = String(opts.command ?? "").trim();

    if (EXACT_READ_TOOLS.has(toolName)) return "read";
    if (EXACT_SEARCH_TOOLS.has(toolName)) return "search";
    if (EXACT_PLAN_TOOLS.has(toolName)) return "plan";
    if (EXACT_SUBAGENT_TOOLS.has(toolName)) return "subagent";
    if (EXACT_FILE_CHANGE_TOOLS.has(toolName)) return "file_change";

    if (/^(cat|head|tail|less|more|bat|sed|awk)\b/i.test(command)) return "read";
    if (/^(rg|grep|find|fd)\b/i.test(command)) return "search";
    if (command) return "shell";
    return "unclassified";
  }

  // Traditional multi-argument form
  for (const candidate of [
    metadata.execution_kind,
    metadata.executionKind,
    peerMetadata?.execution_kind,
    peerMetadata?.executionKind,
  ]) {
    if (["read", "search", "shell", "file_change", "plan", "subagent"].includes(candidate)) {
      return candidate;
    }
  }

  const toolName = String(
    metadata.tool_name ??
    metadata.toolName ??
    metadata.name ??
    metadata.tool ??
    peerMetadata?.tool_name ??
    peerMetadata?.toolName ??
    peerMetadata?.name ??
    peerMetadata?.tool ??
    ""
  ).toLowerCase().trim();

  if (EXACT_READ_TOOLS.has(toolName)) return "read";
  if (EXACT_SEARCH_TOOLS.has(toolName)) return "search";
  if (EXACT_PLAN_TOOLS.has(toolName)) return "plan";
  if (EXACT_SUBAGENT_TOOLS.has(toolName)) return "subagent";
  if (EXACT_FILE_CHANGE_TOOLS.has(toolName)) return "file_change";

  const sourceType = [
    metadata.source_type,
    metadata.sourceType,
    peerMetadata?.source_type,
    peerMetadata?.sourceType,
  ].filter(Boolean).join(" ").toLowerCase();

  const combinedIdent = `${toolName} ${sourceType}`.trim();

  if (/\b(?:view[_ -]?file|read[_ -]?file|read[_ -]?url|list[_ -]?directory|list[_ -]?dir|\bread\b|\bls\b)\b/.test(combinedIdent)) {
    return "read";
  }
  if (/\b(?:grep|search|find|glob)\b/.test(combinedIdent)) {
    return "search";
  }
  if (/\b(?:update[_ -]?plan|todo_list|task_plan)\b/.test(combinedIdent)) {
    return "plan";
  }
  if (/\b(?:invoke_subagent|subagent|agent_task|spawn_agent)\b/.test(combinedIdent)) {
    return "subagent";
  }
  if (/(?:^|\s)(?:apply[_ -]?patch|patch|edit|multi[_ -]?edit|write(?:[_ -]?file)?|create[_ -]?file|str[_ -]?replace[_ -]?editor|file[_ -]?change|code[_ -]?action)(?:\s|$)/.test(combinedIdent)) {
    return "file_change";
  }

  const command = String(part?.command ?? peer?.command ?? "").trim();
  if (/^(cat|head|tail|less|more|bat|sed|awk)\b/i.test(command)) return "read";
  if (/^(rg|grep|find|fd)\b/i.test(command)) return "search";
  if (/shell|command|exec|run_command/.test(combinedIdent)) return "shell";
  if (part?.kind === "file_change" || peer?.kind === "file_change") return "file_change";
  if (isCommandPart(part) || isCommandPart(peer)) return "shell";
  return "unclassified";
}

function inferOutcome(command, result, executionKind) {
  const resultText = cleanText(result?.text);
  const explicitExit = integer(result?.exit_code) ?? integer(command?.exit_code);
  const textExit = exitCodeFromText(resultText);
  const exitCode = explicitExit ?? textExit;
  const status = String(result?.status ?? command?.status ?? "").toLowerCase();

  if ((exitCode != null && exitCode !== 0) || FAILURE_STATUS.test(status)) {
    return { status: result?.status ?? command?.status ?? "failed", exitCode: exitCode ?? 1 };
  }
  if (exitCode === 0 || SUCCESS_STATUSES.has(status)) {
    return { status: result?.status ?? command?.status ?? "completed", exitCode: 0 };
  }
  if (failureText(resultText)) {
    return { status: "failed", exitCode: exitCode ?? 1 };
  }
  if (command && result && (resultText || ["read", "search", "file_change"].includes(executionKind))) {
    return { status: "completed", exitCode: 0 };
  }
  return { status: result?.status ?? command?.status ?? null, exitCode: exitCode ?? null };
}

function applyOutcome(part, outcome) {
  if (!part || !outcome) return;
  if (outcome.status != null) part.status = outcome.status;
  if (outcome.exitCode != null) part.exit_code = outcome.exitCode;
}

function failureText(value) {
  return /(?:process exited with code|exit code|exited with status)\s*[:=]?\s*[1-9]\d*\b/i.test(value)
    || /(?:^|\n)(?:error|failed|failure):\s+/i.test(value);
}

function exitCodeFromText(value) {
  const match = String(value ?? "").match(/(?:process exited with code|exit code|exited with status)\s*[:=]?\s*(-?\d+)\b/i);
  return match ? Number(match[1]) : null;
}

export function extractLocation(command, cwd, metadata = {}) {
  const args = metadata.tool_input ?? metadata.args ?? metadata.arguments ?? {};
  const explicit = (
    metadata.file_path ??
    metadata.filePath ??
    metadata.path ??
    metadata.AbsolutePath ??
    args.AbsolutePath ??
    args.TargetFile ??
    args.target_file ??
    args.file_path ??
    args.path ??
    args.Url ??
    args.url
  );
  const source = String(explicit ?? command ?? "").trim();
  if (!source) return null;
  const cleaned = source
    .replace(/^file:\/\//, "")
    .replace(/^['"]|['"]$/g, "")
    .replace(/[;,|]+$/, "")
    .trim();
  const pathMatches = cleaned.match(/(?:^|\s)(~?(?:\.{0,2}\/|\/|[A-Za-z]:[\\/])[^\s'";,|]+|[^\s'";,|]+\.[A-Za-z0-9_-]+)(?:\s|$)/g);
  const candidate = explicit ? cleaned : pathMatches?.at(-1)?.trim();
  if (!candidate) return null;
  const filePath = normalizePath(candidate, cwd);
  const range = String(command ?? "").match(/(?:sed\s+-n\s+['"]?)?(\d+)(?:,(\d+))?p?['"]?/i);
  const lineStart = integer(
    metadata.line_start ??
    metadata.lineStart ??
    metadata.offset ??
    args.StartLine ??
    args.start_line ??
    args.line_start
  ) ?? integer(range?.[1]);
  const explicitEnd = integer(
    metadata.line_end ??
    metadata.lineEnd ??
    args.EndLine ??
    args.end_line ??
    args.line_end
  );
  const limit = integer(metadata.line_limit ?? metadata.lineLimit ?? metadata.limit ?? args.limit);
  const lineEnd = explicitEnd ?? (lineStart != null && limit != null && limit > 0 ? lineStart + limit - 1 : integer(range?.[2]));
  return { filePath, lineStart, lineEnd };
}

function normalizePath(value, cwd) {
  return path.normalize(path.isAbsolute(value) || !cwd ? value : path.resolve(cwd, value));
}

function formatLocation(location) {
  if (location.lineStart == null) return location.filePath;
  return `${location.filePath}:${location.lineStart}${location.lineEnd != null ? `-${location.lineEnd}` : ""}`;
}

function setExecutionMetadata(part, metadata, executionKind, location) {
  metadata.execution_kind = executionKind;
  if (location) {
    metadata.file_path = location.filePath;
    if (location.lineStart != null) metadata.line_start = location.lineStart;
    if (location.lineEnd != null) metadata.line_end = location.lineEnd;
  }
  writeMetadata(part, metadata);
}

function clearPayloadBudgetMetadata(metadata) {
  for (const key of [
    "truncated",
    "original_chars",
    "display_chars",
    "display_budget_chars",
    "compacted_for_browsing",
    "compaction_budget_chars",
  ]) {
    delete metadata[key];
  }
}

function markUnclassified(part, metadata) {
  metadata.execution_kind = "unclassified";
  writeMetadata(part, metadata);
}

function setFileChangeCard(part, metadata) {
  const card = part.content_card ?? metadata.content_card ?? metadata.contentCard;
  if (card && typeof card === "object") {
    if (typeof card.kind === "string" && card.kind.includes(".")) {
      card.kind = `${card.kind.slice(0, card.kind.lastIndexOf("."))}.file-change`;
    } else if (typeof card.type === "string") {
      card.type = "file-change";
    }
    card.renderer = "diff";
    part.content_card = card;
    delete metadata.content_card;
    delete metadata.contentCard;
  }
  metadata.execution_kind = "file_change";
  writeMetadata(part, metadata);
}

function setEmptyFileChangeResultCard(part, metadata) {
  const card = part.content_card ?? metadata.content_card ?? metadata.contentCard;
  if (card && typeof card === "object") {
    if (typeof card.kind === "string" && card.kind.includes(".")) {
      card.kind = `${card.kind.slice(0, card.kind.lastIndexOf("."))}.result`;
    } else if (typeof card.type === "string") {
      card.type = "result";
    }
    card.renderer = "terminal_output";
    part.content_card = card;
    delete metadata.content_card;
    delete metadata.contentCard;
  }
  metadata.execution_kind = "file_change";
  writeMetadata(part, metadata);
}

function isSuccessful(part) {
  return part?.exit_code === 0 || SUCCESS_STATUSES.has(String(part?.status ?? "").toLowerCase());
}

function isEmptyResultPayload(part) {
  if (!part || typeof part !== "object" || isCommandPart(part) || !isResultPart(part)) return false;
  const metadata = parseMetadata(part.metadata_json);
  const executionKind = metadata.execution_kind ?? metadata.executionKind;
  const text = cleanText(part.text);
  const emptyPayload = !text
    ? isSuccessful(part)
    : /^(?:\{\}|\[\]|null|undefined)$/i.test(text)
      || /(?:^|\n)Output:\s*(?:\{\}|\[\]|null|undefined)\s*$/i.test(text);
  if (!emptyPayload || isFailure(part)) return false;
  return executionKind === "shell" || executionKind === "unclassified" || executionKind === "plan";
}

function isFailure(part) {
  return (typeof part?.exit_code === "number" && part.exit_code !== 0)
    || FAILURE_STATUS.test(String(part?.status ?? ""));
}

function diagnosticText(part, metadata) {
  const stderr = metadata.stderr ?? metadata.error ?? metadata.diagnostic;
  return cleanText(stderr == null ? part.text : stderr);
}

export function canonicalUnifiedDiff(value) {
  const text = cleanText(value);
  if (!text) return null;
  const lines = text.split("\n");
  const marker = lines.findIndex((line) => line.trim() === "[diff_block_start]");
  const start = lines.findIndex((line) => line.startsWith("diff --git ") || line.startsWith("--- ") || /^Binary files /.test(line));
  const selectedStart = start >= 0 ? start : marker >= 0 ? marker + 1 : 0;
  const markerEnd = marker >= 0
    ? lines.findIndex((line, index) => index > marker && line.trim() === "[diff_block_end]")
    : -1;
  let selected = lines.slice(selectedStart, markerEnd >= 0 ? markerEnd : undefined)
    .filter((line) => !/^\[diff_block_(?:start|end)\]$/i.test(line.trim()))
    .filter((line) => !/^(Script completed|Wall time|Process exited|Original token count|Output:|Created At:|Completed At:)\b/i.test(line.trim()))
    .join("\n")
    .trimEnd();
  if (selected.startsWith("@@")) {
    const filePath = diffFilePathFromEnvelope(text);
    if (filePath) {
      const gitPath = filePath.replace(/^[./\\]+/, "").replaceAll("\\", "/");
      selected = [
        `diff --git a/${gitPath} b/${gitPath}`,
        `--- a/${gitPath}`,
        `+++ b/${gitPath}`,
        selected,
      ].join("\n");
    }
  }
  return selected || null;
}

function diffFilePathFromEnvelope(value) {
  const candidate = String(value ?? "").match(/changes were made[^\n]*?\bto:\s*(.+?)\. If relevant/i)?.[1]
    ?? String(value ?? "").match(/Created file\s+(\S+)\s+with requested content/i)?.[1]
    ?? null;
  if (!candidate) return null;
  const withoutScheme = candidate.replace(/^file:\/\//i, "").trim();
  try {
    return decodeURIComponent(withoutScheme);
  } catch {
    return withoutScheme;
  }
}

function cleanText(value) {
  return String(value ?? "")
    .replace(/\r\n?/g, "\n")
    .replace(/\u001B\][^\u0007]*(?:\u0007|\u001B\\)/g, "")
    .replace(/\u001B\[[0-?]*[ -/]*[@-~]/g, "")
    .split("\n")
    .map((line) => (line.includes("\r") ? line.slice(line.lastIndexOf("\r") + 1) : line).trimEnd())
    .join("\n")
    .trim();
}

function parseMetadata(value) {
  if (!value) return {};
  if (typeof value === "object" && !Array.isArray(value)) return { ...value };
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? { ...parsed } : {};
  } catch {
    return {};
  }
}

function writeMetadata(part, metadata) {
  metadata.payload_policy_version = PAYLOAD_POLICY_VERSION;
  part.metadata_json = JSON.stringify(metadata);
}

function integer(value) {
  if (Number.isInteger(value)) return value;
  return /^-?\d+$/.test(String(value ?? "")) ? Number(value) : null;
}

const READ_SHELL_COMMANDS = new Set(["cat", "head", "tail", "less", "more", "bat", "nl", "sed", "awk"]);
const SEARCH_SHELL_COMMANDS = new Set(["rg", "grep", "ag", "ack"]);
const FIND_SHELL_COMMANDS = new Set(["find", "fd"]);
const LIST_SHELL_COMMANDS = new Set(["ls", "tree", "exa", "eza"]);

const SEARCH_COMMANDS_VALUE_FLAGS = new Set([
  "-e", "--regexp", "-f", "--file", "-g", "--glob", "--iglob", "-t", "--type",
  "--type-not", "--type-add", "-m", "--max-count", "-A", "--after-context",
  "-B", "--before-context", "-C", "--context", "-T", "--include", "--exclude",
  "--exclude-dir", "--exclude-from", "-M", "--max-columns", "-j", "--threads",
  "--sort", "--sortr", "--color", "--colors", "--encoding", "-E", "--engine",
  "--max-depth", "-d", "--maxdepth", "--path-separator",
]);

const FIND_MUTATING_FLAGS = new Set([
  "-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0", "-fprintf", "-fls",
]);

export function captureRawCommands(session) {
  const rawCommands = new WeakMap();
  for (const turn of Array.isArray(session?.turns) ? session.turns : []) {
    for (const part of Array.isArray(turn?.parts) ? turn.parts : []) {
      if (!part || typeof part !== "object" || !isCommandPart(part)) continue;
      const raw = cleanText(part.command) || cleanText(part.text);
      if (raw) rawCommands.set(part, raw);
    }
  }
  return rawCommands;
}

function hasShellControlOperators(command) {
  if (!command || /[\r\n`]|\$\(/.test(command)) return true;
  let quote = null;
  let escaped = false;
  for (let i = 0; i < command.length; i += 1) {
    const ch = command[i];
    if (escaped) {
      escaped = false;
      continue;
    }
    if (ch === "\\" && quote !== "'") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === "'" || ch === "\"") {
      quote = ch;
      continue;
    }
    if (ch === "|" || ch === ";" || ch === "&" || ch === ">" || ch === "<") {
      return true;
    }
  }
  return false;
}

function tokenizeShellCommand(command) {
  const tokens = [];
  let current = "";
  let inToken = false;
  let quote = null;
  let escaped = false;
  for (let i = 0; i < command.length; i += 1) {
    const ch = command[i];
    if (escaped) {
      current += ch;
      escaped = false;
      inToken = true;
      continue;
    }
    if (ch === "\\" && quote !== "'") {
      escaped = true;
      inToken = true;
      continue;
    }
    if (quote) {
      if (ch === quote) {
        quote = null;
      } else {
        current += ch;
      }
      inToken = true;
      continue;
    }
    if (ch === "'" || ch === "\"") {
      quote = ch;
      inToken = true;
      continue;
    }
    if (/\s/.test(ch)) {
      if (inToken) {
        tokens.push(current);
        current = "";
        inToken = false;
      }
      continue;
    }
    current += ch;
    inToken = true;
  }
  if (quote || escaped) return null;
  if (inToken) tokens.push(current);
  return tokens;
}

function stripEnvAssignments(tokens) {
  let index = 0;
  while (index < tokens.length && /^[A-Za-z_][A-Za-z0-9_]*=/.test(tokens[index])) {
    index += 1;
  }
  return tokens.slice(index);
}

function isSkillFileTarget(target) {
  return /(?:^|[\\/])skill\.md$/i.test(String(target || "").trim());
}

function isSkillPart(part) {
  if (!part || typeof part !== "object") return false;
  if (part.kind === "skill") return true;
  const cardKind = String(part.content_card?.kind || "").toLowerCase();
  const semanticRole = String(part.content_card?.semantic_role || "").toLowerCase();
  if (cardKind.endsWith(".skill") || semanticRole === "skill" || semanticRole === "skill-content") {
    return true;
  }
  const meta = parseMetadata(part.metadata_json);
  return meta.type === "skill";
}

function isFileChangePart(part) {
  if (!part || typeof part !== "object") return false;
  if (part.kind === "diff") return true;
  const cardKind = String(part.content_card?.kind || "").toLowerCase();
  const semanticRole = String(part.content_card?.semantic_role || "").toLowerCase();
  return cardKind.endsWith(".file_change") || semanticRole === "file-change";
}

function isAmbientPart(part) {
  if (!part || typeof part !== "object") return false;
  const cardKind = String(part.content_card?.kind || "").toLowerCase();
  const semanticRole = String(part.content_card?.semantic_role || "").toLowerCase();
  return cardKind.endsWith(".ambient") || semanticRole === "ambient";
}

function formatReadTargetWithRange(fileToken, cwd, startLine, endLine) {
  if (!fileToken || isSkillFileTarget(fileToken)) return null;
  const resolved = normalizePath(fileToken, cwd);
  if (!resolved || resolved.includes("\n")) return null;
  const start = integer(startLine);
  const end = integer(endLine);
  if (start && start > 0 && end && end >= start) {
    return `${resolved}#L${start}-${end}`;
  }
  if (start && start > 0) {
    return `${resolved}#L${start}`;
  }
  return resolved;
}

function parseSedAmbient(args, cwd) {
  if (args.some((arg) => arg === "-i" || arg.startsWith("-i") || arg === "--in-place" || arg.startsWith("--in-place="))) {
    return null;
  }
  let hasN = false;
  const positional = [];
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "-e" || arg === "--expression" || arg === "-f" || arg === "--file") {
      if (i + 1 >= args.length) return null;
      if (arg === "-f" || arg === "--file") return null;
      positional.push(args[++i]);
      continue;
    }
    if (arg === "-n" || arg === "--quiet" || arg === "--silent") {
      hasN = true;
      continue;
    }
    if (arg.startsWith("-")) return null;
    positional.push(arg);
  }
  if (!hasN || positional.length !== 2) return null;
  const expr = positional[0].trim();
  const file = positional[1];
  const rangeMatch = expr.match(/^(\d+)\s*,\s*(\d+)\s*p$/);
  if (rangeMatch) {
    const formatted = formatReadTargetWithRange(file, cwd, Number(rangeMatch[1]), Number(rangeMatch[2]));
    return formatted ? { line: `Read ${formatted}`, executionKind: "read", filePath: normalizePath(file, cwd), toolName: "sed" } : null;
  }
  const singleMatch = expr.match(/^(\d+)\s*p$/);
  if (singleMatch) {
    const formatted = formatReadTargetWithRange(file, cwd, Number(singleMatch[1]), Number(singleMatch[1]));
    return formatted ? { line: `Read ${formatted}`, executionKind: "read", filePath: normalizePath(file, cwd), toolName: "sed" } : null;
  }
  return null;
}

function parseHeadOrTailAmbient(cmdName, args, cwd) {
  let count = null;
  let startLine = null;
  const files = [];
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "-n" || arg === "--lines") {
      const next = args[++i];
      if (!next) return null;
      if (cmdName === "tail" && /^\+\d+$/.test(next)) {
        startLine = Number(next.slice(1));
      } else if (/^\d+$/.test(next)) {
        count = Number(next);
      } else {
        return null;
      }
      continue;
    }
    const shortN = arg.match(/^-n(\d+)$/);
    if (shortN) {
      count = Number(shortN[1]);
      continue;
    }
    const dashNum = arg.match(/^-(\d+)$/);
    if (dashNum) {
      count = Number(dashNum[1]);
      continue;
    }
    if (arg === "-q" || arg === "--quiet" || arg === "--silent" || arg === "-v") continue;
    if (arg.startsWith("-")) return null;
    files.push(arg);
  }
  if (files.length === 0 || files.length > 3) return null;
  const lines = [];
  let firstPath = null;
  for (const file of files) {
    const resolved = normalizePath(file, cwd);
    if (!resolved || isSkillFileTarget(resolved)) return null;
    if (!firstPath) firstPath = resolved;
    const formatted = cmdName === "head" && count
      ? formatReadTargetWithRange(file, cwd, 1, count)
      : cmdName === "tail" && startLine
        ? formatReadTargetWithRange(file, cwd, startLine, null)
        : formatReadTargetWithRange(file, cwd, null, null);
    if (!formatted) return null;
    lines.push(`Read ${formatted}`);
  }
  return { line: lines.join("\n"), executionKind: "read", filePath: firstPath, toolName: cmdName };
}

function parseSimpleReadAmbient(cmdName, args, cwd) {
  if (cmdName === "awk") {
    if (args.some((arg) => arg === "-i" || arg.startsWith("-i"))) return null;
    const positional = args.filter((arg) => !arg.startsWith("-"));
    if (positional.length !== 2) return null;
    const file = positional[1];
    const resolved = normalizePath(file, cwd);
    if (!resolved || isSkillFileTarget(resolved)) return null;
    const rangeMatch = positional[0].match(/NR\s*>=\s*(\d+)\s*&&\s*NR\s*<=\s*(\d+)/);
    const formatted = rangeMatch
      ? formatReadTargetWithRange(file, cwd, Number(rangeMatch[1]), Number(rangeMatch[2]))
      : formatReadTargetWithRange(file, cwd, null, null);
    return formatted ? { line: `Read ${formatted}`, executionKind: "read", filePath: resolved, toolName: cmdName } : null;
  }

  const files = [];
  let expectFlagVal = false;
  for (const arg of args) {
    if (expectFlagVal) {
      expectFlagVal = false;
      continue;
    }
    if (cmdName === "bat" && (arg === "-r" || arg === "--line-range" || arg === "-l" || arg === "--language" || arg === "--style" || arg === "--theme")) {
      expectFlagVal = true;
      continue;
    }
    if (arg.startsWith("-")) continue;
    files.push(arg);
  }
  if (files.length === 0 || files.length > 3) return null;
  const lines = [];
  let firstPath = null;
  for (const file of files) {
    const resolved = normalizePath(file, cwd);
    if (!resolved || isSkillFileTarget(resolved)) return null;
    if (!firstPath) firstPath = resolved;
    lines.push(`Read ${resolved}`);
  }
  return { line: lines.join("\n"), executionKind: "read", filePath: firstPath, toolName: cmdName };
}

function parseSearchAmbient(cmdName, args, cwd) {
  let pattern = null;
  const targets = [];
  let afterDoubleDash = false;
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (!afterDoubleDash && arg === "--") {
      afterDoubleDash = true;
      continue;
    }
    if (!afterDoubleDash && arg.startsWith("-")) {
      const flagName = arg.includes("=") ? arg.slice(0, arg.indexOf("=")) : arg;
      if (!arg.includes("=") && SEARCH_COMMANDS_VALUE_FLAGS.has(flagName)) {
        const next = args[++i];
        if ((flagName === "-e" || flagName === "--regexp") && next && !pattern) {
          pattern = next;
        }
      } else if (arg.startsWith("-e=") || arg.startsWith("--regexp=")) {
        if (!pattern) pattern = arg.slice(arg.indexOf("=") + 1);
      }
      continue;
    }
    if (pattern == null) {
      pattern = arg;
    } else {
      targets.push(arg);
    }
  }
  const cleanPattern = cleanText(pattern || "");
  if (!cleanPattern || cleanPattern === "." || cleanPattern === "..") return null;
  const shortPattern = cleanPattern.length > 64 ? `${cleanPattern.slice(0, 61)}...` : cleanPattern;
  const normalizedScopes = targets
    .map((target) => normalizePath(target, cwd))
    .filter(Boolean);
  const scopeText = normalizedScopes.length > 0 ? normalizedScopes.join(", ") : "";
  const line = scopeText
    ? `Search "${shortPattern}" @ ${scopeText}`
    : `Search "${shortPattern}"`;
  return {
    line,
    executionKind: "search",
    filePath: normalizedScopes[0] || null,
    toolName: cmdName,
  };
}

function parseFindAmbient(cmdName, args, cwd) {
  if (cmdName === "find") {
    if (args.some((arg) => FIND_MUTATING_FLAGS.has(arg))) return null;
    const roots = [];
    let pattern = null;
    for (let i = 0; i < args.length; i += 1) {
      const arg = args[i];
      if (arg === "-name" || arg === "-iname" || arg === "-path" || arg === "-ipath") {
        if (i + 1 < args.length && !pattern) {
          pattern = args[++i];
        }
        continue;
      }
      if (arg.startsWith("-") || arg === "!" || arg === "(" || arg === ")") {
        break;
      }
      roots.push(arg);
    }
    const dir = normalizePath(roots[0] || ".", cwd) || ".";
    const line = pattern ? `Find ${pattern} @ ${dir}` : `Find @ ${dir}`;
    return { line, executionKind: "search", filePath: dir, toolName: cmdName };
  }

  // fd
  const positional = [];
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg.startsWith("-")) {
      if ((arg === "-e" || arg === "--extension" || arg === "-t" || arg === "--type" || arg === "-d" || arg === "--max-depth" || arg === "-E" || arg === "--exclude") && i + 1 < args.length) {
        i += 1;
      }
      continue;
    }
    positional.push(arg);
  }
  const pattern = cleanText(positional[0] || "");
  const dir = normalizePath(positional[1] || ".", cwd) || ".";
  const line = pattern ? `Find ${pattern} @ ${dir}` : `Find @ ${dir}`;
  return { line, executionKind: "search", filePath: dir, toolName: cmdName };
}

function parseListAmbient(cmdName, args, cwd) {
  const dirs = [];
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg.startsWith("-")) {
      if (cmdName === "tree" && (arg === "-L" || arg === "-P" || arg === "-I") && i + 1 < args.length) {
        i += 1;
      }
      continue;
    }
    dirs.push(arg);
  }
  const target = normalizePath(dirs[0] || ".", cwd) || ".";
  return { line: `List ${target}`, executionKind: "list", filePath: target, toolName: cmdName };
}

function formatAmbientShellCommand(rawCommand, cwd, metadata) {
  const raw = cleanText(rawCommand);
  if (!raw || hasShellControlOperators(raw)) return null;
  if (isSkillFileTarget(raw)) return null;
  const parsedTokens = tokenizeShellCommand(raw);
  if (!parsedTokens || parsedTokens.length === 0) return null;
  const tokens = stripEnvAssignments(parsedTokens);
  if (tokens.length === 0) return null;
  const cmdBase = tokens[0].split(/[\\/]/).pop()?.toLowerCase() || "";
  const args = tokens.slice(1);

  if (cmdBase === "sed") return parseSedAmbient(args, cwd);
  if (cmdBase === "head" || cmdBase === "tail") return parseHeadOrTailAmbient(cmdBase, args, cwd);
  if (READ_SHELL_COMMANDS.has(cmdBase)) return parseSimpleReadAmbient(cmdBase, args, cwd);
  if (SEARCH_SHELL_COMMANDS.has(cmdBase)) return parseSearchAmbient(cmdBase, args, cwd);
  if (FIND_SHELL_COMMANDS.has(cmdBase)) return parseFindAmbient(cmdBase, args, cwd);
  if (LIST_SHELL_COMMANDS.has(cmdBase)) return parseListAmbient(cmdBase, args, cwd);

  // Fallback for already-normalized single-file read commands without raw shell tokens
  if (metadata?.execution_kind === "read" && metadata?.location?.file_path) {
    const formatted = formatReadTargetWithRange(
      metadata.location.file_path,
      cwd,
      metadata.location.line,
      metadata.location.end_line,
    );
    if (formatted) {
      return {
        line: `Read ${formatted}`,
        executionKind: "read",
        filePath: normalizePath(metadata.location.file_path, cwd),
        toolName: cmdBase || "read",
      };
    }
  }
  return null;
}

function extractAmbientFilePaths(part, metadata) {
  const paths = new Set();
  if (typeof metadata?.file_path === "string" && metadata.file_path.trim()) {
    paths.add(metadata.file_path.trim());
  }
  if (typeof metadata?.location?.file_path === "string" && metadata.location.file_path.trim()) {
    paths.add(metadata.location.file_path.trim());
  }
  const text = typeof part?.text === "string" ? part.text : "";
  for (const rawLine of text.split(/\r?\n/)) {
    const match = rawLine.trim().match(/^(?:Read|Outline|Symbol|List)\s+(.+?)(?:#L\d+(?:-\d+)?)?(?:\s+.*)?$/i);
    if (match?.[1]) {
      paths.add(match[1].trim());
    }
  }
  return paths;
}

function isUnmatchedAmbientResult(part, ambientBatch) {
  if (!part || !isResultPart(part) || !Array.isArray(ambientBatch) || ambientBatch.length === 0) {
    return false;
  }
  if (executionIdOf(part)) return false;
  const meta = parseMetadata(part.metadata_json);
  const execKind = String(meta.execution_kind || "").toLowerCase();
  if (execKind !== "read" && execKind !== "search" && execKind !== "list") {
    return false;
  }
  if (execKind === "search" || execKind === "list") {
    const prev = ambientBatch[ambientBatch.length - 1];
    const prevMeta = parseMetadata(prev?.metadata_json);
    const prevKind = String(prevMeta.execution_kind || "").toLowerCase();
    return prevKind === execKind || String(prev?.text || "").trim().startsWith(execKind === "search" ? "Search " : "List ");
  }
  const resultPath = typeof meta.file_path === "string" ? meta.file_path.trim()
    : typeof meta.location?.file_path === "string" ? meta.location.file_path.trim()
      : "";
  const prev = ambientBatch[ambientBatch.length - 1];
  const prevPaths = extractAmbientFilePaths(prev, parseMetadata(prev?.metadata_json));
  if (resultPath && prevPaths.size > 0) {
    return prevPaths.has(resultPath);
  }
  return meta.signal === "ambient";
}

function flushAmbientBatch(batch, out, adapterPrefix) {
  if (batch.length === 0) return;
  if (batch.length === 1) {
    out.push(batch[0]);
    batch.length = 0;
    return;
  }

  const first = batch[0];
  const lines = [];
  const toolNames = new Set();
  const executionKinds = new Set();

  for (const item of batch) {
    const rawLines = String(item?.text || "")
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean);
    lines.push(...rawLines);
    const meta = parseMetadata(item?.metadata_json);
    if (typeof item?.tool_name === "string" && item.tool_name.trim()) {
      toolNames.add(item.tool_name.trim());
    }
    if (Array.isArray(meta.tool_names)) {
      for (const name of meta.tool_names) {
        if (typeof name === "string" && name.trim()) toolNames.add(name.trim());
      }
    }
    if (typeof meta.execution_kind === "string" && meta.execution_kind.trim()) {
      executionKinds.add(meta.execution_kind.trim());
    }
    if (Array.isArray(meta.execution_kinds)) {
      for (const kind of meta.execution_kinds) {
        if (typeof kind === "string" && kind.trim()) executionKinds.add(kind.trim());
      }
    }
  }

  const prefix = adapterPrefix || String(first.content_card?.kind || "").split(".")[0] || "agent";
  const singleTool = toolNames.size === 1 ? [...toolNames][0] : "ambient_batch";
  const mergedMeta = compactObject({
    type: "ambient",
    signal: "ambient",
    batch_count: lines.length,
    tool_names: [...toolNames],
    execution_kinds: [...executionKinds],
    payload_policy_version: PAYLOAD_POLICY_VERSION,
  });

  out.push({
    ...first,
    kind: "tool",
    tool_name: singleTool,
    command: null,
    text: lines.join("\n"),
    content_card: {
      schema_version: 1,
      kind: `${prefix}.ambient`,
      semantic_role: "ambient",
      renderer: "compact_action",
    },
    metadata_json: JSON.stringify(mergedMeta),
  });
  batch.length = 0;
}

export function compactAmbientActions(session, options = {}) {
  if (!session || !Array.isArray(session.turns)) return session;
  const { adapterPrefix = "", rawCommands = null } = options;

  for (const turn of session.turns) {
    const parts = Array.isArray(turn?.parts) ? turn.parts : [];
    if (parts.length === 0) continue;

    const failedExecIds = new Set();
    const ambientExecIds = new Set();

    for (const part of parts) {
      const execId = executionIdOf(part);
      if (execId && isFailure(part)) {
        failedExecIds.add(execId);
      }
    }

    const projected = [];
    for (let i = 0; i < parts.length; i += 1) {
      const part = parts[i];
      if (!part || typeof part !== "object") {
        projected.push(part);
        continue;
      }

      const execId = executionIdOf(part);
      const failed = isFailure(part) || Boolean(execId && failedExecIds.has(execId));

      if (isAmbientPart(part)) {
        if (!failed && execId) ambientExecIds.add(execId);
        projected.push(part);
        continue;
      }

      if (!failed && isCommandPart(part) && !isSkillPart(parts[i + 1])) {
        const meta = parseMetadata(part.metadata_json);
        const cwd = cleanText(part.cwd) || cleanText(meta.cwd) || null;
        const rawCmd = (rawCommands && typeof rawCommands.get === "function" ? rawCommands.get(part) : null)
          || cleanText(part.command)
          || cleanText(part.text);
        const converted = formatAmbientShellCommand(rawCmd, cwd, meta);
        if (converted) {
          const prefix = adapterPrefix
            || String(part.content_card?.kind || "").split(".")[0]
            || "agent";
          const nextMeta = compactObject({
            ...meta,
            type: "ambient",
            signal: "ambient",
            execution_kind: converted.executionKind,
            tool_name: part.tool_name || converted.toolName,
            file_path: converted.filePath,
            payload_policy_version: PAYLOAD_POLICY_VERSION,
          });
          if (execId) ambientExecIds.add(execId);
          projected.push({
            ...part,
            kind: "tool",
            tool_name: part.tool_name || converted.toolName,
            command: null,
            text: converted.line,
            content_card: {
              schema_version: 1,
              kind: `${prefix}.ambient`,
              semantic_role: "ambient",
              renderer: "compact_action",
            },
            metadata_json: JSON.stringify(nextMeta),
          });
          continue;
        }
      }

      projected.push(part);
    }

    const compacted = [];
    const ambientBatch = [];
    for (const part of projected) {
      const execId = executionIdOf(part);
      const failed = isFailure(part) || Boolean(execId && failedExecIds.has(execId));

      if (!failed && !isFileChangePart(part) && !isSkillPart(part) && isResultPart(part)) {
        if (execId && ambientExecIds.has(execId)) {
          continue;
        }
        if (isUnmatchedAmbientResult(part, ambientBatch)) {
          continue;
        }
      }

      if (!failed && isAmbientPart(part)) {
        ambientBatch.push(part);
        continue;
      }

      flushAmbientBatch(ambientBatch, compacted, adapterPrefix);
      compacted.push(part);
    }
    flushAmbientBatch(ambientBatch, compacted, adapterPrefix);
    turn.parts = compacted;
  }

  return session;
}

const LEGACY_AMBIENT_TOOL_NAMES = new Set([
  "read_file",
  "view_file",
  "view_file_outline",
  "read_file_outline",
  "view_code_item",
  "list_dir",
  "list_directory",
  "read_dir",
  "read_url_content",
  "read_resource",
  "grep_search",
  "find_by_name",
  "find_files",
  "search_web",
  "search_docs",
]);

const LEGACY_EDIT_TOOL_NAMES = new Set([
  "edit_file",
  "write_file",
  "write_to_file",
  "replace_file_content",
  "multi_replace_file_content",
]);

function parseLegacyToolBlock(rawText) {
  const text = String(rawText || "");
  if (!text.trim()) return null;
  const lines = text.split(/\r?\n/);
  const firstMatch = lines[0].trim().match(/^Tool:\s*(.+)$/i);
  if (!firstMatch) return null;
  const toolName = firstMatch[1].trim();
  const fields = {};
  for (let i = 1; i < lines.length; i += 1) {
    const line = lines[i];
    const trimmed = line.trim();
    if (!trimmed) continue;
    const kv = trimmed.match(/^([A-Za-z0-9_.-]+):\s*(.*)$/);
    if (!kv) continue;
    const key = kv[1];
    // Stop parsing key/values if we hit multiline code/content blocks
    if (key === "Content" || key === "Replacement" || key === "Target" || key === "CodeContent") {
      fields[key] = kv[2];
      break;
    }
    fields[key] = kv[2];
  }
  return { toolName, fields };
}

function parsePositiveInt(val) {
  if (val == null || val === "") return null;
  const num = Number(val);
  return Number.isInteger(num) && num > 0 ? num : null;
}

function extractTargetFromFields(fields) {
  return cleanText(
    fields.AbsolutePath
    || fields.Path
    || fields.TargetFile
    || fields.DirectoryPath
    || fields.SearchPath
    || fields.SearchDirectory
    || fields.File
    || fields.FilePath
    || fields.Url
    || fields.Uri
    || fields.file_path
    || fields.path,
  ) || null;
}

function convertLegacyToolPartToAmbient(part, prefix, defaultCwd, meta) {
  const parsed = parseLegacyToolBlock(part.text);
  const toolName = cleanText(parsed?.toolName || part.tool_name || meta.tool_name || "");
  if (!toolName) return null;
  const lower = toolName.toLowerCase();
  const fields = parsed?.fields || {};
  const cwd = cleanText(part.cwd || meta.cwd || fields.Cwd || defaultCwd || "") || null;

  if (lower === "run_command" || lower === "command") {
    const cmd = cleanText(fields.CommandLine || fields.Command || part.command || "");
    if (!cmd) return null;
    const converted = formatAmbientShellCommand(cmd, cwd, meta);
    if (!converted) return null;
    return {
      ...part,
      kind: "tool",
      tool_name: converted.toolName || toolName,
      command: null,
      text: converted.line,
      content_card: {
        schema_version: 1,
        kind: `${prefix}.ambient`,
        semantic_role: "ambient",
        renderer: "compact_action",
      },
      metadata_json: JSON.stringify(compactObject({
        ...meta,
        type: "ambient",
        signal: "ambient",
        execution_kind: converted.executionKind,
        tool_name: converted.toolName || toolName,
        file_path: converted.filePath,
        payload_policy_version: PAYLOAD_POLICY_VERSION,
      })),
    };
  }

  if (!LEGACY_AMBIENT_TOOL_NAMES.has(lower)) return null;

  const rawTarget = extractTargetFromFields(fields)
    || cleanText(meta.file_path || meta.location?.file_path || "");
  if (rawTarget && isSkillFileTarget(rawTarget)) {
    return null;
  }
  const target = rawTarget ? normalizePath(rawTarget, cwd) || rawTarget : null;

  let line = null;
  let executionKind = "read";
  if (lower === "read_file" || lower === "view_file") {
    const startLine = parsePositiveInt(fields.StartLine ?? fields.start_line);
    const endLine = parsePositiveInt(fields.EndLine ?? fields.end_line);
    const formatted = target ? formatReadTargetWithRange(target, cwd, startLine, endLine) : null;
    line = formatted ? `Read ${formatted}` : "Read file";
  } else if (lower === "view_file_outline" || lower === "read_file_outline") {
    line = target ? `Outline ${target}` : "Outline file";
  } else if (lower === "view_code_item") {
    const nodePaths = cleanText(fields.NodePaths || fields.NodePath || "");
    if (nodePaths && target) line = `Symbol ${nodePaths} @ ${target}`;
    else line = target ? `Read ${target}` : `Symbol ${nodePaths || "item"}`;
  } else if (lower === "list_dir" || lower === "list_directory" || lower === "read_dir") {
    line = target ? `List ${target}` : "List directory";
    executionKind = "list";
  } else if (lower === "read_url_content") {
    line = target ? `Fetch ${target}` : "Fetch URL";
  } else if (lower === "read_resource") {
    line = target ? `Read ${target}` : "Read resource";
  } else if (lower === "grep_search") {
    const query = cleanText(fields.Query || fields.query || fields.Pattern || fields.pattern || "");
    const scope = target;
    if (query && scope) line = `Search "${query}" @ ${scope}`;
    else if (query) line = `Search "${query}"`;
    else line = scope ? `Search @ ${scope}` : "Search";
    executionKind = "search";
  } else if (lower === "find_by_name" || lower === "find_files") {
    const pattern = cleanText(fields.Pattern || fields.pattern || fields.Query || fields.query || "");
    const scope = target;
    line = pattern && scope ? `Find ${pattern} @ ${scope}` : `Find ${pattern || scope || "*"}`;
    executionKind = "find";
  } else if (lower === "search_web" || lower === "search_docs") {
    const query = cleanText(fields.Query || fields.query || fields.search_query || "");
    line = query ? `Search "${query}"` : `Search (${toolName})`;
    executionKind = "search";
  }

  if (!line) return null;
  return {
    ...part,
    kind: "tool",
    tool_name: toolName,
    command: null,
    text: line,
    content_card: {
      schema_version: 1,
      kind: `${prefix}.ambient`,
      semantic_role: "ambient",
      renderer: "compact_action",
    },
    metadata_json: JSON.stringify(compactObject({
      ...meta,
      type: "ambient",
      signal: "ambient",
      execution_kind: executionKind,
      tool_name: toolName,
      file_path: target,
      payload_policy_version: PAYLOAD_POLICY_VERSION,
    })),
  };
}

function collectTurnFileChangePaths(parts, cwd) {
  const paths = new Set();
  for (const part of parts) {
    if (!isFileChangePart(part)) continue;
    const meta = parseMetadata(part.metadata_json);
    const candidates = [];
    if (typeof meta.file_path === "string") candidates.push(meta.file_path);
    if (typeof meta.path === "string") candidates.push(meta.path);
    if (Array.isArray(meta.files)) {
      for (const f of meta.files) {
        if (typeof f === "string") candidates.push(f);
        else if (typeof f?.path === "string") candidates.push(f.path);
      }
    }
    for (const line of String(part.text || "").split(/\r?\n/)) {
      const m = line.trim().match(/^(?:[+-]|modify|create|delete|edit|update)?\s*(\/.+\S)$/i);
      if (m) candidates.push(m[1]);
    }
    for (const c of candidates) {
      const norm = normalizePath(c, cwd);
      if (norm) paths.add(norm);
    }
  }
  return paths;
}

export function reprojectStoredSession(session, options = {}) {
  if (!session || typeof session !== "object" || !Array.isArray(session.turns)) {
    return session;
  }
  const adapterId = cleanText(options.adapterId || session.adapter_id || "");
  const defaultCwd = cleanText(options.workspacePath || session.workspace_path || "") || null;
  const prefix = adapterId || "agent";
  const rawCommands = new WeakMap();

  const nextTurns = session.turns.map((turn) => {
    const origParts = Array.isArray(turn?.parts) ? turn.parts : [];
    const fileChangePaths = collectTurnFileChangePaths(origParts, defaultCwd);
    const transformedParts = [];

    for (const part of origParts) {
      if (!part || typeof part !== "object") continue;
      const meta = parseMetadata(part.metadata_json);
      const partPrefix = String(part.content_card?.kind || "").split(".")[0] || prefix;
      const failed = isFailure(part);

      if (isAmbientPart(part)) {
        transformedParts.push(part);
        continue;
      }

      if (!failed && isCommandPart(part)) {
        const cloned = {
          ...part,
          cwd: cleanText(part.cwd || meta.cwd || defaultCwd || "") || null,
        };
        const rawCmd = cleanText(part.command || part.text || "");
        if (rawCmd) rawCommands.set(cloned, rawCmd);
        transformedParts.push(cloned);
        continue;
      }

      if (!failed && (part.kind === "tool" || String(part.content_card?.kind || "").endsWith(".tool"))) {
        const parsed = parseLegacyToolBlock(part.text);
        const toolName = cleanText(parsed?.toolName || part.tool_name || meta.tool_name || "").toLowerCase();
        if (parsed && LEGACY_EDIT_TOOL_NAMES.has(toolName) && fileChangePaths.size > 0) {
          const target = extractTargetFromFields(parsed.fields);
          const norm = target ? normalizePath(target, defaultCwd) : null;
          if (norm && fileChangePaths.has(norm)) {
            continue;
          }
        }
        const ambient = convertLegacyToolPartToAmbient(part, partPrefix, defaultCwd, meta);
        if (ambient) {
          transformedParts.push(ambient);
          continue;
        }
      }

      transformedParts.push(part);
    }

    return {
      ...turn,
      parts: transformedParts,
    };
  });

  const nextSession = {
    ...session,
    turns: nextTurns,
  };
  compactAmbientActions(nextSession, { adapterPrefix: prefix, rawCommands });
  for (const turn of nextSession.turns) {
    turn.parts = (Array.isArray(turn.parts) ? turn.parts : []).map((part, idx) => ({
      ...part,
      part_index: idx,
    }));
  }
  return nextSession;
}
