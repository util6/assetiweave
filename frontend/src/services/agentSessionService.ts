import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AgentSessionGetParams,
  AgentSessionGetResult,
  AgentSessionRef,
  AgentSessionUpdatedEvent,
} from "../types/agentSession";

export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  );
}

export async function getAgentSession(
  params: AgentSessionGetParams,
): Promise<AgentSessionGetResult> {
  if (!isTauriRuntime()) {
    return {
      schemaVersion: 1,
      sessionRef: params.sessionRef,
      state: "unavailable",
      reason: "notFoundOrExpired",
    };
  }
  return invoke<AgentSessionGetResult>("agent_session_get", { params });
}

export function subscribeAgentSessionUpdated(
  sessionRef: AgentSessionRef,
  callback: (event: AgentSessionUpdatedEvent) => void,
): Promise<() => void> {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<AgentSessionUpdatedEvent>("agent-session-updated", (event) => {
    if (
      event.payload &&
      event.payload.sessionRef &&
      event.payload.sessionRef.value === sessionRef.value
    ) {
      callback(event.payload);
    }
  });
}
