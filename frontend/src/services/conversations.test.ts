import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  getConversationSyncTask,
  mergeConversationQuestions,
  searchConversationRecords,
  summarizeConversationSyncTask,
  syncConversations,
} from "./conversations";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

describe("conversation services", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("throws write-operation errors in the Tauri runtime", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockRejectedValueOnce(new Error("merge failed"));

    await expect(mergeConversationQuestions(["question-1", "question-2"])).rejects.toThrow("merge failed");
  });

  it("keeps fallback behavior for non-Tauri previews", async () => {
    vi.stubGlobal("window", {});
    invokeMock.mockRejectedValueOnce(new Error("preview backend missing"));

    await expect(mergeConversationQuestions(["preview-question-1", "preview-question-2"])).resolves.toMatchObject({
      dry_run: false,
      affected_question_ids: ["preview-question-1", "preview-question-2"],
    });
  });

  it("starts sync as a background task", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({
      id: "sync-1",
      status: "running",
      source_id: null,
      adapter_id: null,
      dry_run: false,
      started_at: "2026-06-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    });

    await expect(syncConversations({ source_id: null, dry_run: false })).resolves.toMatchObject({
      id: "sync-1",
      status: "running",
    });
    expect(invokeMock).toHaveBeenCalledWith("sync_conversations", {
      params: { source_id: null, dry_run: false },
    });
  });

  it("searches conversation records with content-type filters", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({
      query: "deploy",
      record_kind: "session",
      total_count: 1,
      hits: [],
    });

    await expect(
      searchConversationRecords({
        query: " deploy ",
        record_kind: "session",
        content_types: ["question", "answer"],
        limit: 25,
      }),
    ).resolves.toMatchObject({
      query: "deploy",
      total_count: 1,
    });
    expect(invokeMock).toHaveBeenCalledWith("search_conversation_records", {
      params: {
        query: "deploy",
        record_kind: "session",
        content_types: ["question", "answer"],
        limit: 25,
        offset: 0,
      },
    });
  });

  it("reads the desktop sync background task status", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({
      id: "sync-1",
      status: "completed",
      source_id: null,
      adapter_id: null,
      dry_run: false,
      started_at: "2026-06-15T00:00:00Z",
      finished_at: "2026-06-15T00:00:05Z",
      result: { results: [] },
      error: null,
    });

    await expect(getConversationSyncTask()).resolves.toMatchObject({
      id: "sync-1",
      status: "completed",
    });
    expect(invokeMock).toHaveBeenCalledWith("get_conversation_sync_task");
  });

  it("summarizes completed sync task results for user-facing completion messages", () => {
    expect(
      summarizeConversationSyncTask({
        id: "sync-1",
        status: "completed",
        source_id: null,
        adapter_id: null,
        dry_run: false,
        started_at: "2026-06-15T00:00:00Z",
        finished_at: "2026-06-15T00:00:05Z",
        result: {
          results: [
            { session_count: 10, skipped_session_count: 7, turn_count: 15, warning_count: 1 },
            { session_count: 2, skipped_session_count: 0, turn_count: 3, warning_count: 0 },
          ],
          errors: [{ source_id: "bad-source" }],
        },
        error: null,
      }),
    ).toEqual({
      sourceCount: 2,
      changedSessionCount: 5,
      skippedSessionCount: 7,
      turnCount: 18,
      warningCount: 1,
      errorCount: 1,
    });
  });
});
