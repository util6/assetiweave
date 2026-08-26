#!/usr/bin/env node
/**
 * @file OpenAI Codex / Codex App 会话日志解析适配器 (Codex Conversation Adapter)
 * @description 负责读取与解析 Codex 本地 SQLite 数据库 (`state_5.sqlite`) 及 JSONL (`rollout.jsonl`) 会话日志，
 *              提取消息、命令行指令执行（Tool Exec）、Skill 软链接依赖与终端输出，并归一化为标准的 Card Schema v1 结构。
 */
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { normalizeSessionPayload } from "./payload-policy.mjs";
import shellProjector from "./shell-projector.cjs";
const { projectCommandParts, SHELL_PROJECTOR_VERSION } = shellProjector;

// ---------------------------------------------------------------------------
// 全局常量配置与配额定义 (Global Constants & Budget Rules)
// ---------------------------------------------------------------------------

/** 从 Rust 标准输入 (FD 0) 同步读取传入的 JSON 格式 IPC 请求参数 */
const input = JSON.parse(readFileSync(0, "utf8") || "{}");

/** 标识当前 Codex 内容卡片的 Schema 版本号，用于增量 Token 生成与缓存失效判定 */
const CONTENT_CARD_SCHEMA_VERSION = "codex-content-cards-v17";

/** 单条 Part 节点的文本最大字符数上限 (96KB)，超长则触发截断规则 */
const MAX_PART_TEXT_CHARS = 96 * 1024;

/** 单个 Session 允许提取的最大文本总预算 (384KB)，防止超大日志耗尽内存 */
const MAX_SESSION_TEXT_CHARS = 384 * 1024;

/** 经过低信号算法压缩后的 Tool 文本最大允许预算 (24KB) */
const MAX_COMPACTED_TOOL_TEXT_CHARS = 24 * 1024;

/** 为标准 Part 节点预留的最少保证文本字符数 (96KB) */
const MIN_STANDARD_SESSION_TEXT_CHARS = 96 * 1024;

/** 在压缩终端输出日志时，头部与尾部各自保留的完整行数 */
const BROWSE_OUTPUT_EDGE_LINES = 24;

/** 在命中高信号匹配行 (如 Error/Warning) 时，围绕该行前后提取的上下文行数 */
const BROWSE_OUTPUT_CONTEXT_LINES = 2;

/** 浏览窗口中单行终端文本的最大允许字符数上限 (1200 字符) */
const MAX_BROWSE_OUTPUT_LINE_CHARS = 1200;

/**
 * 识别关键错误、异常与高信号日志信息的正则表达式模式；
 * 当终端输出文本超长时，低信号压缩算法会优先保留匹配此正则的行及其前后上下文。
 */
const SIGNAL_LINE_PATTERN =
  /\b(error|failed|failure|panic|exception|traceback|warning|warn|denied|not found|cannot|could not|timeout|timed out|exit code|failures?|caused by|compilation|syntaxerror|typeerror|referenceerror|assertionerror)\b|error\[[A-Za-z0-9_-]+\]|\b[A-Za-z0-9_./-]+:\d+:\d+\b/i;

// ---------------------------------------------------------------------------
// IPC 与路径工具函数 (Stdio IPC & Path Utilities)
// ---------------------------------------------------------------------------

/**
 * 向标准输出 (stdout) 逐行写出 JSON 格式的 IPC 通信事件，供 Rust 父进程读取
 * @param {string} type - 事件类型 ("item" | "complete" | "error")
 * @param {object} [payload={}] - 伴随事件发送的数据载荷
 */
function emit(type, payload = {}) {
  process.stdout.write(`${JSON.stringify({ type, ...payload })}\n`);
}

/**
 * 当遇到严重错误时，向 Rust 发送 "error" 事件，并发送 "complete" 事件闭合流
 * @param {string} message - 错误描述信息
 */
function fail(message) {
  emit("error", { message });
  emit("complete", { item: {} });
}

/**
 * 将用户路径中的 `~` 符号展开为当前操作系统的真实 Home 绝对路径
 * @param {string} value - 待转换的路径字符串
 * @returns {string} 展开后的绝对路径
 */
function expandPath(value) {
  if (!value) return value;
  if (value === "~") return homedir();
  if (value.startsWith("~/")) return path.join(homedir(), value.slice(2));
  return value;
}

/**
 * 计算输入文本的 SHA-256 哈希散列值 (HEX 编码)
 * @param {string} text - 输入文本
 * @returns {string} 64 位 16 进制 SHA-256 字符串
 */
function sha256(text) {
  return createHash("sha256").update(text).digest("hex");
}

/**
 * 根据文件路径、更新时间戳、文件大小及 mtime 生成强类型增量版本 Token，用于快照对比
 * @param {string} filePath - JSONL 文件的绝对路径
 * @param {string|null} [updatedAt=null] - 数据库记录的更新时间
 * @returns {string} SHA-256 版本 Token 字符串
 */
function fileVersionToken(filePath, updatedAt = null) {
  const stat = statSync(filePath);
  return sha256(`${CONTENT_CARD_SCHEMA_VERSION}\0${updatedAt ?? ""}\0${stat.size}\0${stat.mtimeMs}`);
}

/**
 * 具有并发变更校验的安全文件读取函数；
 * 在读取前后对比文件 Token，若读取期间文件被 Codex 追加写入，则自动重试一次或抛出异常。
 * @param {string} filePath - 文件路径
 * @param {string|null} [updatedAt=null] - 更新时间
 * @returns {{ text: string, versionToken: string }} 稳定内容与校验 Token
 */
function readStableFile(filePath, updatedAt = null) {
  for (let attempt = 0; attempt < 2; attempt++) {
    const before = fileVersionToken(filePath, updatedAt);
    const text = readFileSync(filePath, "utf8");
    const after = fileVersionToken(filePath, updatedAt);
    if (before === after) return { text, versionToken: after };
  }
  throw new Error(`session changed while being read: ${filePath}`);
}

/**
 * 通过子进程调用 `sqlite3 -json` 命令行工具查询 SQLite 数据库并返回解析后的 JSON 数组
 * @param {string} dbPath - SQLite 数据库文件路径
 * @param {string} sql - 欲执行的 SQL 查询语句
 * @returns {Array<object>} 查询结果对象数组
 */
