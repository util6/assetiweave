import path from "node:path";

export const PAYLOAD_POLICY_VERSION = 10;

const SUCCESS_STATUSES = new Set(["success", "succeeded", "completed", "complete", "done", "ok"]);
const FAILURE_STATUS = /^(error|failed|failure|cancelled|canceled|interrupted|timeout|timed_out)$/i;

/**
 * Classifies and trims execution payloads. Aggregated shell commands are expanded in place so
 * every persisted command Part represents exactly one top-level command. Result/command
 * association is performed only with an exact source_execution_id.
 */
export function normalizeSessionPayload(session) {
  for (const turn of Array.isArray(session?.turns) ? session.turns : []) {
    const originalParts = Array.isArray(turn?.parts) ? turn.parts : [];
    const parts = splitAggregatedShellCommandParts(originalParts);
    turn.parts = parts;
    const commands = new Map();
    const results = new Map();

    for (const part of parts) {
      const executionId = executionIdOf(part);
      if (!executionId) continue;
      if (isCommandPart(part)) commands.set(executionId, part);
      else if (isResultPart(part)) results.set(executionId, part);
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
          setExecutionMetadata(part, metadata, executionKind, location);
          continue;
        }
      }
      if (executionKind === "unclassified") markUnclassified(part, metadata);
      else setExecutionMetadata(part, metadata, executionKind, null);
    }

    for (const part of parts) {
      if (!part || typeof part !== "object" || !isResultPart(part) || isCommandPart(part)) continue;
      const metadata = parseMetadata(part.metadata_json);
      const command = commands.get(executionIdOf(part)) ?? null;
      const commandMetadata = parseMetadata(command?.metadata_json);
      const executionKind = classifyExecution(part, metadata, command, commandMetadata);
      const outcome = inferOutcome(command, part, executionKind);
      applyOutcome(part, outcome);

      if (isFailure(part)) {
        part.text = diagnosticText(part, metadata);
        setExecutionMetadata(part, metadata, executionKind, null);
      } else if (part.kind === "file_change" || executionKind === "file_change") {
        part.kind = "file_change";
        part.text = canonicalUnifiedDiff(part.text);
        if (part.text) setFileChangeCard(part, metadata);
        else setEmptyFileChangeResultCard(part, metadata);
      } else if (executionKind === "read" || executionKind === "search" || (
        executionKind !== "unclassified" && isSuccessful(part)
      )) {
        part.text = null;
        clearPayloadBudgetMetadata(metadata);
        setExecutionMetadata(part, metadata, executionKind, null);
      } else if (executionKind === "unclassified") {
        markUnclassified(part, metadata);
      } else {
        setExecutionMetadata(part, metadata, executionKind, null);
      }
    }
    turn.parts = splitFileChangeParts(parts).filter((part) => !isEmptySuccessfulShellResult(part));
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

function splitAggregatedShellCommandParts(parts) {
  const resultsByExecutionId = new Map();
  for (const part of parts) {
    const executionId = executionIdOf(part);
    if (executionId && isResultPart(part) && !isCommandPart(part)) {
      resultsByExecutionId.set(executionId, part);
    }
  }

  const splitExecutions = new Map();
  const expanded = parts.flatMap((part) => {
    if (!part || typeof part !== "object" || !isCommandPart(part) || typeof part.command !== "string") {
      return [part];
    }
    const executionId = executionIdOf(part);
    const metadata = parseMetadata(part.metadata_json);
    const peer = executionId ? resultsByExecutionId.get(executionId) ?? null : null;
    const executionKind = classifyExecution(part, metadata, peer, parseMetadata(peer?.metadata_json));
    if (executionKind !== "shell") return [part];

    const rawCommands = splitTopLevelShellCommands(part.command);
    if (rawCommands.length < 2) return [part];

    let pendingCommandLabel = null;
    const commands = [];
    for (const command of rawCommands) {
      const separator = parseSeparatorPrintCommand(command);
      if (separator) {
        pendingCommandLabel = separator.label;
        continue;
      }
      commands.push({
        command,
        commandLabel: pendingCommandLabel ?? part.command_label ?? null,
      });
      pendingCommandLabel = null;
    }
    if (commands.length === 1) {
      // Retain the original execution ID so its Result remains exactly paired.
      return [{
        ...part,
        command: commands[0].command,
        command_label: commands[0].commandLabel,
      }];
    } else if (commands.length === 0) {
      return [];
    }

    const commandCount = commands.length;
    const lastExecutionId = executionId ? splitExecutionId(executionId, commandCount) : null;
    if (executionId) {
      splitExecutions.set(executionId, { commandCount, lastExecutionId });
    }
    return commands.map(({ command, commandLabel }, index) => {
      const commandPart = {
        ...part,
        command,
        command_label: commandLabel,
        source_execution_id: executionId ? splitExecutionId(executionId, index + 1) : null,
      };
      writeMetadata(commandPart, {
        ...metadata,
        ...(executionId ? { source_execution_parent_id: executionId } : {}),
        command_index: index + 1,
        command_count: commandCount,
      });
      return commandPart;
    });
  });

  for (const part of expanded) {
    if (!part || typeof part !== "object" || isCommandPart(part)) continue;
    const executionId = executionIdOf(part);
    const split = executionId ? splitExecutions.get(executionId) : null;
    if (!split) continue;
    part.source_execution_id = split.lastExecutionId;
    writeMetadata(part, {
      ...parseMetadata(part.metadata_json),
      source_execution_parent_id: executionId,
      command_index: split.commandCount,
      command_count: split.commandCount,
    });
  }
  return expanded;
}

