import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  getConversationSyncTask,
  importConversationSource,
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

  it("imports a conversation source by validating the adapter, adding the source, then starting background sync", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock
      .mockResolvedValueOnce({
        valid: true,
        manifest_path: "/tmp/adapter/conversation-adapter.json",
        manifest_hash: "manifest-hash",
        executable_path: "/tmp/adapter/run",
        executable_hash: "exe-hash",
        manifest: {
          schema_version: 1,
          id: "medical-web",
          name: "Medical Web",
          version: "0.1.0",
          protocol_version: 1,
          command: ["run"],
          capabilities: ["read_session", "web_records"],
          input_kinds: ["directory"],
        },
        warnings: [],
      })
      .mockResolvedValueOnce({
        dry_run: false,
        adapter: {
          id: "medical-web",
          name: "Medical Web",
          kind: "external",
          version: "0.1.0",
          enabled: true,
          manifest_path: "/tmp/adapter/conversation-adapter.json",
          executable_path: "/tmp/adapter/run",
          content_hash: "exe-hash",
          trusted_hash: "exe-hash",
          trust_state: "trusted",
          protocol_version: 1,
          capabilities: ["read_session", "web_records"],
          input_kinds: ["directory"],
          created_at: "2026-06-15T00:00:00Z",
          updated_at: "2026-06-15T00:00:00Z",
        },
        validation: {},
      })
      .mockImplementationOnce(async (_command, payload) => ({
        dry_run: false,
        source: payload.params.source,
      }))
      .mockResolvedValueOnce({
        id: "sync-1",
        status: "running",
        source_id: "medical-web-export",
        adapter_id: null,
        dry_run: false,
        started_at: "2026-06-15T00:00:00Z",
        finished_at: null,
        result: null,
        error: null,
      });
    const progress: string[] = [];

    await expect(
      importConversationSource(
        {
          manifest_path: "/tmp/adapter/conversation-adapter.json",
          record_kind: "web",
          source_id: "medical-web-export",
          source_kind: "directory",
          source_location: "/tmp/export",
          source_name: "医保网页记录",
        },
        (step) => progress.push(step),
      ),
    ).resolves.toMatchObject({
      source: {
        id: "medical-web-export",
        adapter_id: "medical-web",
        name: "医保网页记录",
      },
      task: { id: "sync-1", status: "running" },
    });

    expect(progress).toEqual(["validating", "source", "sync"]);
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      "validate_conversation_adapter",
      "register_conversation_adapter",
      "upsert_conversation_source",
      "sync_conversations",
    ]);
    expect(invokeMock).toHaveBeenNthCalledWith(2, "register_conversation_adapter", {
      params: {
        dry_run: false,
        manifest_path: "/tmp/adapter/conversation-adapter.json",
        yes: true,
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "upsert_conversation_source", {
      params: {
        dry_run: false,
        source: expect.objectContaining({
          adapter_id: "medical-web",
          id: "medical-web-export",
          kind: "directory",
          location: "/tmp/export",
          name: "医保网页记录",
        }),
      },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(4, "sync_conversations", {
      params: { dry_run: false, source_id: "medical-web-export" },
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
        project_path: "/Users/util6/code-space/assetiweave",
        content_types: ["question", "answer"],
        since: "2026-01-01",
        until: "2026-06-01T00:00:00Z",
        timeline: true,
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
        project_path: "/Users/util6/code-space/assetiweave",
        content_types: ["question", "answer"],
        since: "2026-01-01",
        until: "2026-06-01T00:00:00Z",
        timeline: true,
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
