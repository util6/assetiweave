import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  addConversationEntry,
  getConversationSyncTask,
  listConversationAdapters,
  listWebRecordSessions,
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

  it("shows ChatGPT as a web record fallback source in non-Tauri previews", async () => {
    vi.stubGlobal("window", {});
    invokeMock.mockRejectedValue(new Error("preview backend missing"));

    const adapters = await listConversationAdapters();
    const webSessions = await listWebRecordSessions({});

    expect(adapters.find((adapter) => adapter.id === "chatgpt-web")).toMatchObject({
      name: "ChatGPT Web",
      capabilities: expect.arrayContaining(["web_records"]),
    });
    expect(webSessions.find((session) => session.adapter_id === "chatgpt-web")).toMatchObject({
      source_id: "chatgpt-web-export",
      title: "ChatGPT web conversation",
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

  it("adds a conversation entry through the plugin source command", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({
      dry_run: false,
      record_kind: "web",
      adapter: {
        id: "plugin-web",
        name: "Plugin Web",
        kind: "external",
        version: "0.1.0",
        enabled: true,
        trust_state: "trusted",
        capabilities: ["read_session", "web_records"],
        input_kinds: ["directory"],
        created_at: "2026-06-15T00:00:00Z",
        updated_at: "2026-06-15T00:00:00Z",
      },
      source: {
        id: "plugin-web-export",
        adapter_id: "plugin-web",
        name: "Plugin Web Export",
        kind: "directory",
        location: "/tmp/plugin/export",
        config_json: null,
        enabled: true,
        created_at: "2026-06-15T00:00:00Z",
        updated_at: "2026-06-15T00:00:00Z",
      },
      plugin_directory: "/tmp/assetiweave/conversation-adapters/plugin-web",
      manifest_path: "/tmp/assetiweave/conversation-adapters/plugin-web/conversation-adapter.json",
      sync_result: null,
    });

    await expect(
      addConversationEntry({
        plugin_path: "/tmp/plugin",
        source_id: "plugin-web-export",
        source_name: "Plugin Web Export",
        source_kind: "directory",
        location: "/tmp/plugin/export",
        config_json: null,
        record_kind: "web",
        dry_run: false,
        yes: true,
        sync_after_add: false,
      }),
    ).resolves.toMatchObject({
      adapter: { id: "plugin-web" },
      source: { id: "plugin-web-export" },
    });
    expect(invokeMock).toHaveBeenCalledWith("add_conversation_entry", {
      params: {
        plugin_path: "/tmp/plugin",
        source_id: "plugin-web-export",
        source_name: "Plugin Web Export",
        source_kind: "directory",
        location: "/tmp/plugin/export",
        config_json: null,
        record_kind: "web",
        dry_run: false,
        yes: true,
        sync_after_add: false,
      },
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

  it("does not create synthetic search cards in non-Tauri fallback search", async () => {
    vi.stubGlobal("window", {});
    invokeMock.mockRejectedValueOnce(new Error("preview backend missing"));

    await expect(
      searchConversationRecords({
        query: "codex-live",
        content_types: ["command", "result", "code"],
      }),
    ).resolves.toMatchObject({
      total_count: 1,
      hits: [
        {
          block_id: "preview-part-2-command",
          card_type: "command",
          part_id: "preview-part-2",
        },
      ],
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
