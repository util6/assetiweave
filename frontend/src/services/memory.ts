import type {
  MemoryCandidateAcceptParams,
  MemoryItemCreateParams,
  MemoryItemDetail,
  MemoryItemListParams,
  MemoryItemPageResult,
  MemoryItemUpdateParams,
} from "../types/memory";

const DESKTOP_REQUIRED = "Memory writes are available only in the AssetIWeave desktop application.";

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

function requireDesktop(): void {
  if (!isTauriRuntime()) {
    throw new Error(DESKTOP_REQUIRED);
  }
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
import { invoke } from "@tauri-apps/api/core";
import { memoryItemDetailSchema, memoryItemPageSchema } from "../schemas/memory";
