const crypto = require("crypto");

function nested(value, path, fallback = undefined) {
  let current = value;
  for (const key of path) {
    if (Array.isArray(current) && Number.isInteger(key) && key >= 0 && key < current.length) {
      current = current[key];
    } else {
      return fallback;
    }
  }
  return current == null ? fallback : current;
}

function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

function normalizedPart(role, kind, fields = {}) {
  return {
    role,
    kind,
    text: fields.text ?? null,
    language: fields.language ?? null,
    command: fields.command ?? null,
    cwd: fields.cwd ?? null,
    status: fields.status ?? null,
    exit_code: fields.exit_code ?? null,
    metadata_json: fields.metadata_json ?? null
  };
}

function candidateText(candidate) {
  let value = text(nested(candidate, [1, 0], ""));
  if (/^http:\/\/googleusercontent\.com\/card_content\/\d+/.test(value)) {
    value = text(nested(candidate, [22, 0], "")) || value;
  }
  return value.replace(/http:\/\/googleusercontent\.com\/[\w-]+\/\d+\n*/g, "").trim();
}

function parseFencedCode(value, filename) {
  const source = text(value);
  if (!source) return null;
  const match = source.match(/^```([A-Za-z0-9_+-]*)\n([\s\S]*?)\n?```$/);
  if (match) {
    return {
      language: match[1] || inferLanguage(filename),
      code: match[2].trimEnd()
    };
  }
  if (/^\s*<!doctype html/i.test(source) || /^\s*<html[\s>]/i.test(source)) {
    return { language: "html", code: source.trimEnd() };
  }
  return null;
}

function inferLanguage(filename) {
  const lower = String(filename || "").toLowerCase();
  if (lower.endsWith(".html") || lower.endsWith(".htm")) return "html";
  if (lower.endsWith(".css")) return "css";
  if (lower.endsWith(".js") || lower.endsWith(".mjs") || lower.endsWith(".cjs")) return "javascript";
  if (lower.endsWith(".jsx")) return "jsx";
  if (lower.endsWith(".ts")) return "typescript";
  if (lower.endsWith(".tsx")) return "tsx";
  if (lower.endsWith(".json")) return "json";
  if (lower.endsWith(".py")) return "python";
  if (lower.endsWith(".go")) return "go";
  if (lower.endsWith(".rs")) return "rust";
  if (lower.endsWith(".java")) return "java";
  if (lower.endsWith(".sql")) return "sql";
  if (lower.endsWith(".md")) return "markdown";
  if (lower.endsWith(".yaml") || lower.endsWith(".yml")) return "yaml";
  if (lower.endsWith(".sh") || lower.endsWith(".bash")) return "bash";
  return null;
}

function looksLikeFilename(value) {
  return /\.(html?|css|m?js|cjs|jsx|tsx?|json|py|go|rs|java|sql|md|ya?ml|sh|bash|txt)$/i.test(String(value || ""));
}

function looksLikeCode(value) {
  const source = text(value);
  return source.startsWith("```") || /^\s*<!doctype html/i.test(source) || /^\s*<html[\s>]/i.test(source);
}

function findFirstString(node, predicate) {
  if (!Array.isArray(node)) return null;
  for (const item of node) {
    if (typeof item === "string" && predicate(item)) return item;
  }
  return null;
}

function extractArtifactParts(candidate) {
  const parts = [];
  const seen = new Set();
  const visit = (node) => {
    if (!Array.isArray(node)) return;
    const filename = findFirstString(node, looksLikeFilename);
    const codeSource = findFirstString(node, looksLikeCode);
    if (filename && codeSource) {
      const parsed = parseFencedCode(codeSource, filename);
      if (parsed && parsed.code) {
        const fingerprint = crypto.createHash("sha256").update(`${filename}\n${parsed.code}`).digest("hex");
        if (!seen.has(fingerprint)) {
          seen.add(fingerprint);
          parts.push(normalizedPart("assistant", "code_block", {
            text: parsed.code,
            language: parsed.language,
            metadata_json: JSON.stringify({ filename })
          }));
        }
      }
    }
    for (const child of node) visit(child);
  };
  visit(candidate);
  return parts;
}

