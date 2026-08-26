/**
 * Versioned shell command display projector.
 *
 * This module is intentionally side-effect free: raw command Parts stay intact
 * in persistence and callers receive temporary display nodes only.
 */
const SHELL_PROJECTOR_VERSION = "shell-projector-v1";
const SHELL_PROJECTOR_SCHEMA_VERSION = 1;

/**
 * Parses print commands that only emit a decorative separator, such as
 * `printf '%s\n' '--- staged diff stat ---'`, `printf '\n--- tests ---\n'`,
 * `echo '=== results ==='`, or `echo "== file counts =="`.
 * These are commonly injected between real commands in aggregated shell scripts
 * and carry no Command semantic value. A wrapped title becomes the label of the
 * next real command; an unlabeled divider is discarded without changing its label.
 *
 * Double-quoted bodies containing shell variable substitution (`$var`, `${var}`,
 * `$(cmd)`) are rejected because the label text is only known at runtime.
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

  // Reject double-quoted bodies with shell variable substitution or command
  // substitution — the printed label would only be known at runtime.
  if (/(?:^|[^\\])\$(?:[A-Za-z_{(]|\d)/.test(body)) return null;

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
  if (!source || isUnsplittableShellScript(source)) return source ? [source] : [];

  // If the source contains structured shell keywords (for/while/if/case/...),
  // try block-aware splitting first. This correctly handles composite scripts
  // like `for ... done\nprintf '== label =='\nfind ...`.
  if (hasStructuredShellKeywords(source)) {
    const blocks = splitStructuredShellBlocks(source);
    if (blocks.length > 1) return blocks;
    // Single block means the whole script is one compound statement — keep it.
    return [source];
  }

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

/**
 * Detects scripts that must never be split, regardless of keywords:
 * heredocs and bare `{ ... }` compound commands at the top level.
 */
