/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { defaultSettings } from "../../store/settings/settingsSchema";
import { ConversationsPage } from "./ConversationsPage";

const startSyncMock = vi.hoisted(() => vi.fn());
const listConversationAdaptersMock = vi.hoisted(() => vi.fn());
const listConversationSessionsMock = vi.hoisted(() => vi.fn());
const listWebRecordSessionsMock = vi.hoisted(() => vi.fn());
const conversationSyncTaskMock = vi.hoisted(() => ({ current: null as null | Record<string, unknown> }));

vi.mock("../../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({
    startSync: startSyncMock,
    task: conversationSyncTaskMock.current,
  }),
}));

vi.mock("../../store/settings/AppSettingsProvider", async () => {
  const actual = await vi.importActual<typeof import("../../store/settings/AppSettingsProvider")>(
    "../../store/settings/AppSettingsProvider",
  );
  return {
    ...actual,
    useAppSettings: () => ({
      resetSettings: vi.fn(),
      settings: defaultSettings,
      settingsError: null,
      settingsLoaded: true,
      storageInfo: {},
      updateSetting: vi.fn(),
    }),
  };
});

vi.mock("../../services/conversations", async () => {
  const actual = await vi.importActual<typeof import("../../services/conversations")>(
    "../../services/conversations",
  );
  return {
    ...actual,
    listConversationAdapters: listConversationAdaptersMock,
    listConversationSessions: listConversationSessionsMock,
    listWebRecordSessions: listWebRecordSessionsMock,
  };
});

describe("ConversationsPage sync scope", () => {
  beforeEach(() => {
    conversationSyncTaskMock.current = null;
    window.scrollTo = vi.fn();
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    startSyncMock.mockReset().mockResolvedValue({
      adapter_id: null,
      dry_run: false,
      error: null,
      finished_at: null,
      id: "sync-1",
      result: null,
      source_id: null,
      started_at: "2026-06-15T00:00:00Z",
      status: "running",
    });
    listConversationAdaptersMock.mockReset().mockResolvedValue([]);
    listConversationSessionsMock.mockReset().mockResolvedValue([]);
    listWebRecordSessionsMock.mockReset().mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("syncs only conversation sessions from the conversations page", async () => {
    renderConversationsPage("session");

    fireEvent.click(screen.getByRole("button", { name: "Sync" }));

    await waitFor(() =>
      expect(startSyncMock).toHaveBeenCalledWith({
        dry_run: false,
        record_kind: "session",
        source_id: null,
      }),
    );
  });

  it("syncs only web records from the web records page", async () => {
    renderConversationsPage("web");

    fireEvent.click(screen.getByRole("button", { name: "Sync" }));

    await waitFor(() =>
      expect(startSyncMock).toHaveBeenCalledWith({
        dry_run: false,
        record_kind: "web",
        source_id: null,
      }),
    );
  });

  it("clears session sync progress when switching to web records", async () => {
    const view = renderConversationsPage("session");

    fireEvent.click(screen.getByRole("button", { name: "Sync" }));

    expect(await screen.findByText("Reading and importing conversations")).toBeTruthy();

    view.rerender(
      <I18nProvider>
        <ConversationsPage
          appShortcuts={[]}
          onManualOpen={vi.fn()}
          onNotifyError={vi.fn()}
          onOpenSettings={vi.fn()}
          recordKind="web"
        />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.queryByText("Reading and importing conversations")).toBeNull();
    });
  });

  it("does not leave a non-dismissible sync summary after the completed progress is dismissed", async () => {
    const summary = "Added/updated 1 web records and 3 content items, skipped 0 unchanged records across 1 sources.";
    conversationSyncTaskMock.current = {
      adapter_id: null,
      dry_run: false,
      error: null,
      finished_at: "2026-06-15T00:00:05Z",
      id: "sync-completed",
      record_kind: "web",
      result: {
        errors: [],
        results: [
          {
            adapter_id: "chatgpt-web",
            record_kind: "web",
            session_count: 1,
            skipped_session_count: 0,
            source_id: "chatgpt-web-export",
            turn_count: 3,
            warning_count: 0,
          },
        ],
      },
      source_id: null,
      started_at: "2026-06-15T00:00:00Z",
      status: "completed",
    };

    renderConversationsPage("web");

    expect(await screen.findByText("Web record sync completed")).toBeTruthy();
    expect(screen.getAllByText(summary)).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "Dismiss sync progress" }));

    await waitFor(() => {
      expect(screen.queryByText(summary)).toBeNull();
    });
  });

  it("keeps completed sync progress dismissed after leaving and returning to the page", async () => {
    conversationSyncTaskMock.current = {
      adapter_id: null,
      dry_run: false,
      error: null,
      finished_at: "2026-06-15T00:00:05Z",
      id: "sync-completed-return",
      record_kind: "web",
      result: {
        errors: [],
        results: [
          {
            adapter_id: "chatgpt-web",
            record_kind: "web",
            session_count: 1,
            skipped_session_count: 0,
            source_id: "chatgpt-web-export",
            turn_count: 3,
            warning_count: 0,
          },
        ],
      },
      source_id: null,
      started_at: "2026-06-15T00:00:00Z",
      status: "completed",
    };

    const view = renderConversationsPage("web");

    expect(await screen.findByText("Web record sync completed")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Dismiss sync progress" }));

    await waitFor(() => {
      expect(screen.queryByText("Web record sync completed")).toBeNull();
    });

    view.unmount();
    renderConversationsPage("web");

    await expect(
      screen.findByText("Web record sync completed", {}, { timeout: 200 }),
    ).rejects.toThrow();
  });
});

function renderConversationsPage(recordKind: "session" | "web") {
  return render(
    <I18nProvider>
      <ConversationsPage
        appShortcuts={[]}
        onManualOpen={vi.fn()}
        onNotifyError={vi.fn()}
        onOpenSettings={vi.fn()}
        recordKind={recordKind}
      />
    </I18nProvider>,
  );
}