function sqliteJson(dbPath, sql) {
  const result = spawnSync("sqlite3", ["-json", dbPath, sql], { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error((result.stderr || `sqlite3 exited with ${result.status}`).trim());
  }
  const text = result.stdout.trim();
  return text ? JSON.parse(text) : [];
}

/**
 * 对 SQL 标识符 (如表名、列名) 进行安全转义转置，防止 SQL 注入风险
 * @param {string} name - 列名或表名
 * @returns {string} 双引号包裹并转义好的标识符字符串
 */
function quoteIdent(name) {
  return `"${String(name).replaceAll("\"", "\"\"")}"`;
}

/**
 * 从给定的数据库列名列表中，匹配并返回第一个存在的候选列名
 * @param {Array<string>} columns - 数据库表中实际存在的列名列表
 * @param {Array<string>} candidates - 期望匹配的候选列名列表
 * @returns {string|undefined} 匹配到的列名
 */
function pick(columns, candidates) {
  return candidates.find((name) => columns.includes(name));
}

// ---------------------------------------------------------------------------
// 文本抽取与 JSON 解析工具 (Text Extraction & Parsing Helpers)
// ---------------------------------------------------------------------------

/**
 * 从多种数据结构中提取纯文本字符串内容 (如 ChatGPT/Codex 的 content 数组结构)
 * @param {string|Array<object|string>|null} content - 消息文本或内容节点数组
 * @returns {string} 提取到的纯文本
 */
function contentText(content) {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .map((item) => {
      if (typeof item === "string") return item;
      return item?.text ?? item?.content ?? "";
    })
    .filter(Boolean)
    .join("\n\n");
}

/**
 * 安全地尝试解析 JSON 文本；若失败或非对象/数组类型则返回 null
 * @param {unknown} value - 待解析的输入对象或文本
 * @returns {object|Array|null} 解析出的 JSON 结构或 null
 */
function parseJsonValue(value) {
  if (typeof value !== "string") return null;
  const text = value.trim();
  if (!text.startsWith("{") && !text.startsWith("[")) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/**
 * 将标量值 (String/Number/Boolean) 转为标准字符串
 * @param {unknown} value - 输入值
 * @returns {string|null} 转换后的字符串或 null
 */
function valueAsString(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return null;
}

/**
 * 将包含对象在内的任何值转换为展示用的字符串 (对象会被序列化为 JSON)
 * @param {unknown} value - 输入值
 * @returns {string|null} 格式化后的展示文本
 */
function valueAsDisplayString(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value && typeof value === "object") return JSON.stringify(value);
  return null;
}

/**
 * 从类似 JavaScript 代码片段的字符串中，通过正则表达式正则匹配提取指定名称的字符串属性值
 * @param {unknown} value - 原始文本或代码段
 * @param {Array<string>} names - 欲提取的字段属性名数组
 * @returns {string|null} 提取到的字符串或 null
 */
function javascriptStringField(value, names) {
  const source = String(value ?? "");
  for (const name of names) {
    const escapedName = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const propertyName = `(?:${escapedName}|\"${escapedName}\")`;
    const match = source.match(
      new RegExp(`(?:^|[,{\\s])${propertyName}\\s*:\\s*(\"(?:\\\\.|[^\"\\\\])*\")`),
    );
    if (!match) continue;
    try {
      const parsed = JSON.parse(match[1]);
      if (typeof parsed === "string" && parsed.trim()) return parsed;
    } catch {
      // 忽略格式损坏的字符串，按普通数据继续处理
    }
  }
  return null;
}

/**
 * 构造包含 `content_card` 元数据的标准 JSON 格式字符串
 * @param {object} contentCard - 内容卡片定义属性对象
 * @param {object} [extra={}] - 附加的扩展元数据字段
 * @returns {string} 序列化后的 JSON 字符串
 */
function metadata(contentCard, extra = {}) {
  return JSON.stringify({
    ...(extra && typeof extra === "object" && !Array.isArray(extra) ? extra : {}),
    content_card: contentCard,
  });
}

/**
 * 剔除对象中所有值为 null、undefined 或空字符串的冗余字段
 * @param {object} value - 原始键值对对象
 * @returns {object} 精简后的对象
 */
function compactObject(value) {
  return Object.fromEntries(
    Object.entries(value).filter(([, entry]) => entry !== null && entry !== undefined && entry !== ""),
  );
}

/**
 * 提取载荷中精简的根级元数据字段，避免包含臃肿的 Tool 复杂 Body
 * @param {object} value - 原始 Payload 对象
 * @returns {object} 精简后的属性字典对象
 */
function smallMetadata(value) {
  if (!value || typeof value !== "object") return {};
  return compactObject({
    source_type: value.type,
    name: value.name,
    tool: value.tool ?? value.tool_name ?? value.toolName,
    call_id: value.call_id ?? value.callID,
  });
}

/**
 * 提取 Tool 执行调用的唯一 Execution ID 标志符，供前端对齐 Command 指令卡片与 Result 结果卡片
 * @param {object} value - 原始 Payload 对象
 * @returns {string|null} 提取出的 ID 或 null
 */
function sourceExecutionId(value) {
  if (!value || typeof value !== "object") return null;
  const candidate = value.call_id ?? value.callID ?? value.tool_use_id ?? value.toolUseID;
  return typeof candidate === "string" && candidate.trim() ? candidate.trim() : null;
}

/**
 * 根据指定最大字符配额截断超长文本
 * @param {unknown} value - 输入原始文本
 * @param {number} maxChars - 允许的最大字符配额
 * @returns {{ text: string, truncated: boolean, originalChars: number }} 截断后的文本对象
 */
function truncateText(value, maxChars) {
  const text = String(value ?? "");
  if (text.length <= maxChars) return { text, truncated: false, originalChars: text.length };
  return {
    text: maxChars > 0 ? text.slice(0, maxChars).trimEnd() : "",
    truncated: true,
    originalChars: text.length,
  };
}

/**
 * 在 Part 的 `metadata_json` 中打上 `truncated: true` 标记及原始字符数统计信息
 * @param {object} part - 目标 Part 节点
 * @param {number} originalChars - 原始字符总数
 * @param {number} budget - 分配的字符配额
 */
function markPartTruncated(part, originalChars, budget) {
  const metadataValue = parseJsonValue(part.metadata_json) ?? {};
  metadataValue.truncated = true;
  metadataValue.original_chars = Math.max(Number(metadataValue.original_chars) || 0, originalChars);
  metadataValue.display_chars = String(part.text ?? "").length;
  metadataValue.display_budget_chars = budget;
  part.metadata_json = JSON.stringify(metadataValue);
}

/**
 * 在 Part 的 `metadata_json` 中打上 `compacted_for_browsing: true` 低信号文本压缩标记
 * @param {object} part - 目标 Part 节点
 * @param {number} originalChars - 压缩前的原始字符数
 * @param {number} budget - 压缩配额字符数
 */
function markPartCompactedForBrowsing(part, originalChars, budget) {
  const metadataValue = parseJsonValue(part.metadata_json) ?? {};
  metadataValue.truncated = true;
  metadataValue.compacted_for_browsing = true;
  metadataValue.original_chars = Math.max(Number(metadataValue.original_chars) || 0, originalChars);
  metadataValue.display_chars = String(part.text ?? "").length;
  metadataValue.compaction_budget_chars = budget;
  part.metadata_json = JSON.stringify(metadataValue);
}

/**
 * 获取 Part 卡片对应内容的类型名称 (如 answer, code, command, result)
 * @param {object} part - 目标 Part 节点
 * @returns {string|null} 卡片类型名称或 null
 */
function contentCardType(part) {
  const metadataValue = parseJsonValue(part.metadata_json);
  const card = metadataValue?.content_card ?? metadataValue?.contentCard;
  const type = card && typeof card === "object" && !Array.isArray(card) ? card.type : null;
  return typeof type === "string" ? type : null;
}

/**
 * 判断 Part 节点是否为高优先级浏览节点 (例如助手回答 answer 或生成的代码块 code)
 * @param {object} part - Part 节点
 * @returns {boolean} 是否为高优先级
 */
function isHighPriorityBrowsePart(part) {
  const type = contentCardType(part);
  return part.role === "assistant" || type === "answer" || type === "code";
}

/**
 * 【智能文本配额分配器】：计算 Session 内高优先级 Part (AI回答) 与标准 Part (工具输出) 的可分配字符配额
 * @param {object} session - Session 目标对象
 * @returns {{ highPriority: number, standard: number }} 算好的配额字典对象
 */
function highPriorityTextBudget(session) {
  let highPriorityTotal = 0;
  let standardTotal = 0;
  for (const turn of session.turns) {
    for (const part of turn.parts) {
      if (typeof part.text !== "string" || !part.text) continue;
      if (isHighPriorityBrowsePart(part)) {
        highPriorityTotal += Math.min(MAX_PART_TEXT_CHARS, part.text.length);
      } else {
        standardTotal += Math.min(MAX_PART_TEXT_CHARS, part.text.length);
      }
    }
  }
  const standardFloor = Math.min(standardTotal, MIN_STANDARD_SESSION_TEXT_CHARS, MAX_SESSION_TEXT_CHARS);
  const highPriorityBudget = Math.min(highPriorityTotal, MAX_SESSION_TEXT_CHARS - standardFloor);
  return {
    highPriority: highPriorityBudget,
    standard: Math.min(standardTotal, MAX_SESSION_TEXT_CHARS - highPriorityBudget),
  };
}

/**
 * 裁剪单行过长文本，防止前端终端行渲染崩溃
 * @param {unknown} value - 行文本
 * @returns {string} 裁剪后的行
 */
function browseLine(value) {
  const text = String(value ?? "");
  if (text.length <= MAX_BROWSE_OUTPUT_LINE_CHARS) return text;
  return `${text.slice(0, MAX_BROWSE_OUTPUT_LINE_CHARS)} [line truncated]`;
}

/**
 * 将区间有重叠或连续的多个索引区间合并为一个完整的无重叠区间列表
 * @param {Array<{start: number, end: number}>} ranges - 输入的区间数组
 * @returns {Array<{start: number, end: number}>} 合并后的区间数组
 */
function mergedRanges(ranges) {
  return ranges
    .filter((range) => range.end > range.start)
    .sort((a, b) => a.start - b.start)
    .reduce((merged, range) => {
      const previous = merged.at(-1);
      if (!previous || range.start > previous.end) {
        merged.push({ ...range });
      } else {
        previous.end = Math.max(previous.end, range.end);
      }
      return merged;
    }, []);
}

/**
 * 【低信号文本智能压缩算法】：
 * 1. 保留日志头部与尾部各自 24 行；
 * 2. 通过正则表达式 `SIGNAL_LINE_PATTERN` 扫描并匹配包含 Error, Warning, Panic 等关键词的关键行；
 * 3. 提取这些关键行前后 2 行的上下文切片，并合并为保留区间；
 * 4. 其它低信号冗余行将被替换为 `... omitted N low-signal lines ...` 提示，大幅压缩文本体积同时保留核心故障诊断线索。
 *
 * @param {unknown} value - 原始 Tool 输出文本
 * @param {number} maxChars - 压缩目标预算上限
 * @returns {{ text: string, compacted: boolean, originalChars: number }} 压缩结果
 */
function compactToolTextForBrowsing(value, maxChars) {
  const text = String(value ?? "");
  if (text.length <= maxChars) return { text, compacted: false, originalChars: text.length };

  const lines = text.split(/\r?\n/);
  const ranges = [
    { start: 0, end: Math.min(BROWSE_OUTPUT_EDGE_LINES, lines.length) },
    { start: Math.max(0, lines.length - BROWSE_OUTPUT_EDGE_LINES), end: lines.length },
  ];
  lines.forEach((line, index) => {
    if (!SIGNAL_LINE_PATTERN.test(line)) return;
    ranges.push({
      start: Math.max(0, index - BROWSE_OUTPUT_CONTEXT_LINES),
      end: Math.min(lines.length, index + BROWSE_OUTPUT_CONTEXT_LINES + 1),
    });
  });

  const budget = Math.max(0, maxChars);
  const pieces = [];
  let previousEnd = 0;
  for (const range of mergedRanges(ranges)) {
    const prefix = range.start > previousEnd ? `\n... omitted ${range.start - previousEnd} low-signal lines ...\n` : "";
    const block = `${prefix}${lines.slice(range.start, range.end).map(browseLine).join("\n")}`;
    const candidate = `${pieces.join("\n")}${pieces.length ? "\n" : ""}${block}`;
    if (candidate.length > budget) break;
    pieces.push(block);
    previousEnd = range.end;
  }
  const compacted = pieces.join("\n") || text.slice(0, budget).trimEnd();
  return { text: compacted, compacted: true, originalChars: text.length };
}

/**
 * 遍历 Session 全部 Part 节点，对低于阈值的超长 Tool / Result 输出应用 `compactToolTextForBrowsing` 低信号压缩
 * @param {object} session - Session 目标对象
 */
function compactLowSignalToolOutput(session) {
  for (const turn of session.turns) {
    for (const part of turn.parts) {
      if (typeof part.text !== "string" || !part.text) continue;
      if (part.content_card?.renderer === "diff") continue;
      const type = contentCardType(part);
      if (part.role !== "tool" && type !== "result" && type !== "tool") continue;
      const compacted = compactToolTextForBrowsing(part.text, MAX_COMPACTED_TOOL_TEXT_CHARS);
      if (!compacted.compacted) continue;
      part.text = compacted.text;
      markPartCompactedForBrowsing(part, compacted.originalChars, MAX_COMPACTED_TOOL_TEXT_CHARS);
    }
  }
}

/**
 * 【全局文本配额分配与应用函数】：顺序执行低信号压缩、字符总预算计算与Part文本截断应用
 * @param {object} session - Session 目标对象
 * @returns {object} 处理完毕的 Session 对象
 */
function applyTextBudgets(session) {
  compactLowSignalToolOutput(session);
  const budgets = highPriorityTextBudget(session);
  let highPriorityRemaining = budgets.highPriority;
  let standardRemaining = budgets.standard;
  for (const turn of session.turns) {
    for (const part of turn.parts) {
      if (typeof part.text !== "string" || !part.text) continue;
      if (part.content_card?.renderer === "diff") continue;
      const original = part.text;
      const highPriority = isHighPriorityBrowsePart(part);
      const available = highPriority ? highPriorityRemaining : standardRemaining;
      const maxChars = Math.max(0, Math.min(MAX_PART_TEXT_CHARS, available));
      const truncated = truncateText(original, maxChars);
      part.text = truncated.text;
      if (highPriority) {
        highPriorityRemaining = Math.max(0, highPriorityRemaining - part.text.length);
      } else {
        standardRemaining = Math.max(0, standardRemaining - part.text.length);
      }
      if (truncated.truncated || original.length !== part.text.length) {
        markPartTruncated(part, truncated.originalChars, maxChars);
      }
    }
  }
  return normalizeSessionPayload(session);
}

/**
 * 过滤空 Turn 节点并自动修正 turn_index 索引下标
 * @param {Array<object>} turns - 原始 Turn 列表
 * @returns {Array<object>} 修正后的 Turn 列表
 */
function displayTurns(turns) {
  return turns
    .filter((turn) => Array.isArray(turn.parts) && turn.parts.length > 0)
    .map((turn, index) => ({
      ...turn,
      turn_index: index,
    }));
}

/**
 * 创建标准文本 Part 节点的工厂函数
 * @param {string} role - 角色 ("user" | "assistant" | "system" | "tool")
 * @param {string} text - 消息文本
 * @returns {object|null} 构造出的 Part 对象
 */
function textPart(role, text) {
  const trimmed = String(text ?? "").trim();
  if (!trimmed) return null;
  return {
    role,
    kind: "text",
    text: trimmed,
    language: null,
    command: null,
    cwd: null,
    status: null,
    exit_code: null,
    metadata_json: role === "assistant" ? metadata({ type: "answer", format: "markdown" }) : null,
  };
}

/**
 * 【Skill 依赖解析器】：解析用户提问中的 `<skill>...</skill>` 显式 XML 注入块，
 * 以及 Markdown 软链接格式 `[$name](...SKILL.md)`，提取出所关联的 Skill 路径列表。
 *
 * @param {string|null} content - 用户输入文本
 * @returns {{ skills: Array<{name: string, path: string}>, text: string, skillOnly: boolean }} 提取出的 Skill 信息与纯文本
 */
function parseUserMessage(content) {
  const original = String(content ?? "");
  const skills = [];
  const withoutSkillBlocks = original.replace(/<skill>([\s\S]*?)<\/skill>/gi, (_match, body) => {
    const name = xmlTagValue(body, "name");
    const skillPath = xmlTagValue(body, "path");
    if (skillPath) skills.push({ name, path: skillPath });
    return "";
  });
  const withoutInlineReferences = withoutSkillBlocks.replace(
    /\[\$([^\]\r\n]+)\]\(([^)\r\n]*SKILL\.md)\)/gi,
    (_match, name, skillPath) => {
      skills.push({ name: String(name).trim(), path: normalizeSkillPath(skillPath) });
      return "";
    },
  );
  return {
    skills: uniqueSkills(skills),
    text: withoutInlineReferences.replace(/[ \t]+\n/g, "\n").trim(),
    skillOnly: skills.length > 0 && !withoutSkillBlocks.trim(),
  };
}

