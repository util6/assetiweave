import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  memoryRecallSearchResultSchema,
  memoryRecallSessionSchema,
  memoryContextResultSchema,
  memoryProjectViewSchema,
  memoryRebuildResultSchema,
  memoryTaskViewSchema,
  recentMemoryEventTargetSchema,
  recentMemorySessionSchema,
} from "../schemas/memory";
import type {
  MemoryScope,
  MemoryRecallSearchResult,
  MemoryRecallSession,
  MemoryContextResult,
  MemoryProjectView,
  MemoryRebuildResult,
  MemoryTaskView,
  RecentMemoryEventTarget,
  RecentMemorySession,
  RecentConversationView,
} from "../types/memory";

const DESKTOP_REQUIRED = "Memory writes are available only in the AssetIWeave desktop application.";

export function subscribeMemoryTasks(listener: () => void) {
  if (!isTauriRuntime()) return Promise.resolve(() => undefined);
  return listen<void>("memory-task-updated", () => listener());
}

export async function listMemoryRecent(
  params: { view?: RecentConversationView; limit?: number; offset?: number } = {},
): Promise<RecentMemorySession[]> {
  if (!isTauriRuntime()) return [];
  return recentMemorySessionSchema.array().parse(await invoke("list_memory_recent", {
    params: {
      view: params.view ?? "project",
      limit: params.limit ?? 50,
      offset: params.offset ?? 0,
    },
  }));
}

export async function resolveMemoryContext(params: {
  project_path?: string | null;
  query?: string | null;
  token_budget?: number;
} = {}): Promise<MemoryContextResult | null> {
  if (!isTauriRuntime()) return null;
  return memoryContextResultSchema.parse(await invoke("resolve_memory_context", {
    params: {
      project_path: params.project_path ?? null,
      query: params.query ?? null,
      token_budget: params.token_budget ?? 2000,
    },
  }));
}

export async function getMemoryProject(projectPath: string): Promise<MemoryProjectView | null> {
  if (!isTauriRuntime()) return null;
  const value = await invoke("get_memory_project", { params: { project_path: projectPath } });
  return value == null ? null : memoryProjectViewSchema.parse(value);
}

export async function rebuildMemoryScope(scope: MemoryScope = emptyMemoryScope()): Promise<MemoryRebuildResult> {
  requireDesktop();
  return memoryRebuildResultSchema.parse(await invoke("rebuild_memory_scope", { params: { scope } }));
}

export async function listMemoryPublicTasks(activeOnly = false): Promise<MemoryTaskView[]> {
  if (!isTauriRuntime()) return [];
  return memoryTaskViewSchema.array().parse(await invoke("list_memory_public_tasks", {
    params: { active_only: activeOnly },
  }));
}

export async function getMemoryPublicTask(taskId: string): Promise<MemoryTaskView | null> {
  if (!isTauriRuntime()) return null;
  const value = await invoke("get_memory_public_task", { params: { task_id: taskId } });
  return value == null ? null : memoryTaskViewSchema.parse(value);
}

export async function cancelMemoryPublicTask(taskId: string): Promise<MemoryTaskView> {
  requireDesktop();
  return memoryTaskViewSchema.parse(await invoke("cancel_memory_public_task", {
    params: { task_id: taskId },
  }));
}

export async function retryMemoryPublicTask(taskId: string): Promise<MemoryTaskView> {
  requireDesktop();
  return memoryTaskViewSchema.parse(await invoke("retry_memory_public_task", {
    params: { task_id: taskId },
  }));
}

export async function getMemoryRecentEventTarget(eventId: string): Promise<RecentMemoryEventTarget | null> {
  if (!isTauriRuntime()) return null;
  const value = await invoke("get_memory_recent_event_target", { eventId });
  return value == null ? null : recentMemoryEventTargetSchema.parse(value);
}

export async function searchMemoryRecall(params: {
  query: string;
  scope?: MemoryScope;
  since?: string | null;
  until?: string | null;
  file?: string | null;
  command?: string | null;
  error?: string | null;
  limit?: number;
  offset?: number;
}): Promise<MemoryRecallSearchResult | null> {
  if (!isTauriRuntime()) return null;
  return memoryRecallSearchResultSchema.parse(await invoke("search_memory_recall", {
    params: { ...params, scope: params.scope ?? emptyMemoryScope() },
  }));
}

export async function createMemoryRecallSession(scope: MemoryScope = emptyMemoryScope()): Promise<MemoryRecallSession> {
  requireDesktop();
  return memoryRecallSessionSchema.parse(await invoke("create_memory_recall_session", { params: { scope } }));
}

export async function getMemoryRecallSession(sessionId: string): Promise<MemoryRecallSession> {
  requireDesktop();
  return memoryRecallSessionSchema.parse(await invoke("get_memory_recall_session", {
    params: { session_id: sessionId },
  }));
}

export async function sendMemoryRecallTurn(sessionId: string, query: string): Promise<MemoryRecallSession> {
  requireDesktop();
  return memoryRecallSessionSchema.parse(await invoke("send_memory_recall_turn", {
    params: { session_id: sessionId, query },
  }));
}

export async function cancelMemoryRecallTurn(turnId: string): Promise<MemoryRecallSession> {
  requireDesktop();
  return memoryRecallSessionSchema.parse(await invoke("cancel_memory_recall_turn", {
    params: { turn_id: turnId },
  }));
}

export function emptyMemoryScope(): MemoryScope {
  return { app_id: null, source_id: null, project_path: null, session_id: null };
}

function requireDesktop(): void {
  if (!isTauriRuntime()) {
    throw new Error(DESKTOP_REQUIRED);
  }
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
