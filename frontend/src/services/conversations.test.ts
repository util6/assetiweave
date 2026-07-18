import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  checkConversationAdapterPackageUpdates,
  getConversationAdapterPackageTask,
  getConversationSyncTask,
  installConversationAdapterPackage,
  installConversationScript,
  importConversationSource,
  deleteConversationAdapterPackageVersion,
  listInstalledConversationAdapterPackageVersions,
  listConversationAdapterPackages,
  listConversationAdapterPackageReleases,
  listConversationScriptCatalog,
  listConversationAdapterRuntimeStatuses,
  mergeConversationQuestions,
  prepareConversationAdapterPackageChange,
  rollbackConversationAdapterPackageVersion,
  setConversationAdapterPackageUpdatePolicy,
  switchConversationAdapterPackageVersion,
  searchConversationRecords,
  summarizeConversationSyncTask,
  syncConversations,
  uninstallConversationAdapterPackage,
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

  it("reports preview runtime diagnostics without calling Tauri", async () => {
    vi.stubGlobal("window", {});

    await expect(listConversationAdapterRuntimeStatuses()).resolves.toEqual([
      expect.objectContaining({ available: true, kind: "node", program: "node", required_version: ">=20" }),
      expect.objectContaining({
        available: false,
        hint: expect.stringContaining("Python 3.10"),
        kind: "python",
        program: "python3",
      }),
      expect.objectContaining({ available: true, kind: "bash", program: "bash" }),
    ]);
    expect(invokeMock).not.toHaveBeenCalled();
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

  it("loads the conversation script catalog with an optional catalog URL", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce([]);

    await expect(listConversationScriptCatalog("https://example.test/catalog.json")).resolves.toEqual([]);

    expect(invokeMock).toHaveBeenCalledWith("list_conversation_script_catalog", {
      params: { catalog_url: "https://example.test/catalog.json" },
    });
  });

  it("loads conversation adapter packages with an optional catalog URL", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce([]);

    await expect(listConversationAdapterPackages("https://example.test/catalog.json")).resolves.toEqual([]);

    expect(invokeMock).toHaveBeenCalledWith("list_conversation_adapter_packages", {
      params: { catalog_url: "https://example.test/catalog.json" },
    });
  });

  it("includes explicit version direction in preview package entries", async () => {
    vi.stubGlobal("window", {});
    invokeMock.mockRejectedValueOnce(new Error("preview backend missing"));

    const entries = await listConversationAdapterPackages();
    const zcodeEntry = entries.find((entry) => entry.item.adapter_id === "zcode");

    expect(entries.length).toBeGreaterThan(0);
    expect(entries.every((entry) => typeof entry.ahead_of_release === "boolean")).toBe(true);
    expect(entries.every((entry) => !(entry.update_available && entry.ahead_of_release))).toBe(true);
    expect(zcodeEntry).toMatchObject({
      ahead_of_release: true,
      status: "ahead_of_release",
      update_available: false,
    });
  });

  it("routes installed-version lifecycle operations through the shared package API", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValue([]);

    await listInstalledConversationAdapterPackageVersions("io.example.adapter");
    await switchConversationAdapterPackageVersion({ packageId: "io.example.adapter", version: "1.2.0", confirmed: true });
    await rollbackConversationAdapterPackageVersion({ packageId: "io.example.adapter", confirmed: true });
    await deleteConversationAdapterPackageVersion({ packageId: "io.example.adapter", version: "1.1.0", confirmed: true });

    expect(invokeMock).toHaveBeenNthCalledWith(1, "list_installed_conversation_adapter_package_versions", {
      params: { package_id: "io.example.adapter" },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "switch_conversation_adapter_package_version", {
      params: { package_id: "io.example.adapter", version: "1.2.0", dry_run: false, yes: true },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(3, "rollback_conversation_adapter_package_version", {
      params: { package_id: "io.example.adapter", version: null, dry_run: false, yes: true },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(4, "delete_conversation_adapter_package_version", {
      params: { package_id: "io.example.adapter", version: "1.1.0", dry_run: false, yes: true },
    });
  });

  it("persists the selected package update policy", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({ package_id: "io.example.adapter", update_policy: "follow_beta" });

    await setConversationAdapterPackageUpdatePolicy({
      packageId: "io.example.adapter",
      updatePolicy: "follow_beta",
    });

    expect(invokeMock).toHaveBeenCalledWith("set_conversation_adapter_package_update_policy", {
      params: { package_id: "io.example.adapter", update_policy: "follow_beta" },
    });
  });

  it("checks updates explicitly and uninstalls only the registered runtime", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce([]).mockResolvedValueOnce({ status: "running" });

    await checkConversationAdapterPackageUpdates({ force: true });
    await uninstallConversationAdapterPackage({
      packageId: "io.example.adapter",
      confirmed: true,
    });

    expect(invokeMock).toHaveBeenNthCalledWith(1, "check_conversation_adapter_package_updates", {
      params: { catalog_url: null, force: true },
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "uninstall_conversation_adapter_package", {
      params: { package_id: "io.example.adapter", dry_run: false, yes: true },
    });
  });

  it("returns the bundled script catalog fallback for browser previews", async () => {
    vi.stubGlobal("window", {});
    invokeMock.mockRejectedValueOnce(new Error("preview backend missing"));

    const entries = await listConversationScriptCatalog();

    expect(entries.map((entry) => entry.item.id)).toEqual([
      "codex-session",
      "opencode-session",
      "claude-code-session",
      "zcode-session",
      "chatgpt-web",
      "qwen-web",
      "gemini-web",
    ]);
    expect(entries.filter((entry) => entry.item.record_kind === "web").map((entry) => entry.item.adapter_id)).toEqual([
      "chatgpt-web",
      "qwen-web",
      "gemini-web",
    ]);
  });

  it("starts conversation script installs as background tasks", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({
      id: "install-1",
      status: "running",
      item_id: "codex-session",
      catalog_url: null,
      dry_run: false,
      started_at: "2026-06-28T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    });

    await expect(installConversationScript({ itemId: "codex-session" })).resolves.toMatchObject({
      id: "install-1",
      status: "running",
    });

    expect(invokeMock).toHaveBeenCalledWith("install_conversation_script", {
      params: {
        catalog_url: null,
        dry_run: false,
        item_id: "codex-session",
        yes: true,
      },
    });
  });

  it("starts conversation adapter package installs as background tasks", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({
      id: "install-1",
      status: "running",
      item_id: "codex-session",
      package_id: "codex-session",
      catalog_url: null,
      dry_run: false,
      phase: "installing",
      started_at: "2026-06-28T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    });

    await expect(installConversationAdapterPackage({
      packageId: "codex-session",
      confirmed: true,
    })).resolves.toMatchObject({
      id: "install-1",
      package_id: "codex-session",
      status: "running",
    });

    expect(invokeMock).toHaveBeenCalledWith("install_conversation_adapter_package", {
      params: {
        catalog_url: null,
        dry_run: false,
        package_id: "codex-session",
        version: null,
        yes: true,
      },
    });
  });

  it("preflights package changes without auto-confirming them", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce({
      action: "install",
      origin: "managed_release",
      package_id: "codex-session",
      adapter_id: null,
      managed_paths: [],
      affected_sources: [],
      task_conflicts: [],
      preserves_conversation_records: true,
      risk: "high_risk_write",
      confirmation_required: true,
    });

    await prepareConversationAdapterPackageChange({
      action: "install",
      packageId: "codex-session",
    });

    expect(invokeMock).toHaveBeenCalledWith("prepare_conversation_adapter_package_change", {
      params: {
        action: "install",
        adapter_id: null,
        package_id: "codex-session",
      },
    });
  });

  it("lists Catalog v2 releases for an exact package", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce([]);

    await listConversationAdapterPackageReleases({
      packageId: "io.github.util6.codex-session",
      refresh: true,
    });

    expect(invokeMock).toHaveBeenCalledWith("list_conversation_adapter_package_releases", {
      params: {
        catalog_url: null,
        package_id: "io.github.util6.codex-session",
        refresh: true,
      },
    });
  });

  it("polls the conversation adapter package task", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValueOnce(null);

    await expect(getConversationAdapterPackageTask()).resolves.toBeNull();

    expect(invokeMock).toHaveBeenCalledWith("get_conversation_adapter_package_task");
  });

  it("imports a conversation source by validating the adapter, adding the source, then starting background sync", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock
      .mockResolvedValueOnce({
        valid: true,
        manifest_path: "/tmp/adapter/conversation-adapter.json",
        content_hash: "adapter-content-hash",
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
          content_hash: "adapter-content-hash",
          trusted_hash: "adapter-content-hash",
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
        yes: false,
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
      params: { dry_run: false, record_kind: "web", source_id: "medical-web-export" },
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
      incrementalStatsAvailable: false,
      discoveredSessionCount: 0,
      changedSessionCount: 5,
      skippedSessionCount: 7,
      retainedSessionCount: 0,
      turnCount: 18,
      warningCount: 1,
      errorCount: 1,
    });
  });

  it("summarizes incremental discovery, active, skipped, and retained sessions", () => {
    const summary = summarizeConversationSyncTask({
      id: "sync-incremental",
      status: "completed",
      source_id: null,
      adapter_id: null,
      dry_run: false,
      started_at: "2026-07-16T00:00:00Z",
      finished_at: "2026-07-16T00:00:01Z",
      result: {
        results: [{
          incremental: true,
          session_count: 20,
          active_session_count: 2,
          skipped_session_count: 18,
          retained_session_count: 3,
          turn_count: 4,
          warning_count: 0,
        }],
        errors: [],
      },
      error: null,
    });

    expect(summary).toMatchObject({
      incrementalStatsAvailable: true,
      discoveredSessionCount: 20,
      changedSessionCount: 2,
      skippedSessionCount: 18,
      retainedSessionCount: 3,
    });
  });
});
