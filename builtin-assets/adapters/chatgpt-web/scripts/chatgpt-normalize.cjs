/**
 * @file chatgpt-normalize.cjs — ChatGPT 网页端 API 响应格式归一化转换模块
 *
 * 职责：
 * 1. 处理 ChatGPT JSON 树结构 (mapping / current_node) 的树节点回溯
 * 2. 提取文本内容与多媒体附件 (Image / File Assets)
 * 3. 过滤系统内部/可视隐藏消息 (visually hidden / redacted)
 * 4. 将原始节点列表转换为标准的 Session Turn 与 Part 数据块结构
 */
"use strict";

/**
 * 辅助字符串清理函数
 * @param {any} value - 待清理的值
 * @returns {string} 裁剪后的字符串
 */
function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

/**
 * 解析并格式化时间戳为 ISO8601 字符串
 *
 * @param {number|string|any} value - 时间戳数值 (秒/毫秒) 或时间字符串
 * @returns {string|null} ISO 时间格式字符串或 null
 */
function timestamp(value) {
  if (typeof value === "number" && Number.isFinite(value)) {
    const millis = value > 1000000000000 ? value : value * 1000;
    const date = new Date(millis);
    return Number.isNaN(date.getTime()) ? null : date.toISOString();
  }
  if (typeof value === "string" && value.trim()) {
    const numeric = Number(value);
    if (Number.isFinite(numeric)) return timestamp(numeric);
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? null : date.toISOString();
  }
  return null;
}

/**
 * 构造统一的标准归一化 Part 数据结构
 *
 * @param {string} role - 角色标识 ("user" | "assistant" | "tool" | "system")
 * @param {string} kind - 类型 ("text" | "command" | "result")
 * @param {object} [fields={}] - 拓展字段 (text, language, command, status等)
 * @returns {object} 标准化 Part 对象
 */
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
 * 规整规范化角色字符串
 * @param {any} value - 原始角色标识
 * @returns {string} 标准化角色
 */
function normalizeRole(value) {
  const role = text(value).toLowerCase();
  if (role === "user") return "user";
  if (role === "assistant") return "assistant";
  if (role === "tool") return "tool";
  if (role === "system") return "system";
  return "";
}

/**
 * 判断消息节点是否属于隐藏/不可见/系统内部屏蔽消息
 * @param {object} message - ChatGPT 消息对象
 * @returns {boolean} 是否应被过滤隐藏
 */
function hiddenMessage(message) {
  const metadata = message && message.metadata ? message.metadata : {};
  return Boolean(
    metadata.is_visually_hidden_from_conversation ||
      metadata.is_user_system_message ||
      metadata.is_redacted
  );
}

/**
 * 从附件节点获取展示标签名称
 * @param {object} value - 附件节点
 * @returns {string} 标签名
 */
function attachmentLabel(value) {
  if (!value || typeof value !== "object") return "";
  return (
    text(value.name) ||
    text(value.file_name) ||
    text(value.filename) ||
    text(value.title) ||
    text(value.mime_type)
  );
}

/**
 * 从附件节点提取资源指针或下载 URL
 * @param {object} value - 附件节点
 * @returns {string} 资源 URL 或 ID
 */
function attachmentURL(value) {
  if (!value || typeof value !== "object") return "";
  if (typeof value.asset_pointer === "string") return value.asset_pointer.trim();
  if (typeof value.url === "string") return value.url.trim();
  if (value.image_url && typeof value.image_url.url === "string") return value.image_url.url.trim();
  if (typeof value.download_url === "string") return value.download_url.trim();
  if (typeof value.file_id === "string") return value.file_id.trim();
  return "";
}

/**
 * 解析收集多媒体/文件/图片类型的 Part 对象
 * @param {object} part - 原始 Content Part 节点
 * @param {string[]} texts - 文本列表收集器
 * @param {object[]} attachments - 附件列表收集器
 */
function collectObjectPart(part, texts, attachments) {
  const contentType = text(part.content_type).toLowerCase();
  if (contentType === "text" || typeof part.text === "string") {
    const value = text(part.text);
    if (value) texts.push(value);
    return;
  }
  if (
    contentType.includes("image") ||
    contentType.includes("file") ||
    part.asset_pointer ||
    part.file_id ||
    part.url ||
    part.image_url
  ) {
    const url = attachmentURL(part);
    const label = attachmentLabel(part);
    if (url || label) attachments.push({ url, label });
    return;
  }
  const fallback = text(part.content) || text(part.value) || text(part.result);
  if (fallback) texts.push(fallback);
}

