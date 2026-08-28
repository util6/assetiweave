import fs from "node:fs";
import readline from "node:readline";

const mode = process.env.ASSETIWEAVE_FAKE_ACP_MODE ?? "happy";
const recordPath = process.env.ASSETIWEAVE_FAKE_ACP_RECORD_PATH;
const sessionId = "fixture-session";
let pendingPromptId;
let pendingPermissionId;

function record(event, fields = {}) {
  if (!recordPath) {
    return;
  }
  fs.appendFileSync(recordPath, `${JSON.stringify({ event, ...fields })}\n`);
}

function send(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}

function respond(id, result) {
  send({ jsonrpc: "2.0", id, result });
}

function fail(id, code = -32603, message = "FAKE_PRIVATE_ERROR_MUST_NOT_ESCAPE") {
  send({
    jsonrpc: "2.0",
    id,
    error: { code, message },
  });
}

function update(updateValue, targetSessionId = sessionId) {
  send({
    jsonrpc: "2.0",
    method: "session/update",
    params: { sessionId: targetSessionId, update: updateValue },
  });
}

function text(value, targetSessionId = sessionId) {
  update(
    {
      sessionUpdate: "agent_message_chunk",
      content: { type: "text", text: value },
    },
    targetSessionId,
  );
}

function finishPrompt(id, stopReason = "end_turn") {
  respond(id, { stopReason });
}

function handleInitialize(message) {
  record("initialize", {
    clientName: message.params?.clientInfo?.name,
    terminal: message.params?.clientCapabilities?.terminal,
  });
  if (mode === "initialize_timeout") {
    return;
  }
  if (mode === "initialize_error") {
    fail(message.id);
    return;
  }
  const supportsClose = !["no_close", "initialize_timeout"].includes(mode);
  const supportsDelete = !["initialize_timeout", "no_delete", "no_delete_empty"].includes(mode);
  respond(message.id, {
    protocolVersion: 1,
    agentCapabilities: supportsClose || supportsDelete
      ? {
          sessionCapabilities: {
            ...(supportsClose ? { close: {} } : {}),
            ...(supportsDelete ? { delete: {} } : {}),
          },
        }
      : {},
    agentInfo: { name: "assetiweave-fake-acp", version: "1" },
  });
}

function handleNewSession(message) {
  record("new", {
    cwd: message.params?.cwd,
    mcpCount: message.params?.mcpServers?.length ?? -1,
    additionalDirectoryCount:
      message.params?.additionalDirectories?.length ?? 0,
  });
  if (mode === "new_error") {
    fail(message.id);
    return;
  }
  respond(message.id, {
    sessionId,
    configOptions: mode === "no_models" ? [] : [
      {
        id: "model",
        name: "Model",
        category: "model",
        type: "select",
        currentValue: "fixture/model-fast",
        options: [
          { value: "fixture/model-fast", name: "Fixture Fast", description: "Fast fixture model" },
          { value: "fixture/model-accurate", name: "Fixture Accurate" },
        ],
      },
    ],
  });
}

function handleSetConfig(message) {
  record("model", {
    configId: message.params?.configId,
    value: message.params?.value,
  });
  if (mode === "model_reject") {
    fail(message.id, -32602);
    return;
  }
  if (mode === "model_timeout") {
    return;
  }
  respond(message.id, { configOptions: [] });
}

function handlePrompt(message) {
  record("prompt", {
    blockTypes: (message.params?.prompt ?? []).map((block) => block.type),
  });
  const id = message.id;
  switch (mode) {
    case "chunked":
      text("你");
      text("好🌍");
      finishPrompt(id);
      return;
    case "thinking":
      update({
        sessionUpdate: "agent_thought_chunk",
        content: { type: "text", text: "PRIVATE_THOUGHT" },
      });
      text("visible");
      finishPrompt(id);
      return;
    case "wrong_session":
      text("wrong", "other-session");
      text("right");
      finishPrompt(id);
      return;
    case "empty":
    case "no_delete_empty":
      finishPrompt(id);
      return;
    case "oversized":
      text("x".repeat(Number(process.env.ASSETIWEAVE_FAKE_ACP_TEXT_BYTES ?? 2048)));
      pendingPromptId = id;
      return;
    case "late_chunk":
      text("before ");
      text("late");
      finishPrompt(id);
      return;
    case "permission":
      pendingPromptId = id;
      pendingPermissionId = 9001;
      send({
        jsonrpc: "2.0",
        id: pendingPermissionId,
        method: "session/request_permission",
        params: {
          sessionId,
          toolCall: {
            toolCallId: "permission-tool",
            rawInput: { secret: "RAW_PERMISSION_SECRET" },
          },
          options: [
            { optionId: "reject", name: "Reject", kind: "reject_once" },
          ],
        },
      });
      return;
    case "tool_call":
      update({
        sessionUpdate: "tool_call",
        toolCallId: "fixture-tool",
        title: "fixture tool",
        kind: "read",
        status: "pending",
        rawInput: { secret: "RAW_TOOL_SECRET" },
      });
      pendingPromptId = id;
      return;
    case "cancel_wait":
      pendingPromptId = id;
      return;
    case "disconnect":
      process.exit(0);
      return;
    case "exit_during_prompt":
      process.exit(17);
      return;
    default:
      text("translated");
      finishPrompt(id);
  }
}

function handleClose(message) {
  record("close");
  if (mode === "close_error") {
    fail(message.id);
    return;
  }
  if (mode === "close_hang") {
    return;
  }
  respond(message.id, {});
}

function handleDelete(message) {
  record("delete", { sessionId: message.params?.sessionId });
  if (mode === "delete_error") {
    fail(message.id);
    return;
  }
  if (mode === "delete_not_found") {
    fail(message.id, -32602, `Session not found: ${sessionId}`);
    return;
  }
  if (mode === "delete_hang") {
    return;
  }
  respond(message.id, {});
}

function handleCancel(message) {
  record("cancel", { sessionId: message.params?.sessionId });
  if (pendingPromptId !== undefined) {
    finishPrompt(pendingPromptId, "cancelled");
    pendingPromptId = undefined;
  }
}

function handleResponse(message) {
  if (message.id !== pendingPermissionId) {
    return;
  }
  record("permission_response", {
    outcome: message.result?.outcome?.outcome,
  });
  pendingPermissionId = undefined;
  if (pendingPromptId !== undefined) {
    finishPrompt(pendingPromptId, "cancelled");
    pendingPromptId = undefined;
  }
}

const input = readline.createInterface({ input: process.stdin });
const keepAlive = mode.startsWith("no_delete") ? setInterval(() => {}, 1_000) : undefined;
input.on("line", (line) => {
  let message;
  try {
    message = JSON.parse(line);
  } catch {
    record("invalid_json");
    return;
  }

  if (message.method === "initialize") {
    handleInitialize(message);
  } else if (message.method === "session/new") {
    handleNewSession(message);
  } else if (message.method === "session/set_config_option") {
    handleSetConfig(message);
  } else if (message.method === "session/prompt") {
    handlePrompt(message);
  } else if (message.method === "session/cancel") {
    handleCancel(message);
  } else if (message.method === "session/close") {
    handleClose(message);
  } else if (message.method === "session/delete") {
    handleDelete(message);
  } else if (Object.hasOwn(message, "id")) {
    handleResponse(message);
  }
});

input.on("close", () => record("stdin_closed"));
process.on("SIGTERM", () => {
  if (keepAlive) clearInterval(keepAlive);
  record("sigterm");
  process.exit(0);
});
