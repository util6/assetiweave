/**
 * @file qwen-normalize.cjs — 通义千问 (Qwen) Web 响应格式归一化转换模块
 *
 * 职责：
 * 1. 拆解 Qwen 多轮对话 Round (request_messages / response_messages / qwen_response_messages) 结构
 * 2. 提炼 Assistant 回答、Plugin 插件调用结果与 ReferenceLink 参考链接
 * 3. 构造标准化的 Session Turn 与 Part 结构
 */
"use strict";

/** 辅助字符串清理函数 */
function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

/** 获取消息列表中的首个有效文本内容 */
function firstContent(messages) {
  if (!Array.isArray(messages)) return "";
  for (const message of messages) {
    const content = text(message && message.content);
    if (content) return content;
  }
  return "";
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

/** 解析 JSON 字符串并安全处理异常 */
function parseJSON(value) {
  const source = text(value);
  if (!source) return null;
  try {
    return JSON.parse(source);
  } catch {
    return null;
  }
}

/** 获取引用/插件链接展示标题 */
function linkLabel(link, index) {
  return (
    text(link && link.title) ||
    text(link && link.name) ||
    text(link && link.source) ||
    `reference-${index + 1}`
  );
}

/** 格式化搜索引用与插件结果链接为 Markdown 段落 */
function formatLinks(label, links) {
  if (!Array.isArray(links) || links.length === 0) return "";
  const lines = [label];
  links.forEach((link, index) => {
    const url = text(link && link.url);
    const title = linkLabel(link, index);
    const summary = text(link && (link.body || link.summary));
    const suffix = summary && summary !== title ? ` - ${summary}` : "";
    if (url) {
      lines.push(`- [${title}](${url})${suffix}`);
    } else {
      lines.push(`- ${title}${suffix}`);
    }
  });
  return lines.join("\n");
}

/** 解析插件工具调用的 JSON 响应 Payload */
function pluginResultPayload(response) {
  const payload = parseJSON(response && response.content);
  if (!payload || typeof payload !== "object") return null;
  if (typeof payload.pluginResult === "string") {
    return parseJSON(payload.pluginResult);
  }
  return payload;
}

/** 从响应消息中提取插件结果与参考引用内容 */
function resultContents(round) {
  const contents = [];
  const seen = new Set();
  const append = (value) => {
    const content = text(value);
    if (!content || seen.has(content)) return;
    seen.add(content);
    contents.push(content);
  };

  for (const response of Array.isArray(round && round.qwen_response_messages)
    ? round.qwen_response_messages
    : []) {
    if (!response || response.status === "interrupted") continue;
    if (response.role === "plugin" && response.contentType === "plugin") {
      const payload = pluginResultPayload(response);
      append(formatLinks("Qwen tool result:", payload && payload.links));
    } else if (response.role === "assistant" && response.contentType === "referenceLink") {
      const payload = parseJSON(response.content);
      append(formatLinks("Qwen references:", payload && payload.links));
    } else if (response.role === "assistant" && response.contentType === "card") {
      const payload = parseJSON(response.content);
      const cardText = text(payload && payload.content);
      if (cardText) append(cardText);
    }
  }

  return contents;
}

/** 从响应消息中提取 Assistant 文本和 iframe 内容 */
function assistantContents(round) {
  const contents = [];
  const seen = new Set();
  const append = (value) => {
    const content = text(value);
    if (!content || seen.has(content)) return;
    seen.add(content);
    contents.push(content);
  };

  for (const response of Array.isArray(round && round.response_messages)
    ? round.response_messages
    : []) {
    if (response && response.mime_type === "multi_load/iframe") {
      append(response.content);
    }
  }

  for (const response of Array.isArray(round && round.qwen_response_messages)
    ? round.qwen_response_messages
    : []) {
    if (response && response.role === "assistant" && response.contentType === "text") {
      append(response.content);
    }
  }

  return contents;
}

/**
 * 将单个 Qwen 交互 Round 转换为规范化的 Turn 对象
 *
 * @param {object} round - 原始 Round JSON 对象
 * @param {number} index - 轮次序号
 * @returns {object|null} 归一化 Turn 对象
 */
function normalizeRound(round, index) {
  const userText = firstContent(round && round.request_messages);
  if (!userText) return null;
  const parts = [
    ...assistantContents(round).map((content) => normalizedPart("assistant", "text", {
      text: content,
      metadata_json: metadata({ type: "answer", format: "markdown" })
    })),
    ...resultContents(round).map((content) => normalizedPart("tool", "tool", {
      text: content,
      status: "completed",
      metadata_json: metadata({ type: "result", format: "markdown" })
    }))
  ];
  return {
    external_id: text(round.req_id) || `turn-${index + 1}`,
    turn_index: index,
    user_text: userText,
    title: null,
    started_at: text(round.create_time) || null,
    ended_at: text(round.update_time) || null,
    parts
  };
}

module.exports = {
  assistantContents,
  normalizeRound
};