/**
 * 提取 ChatGPT 消息 content 结构中的主要文本和多媒体附件
 *
 * @param {object} content - ChatGPT 节点 content 对象
 * @returns {{ text: string, attachments: object[] }} 提取出的组合文本与附件
 */
function extractContent(content) {
  const texts = [];
  const attachments = [];
  if (!content || typeof content !== "object") return { text: "", attachments };

  const directText = text(content.text) || text(content.result);
  if (directText) texts.push(directText);

  if (Array.isArray(content.parts)) {
    for (const part of content.parts) {
      if (typeof part === "string") {
        const value = text(part);
        if (value) texts.push(value);
      } else if (part && typeof part === "object") {
        collectObjectPart(part, texts, attachments);
      }
    }
  }

  return {
    text: texts.join("\n\n").trim(),
    attachments
  };
}

/**
 * 格式化附件列表为 Markdown 文本附录
 *
 * @param {string} label - 附录标题
 * @param {object[]} attachments - 附件对象列表
 * @returns {string} 格式化后的 Markdown 块文本
 */
function formatAttachments(label, attachments) {
  if (!attachments.length) return "";
  const lines = [label];
  for (const attachment of attachments) {
    if (attachment.url && attachment.label && /^https?:\/\//i.test(attachment.url)) {
      lines.push(`- [${attachment.label}](${attachment.url})`);
    } else if (attachment.url && attachment.label) {
      lines.push(`- ${attachment.label}: ${attachment.url}`);
    } else {
      lines.push(`- ${attachment.url || attachment.label}`);
    }
  }
  return lines.join("\n");
}

/**
 * 根据 ChatGPT 会话图节点的 current_node 游标，自底向上追踪计算从根节点到主线末端的正确对话链路径
 *
 * @param {object} conversation - ChatGPT 原始 Conversation JSON
 * @returns {object[]} 按时间主线正序排列的消息节点数组
 */
function pathToCurrent(conversation) {
  const mapping = conversation && conversation.mapping && typeof conversation.mapping === "object"
    ? conversation.mapping
    : {};
  const currentID = text(conversation && conversation.current_node);
  if (currentID && mapping[currentID]) {
    const path = [];
    const seen = new Set();
    let node = mapping[currentID];
    while (node && !seen.has(node.id)) {
      seen.add(node.id);
      path.unshift(node);
      node = node.parent ? mapping[node.parent] : null;
    }
    return path;
  }

  // 若无 current_node 指针，退化按节点的创建时间正序排序
  return Object.values(mapping).sort((left, right) => {
    const leftTime = (left.message && Number(left.message.create_time)) || 0;
    const rightTime = (right.message && Number(right.message.create_time)) || 0;
    if (leftTime !== rightTime) return leftTime - rightTime;
    return text(left.id).localeCompare(text(right.id));
  });
}

/**
 * 解析整条 ChatGPT 原始会话，将其转换为标准化的 Session Turns 数组
 *
 * @param {object} conversation - ChatGPT 原始 Conversation JSON
 * @returns {object[]} 标准化 Turn 轮次结构数组
 */
function parseConversation(conversation) {
  const turns = [];
  let current = null;
  for (const node of pathToCurrent(conversation)) {
    const message = node && node.message;
    if (!message || hiddenMessage(message)) continue;
    const role = normalizeRole(message.author && message.author.role);
    if (!role || role === "system") continue;
    const content = extractContent(message.content);
    const messageText = content.text;

    // 用户消息发起新轮次 (Turn)
    if (role === "user") {
      if (current) turns.push(current);
      const userText = [
        messageText,
        formatAttachments("User attachments:", content.attachments)
      ].filter(Boolean).join("\n\n");
      if (!userText) {
        current = null;
        continue;
      }
      current = {
        external_id: text(message.id) || text(node.id) || `turn-${turns.length + 1}`,
        turn_index: turns.length,
        user_text: userText,
        title: null,
        started_at: timestamp(message.create_time),
        ended_at: null,
        parts: []
      };
      continue;
    }

    if (!messageText) continue;
    if (!current) {
      current = {
        external_id: text(message.id) || text(node.id) || `turn-${turns.length + 1}`,
        turn_index: turns.length,
        user_text: "[ChatGPT continuation without visible user prompt]",
        title: null,
        started_at: timestamp(message.create_time),
        ended_at: null,
        parts: []
      };
    }
    // 模型响应加入当前 Turn 的 Parts 列表
    current.parts.push(normalizedPart(role, "text", { text: messageText }));
    const endedAt = timestamp(message.update_time) || timestamp(message.create_time);
    if (endedAt) current.ended_at = endedAt;
  }
  if (current) turns.push(current);
  return turns;
}

module.exports = {
  extractContent,
  formatAttachments,
  parseConversation,
  timestamp
};
