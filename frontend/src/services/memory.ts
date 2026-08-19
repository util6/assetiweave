import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  memoryDreamPreviewSchema,
  memoryDreamNoteDetailSchema,
  memoryDreamNotePageSchema,
  memoryDreamRunResultSchema,
  memoryItemDetailSchema,
  memoryItemPageSchema,
  memoryTaskSnapshotSchema,
  memoryOverviewSchema,
  memoryRecallPreviewSchema,
  memoryRecallRunResultSchema,
  memoryVerifyResultSchema,
} from "../schemas/memory";
import type {
  MemoryCandidateAcceptParams,
  MemoryDreamPreview,
  MemoryDreamNoteDetail,
  MemoryDreamNotePage,
  MemoryDreamNoteStatus,
  MemoryDreamRunResult,
  MemoryDreamTrigger,
  MemoryItemCreateParams,
  MemoryItemDetail,
  MemoryItemListParams,
  MemoryItemPageResult,
  MemoryItemUpdateParams,
  MemoryScope,
  MemoryTaskSnapshot,
  MemoryTaskStartParams,
  MemoryOverview,
  MemoryRecallPreview,
  MemoryRecallPreviewParams,
  MemoryRecallRunResult,
  MemoryVerifyResult,
} from "../types/memory";

const DESKTOP_REQUIRED = "Memory writes are available only in the AssetIWeave desktop application.";
const MEMORY_TASK_UPDATED_EVENT = "memory-task-updated";

export async function listMemoryItems(params: MemoryItemListParams = {}): Promise<MemoryItemPageResult> {
  if (!isTauriRuntime()) {
    return {
      availability: "browser_preview",
      total_count: 0,
      items: [],
      limit: params.limit ?? 50,
      offset: params.offset ?? 0,
    };
  }
  const page = memoryItemPageSchema.parse(await invoke("list_memory_items", { params }));
  return { ...page, availability: "tauri" };
}

export async function getMemoryItem(itemId: string): Promise<MemoryItemDetail> {
  requireDesktop();
  return memoryItemDetailSchema.parse(
    await invoke("get_memory_item", { params: { item_id: itemId } }),
  );
}

export async function createMemoryItem(params: MemoryItemCreateParams): Promise<MemoryItemDetail> {
  requireDesktop();
  return memoryItemDetailSchema.parse(await invoke("create_memory_item", { params }));
}

export async function updateMemoryItem(params: MemoryItemUpdateParams): Promise<MemoryItemDetail> {
  requireDesktop();
  return memoryItemDetailSchema.parse(await invoke("update_memory_item", { params }));
}

export async function archiveMemoryItem(itemId: string): Promise<MemoryItemDetail> {
  requireDesktop();
  return memoryItemDetailSchema.parse(
    await invoke("archive_memory_item", { params: { item_id: itemId } }),
  );
}

export async function acceptMemoryCandidate(params: MemoryCandidateAcceptParams): Promise<MemoryItemDetail> {
  requireDesktop();
  return memoryItemDetailSchema.parse(await invoke("accept_memory_candidate", { params }));
}

export async function rejectMemoryCandidate(itemId: string): Promise<MemoryItemDetail> {
  requireDesktop();
  return memoryItemDetailSchema.parse(
    await invoke("reject_memory_candidate", { params: { item_id: itemId } }),
  );
}

export async function getMemoryDreamStatus(scope?: MemoryScope): Promise<MemoryDreamPreview | null> {
  if (!isTauriRuntime()) return null;
  return memoryDreamPreviewSchema.parse(
    await invoke("memory_dream_status", { params: { scope: scope ?? emptyMemoryScope() } }),
  );
}

export async function getMemoryOverview(scope?: MemoryScope): Promise<MemoryOverview | null> {
  if (!isTauriRuntime()) return null;
  return memoryOverviewSchema.parse(
    await invoke("memory_overview", { params: { scope: scope ?? emptyMemoryScope() } }),
  );
}

export async function listMemoryDreamNotes(params: {
  statuses?: MemoryDreamNoteStatus[];
  scope?: MemoryScope | null;
  limit?: number;
  offset?: number;
} = {}): Promise<MemoryDreamNotePage | null> {
  if (!isTauriRuntime()) return null;
  return memoryDreamNotePageSchema.parse(await invoke("list_memory_dream_notes", { params }));
}

