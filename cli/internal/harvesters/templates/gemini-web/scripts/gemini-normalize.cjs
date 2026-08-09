/**
 * @file gemini-normalize.cjs — Google Gemini 网页端 API 响应格式归一化转换模块
 *
 * 职责：
 * 1. 解码 Google Protobuf/Batchexecute 深度嵌套数组结构
 * 2. 从响应 Candidate 节点中提取答案文本、代码块 Artifact 与多媒体/生成的图片链接
 * 3. 将嵌套数据归一化为标准的 Session Turn 与 Part 数据块结构
 */
const crypto = require("crypto");

/**
 * 针对深度嵌套数组的强类型路径安全访问器
 *
 * @param {any} value - 根对象/数组
 * @param {(number|string)[]} path - 下标路径数组 (如 [0, 1, 3])
 * @param {any} [fallback=undefined] - 默认回退值
 * @returns {any} 找到的值或 fallback
 */
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

/** 辅助字符串清理函数 */
function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

/** 构造标准化的内容卡片元数据 JSON 字符串 */
function metadata(contentCard, extra = {}) {
  return JSON.stringify({
    ...extra,
    content_card: contentCard
  });
}

/** 构造统一的标准归一化 Part 数据结构 */
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

/**
 * 从 Candidate 节点中提炼候选回答文本，并剥离 googleusercontent 的内部占位 URL
 * @param {array} candidate - 原始 Candidate 数组
 * @returns {string} 过滤后的正文文本
 */
function candidateText(candidate) {
  let value = text(nested(candidate, [1, 0], ""));
  if (/^http:\/\/googleusercontent\.com\/card_content\/\d+/.test(value)) {
    value = text(nested(candidate, [22, 0], "")) || value;
  }
  return value.replace(/http:\/\/googleusercontent\.com\/[\w-]+\/\d+\n*/g, "").trim();
}

/**
 * 解析围栏代码块并提取语言及纯代码内容
 * @param {string} value - 源码文本
 * @param {string} [filename] - 文件名
 */
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

/** 根据文件名后缀推断代码编程语言 */
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

/** 判断字符串是否符合文件名特征 */
function looksLikeFilename(value) {
  return /\.(html?|css|m?js|cjs|jsx|tsx?|json|py|go|rs|java|sql|md|ya?ml|sh|bash|txt)$/i.test(String(value || ""));
}

/** 判断字符串是否包含围栏代码特征 */
function looksLikeCode(value) {
  const source = text(value);
  return source.startsWith("```") || /^\s*<!doctype html/i.test(source) || /^\s*<html[\s>]/i.test(source);
}

/** 深度遍历数组查找首个符合条件的字符串 */
function findFirstString(node, predicate) {
  if (!Array.isArray(node)) return null;
  for (const item of node) {
    if (typeof item === "string" && predicate(item)) return item;
  }
  return null;
}

/** 从 Gemini 节点结构中深度搜索与提炼生成的代码 Artifact */
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
            metadata_json: metadata({ type: "code", language: parsed.language }, { filename })
          }));
        }
      }
    }
    for (const child of node) visit(child);
  };
  visit(candidate);
  return parts;
}

/** 判断是否为 Google 托管的媒体资源 URL */
function isMediaURL(value) {
  return (
    /^https:\/\/lh3\.googleusercontent\.com\/gg\//.test(value) ||
    /^http:\/\/googleusercontent\.com\/image_generation_content\/\d+/.test(value)
  );
}

/** 判断是否符合媒体扩展名特征 */
function looksLikeMediaFilename(value) {
  return /\.(png|jpe?g|webp|gif|heic|heif|mp4|mov|webm|m4v|pdf)$/i.test(String(value || ""));
}

/** 判断是否符合 MIME Type 特征 */
function looksLikeMime(value) {
  return /^(image|video|application\/pdf)\//i.test(String(value || ""));
}

/** 从节点子结构中递归收集关联的图片/视频/PDF媒体引用指针 */
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

/** 将媒体引用格式化为 Markdown 附件段落 */
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

/**
 * 将 Batchexecute 中的单个原始 Turn 转译为标准的 Turn 结构
 *
 * @param {string} cid - Conversation ID
 * @param {array} rawTurn - 原始嵌套 Turn 数组
 * @param {number} index - 轮次序号
 * @returns {object|null}
 */
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
        parts.push(normalizedPart("assistant", "text", {
          text: answer,
          metadata_json: metadata({ type: "answer", format: "markdown" })
        }));
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
          parts.push(normalizedPart("tool", "tool", {
            text: mediaText,
            status: "completed",
            metadata_json: metadata({ type: "result", format: "markdown" })
          }));
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

/**
 * 解析 Gemini 详情接口的完整的 Batchexecute 主响应体
 *
 * @param {string} cid - 会话 external_id
 * @param {array} body - 解码后的 JSON 响应体
 * @returns {object[]} 标准化 Turn 轮次数组
 */
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