function splitExecutionId(executionId, commandIndex) {
  return `${executionId}:command:${commandIndex}`;
}

/**
 * Parses print commands that only emit a decorative separator, such as
 * `printf '%s\n' '--- staged diff stat ---'`, `printf '\n--- tests ---\n'`,
 * or `echo '=== results ==='`.
 * These are commonly injected between real commands in aggregated shell scripts
 * and carry no Command semantic value. A wrapped title becomes the label of the
 * next real command; an unlabeled divider is discarded without changing its label.
 */
function parseSeparatorPrintCommand(command) {
  const trimmed = String(command ?? "").trim();
  let body = null;

  const printfWithArgument = trimmed.match(/^printf\s+(['"])([\s\S]*?)\1\s+(['"])([\s\S]*?)\3\s*$/);
  if (printfWithArgument) {
    const format = printfWithArgument[2];
    const substitutions = format.match(/%s/g) ?? [];
    if (substitutions.length === 1 && /^(?:%s|\\[nrt]|\s)+$/.test(format)) {
      body = printfWithArgument[4];
    }
  } else {
    const printfLiteral = trimmed.match(/^printf\s+(['"])([\s\S]*?)\1\s*$/);
    const echoLiteral = trimmed.match(/^echo\s+(?:-[A-Za-z]+\s+)*(['"])([\s\S]*?)\1\s*$/);
    body = printfLiteral?.[2] ?? echoLiteral?.[2] ?? null;
  }
  if (body == null) return null;

  const printedText = body
    .replace(/^(?:(?:\\[nrt])+|\s)+|(?:(?:\\[nrt])+|\s)+$/g, "")
    .trim();
  if (!printedText) return null;

  const divider = "[-=*~_─━—–]";
  if (new RegExp(`^${divider}+$`, "u").test(printedText)) {
    return { label: null };
  }
  const wrappedLabel = printedText.match(
    new RegExp(`^${divider}{2,}\\s*(.{1,80}?)\\s*${divider}{2,}$`, "u"),
  );
  if (!wrappedLabel) return null;
  const label = wrappedLabel[1].replace(/\\s+/g, " ").trim();
  return { label: label || null };
}

function splitTopLevelShellCommands(value) {
  const source = String(value ?? "").replace(/\r\n?/g, "\n").trim();
  if (!source || isComplexShellScript(source)) return source ? [source] : [];

  const commands = [];
  let start = 0;
  let quote = null;
  let escaped = false;
  let parenDepth = 0;
  let braceDepth = 0;
  let bracketDepth = 0;
  let previousNonWhitespace = null;

  const pushCommand = (end) => {
    const command = source.slice(start, end).trim();
    if (command) commands.push(command);
  };

  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    const precedingNonWhitespace = previousNonWhitespace;
    if (!/\s/.test(char)) previousNonWhitespace = char;
    if (escaped) {
      escaped = false;
      continue;
    }
    if (char === "\\" && quote !== "'") {
      escaped = true;
      continue;
    }
    if (quote) {
      if (char === quote) quote = null;
      continue;
    }
    if (char === "'" || char === '"' || char === "`") {
      quote = char;
      continue;
    }
    if (char === "(") parenDepth += 1;
    else if (char === ")") parenDepth = Math.max(0, parenDepth - 1);
    else if (char === "{") braceDepth += 1;
    else if (char === "}") braceDepth = Math.max(0, braceDepth - 1);
    else if (char === "[") bracketDepth += 1;
    else if (char === "]") bracketDepth = Math.max(0, bracketDepth - 1);

    if (parenDepth || braceDepth || bracketDepth) continue;
    let nextNonWhitespace = next;
    if (char === "\n") {
      let cursor = index + 1;
      while (source[cursor] === " " || source[cursor] === "\t") cursor += 1;
      nextNonWhitespace = source[cursor];
    }
    const continuedPipeline = char === "\n"
      && (precedingNonWhitespace === "|" || nextNonWhitespace === "|");
    let separatorLength = 0;
    if (!continuedPipeline) {
      if ((char === "&" && next === "&") || (char === "|" && next === "|")) {
        separatorLength = 2;
      } else if (char === ";" || char === "\n") {
        separatorLength = 1;
      }
    }
    if (!separatorLength) continue;
    pushCommand(index);
    index += separatorLength - 1;
    start = index + 1;
  }
  pushCommand(source.length);
  return commands.length > 0 ? commands : [source];
}

function isComplexShellScript(source) {
  return /<<-?\s*['"]?[A-Za-z_][A-Za-z0-9_]*['"]?/.test(source)
    || /(?:^|[;&|\n]\s*)(?:for|select|while|until|if|case|function)\b/.test(source)
    || /^\s*\{(?:\s|\n)/.test(source);
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
  return part?.kind === "file_change" || cardTypeOf(part) === "result";
}

function cardTypeOf(part, metadata = parseMetadata(part?.metadata_json)) {
  const card = part?.content_card ?? metadata.content_card ?? metadata.contentCard;
  const kind = typeof card?.kind === "string" ? card.kind : card?.type;
  return typeof kind === "string" ? kind.slice(kind.lastIndexOf(".") + 1) : null;
}

function classifyExecution(part, metadata, peer, peerMetadata = {}) {
  for (const candidate of [metadata.execution_kind, metadata.executionKind, peerMetadata.execution_kind, peerMetadata.executionKind]) {
    if (["read", "search", "shell", "file_change"].includes(candidate)) return candidate;
  }
  const sourceType = [
    metadata.source_type,
    metadata.sourceType,
    metadata.name,
    metadata.tool,
    peerMetadata.source_type,
    peerMetadata.sourceType,
    peerMetadata.name,
    peerMetadata.tool,
  ].filter(Boolean).join(" ").toLowerCase();
  if (/view[_ -]?file|read[_ -]?file|list[_ -]?directory|\bread\b|\bls\b/.test(sourceType)) return "read";
  if (/grep|search|find|glob/.test(sourceType)) return "search";
  if (/(?:^|\s)(?:apply[_ -]?patch|patch|edit|multi[_ -]?edit|write(?:[_ -]?file)?|create[_ -]?file|str[_ -]?replace[_ -]?editor|file[_ -]?change|code[_ -]?action)(?:\s|$)/.test(sourceType)) return "file_change";

  const command = String(part?.command ?? peer?.command ?? "").trim();
  if (/^(cat|head|tail|less|more|bat|sed|awk)\b/i.test(command)) return "read";
  if (/^(rg|grep|find|fd)\b/i.test(command)) return "search";
  if (/shell|command|exec|run_command/.test(sourceType)) return "shell";
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

function extractLocation(command, cwd, metadata) {
  const explicit = metadata.file_path ?? metadata.filePath ?? metadata.path ?? metadata.AbsolutePath;
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
  const lineStart = integer(metadata.line_start ?? metadata.lineStart ?? metadata.offset) ?? integer(range?.[1]);
  const explicitEnd = integer(metadata.line_end ?? metadata.lineEnd);
  const limit = integer(metadata.line_limit ?? metadata.lineLimit ?? metadata.limit);
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

function isEmptySuccessfulShellResult(part) {
  if (!part || typeof part !== "object" || isCommandPart(part) || !isResultPart(part)) return false;
  const metadata = parseMetadata(part.metadata_json);
  const executionKind = metadata.execution_kind ?? metadata.executionKind;
  return executionKind === "shell" && !cleanText(part.text) && isSuccessful(part);
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