/**
 * 从 XML 文本段落中提取指定标签名内的文本内容
 * @param {string} value - XML 片段
 * @param {string} tag - 欲匹配的 XML 标签名
 * @returns {string|null} 解码后的属性文本或 null
 */
function xmlTagValue(value, tag) {
  const match = String(value ?? "").match(new RegExp(`<${tag}>([\\s\\S]*?)<\\/${tag}>`, "i"));
  return match ? decodeXmlText(match[1]).trim() || null : null;
}

/**
 * 解码 XML 实体字符 (如 &lt;, &gt;, &quot; 等)
 * @param {string} value - 带有实体的 XML 字符串
 * @returns {string} 解码后的纯文本
 */
function decodeXmlText(value) {
  return String(value ?? "")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&#39;", "'")
    .replaceAll("&amp;", "&");
}

/**
 * 规范化 Skill 绝对/相对路径，去除包含在 `<...>` 尖括号内的路径外壳
 * @param {string|null} value - 原始路径字符串
 * @returns {string} 规范化后的路径字符串
 */
function normalizeSkillPath(value) {
  const trimmed = String(value ?? "").trim();
  if (trimmed.startsWith("<") && trimmed.endsWith(">")) return trimmed.slice(1, -1).trim();
  return trimmed;
}

/**
 * 对 Skill 依赖数组按路径进行去重处理
 * @param {Array<{name: string, path: string}>} skills - 原始 Skill 列表
 * @returns {Array<{name: string, path: string}>} 去重后的 Skill 列表
 */
function uniqueSkills(skills) {
  const seen = new Set();
  return skills.filter((skill) => {
    if (!skill.path || seen.has(skill.path)) return false;
    seen.add(skill.path);
    return true;
  });
}

/**
 * 将解析出的 Skill 关联节点追加为 Turn 的 `role: "system"` Metadata Part，生成 `codex.skill` 卡片
 * @param {object} turn - 目标 Turn 节点
 * @param {Array<{name: string, path: string}>} skills - 所依赖的 Skill 列表
 */
function appendSkillParts(turn, skills) {
  const existingPaths = new Set(
    turn.parts
      .filter((part) => contentCardType(part) === "skill")
      .map((part) => String(part.text ?? "")),
  );
  for (const skill of skills) {
    if (!skill.path || existingPaths.has(skill.path)) continue;
    existingPaths.add(skill.path);
    turn.parts.push({
      role: "system",
      kind: "metadata",
      text: skill.path,
      language: null,
      command: null,
      cwd: null,
      status: null,
      exit_code: null,
      metadata_json: metadata(
        { type: "skill", format: "path" },
        compactObject({ skill_name: skill.name, skill_path: skill.path }),
      ),
    });
  }
}

/**
 * 当在工具执行或命令行交互中自动检测到 SKILL.md 读取时，向 Turn 追加系统级 Skill Card 标记 Part
 * @param {object} turn - 目标 Turn 节点
 * @param {object} skillDocument - 解析出的 Skill YAML 属性对象
 * @param {string} skillPath - 自动提取到的 SKILL.md 路径
 */