export async function getMemoryDreamNote(noteId: string): Promise<MemoryDreamNoteDetail> {
  requireDesktop();
  return memoryDreamNoteDetailSchema.parse(
    await invoke("get_memory_dream_note", { params: { note_id: noteId } }),
  );
}

export async function archiveMemoryDreamNote(noteId: string): Promise<MemoryDreamNoteDetail> {
  requireDesktop();
  return memoryDreamNoteDetailSchema.parse(
    await invoke("archive_memory_dream_note", { params: { note_id: noteId } }),
  );
}

export async function promoteMemoryDreamNote(noteId: string): Promise<MemoryItemDetail[]> {
  requireDesktop();
  return memoryItemDetailSchema.array().parse(
    await invoke("promote_memory_dream_note", { params: { note_id: noteId } }),
  );
}

export async function previewMemoryDream(params: {
  scope?: MemoryScope;
  trigger?: MemoryDreamTrigger;
} = {}): Promise<MemoryDreamPreview> {
  requireDesktop();
  return memoryDreamPreviewSchema.parse(await invoke("preview_memory_dream", {
    params: {
      scope: params.scope ?? emptyMemoryScope(),
      trigger: params.trigger ?? "manual",
    },
  }));
}

export async function runMemoryDream(params: {
  scope?: MemoryScope;
  trigger?: MemoryDreamTrigger;
  dry_run?: boolean;
} = {}): Promise<MemoryDreamRunResult> {
  requireDesktop();
  return memoryDreamRunResultSchema.parse(await invoke("run_memory_dream", {
    params: {
      scope: params.scope ?? emptyMemoryScope(),
      trigger: params.trigger ?? "manual",
      dry_run: params.dry_run ?? false,
    },
  }));
}

export async function startMemoryTask(params: MemoryTaskStartParams): Promise<MemoryTaskSnapshot> {
  requireDesktop();
  return memoryTaskSnapshotSchema.parse(await invoke("start_memory_task", {
    params: {
      ...params,
      scope: params.scope ?? emptyMemoryScope(),
      trigger: params.trigger ?? "manual",
      dry_run: params.dry_run ?? false,
    },
  }));
}

export async function getMemoryTask(taskId: string): Promise<MemoryTaskSnapshot | null> {
  if (!isTauriRuntime()) return null;
  const value = await invoke("get_memory_task", { params: { task_id: taskId } });
  return value == null ? null : memoryTaskSnapshotSchema.parse(value);
}

export async function listMemoryTasks(): Promise<MemoryTaskSnapshot[]> {
  if (!isTauriRuntime()) return [];
  return memoryTaskSnapshotSchema.array().parse(await invoke("list_memory_tasks"));
}

export async function cancelMemoryTask(taskId: string): Promise<MemoryTaskSnapshot> {
  requireDesktop();
  return memoryTaskSnapshotSchema.parse(
    await invoke("cancel_memory_task", { params: { task_id: taskId } }),
  );
}

export function subscribeMemoryTasks(listener: (snapshot: MemoryTaskSnapshot) => void) {
  if (!isTauriRuntime()) {
    return Promise.resolve(() => undefined);
  }
  return listen<MemoryTaskSnapshot>(MEMORY_TASK_UPDATED_EVENT, (event) => {
    listener(event.payload);
  });
}

export async function previewMemoryRecall(params: MemoryRecallPreviewParams): Promise<MemoryRecallPreview | null> {
  if (!isTauriRuntime()) return null;
  return memoryRecallPreviewSchema.parse(await invoke("preview_memory_recall", { params: {
    ...params, scope: params.scope ?? emptyMemoryScope(),
  } }));
}

export async function runMemoryRecall(params: MemoryRecallPreviewParams & { synthesize?: boolean; dry_run?: boolean }): Promise<MemoryRecallRunResult> {
  requireDesktop();
  return memoryRecallRunResultSchema.parse(await invoke("run_memory_recall", { params: {
    ...params, scope: params.scope ?? emptyMemoryScope(), synthesize: params.synthesize ?? false, dry_run: params.dry_run ?? false,
  } }));
}

export async function verifyMemoryItems(itemIds: string[]): Promise<MemoryVerifyResult> {
  requireDesktop();
  return memoryVerifyResultSchema.parse(await invoke("verify_memory", { params: { item_ids: itemIds } }));
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
