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
  const parts = assistantContents(round).map((content) => ({
    role: "assistant",
    kind: "text",
    text: content,
    language: null,
    command: null,
    cwd: null,
    status: null,
    exit_code: null,
    metadata_json: null
  }));
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