function isMediaURL(value) {
  return (
    /^https:\/\/lh3\.googleusercontent\.com\/gg\//.test(value) ||
    /^http:\/\/googleusercontent\.com\/image_generation_content\/\d+/.test(value)
  );
}

function looksLikeMediaFilename(value) {
  return /\.(png|jpe?g|webp|gif|heic|heif|mp4|mov|webm|m4v|pdf)$/i.test(String(value || ""));
}

function looksLikeMime(value) {
  return /^(image|video|application\/pdf)\//i.test(String(value || ""));
}

function extractMediaRefs(node) {
  const refs = [];
  const seen = new Set();
  const visit = (value, siblings = []) => {
    if (typeof value === "string" && isMediaURL(value)) {
      if (seen.has(value)) return;
      seen.add(value);
      refs.push({
        url: value,
        filename: siblings.find((item) => typeof item === "string" && looksLikeMediaFilename(item)) || null,
        mime: siblings.find((item) => typeof item === "string" && looksLikeMime(item)) || null
      });
      return;
    }
    if (Array.isArray(value)) {
      for (const child of value) visit(child, value);
    }
  };
  visit(node);
  return refs;
}

function formatMediaRefs(label, refs) {
  if (!refs.length) return "";
  const lines = [label];
  refs.forEach((ref, index) => {
    const name = ref.filename || ref.mime || `media-${index + 1}`;
    const mime = ref.mime ? ` (${ref.mime})` : "";
    lines.push(`- [${name}](${ref.url})${mime}`);
  });
  return lines.join("\n");
}

function normalizeTurn(cid, rawTurn, index) {
  const userMedia = extractMediaRefs(nested(rawTurn, [2], []));
  let userText = text(nested(rawTurn, [2, 0, 0], ""));
  if (userMedia.length) {
    userText = [userText, formatMediaRefs("User attachments:", userMedia)].filter(Boolean).join("\n\n");
  }

  const rid = text(nested(rawTurn, [0, 1], "")) || `${cid}-turn-${index + 1}`;
  const parts = [];
  const seenAnswers = new Set();
  const seenMediaBlocks = new Set();
  const candidates = nested(rawTurn, [3, 0], []);
  if (Array.isArray(candidates)) {
    for (const candidate of candidates) {
      const answer = candidateText(candidate);
      if (answer && !seenAnswers.has(answer)) {
        seenAnswers.add(answer);
        parts.push(normalizedPart("assistant", "text", { text: answer }));
      }
      for (const part of extractArtifactParts(candidate)) {
        parts.push(part);
      }
      const mediaRefs = extractMediaRefs(candidate);
      const mediaText = formatMediaRefs("Gemini media:", mediaRefs);
      if (mediaText) {
        const fingerprint = crypto.createHash("sha256").update(mediaText).digest("hex");
        if (!seenMediaBlocks.has(fingerprint)) {
          seenMediaBlocks.add(fingerprint);
          parts.push(normalizedPart("assistant", "text", { text: mediaText }));
        }
      }
    }
  }

  if (!userText) {
    if (!parts.length) return null;
    userText = "[Gemini continuation without visible user prompt]";
  }

  return {
    external_id: rid,
    turn_index: index,
    user_text: userText,
    title: null,
    started_at: null,
    ended_at: null,
    parts
  };
}

function parseDetailBody(cid, body) {
  const rawTurns = nested(body, [0], []);
  if (!Array.isArray(rawTurns)) return [];
  const turns = [];
  for (const rawTurn of rawTurns.slice().reverse()) {
    const turn = normalizeTurn(cid, rawTurn, turns.length);
    if (turn) turns.push(turn);
  }
  return turns;
}

module.exports = {
  candidateText,
  extractArtifactParts,
  extractMediaRefs,
  formatMediaRefs,
  normalizeTurn,
  parseDetailBody
};