function appendDetectedSkill(turn, skillDocument, skillPath) {
  if (!skillPath) return;
  turn.parts.push({
    role: "system",
    kind: "metadata",
    text: skillPath,
    language: null,
    command: null,
    cwd: null,
    status: null,
    exit_code: null,
    metadata_json: metadata(
      { type: "skill", format: "path" },
      compactObject({ skill_name: skillDocument.name, skill_path: skillPath, detected_from_tool: true }),
    ),
  });
}

/**
 * 从执行命令行字符串 (如 `cat /path/to/SKILL.md`) 中，提取包含 `SKILL.md` 的绝对/相对文件路径
 * @param {string|null} command - 命令行指令文本
 * @returns {string|null} 匹配到的 SKILL.md 路径或 null
 */
function skillPathFromCommand(command) {
  const match = String(command ?? "").match(/(?:^|\s)(["']?)([^\s"']*SKILL\.md)\1(?=\s|$)/i);
  return match ? normalizeSkillPath(match[2]) : null;
}

/**
 * 从 Tool 执行输出的各种碎片或文本内容中，尝试寻找并解析 `<skill>...</skill>` 块或 SKILL.md YAML Front-Matter
 * @param {unknown} value - Tool 输出文本或 JSON 数据
 * @returns {{ name: string, text: string, path: string|null }|null} 解析成功则返回文档对象，否则返回 null
 */
function skillDocumentFromOutput(value) {
  for (const fragment of toolOutputTextFragments(value)) {
    const skillBlock = fragment.match(/<skill>\s*([\s\S]*?)<\/skill>/i);
    const skillPath = skillBlock ? xmlTagValue(skillBlock[1], "path") : null;
    const body = skillBlock
      ? skillBlock[1].replace(/^[\s\S]*?<\/path>\s*/i, "")
      : fragment;
    const document = skillDocumentFromText(body);
    if (document) return { ...document, path: skillPath };
  }
  return null;
}

/**
 * 递归深度拆解嵌套对象或数组，收集其中可能包含 Skill 定义文本的字符串片段
 * @param {unknown} value - 输入值
 * @param {Array<string>} [fragments=[]] - 累积的字符串片段数组
 * @param {number} [depth=0] - 递归深度
 * @returns {Array<string>} 提取到的文本片段列表
 */
function toolOutputTextFragments(value, fragments = [], depth = 0) {
  if (depth > 5 || value == null) return fragments;
  if (typeof value === "string") {
    fragments.push(value);
    const parsed = parseJsonValue(value);
    if (parsed) toolOutputTextFragments(parsed, fragments, depth + 1);
    return fragments;
  }
  if (Array.isArray(value)) {
    value.forEach((entry) => toolOutputTextFragments(entry, fragments, depth + 1));
    return fragments;
  }
  if (typeof value === "object") {
    for (const key of ["text", "content", "output", "result"]) {
      if (value[key] != null) toolOutputTextFragments(value[key], fragments, depth + 1);
    }
  }
  return fragments;
}

/**
 * 从 Skill MD 文本中解析 `---` 包裹的 YAML Front-Matter 头信息，提取 `name` 与 `description`
 * @param {string} value - MD 文本
 * @returns {{ name: string, text: string }|null} 解析出的属性对象或 null
 */
function skillDocumentFromText(value) {
  const text = String(value ?? "");
  const start = text.search(/(?:^|\r?\n)---\r?\n/);
  if (start < 0) return null;
  const document = text.slice(start + (text[start] === "\n" ? 1 : 0)).trim();
  const frontMatter = document.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
  if (!frontMatter) return null;
  const name = frontMatter[1].match(/^name:\s*(.+?)\s*$/m)?.[1]?.trim() || null;
  const description = frontMatter[1].match(/^description:\s*(.+?)\s*$/m)?.[1]?.trim() || null;
  if (!name || !description) return null;
  return { name, text: document };
}

/**
 * 直接从对象的指定候选字段列表中获取非空字符串
 * @param {object} value - 目标对象
 * @param {Array<string>} names - 字段候选名数组
 * @returns {string|null} 匹配到的字符串或 null
 */
function directStringField(value, names) {
  if (!value || typeof value !== "object") return null;
  for (const name of names) {
    const text = valueAsString(value[name]);
    if (text?.trim()) return text;
  }
  return null;
}

/**
 * 递归多层级搜索嵌套 JSON 对象或代码中的字符串属性字段 (如 `command`, `cmd`, `shell_command`)
 * @param {unknown} value - 输入的原始 Payload 或文本
 * @param {Array<string>} names - 候选字段名列表
 * @param {number} [depth=0] - 递归搜索深度
 * @returns {string|null} 搜索到的字符串或 null
 */
function nestedStringField(value, names, depth = 0) {
  if (depth > 6 || !value) return null;
  if (typeof value === "string") {
    const parsed = parseJsonValue(value);
    if (parsed) {
      const nested = nestedStringField(parsed, names, depth + 1);
      if (nested) return nested;
    }
    return javascriptStringField(value, names);
  }
  if (typeof value !== "object") return null;
  const direct = directStringField(value, names);
  if (direct) return direct;
  for (const key of ["arguments", "args"]) {
    const child = value[key];
    if (child == null) continue;
    const parsed = parseJsonValue(child);
    const nested = nestedStringField(parsed ?? child, names, depth + 1);
    if (nested) return nested;
  }
  for (const key of ["action", "input", "tool_input", "toolInput", "state", "request", "params", "parameters"]) {
    const nested = nestedStringField(value[key], names, depth + 1);
    if (nested) return nested;
  }
  return null;
}

/**
 * 递归搜索嵌套 JSON 对象中的整型数值字段 (如 `exit_code`, `code`, `status`)
 * @param {unknown} value - 输入的原始 Payload 或文本
 * @param {Array<string>} names - 候选字段名列表
 * @param {number} [depth=0] - 递归深度
 * @returns {number|null} 搜索到的整数或 null
 */
function nestedIntegerField(value, names, depth = 0) {
  if (depth > 6 || !value || typeof value !== "object") return null;
  for (const name of names) {
    const child = value[name];
    if (Number.isInteger(child)) return child;
    if (typeof child === "string" && /^-?\d+$/.test(child.trim())) return Number(child);
  }
  for (const key of ["arguments", "args"]) {
    const child = value[key];
    if (child == null) continue;
    const parsed = parseJsonValue(child);
    const nested = nestedIntegerField(parsed ?? child, names, depth + 1);
    if (nested != null) return nested;
  }
  for (const key of ["action", "input", "tool_input", "toolInput", "state", "request", "params", "parameters"]) {
    const nested = nestedIntegerField(value[key], names, depth + 1);
    if (nested != null) return nested;
  }
  return null;
}

/** 从 Payload 提取命令行指令字符串 */
function commandFromPayload(payload) {
  return nestedStringField(payload, ["command", "cmd", "shell_command"]);
}

function normalizedDiffPath(filePath, projectPath) {
  const source = String(filePath ?? "").trim();
  if (!source) return null;
  if (path.isAbsolute(source) && projectPath) {
    const relative = path.relative(projectPath, source).replaceAll("\\", "/");
    if (relative && relative !== ".." && !relative.startsWith("../")) return relative;
  }
  return source.replace(/^[./\\]+/, "").replaceAll("\\", "/") || null;
}

function contentLines(value) {
  const text = String(value ?? "").replace(/\r\n?/g, "\n");
  const lines = text.split("\n");
  if (lines.at(-1) === "") lines.pop();
  return lines;
}

function addedFileDiff(filePath, content) {
  const lines = contentLines(content);
  return [
    `diff --git a/${filePath} b/${filePath}`,
    "new file mode 100644",
    "--- /dev/null",
    `+++ b/${filePath}`,
    `@@ -0,0 +1,${lines.length} @@`,
    ...lines.map((line) => `+${line}`),
  ].join("\n");
}

function deletedFileDiff(filePath, content) {
  const lines = contentLines(content);
  return [
    `diff --git a/${filePath} b/${filePath}`,
    "deleted file mode 100644",
    `--- a/${filePath}`,
    "+++ /dev/null",
    `@@ -1,${lines.length} +0,0 @@`,
    ...lines.map((line) => `-${line}`),
  ].join("\n");
}

function updatedFileDiff(oldPath, newPath, unifiedDiff) {
  const moved = oldPath !== newPath;
  const patchText = String(unifiedDiff ?? "").replace(/\r\n?/g, "\n").trim();
  if (patchText.startsWith("diff --git ")) return patchText;
  return [
    `diff --git a/${oldPath} b/${newPath}`,
    ...(moved ? [`rename from ${oldPath}`, `rename to ${newPath}`] : []),
    `--- a/${oldPath}`,
    `+++ b/${newPath}`,
    patchText,
  ].filter(Boolean).join("\n");
}

function canonicalPatchChanges(changes, projectPath) {
  if (!changes || typeof changes !== "object" || Array.isArray(changes)) return null;
  const diffs = [];
  const files = [];
  for (const [sourcePath, change] of Object.entries(changes)) {
    if (!change || typeof change !== "object" || Array.isArray(change)) continue;
    const oldPath = normalizedDiffPath(sourcePath, projectPath);
    const newPath = normalizedDiffPath(change.move_path ?? change.movePath ?? sourcePath, projectPath);
    if (!oldPath || !newPath) continue;
    const changeType = String(change.type ?? "update").toLowerCase();
    if (changeType === "add" || changeType === "added" || changeType === "create") {
      diffs.push(addedFileDiff(newPath, change.content));
    } else if (changeType === "delete" || changeType === "deleted" || changeType === "remove") {
      diffs.push(deletedFileDiff(oldPath, change.content));
    } else {
      const unifiedDiff = change.unified_diff ?? change.unifiedDiff ?? change.patch;
      if (typeof unifiedDiff !== "string" || !unifiedDiff.trim()) continue;
      diffs.push(updatedFileDiff(oldPath, newPath, unifiedDiff));
    }
    files.push(newPath);
  }
  return diffs.length ? { diff: diffs.join("\n"), files } : null;
}

function normalizedLegacyPatchHunks(lines) {
  const result = [];
  let index = 0;
  while (index < lines.length) {
    const line = lines[index];
    if (!line.startsWith("@@")) {
      index += 1;
      continue;
    }
    const hunkLines = [];
    index += 1;
    while (index < lines.length && !lines[index].startsWith("@@")) {
      hunkLines.push(lines[index]);
      index += 1;
    }
    const oldLines = hunkLines.filter((entry) => !entry.startsWith("+")).length;
    const newLines = hunkLines.filter((entry) => !entry.startsWith("-")).length;
    result.push(`@@ -1,${oldLines} +1,${newLines} @@`, ...hunkLines);
  }
  return result.join("\n");
}

function canonicalLegacyApplyPatch(value, projectPath) {
  const lines = String(value ?? "").replace(/\r\n?/g, "\n").split("\n");
  const diffs = [];
  const files = [];
  for (let index = 0; index < lines.length; index += 1) {
    const header = lines[index].match(/^\*\*\* (Add|Delete|Update) File:\s*(.+?)\s*$/);
    if (!header) continue;
    const operation = header[1].toLowerCase();
    const oldPath = normalizedDiffPath(header[2], projectPath);
    let newPath = oldPath;
    const body = [];
    for (index += 1; index < lines.length; index += 1) {
      const move = lines[index].match(/^\*\*\* Move to:\s*(.+?)\s*$/);
      if (move) {
        newPath = normalizedDiffPath(move[1], projectPath);
        continue;
      }
      if (/^\*\*\* (?:Add|Delete|Update) File:/.test(lines[index]) || lines[index] === "*** End Patch") {
        index -= 1;
        break;
      }
      body.push(lines[index]);
    }
    if (!oldPath || !newPath) continue;
    if (operation === "add") {
      diffs.push(addedFileDiff(newPath, body.map((line) => line.startsWith("+") ? line.slice(1) : line).join("\n")));
    } else if (operation === "delete") {
      diffs.push(deletedFileDiff(oldPath, body.map((line) => line.startsWith("-") ? line.slice(1) : line).join("\n")));
    } else {
      const patchText = body.some((line) => /^@@ -\d/.test(line))
        ? body.join("\n")
        : normalizedLegacyPatchHunks(body);
      if (!patchText) continue;
      diffs.push(updatedFileDiff(oldPath, newPath, patchText));
    }
    files.push(newPath);
  }
  return diffs.length ? { diff: diffs.join("\n"), files } : null;
}

function patchExecutionsFromRollout(text) {
  let projectPath = null;
  const patchInputs = new Map();
  const records = new Map();
  const nestedPatchCallIds = new Set();
  const openNestedPatchCalls = [];
  const nestedPatchCallsByTurn = new Map();
  const nestedPatchTurnIds = new Map();
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let parsed;
    try {
      parsed = JSON.parse(line);
    } catch {
      continue;
    }
    const payload = parsed.payload ?? parsed;
    if (!projectPath && parsed.type === "session_meta" && typeof payload?.cwd === "string") {
      projectPath = payload.cwd;
    }
    const callId = sourceExecutionId(payload);
    const toolName = String(toolNameFromPayload(payload) ?? "").toLowerCase();
    if (callId && toolName === "apply_patch" && typeof payload.input === "string") {
      patchInputs.set(callId, payload.input);
    }
    if (
      callId
      && toolName === "exec"
      && typeof payload.input === "string"
      && /\btools\s*\.\s*apply_patch\s*\(/.test(payload.input)
    ) {
      nestedPatchCallIds.add(callId);
      openNestedPatchCalls.push(callId);
      const turnId = payload.turn_id ?? payload.internal_chat_message_metadata_passthrough?.turn_id;
      if (typeof turnId === "string" && turnId.trim()) {
        const normalizedTurnId = turnId.trim();
        const turnCalls = nestedPatchCallsByTurn.get(normalizedTurnId) ?? [];
        turnCalls.push(callId);
        nestedPatchCallsByTurn.set(normalizedTurnId, turnCalls);
        nestedPatchTurnIds.set(callId, normalizedTurnId);
      }
    }
    if (callId && payload?.type === "patch_apply_end" && payload.success !== false) {
      const record = canonicalPatchChanges(payload.changes, projectPath);
      if (record) {
        const turnId = typeof payload.turn_id === "string" ? payload.turn_id.trim() : "";
        const turnCallId = turnId ? nestedPatchCallsByTurn.get(turnId)?.at(-1) : null;
        const targetCallId = patchInputs.has(callId) ? callId : turnCallId ?? openNestedPatchCalls.at(-1) ?? callId;
        const existing = records.get(targetCallId);
        records.set(targetCallId, {
          diff: existing ? `${existing.diff}\n${record.diff}` : record.diff,
          files: [...new Set([...(existing?.files ?? []), ...record.files])],
          status: payload.status ?? "completed",
          exitCode: 0,
        });
      }
    }
    if (callId && isToolResultPayload(payload) && nestedPatchCallIds.has(callId)) {
      const index = openNestedPatchCalls.lastIndexOf(callId);
      if (index >= 0) openNestedPatchCalls.splice(index, 1);
      const turnId = nestedPatchTurnIds.get(callId);
      const turnCalls = turnId ? nestedPatchCallsByTurn.get(turnId) : null;
      const turnIndex = turnCalls?.lastIndexOf(callId) ?? -1;
      if (turnCalls && turnIndex >= 0) turnCalls.splice(turnIndex, 1);
      if (turnId && turnCalls?.length === 0) nestedPatchCallsByTurn.delete(turnId);
    }
  }
  for (const [callId, patchInput] of patchInputs) {
    if (records.has(callId)) continue;
    const record = canonicalLegacyApplyPatch(patchInput, projectPath);
    if (record) records.set(callId, { ...record, status: "completed", exitCode: 0 });
  }
  return { projectPath, records, nestedPatchCallIds };
}

/** 从 Payload 提取工作目录 (CWD) 绝对路径 */
function cwdFromPayload(payload) {
  return nestedStringField(payload, ["cwd", "workdir", "working_directory", "workingDirectory"]);
}

/** 从 Payload 提取执行状态 (status / state) */
function statusFromPayload(payload) {
  return nestedStringField(payload, ["status", "state"]);
}

/** 从 Payload 提取退出代码 (exit_code) */
function exitCodeFromPayload(payload) {
  return nestedIntegerField(payload, ["exit_code", "exitCode", "code"]);
}

/** 从 Payload 提取工具输出文本 */
function outputTextFromPayload(payload) {
  return (
    contentText(payload?.content) ||
    valueAsDisplayString(payload?.output) ||
    valueAsDisplayString(payload?.result) ||
    ""
  );
}

/** 从 Payload 提取工具名称 (tool / name) */
function toolNameFromPayload(payload) {
  return directStringField(payload, ["name", "tool_name", "toolName", "tool"]);
}

/** 判断 Payload 是否代表 Tool 的返回结果 (Result) */
function isToolResultPayload(payload) {
  const type = String(payload?.type ?? "").toLowerCase();
  return (
    type.includes("output") ||
    type.includes("result") ||
    payload?.output != null ||
    payload?.result != null
  );
}

/** 获取 Tool 的格式化展示名称与文本 */
function toolDisplayText(payload, content) {
  const text = String(content ?? "").trim();
  if (text) return text;
  const toolName = toolNameFromPayload(payload);
  const type = String(payload?.type ?? "tool").trim() || "tool";
  if (toolName?.trim()) return `${type}: ${toolName.trim()}`;
  return "";
}

/** 判断是否为低信号控制性质的工具调用 (如 `wait` 心跳检测) */
function isLowSignalControlTool(payload) {
  const type = String(payload?.type ?? "").toLowerCase();
  const name = String(toolNameFromPayload(payload) ?? "").trim().toLowerCase();
  return type === "function_call" && name === "wait";
}

/** 判断是否为 update_plan 任务列表工具调用 */
function isPlanTool(payload) {
  const name = String(toolNameFromPayload(payload) ?? payload?.name ?? "").toLowerCase();
  const type = String(payload?.type ?? "").toLowerCase();
  return name === "update_plan" || type === "update_plan";
}

/** 解析 update_plan 的参数对象 */
function parsePlanArguments(payload) {
  const raw = payload?.arguments ?? payload?.input ?? payload?.params;
  if (raw && typeof raw === "object" && !Array.isArray(raw)) {
    return raw;
  }
  if (typeof raw === "string") {
    const trimmed = raw.trim();
    const parsed = parseJsonValue(trimmed);
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      return parsed;
    }
    const match = trimmed.match(/update_plan\s*\(\s*(\{[\s\S]*\})\s*\)/);
    if (match) {
      const codeJson = parseJsonValue(match[1]);
      if (codeJson && typeof codeJson === "object") return codeJson;
    }
  }
  if (Array.isArray(payload?.plan)) {
    return { explanation: payload.explanation, plan: payload.plan };
  }
  return null;
}

/** 将任务列表与阶段说明格式化为精美的 Markdown 文本 */
function formatPlanMarkdown(data) {
  if (!data || typeof data !== "object") return "";
  const explanation = typeof data.explanation === "string" ? data.explanation.trim() : "";
  const plan = Array.isArray(data.plan) ? data.plan : [];

  const sections = [];
  if (explanation) {
    sections.push(`> 📌 **阶段目标**：${explanation}`);
  }

  if (plan.length > 0) {
    const items = plan.map((item) => {
      const step = typeof item === "string" ? item : String(item?.step ?? item?.title ?? item?.name ?? "").trim();
      const status = typeof item === "object" ? String(item?.status ?? "").toLowerCase() : "";
      if (status === "completed" || status === "complete" || status === "done") {
        return `- [x] ✅ ${step}`;
      } else if (status === "in_progress" || status === "running" || status === "active") {
        return `- [ ] 🔄 **${step}** *(进行中)*`;
      } else {
        return `- [ ] ⏳ ${step}`;
      }
    });
    sections.push(items.join("\n"));
  } else if (!explanation) {
    sections.push("*(任务列表已初始化)*");
  }

  return sections.join("\n\n").trim();
}

/** 判断 JSON Payload 事件类型是否属于工具执行或 Tool 调用的相关事件 */
function isToolEvent(payload) {
  const type = String(payload?.type ?? "");
  return (
    type.includes("tool") ||
    type.includes("function") ||
    type.includes("exec") ||
    type.includes("shell") ||
    type === "patch" ||
    payload?.tool_use_id != null ||
    payload?.toolUseID != null ||
    payload?.call_id != null ||
    payload?.callID != null ||
    payload?.tool_name != null ||
    payload?.toolName != null
  );
}

/** 从 Turn 列表中扫描并推断项目根目录 CWD */
function inferProjectPath(turns) {
  for (const turn of turns) {
    for (const part of turn.parts) {
      if (part.cwd?.trim()) return part.cwd;
    }
  }
  return null;
}

/**
 * 【JSONL 会话日志解析核心引擎】：
 * 1. 逐行读取与解析 Codex `rollout.jsonl` 中的日志条目；
 * 2. 识别并匹配 `user` 提问、完整的 `assistant` Markdown 回复以及 `tool` / `exec` 指令执行事件；
 * 3. 解析 Skill 注入依赖、对齐异步 Tool 执行结果并提取项目工作目录 CWD；
 * 4. 组装并返回标准化 Turn 列表以及推导出的项目路径。
 *
 * @param {string} text - JSONL 日志文件的完整文本内容
 * @returns {{ turns: Array<object>, projectPath: string|null }} 解析后的 Turn 数组与项目路径
 */
function normalizeTurns(text) {
  const turns = [];
  let current = null;
  const patchExecutions = patchExecutionsFromRollout(text);
  let projectPath = patchExecutions.projectPath;
  let pendingSkillRead = null;
  const planCallIds = new Set();
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    let parsed;
    try {
      parsed = JSON.parse(line);
    } catch {
      continue;
    }
    const payload = parsed.payload ?? parsed;
    projectPath ??= cwdFromPayload(parsed) ?? cwdFromPayload(payload);
    const role = payload.role;
    const type = payload.type;
    if (type === "message" && role === "user") {
      const userText = contentText(payload.content);
      const userMessage = parseUserMessage(userText);
      if (userMessage.skillOnly) {
        if (current) appendSkillParts(current, userMessage.skills);
        continue;
      }
      if (current) turns.push(current);
      if (!userMessage.text) {
        current = null;
        continue;
      }
      current = {
        external_id: payload.id ?? `turn-${turns.length + 1}`,
        turn_index: turns.length,
        user_text: userMessage.text,
        title: null,
        started_at: parsed.timestamp ?? payload.timestamp ?? null,
        ended_at: null,
        parts: [],
      };
      appendSkillParts(current, userMessage.skills);
    } else if (current && type === "message" && role === "assistant") {
      const text = contentText(payload.content);
      if (text.trim()) {
        const answer = textPart("assistant", text);
        if (answer) current.parts.push(answer);
      }
      current.ended_at = parsed.timestamp ?? payload.timestamp ?? current.ended_at;
    } else if (current && isToolEvent(payload)) {
      const sourceId = sourceExecutionId(payload);
      const patchExecution = sourceId ? patchExecutions.records.get(sourceId) : null;
      const patchTool = String(toolNameFromPayload(payload) ?? "").toLowerCase() === "apply_patch";
      const nestedPatchTool = sourceId
        && String(toolNameFromPayload(payload) ?? "").toLowerCase() === "exec"
        && patchExecutions.nestedPatchCallIds.has(sourceId);
      const patchInvocation = patchExecution && !isToolResultPayload(payload) && (patchTool || nestedPatchTool);
      const command = patchInvocation
        ? patchExecution.files.join("\n")
        : commandFromPayload(payload);
      const cwd = patchExecution ? projectPath : cwdFromPayload(payload);
      const text = patchExecution && isToolResultPayload(payload)
        ? patchExecution.diff
        : outputTextFromPayload(payload);
      const status = statusFromPayload(payload) ?? (patchExecution && isToolResultPayload(payload) ? patchExecution.status : null);
      const exitCode = exitCodeFromPayload(payload) ?? (patchExecution && isToolResultPayload(payload) ? patchExecution.exitCode : null);
      const executionId = sourceId ?? (
        command?.trim() && text.trim()
          ? `inline-${createHash("sha256").update(JSON.stringify(payload)).digest("hex").slice(0, 24)}`
          : null
      );
      const readSkillPath = command ? skillPathFromCommand(command) : pendingSkillRead?.path ?? null;
      const skillDocument = skillDocumentFromOutput(text);
      const skillPath = readSkillPath ?? skillDocument?.path ?? null;
      const skillRead = Boolean(skillDocument && skillPath);
      if (command?.trim()) {
        current.parts.push({
          role: "tool",
          kind: "command",
          text: null,
          language: null,
          command,
          cwd,
          status: null,
          exit_code: null,
          source_execution_id: executionId,
          command_label: patchInvocation ? "Edit" : toolNameFromPayload(payload),
          metadata_json: metadata(
            compactObject({ type: "command", cwd }),
            compactObject({ ...smallMetadata(payload), execution_kind: patchInvocation ? "file_change" : undefined }),
          ),
        });
      }
      if (command?.trim() && skillRead) {
        appendDetectedSkill(current, skillDocument, skillPath);
        pendingSkillRead = null;
      } else if (command?.trim() && text.trim()) {
        current.parts.push({
          role: "tool",
          kind: "tool",
          text: text.trim(),
          language: null,
          command: null,
          cwd: null,
          status,
          exit_code: exitCode,
          source_execution_id: executionId,
          metadata_json: metadata(
            compactObject({ type: "result", format: "plain", status, exit_code: exitCode }),
            smallMetadata(payload),
          ),
        });
      }
      if (!command?.trim()) {
        const displayText = toolDisplayText(payload, text);
        const planCall = isPlanTool(payload) && !isToolResultPayload(payload);
        const planData = planCall ? parsePlanArguments(payload) : null;
        const planMarkdown = planCall ? formatPlanMarkdown(planData) : "";
        if (planCall && executionId) planCallIds.add(executionId);
        if (skillRead) {
          appendDetectedSkill(current, skillDocument, skillPath);
          pendingSkillRead = null;
        } else if (planCall && planMarkdown) {
          current.parts.push({
            role: "tool",
            kind: "tool",
            text: planMarkdown,
            language: null,
            command: null,
            cwd: null,
            status: "completed",
            exit_code: 0,
            source_execution_id: executionId,
            metadata_json: metadata(
              compactObject({
                type: "plan",
                format: "markdown",
                status: "completed",
                exit_code: 0,
              }),
              compactObject({
                ...smallMetadata(payload),
                execution_kind: "plan",
              }),
            ),
          });
        } else if (isLowSignalControlTool(payload)) {
          // 维持隐形的占位 Part，确保重同步时后方 Part 的唯一 ID 保持稳定
          current.parts.push({
            role: "tool",
            kind: "tool",
            text: null,
            language: null,
            command: null,
            cwd: null,
            status: null,
            exit_code: null,
            source_execution_id: null,
            metadata_json: null,
          });
        } else if (displayText) {
          const result = isToolResultPayload(payload);
          current.parts.push({
            role: "tool",
            kind: "tool",
            text: displayText,
            language: null,
            command: null,
            cwd: null,
            status: result ? status : null,
            exit_code: result ? exitCode : null,
            source_execution_id: result ? executionId : null,
            metadata_json: metadata(
              compactObject({
                type: result ? "result" : "tool",
                format: result ? "plain" : undefined,
                status: result ? status : null,
                exit_code: result ? exitCode : null,
              }),
              compactObject({
                ...smallMetadata(payload),
                execution_kind: (isPlanTool(payload) && isToolResultPayload(payload)) ? "plan" : undefined,
              }),
            ),
          });
        }
      }
      if (command?.trim() && !skillRead) {
        pendingSkillRead = readSkillPath ? { path: readSkillPath } : null;
      } else if (!command?.trim() && !skillRead && isToolResultPayload(payload)) {
        pendingSkillRead = null;
      }
      current.ended_at = parsed.timestamp ?? payload.timestamp ?? current.ended_at;
    }
  }
  if (current) turns.push(current);
  return { turns, projectPath };
}

/**
 * 查询 Codex 的 SQLite 数据库 (`state_5.sqlite`) 表中的 `threads` 基础会话元数据列表
 * @returns {Array<object>} 查到的 Session 数据库元数据行对象列表
 */
function sessionRows() {
  let location = expandPath(input.source?.location);
  if (!location) return [];
  let dbPath = location;
  try {
    if (existsSync(location) && statSync(location).isDirectory()) {
      dbPath = path.join(location, "state_5.sqlite");
    }
  } catch {
    return [];
  }
  if (!existsSync(dbPath)) return [];
  const columns = sqliteJson(dbPath, "PRAGMA table_info(threads)").map((row) => row.name);
  const idCol = pick(columns, ["id", "thread_id", "session_id"]);
  const rolloutCol = pick(columns, ["rollout_path", "path", "file_path", "jsonl_path"]);
  if (!idCol || !rolloutCol) return [];
  const titleCol = pick(columns, ["title", "name"]);
  const updatedCol = pick(columns, ["updated_at", "last_updated_at", "mtime", "created_at"]);
  const sql = `SELECT ${quoteIdent(idCol)} AS id, ${quoteIdent(rolloutCol)} AS rollout_path, ${titleCol ? quoteIdent(titleCol) : "NULL"} AS title, ${updatedCol ? quoteIdent(updatedCol) : "NULL"} AS updated_at FROM threads ORDER BY rowid DESC`;
  return sqliteJson(dbPath, sql).map((row) => ({ ...row, rollout_path: expandPath(row.rollout_path) }));
}

/**
 * 【会话列举入口】：供 Rust 侧 `list_sessions`IPC 方法调用，快速提取轻量会话描述符列表
 * @returns {Array<object>} 包含 external_id, updated_at, source_locator, version_token 的描述符列表
 */
function listSessions() {
  return sessionRows().flatMap((row) => {
    if (!row.rollout_path || !existsSync(row.rollout_path)) return [];
    return [{
      external_id: String(row.id),
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: row.rollout_path,
      version_token: fileVersionToken(row.rollout_path, row.updated_at),
    }];
  });
}

/**
 * 【会话读取入口 & 核心格式化流水线】：
 * 供 Rust 侧 `read_session` IPC 方法调用。
 * 负责从 SQLite 读取会话元数据，定位对应 JSONL 文件，同步读取日志内容，
 * 依次触发：JSONL 逐行拆解 -> 消息/指令分发 -> 结构化卡片生成 -> 文本压缩与配额限制，
 * 最终输出符合规范的标准 Session 对象数组。
 *
 * @returns {Array<object>} 解析并完成卡片映射与配额限制后的 Session 对象数组
 */
function readSession() {
  // 1. 获取 Rust 入参中指定的 session_id (若为 null，则代表需要读取全量会话)
  const requestedSessionId = input.params?.session_id ?? null;

  // 2. 查询 SQLite 获取数据库会话行，按请求 ID 过滤后，对每个 Session 执行格式化流水线
  return sessionRows().filter((row) => !requestedSessionId || String(row.id) === String(requestedSessionId)).flatMap((row) => {
    // 3. 展开并校验 `.jsonl` 会话日志文件的绝对路径，不存在则忽略
    const rolloutPath = expandPath(row.rollout_path);
    if (!rolloutPath || !existsSync(rolloutPath)) return [];

    // 4. 【步骤一：原子稳定读取】同步读取 JSONL 日志文本，生成防止并发追加写入的校验 Version Token
    const { text, versionToken } = readStableFile(rolloutPath, row.updated_at);

    // 5. 【步骤二：核心解析引擎】将 JSONL 逐行拆解为 User 提问、Assistant Markdown、Tool 命令与 Skill 依赖
    const parsed = normalizeTurns(text);

    // 6. 过滤空 Turn 节点并自动重新建立 0-based 连续索引下标
    const turns = displayTurns(parsed.turns);
    if (!turns.length) return [];

    // 7. 组装 Session 基础元数据结构对象
    const rawSession = {
      external_id: String(row.id),
      title: row.title == null ? null : String(row.title),
      project_path: parsed.projectPath ?? inferProjectPath(turns),
      started_at: turns[0]?.started_at ?? null,
      updated_at: row.updated_at == null ? null : String(row.updated_at),
      source_locator: rolloutPath,
      source_fingerprint: versionToken,
      turns,
    };

    // 8. 【步骤三：卡片终结映射】为 Parts 节点映射结构化 Content Card (如 renderer="code" / "command" / "path")
    const cardSession = finalizeStructuredContentCards(rawSession);

    // 9. 【步骤四：文本压缩与配额】执行低信号终端日志压缩算法，并应用 384KB Session 字符总上限
    const finalizedSession = applyTextBudgets(cardSession);

    // 10. 返回最终处理完成的 Session 数组元素
    return [finalizedSession];
  });
}

/**
 * 深度解包并提取 Tool 执行输出文本内容，处理 JSON 转义或嵌套数组结构
 * @param {unknown} value - 输入的原始输出数据
 * @param {number} [depth=0] - 递归深度
 * @returns {string} 提取到的纯文本
 */
function executionTextValue(value, depth = 0) {
  if (value == null || depth > 6) return "";
  if (typeof value === "string") {
    const source = value.trim();
    if (/^[\[{\"]/.test(source)) {
      try {
        return executionTextValue(JSON.parse(source), depth + 1);
      } catch {
        // 普通终端行可能以括号开头，异常时保留原始文本
      }
    }
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (Array.isArray(value)) {
    const structuredFragments = value.length > 0 && value.every((entry) => (
      entry && typeof entry === "object" && !Array.isArray(entry)
      && ["stdout", "stderr", "output", "result", "content", "text", "message"].some((key) => key in entry)
    ));
    if (!structuredFragments) return JSON.stringify(value, null, 2);
    return value.map((entry) => executionTextValue(entry, depth + 1)).filter(Boolean).reduce(
      (text, fragment) => !text || text.endsWith("\n") || fragment.startsWith("\n")
        ? text + fragment
        : `${text}\n${fragment}`,
      "",
    );
  }
  if (typeof value === "object") {
    const streams = [value.stdout, value.stderr]
      .map((entry) => executionTextValue(entry, depth + 1))
      .filter((entry) => entry.trim());
    if (streams.length) return streams.join("\n");
    for (const key of ["output", "result", "content", "text", "message"]) {
      if (value[key] != null) return executionTextValue(value[key], depth + 1);
    }
    return JSON.stringify(value, null, 2);
  }
  return String(value);
}

/**
 * 清理 ANSI 颜色控制字符、\r 回车符以及零宽不可见字符
 * @param {unknown} value - 输入终端文本
 * @returns {string} 清洗后的文本
 */
function normalizeTerminalText(value) {
  return String(value ?? "")
    .replace(/\r\n/g, "\n")
    .replace(/\u001B\][^\u0007]*(?:\u0007|\u001B\\)/g, "")
    .replace(/\u001B\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/[\u0000\u0008\u000B\u000C\u000E-\u001F\u007F]/g, "")
    .split("\n")
    .map((line) => (line.includes("\r") ? line.slice(line.lastIndexOf("\r") + 1) : line).trimEnd())
    .join("\n")
    .trim()
    .replace(/\n{3,}/g, "\n\n");
}

/**
 * 剥离包装在终端输出外层的日志元数据 Header ("Created At: ...", "Wall time: ...")
 * @param {unknown} value - 带有 Header 外壳的原始终端文本
 * @returns {string} 剥离 Header 后的纯净输出
 */
function stripExecutionEnvelope(value) {
  const lines = String(value ?? "").split("\n");
  const header = /^(?:Created At:.*|Completed At:.*|Script completed(?: successfully)?(?: in .*)?|Script running with cell ID\b.*|Wall time\b.*|Chunk ID:.*|Process exited with code\b.*|Original token count:.*|The command completed successfully\.?|(?:Final |Original |Command )?Output:)$/i;
  let cursor = 0;
  let matched = false;
  while (cursor < lines.length) {
    const line = lines[cursor].trim();
    if (header.test(line)) {
      matched = true;
      cursor += 1;
    } else if (matched && !line) {
      cursor += 1;
    } else {
      break;
    }
  }
  return matched ? normalizeTerminalText(lines.slice(cursor).join("\n")) : value;
}

/**
 * 格式化与规范化执行结果文本
 * @param {unknown} value - 原始结果文本
 * @returns {string} 格式化后的文本
 */
function normalizeExecutionResultText(value) {
  const withoutEnvelope = stripExecutionEnvelope(normalizeTerminalText(executionTextValue(value)));
  return normalizeTerminalText(executionTextValue(withoutEnvelope));
}

/**
 * 格式化与规范化命令行指令文本
 * @param {unknown} value - 原始指令文本
 * @returns {string} 规范化后的指令字符串
 */
function normalizeExecutionCommandText(value) {
  const source = String(value ?? "").trim();
  if (/^[{\"]/.test(source)) {
    try {
      const parsed = JSON.parse(source);
      if (typeof parsed === "string") return normalizeTerminalText(parsed);
      if (parsed && typeof parsed === "object") {
        const command = parsed.command ?? parsed.cmd ?? parsed.shell_command;
        if (typeof command === "string") return normalizeTerminalText(command);
      }
    } catch {
      // 保留类似 JSON 的命令语法
    }
  }
  return normalizeTerminalText(value);
}

/**
 * 提取结构化卡片类型名称
 * @param {object} part - Part 节点
 * @param {object} legacyCard - 兼容的旧版卡片对象
 * @returns {string|null} 卡片类型名称或 null
 */
function structuredCardType(part, legacyCard) {
  if (typeof legacyCard?.type === "string") return legacyCard.type;
  const kind = part?.content_card?.kind;
  return typeof kind === "string" ? kind.slice(kind.lastIndexOf(".") + 1) : null;
}

/**
 * 【结构化卡片终结映射】：
 * 1. 遍历 Session 全部 Part 节点，清洗 result 与 command 中的控制符与冗余 Header；
 * 2. 补全 `content_card: { schema_version: 1, kind: "codex.xxx", renderer: "..." }` 卡片属性；
 * 3. 最终通过 `normalizeSessionPayload` 策略清洗并导出。
 *
 * @param {object} session - 输入的 Session
 * @returns {object} 契约归一化后的 Session
 */
function finalizeStructuredContentCards(session) {
  for (const turn of Array.isArray(session?.turns) ? session.turns : []) {
    for (const part of Array.isArray(turn?.parts) ? turn.parts : []) {
      const metadataValue = parseStructuredMetadata(part.metadata_json);
      const legacyCard = metadataValue.content_card ?? metadataValue.contentCard;
      const cardType = structuredCardType(part, legacyCard);
      if (cardType === "result" && typeof part.text === "string") {
        part.text = normalizeExecutionResultText(part.text) || null;
      } else if (cardType === "command") {
        const field = typeof part.command === "string" ? "command" : typeof part.text === "string" ? "text" : null;
        if (field) part[field] = normalizeExecutionCommandText(part[field]) || null;
      }
      if (!part.content_card && legacyCard && typeof legacyCard === "object" && typeof legacyCard.type === "string") {
        part.content_card = {
          schema_version: 1,
          kind: `codex.${legacyCard.type}`,
          renderer: structuredCardRenderer(legacyCard),
        };
      }
      delete metadataValue.content_card;
      delete metadataValue.contentCard;
      part.metadata_json = Object.keys(metadataValue).length > 0 ? JSON.stringify(metadataValue) : null;
    }
  }
  return normalizeSessionPayload(session);
}

/**
 * 解析元数据 JSON 对象
 * @param {unknown} value - JSON 字符串或对象
 * @returns {object} 解析出的元数据对象
 */
function parseStructuredMetadata(value) {
  if (!value) return {};
  if (typeof value === "object" && !Array.isArray(value)) return { ...value };
  try {
    const parsed = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? { ...parsed } : {};
  } catch {
    return {};
  }
}

/**
 * 映射卡片对应的前端渲染器 (Renderer) 标识符 (如 code, command, terminal_output, path, markdown 等)
 * @param {object} card - 卡片对象
 * @returns {string} 渲染器标识字符串
 */
function structuredCardRenderer(card) {
  if (card.type === "code") return "code";
  if (card.type === "command") return "command";
  if (card.type === "result") {
    return ["markdown", "json"].includes(card.format) ? card.format : "terminal_output";
  }
  if (["markdown", "plain", "json"].includes(card.format)) return card.format;
  if (card.type === "answer") return "markdown";
  if (card.type === "plan") return "markdown";
  if (card.type === "skill") return "path";
  if (card.type === "skill-content") return "markdown";
  return "plain";
}

// ---------------------------------------------------------------------------
// 【顶级程序入口 (Top-Level Execution Entry)】
//
// 当 Rust 后端以子进程形式 (Child Process) 启动本 node 脚本时：
// 1. 本脚本顶部代码 `const input = JSON.parse(readFileSync(0, "utf8"))` 会首先同步读取 Rust 写入 stdin (FD 0) 的 JSON 入参请求对象；
// 2. 所有工具函数加载完成后，程序直接执行到本 `try...catch` 顶级入口块；
// 3. 根据 `input.method` 方法名分发执行具体的解析动作，并通过 `emit()` (即 `process.stdout.write`) 逐行发送 JSON 格式的事件；
// 4. 无论是正常完成还是捕获到运行时异常，本脚本最终都会发出 `complete` 或 `error` 事件，确保 Rust 端不会挂起死锁。
// ---------------------------------------------------------------------------
try {
  // 1. 【方法分支: probe】探针心跳检测
  //    用于 Rust 后端在初始化或检测适配器可用性时，轻量验证本 Adapter 脚本能否在当前系统环境中正确被 Node.js 执行。
  if (input.method === "project_command_parts") {
    const projections = projectCommandParts(input.params?.parts ?? input.params?.command_parts);
    for (const projection of projections) emit("item", { item: { kind: "command_projection", ...projection } });
    emit("complete", { item: { projection_count: projections.length, projector_version: SHELL_PROJECTOR_VERSION } });
  } else if (input.method === "probe") {
    // 立即向 stdout 发送 complete 信号，表示探针响应成功且会话总数为 0
    emit("complete", { item: { session_count: 0 } });

  // 2. 【方法分支: list_sessions】会话列表元数据列举
  //    当用户在前端操作“同步/刷会话列表”时触发，仅读取 SQLite (`state_5.sqlite`) 表中的摘要信息，速度极快。
  } else if (input.method === "list_sessions") {
    // 调用 sessionRows() 查询数据库并构造成标准描述符数组 [{ external_id, updated_at, source_locator, version_token }]
    const descriptors = listSessions();

    // 逐条将描述符作为 "item" 事件输出到 stdout 流中，供 Rust 增量对比 version_token
    for (const descriptor of descriptors) {
      emit("item", { item: { kind: "session_descriptor", ...descriptor } });
    }

    // 所有描述符发送完毕后，发送 complete 标识并告知本次 Snapshot 提取完成及总条数
    emit("complete", { item: { session_count: descriptors.length, snapshot_complete: true } });

  // 3. 【方法分支: read_session】会话详情读取与深度归一化解析
  //    当需要同步特定会话 (或全量同步) 的具体 Turn/Part 卡片内容时触发。
  } else if (input.method === "read_session") {
    // 读取指定/全部 JSONL 日志、归一化 Turn、切分代码块、提取 Skill 依赖、应用日志压缩与文本配额
    const sessions = readSession();

    // 逐条将完整格式化后的 Session 对象作为 "item" 事件流式写给 Rust
    for (const session of sessions) {
      emit("item", { item: { kind: "session", session } });
    }

    // 发送 complete 信号标识读取完成
    emit("complete", { item: { session_count: sessions.length } });

  // 4. 【异常分支: 未知方法名】
  } else {
    fail(`unsupported method: ${input.method}`);
  }
} catch (error) {
  // 捕获所有未处理的运行时异常 (例如文件读取权限不足、SQL 语句失败、JSON 解析崩坏等)，
  // 通过 fail() 向 Rust 发送 error 事件并安全收尾，防止子进程非正常挂起。
  fail(error instanceof Error ? error.message : String(error));
}