function isUnsplittableShellScript(source) {
  return /<<-?\s*['"]?[A-Za-z_][A-Za-z0-9_]*['"]?/.test(source)
    || /^\s*\{(?:\s|\n)/.test(source);
}

/**
 * Returns true when the source contains structured shell keywords that
 * require block-aware splitting (for/while/until/select/if/case/function).
 */
function hasStructuredShellKeywords(source) {
  return /(?:^|[;&|\n]\s*)(?:for|select|while|until|if|case|function)\b/.test(source);
}

/**
 * Splits a compound shell script at top-level statement boundaries while
 * keeping structured blocks (for...done, while...done, until...done,
 * select...done, if...fi, case...esac) intact as single commands.
 *
 * The splitter tracks a keyword nesting stack:
 *   for/while/until/select → expects `done`
 *   if → expects `fi`
 *   case → expects `esac`
 *
 * Splitting on `;` and `\n` only happens when the stack is empty and we
 * are outside quotes, parentheses, braces, and brackets.
 *
 * Returns the raw command strings. If the script cannot be cleanly split
 * (e.g. unbalanced blocks), returns the entire source as a single element.
 */
function splitStructuredShellBlocks(source) {
  // Tokenize into words and separators, respecting quotes and escapes.
  const commands = [];
  let start = 0;
  let quote = null;
  let escaped = false;
  let parenDepth = 0;
  let braceDepth = 0;
  let bracketDepth = 0;
  const blockStack = []; // expected closing keywords
  let previousNonWhitespace = null;
  // Track whether we just closed a block — this prevents splitting the
  // separator immediately after `done`/`fi`/`esac` from being absorbed
  // into the previous command. The closing keyword itself is the last
  // token of the block, but the `;` or `\n` after it is the separator.
  let justClosedBlock = false;

  // Collect the word at the current scan position for keyword matching.
  // Returns the word starting at `pos` up to the next non-identifier char.
  const wordAt = (pos) => {
    const match = source.slice(pos).match(/^[A-Za-z_][A-Za-z0-9_]*/);
    return match ? match[0] : null;
  };

  // Check that the character before `pos` is a valid keyword boundary
  // (start-of-string, whitespace, `;`, `|`, `&`, `\n`, or `(`).
  const isKeywordBoundary = (pos) => {
    if (pos <= 0) return true;
    const c = source[pos - 1];
    return /[\s;&|\n(]/.test(c);
  };

  const pushCommand = (end) => {
    const command = source.slice(start, end).trim();
    if (command) commands.push(command);
  };

  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];
    const precedingNonWhitespace = previousNonWhitespace;
    if (!/\s/.test(char)) {
      previousNonWhitespace = char;
      justClosedBlock = false;
    }

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
    if (char === "(") { parenDepth += 1; continue; }
    if (char === ")") { parenDepth = Math.max(0, parenDepth - 1); continue; }
    if (parenDepth) continue;

    if (char === "{") { braceDepth += 1; continue; }
    if (char === "}") { braceDepth = Math.max(0, braceDepth - 1); continue; }
    if (char === "[") { bracketDepth += 1; continue; }
    if (char === "]") { bracketDepth = Math.max(0, bracketDepth - 1); continue; }
    if (braceDepth || bracketDepth) continue;

    // Keyword detection: only at word boundaries outside quotes/nesting.
    if (/[A-Za-z_]/.test(char) && isKeywordBoundary(index)) {
      const word = wordAt(index);
      if (word) {
        // Check the character after the word is also a boundary.
        const afterWord = index + word.length;
        const charAfter = source[afterWord];
        const wordEnds = afterWord >= source.length || /[\s;&|\n)]/.test(charAfter);
        if (wordEnds) {
          if (word === "for" || word === "while" || word === "until" || word === "select") {
            blockStack.push("done");
          } else if (word === "if") {
            blockStack.push("fi");
          } else if (word === "case") {
            blockStack.push("esac");
          } else if (blockStack.length > 0 && word === blockStack[blockStack.length - 1]) {
            blockStack.pop();
            justClosedBlock = true;
          }
        }
        // Skip past the keyword to avoid re-scanning its characters.
        if (word.length > 1) {
          index += word.length - 1;
          continue;
        }
      }
    }

    // Only split when the block stack is empty (top-level).
    if (blockStack.length > 0) continue;

    // Pipeline continuation: don't split on `\n` when adjacent to `|`.
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

  // If any blocks were left unclosed, the parse is unreliable — fall back.
  if (blockStack.length > 0) return [source];

  return commands.length > 0 ? commands : [source];
}


function compactNode(value) {
  return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== ""));
}

function projectCommandPart(part) {
  if (!part || typeof part !== "object" || typeof part.command !== "string") {
    throw new TypeError("project_command_parts.parts entries require a command string");
  }
  const nodes = [];
  let pendingLabel = null;
  for (const command of splitTopLevelShellCommands(part.command)) {
    const separator = parseSeparatorPrintCommand(command);
    if (separator) {
      if (separator.label != null) pendingLabel = separator.label;
      continue;
    }
    nodes.push(compactNode({
      display_order: nodes.length,
      command,
      command_label: pendingLabel ?? part.command_label ?? null,
    }));
    pendingLabel = null;
  }
  return {
    part_id: part.part_id ?? part.id ?? null,
    schema_version: SHELL_PROJECTOR_SCHEMA_VERSION,
    projector_version: SHELL_PROJECTOR_VERSION,
    nodes,
  };
}

function projectCommandParts(parts) {
  if (!Array.isArray(parts)) throw new TypeError("project_command_parts.params.parts must be an array");
  return parts.map(projectCommandPart);
}


module.exports = { SHELL_PROJECTOR_VERSION, SHELL_PROJECTOR_SCHEMA_VERSION, parseSeparatorPrintCommand, splitTopLevelShellCommands, projectCommandPart, projectCommandParts };
