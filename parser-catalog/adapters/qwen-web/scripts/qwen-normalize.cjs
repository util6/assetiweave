function text(value) {
  return typeof value === "string" ? value.trim() : "";
}

function firstContent(messages) {
  if (!Array.isArray(messages)) return "";
  for (const message of messages) {
    const content = text(message && message.content);
    if (content) return content;
  }
  return "";
}

function metadata(contentCard, extra = {}) {
  return JSON.stringify({
    ...extra,
    content_card: contentCard
  });
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

function parseJSON(value) {
  const source = text(value);
  if (!source) return null;
  try {
    return JSON.parse(source);
  } catch {
    return null;
  }
}

function linkLabel(link, index) {
  return (
    text(link && link.title) ||
    text(link && link.name) ||
    text(link && link.source) ||
    `reference-${index + 1}`
  );
}

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

function pluginResultPayload(response) {
  const payload = parseJSON(response && response.content);
  if (!payload || typeof payload !== "object") return null;
  if (typeof payload.pluginResult === "string") {
    return parseJSON(payload.pluginResult);
  }
  return payload;
}

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
