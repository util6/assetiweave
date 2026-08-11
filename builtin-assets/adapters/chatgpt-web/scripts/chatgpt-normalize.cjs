function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

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

function normalizeRole(value) {
  const role = text(value).toLowerCase();
  if (role === "user") return "user";
  if (role === "assistant") return "assistant";
  if (role === "tool") return "tool";
  if (role === "system") return "system";
  return "";
}

function hiddenMessage(message) {
  const metadata = message && message.metadata ? message.metadata : {};
  return Boolean(
    metadata.is_visually_hidden_from_conversation ||
      metadata.is_user_system_message ||
      metadata.is_redacted
  );
}

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

function attachmentURL(value) {
  if (!value || typeof value !== "object") return "";
  if (typeof value.asset_pointer === "string") return value.asset_pointer.trim();
  if (typeof value.url === "string") return value.url.trim();
  if (value.image_url && typeof value.image_url.url === "string") return value.image_url.url.trim();
  if (typeof value.download_url === "string") return value.download_url.trim();
  if (typeof value.file_id === "string") return value.file_id.trim();
  return "";
}

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

  return Object.values(mapping).sort((left, right) => {
    const leftTime = (left.message && Number(left.message.create_time)) || 0;
    const rightTime = (right.message && Number(right.message.create_time)) || 0;
    if (leftTime !== rightTime) return leftTime - rightTime;
    return text(left.id).localeCompare(text(right.id));
  });
}

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
