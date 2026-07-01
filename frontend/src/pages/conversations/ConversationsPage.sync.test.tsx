/* @vitest-environment jsdom */

import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { defaultSettings } from "../../store/settings/settingsSchema";
import type { ConversationAdapter, ConversationSessionDetail, ConversationSessionListItem } from "../../types";
import { ConversationsPage } from "./ConversationsPage";

type ConversationNotify = ComponentProps<typeof ConversationsPage>["onNotify"];

const startSyncMock = vi.hoisted(() => vi.fn());
const exportConversationSessionMock = vi.hoisted(() => vi.fn());
const getConversationSessionMock = vi.hoisted(() => vi.fn());
const listConversationAdaptersMock = vi.hoisted(() => vi.fn());
const listConversationSessionsMock = vi.hoisted(() => vi.fn());
const listWebRecordSessionsMock = vi.hoisted(() => vi.fn());
const searchConversationRecordsMock = vi.hoisted(() => vi.fn());
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
    exportConversationSession: exportConversationSessionMock,
    getConversationSession: getConversationSessionMock,
    listConversationAdapters: listConversationAdaptersMock,
    listConversationSessions: listConversationSessionsMock,
    listWebRecordSessions: listWebRecordSessionsMock,
    searchConversationRecords: searchConversationRecordsMock,
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
    exportConversationSessionMock.mockReset().mockResolvedValue({
      dry_run: false,
      output_path: "/tmp/export/export-target.md",
      question_ids: [],
      session_id: "session-export-target",
    });
    getConversationSessionMock.mockReset().mockResolvedValue(conversationSessionDetail);
    listConversationAdaptersMock.mockReset().mockResolvedValue([]);
    listConversationSessionsMock.mockReset().mockResolvedValue([]);
    listWebRecordSessionsMock.mockReset().mockResolvedValue([]);
    searchConversationRecordsMock.mockReset().mockResolvedValue({
      hits: [],
      query: "deploy",
      record_kind: "session",
      scope: {
        adapter_id: null,
        content_types: ["question", "answer", "tool", "command", "code", "result"],
        limit: 50,
        offset: 0,
        project_path: null,
        query: "deploy",
        record_kind: "session",
        since: null,
        timeline: false,
        until: null,
      },
      total_count: 0,
    });
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

  it("uses the notification outlet instead of an inline status report after exporting from detail view", async () => {
    const onNotify = vi.fn((_: Parameters<ConversationNotify>[0]) => undefined);
    listConversationAdaptersMock.mockResolvedValue([conversationAdapter]);
    listConversationSessionsMock.mockResolvedValue([conversationSession]);

    renderConversationsPage("session", { onNotify });

    fireEvent.click(await screen.findByRole("button", { name: "Open session Export target" }));
    fireEvent.click(await screen.findByRole("button", { name: "Export Markdown" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm export" }));

    await waitFor(() =>
      expect(exportConversationSessionMock).toHaveBeenCalledWith(
        "session-export-target",
        "~/Desktop/assetiweave-conversations",
        false,
        [],
        {
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        },
      ),
    );
    expect(onNotify).toHaveBeenCalledWith({
      messageKey: "conversation.status.exported",
      tone: "success",
    });
    expect(screen.queryByText("Exported session Markdown")).toBeNull();
  });

  it.each(["session", "web"] as const)(
    "coalesces content card typing before searching on %s pages",
    async (recordKind) => {
      vi.useFakeTimers();
      try {
        renderConversationsPage(recordKind);

        const searchInput = screen.getByPlaceholderText("Search content and jump to cards...") as HTMLInputElement;
        fireEvent.change(searchInput, { target: { value: "d" } });
        await act(async () => {
          await vi.advanceTimersByTimeAsync(300);
        });
        fireEvent.change(searchInput, { target: { value: "de" } });
        await act(async () => {
          await vi.advanceTimersByTimeAsync(300);
        });
        fireEvent.change(searchInput, { target: { value: "deploy" } });

        expect(searchInput.value).toBe("deploy");
        expect(screen.queryByText("Searching content...")).toBeNull();
        expect(searchConversationRecordsMock).not.toHaveBeenCalled();

        await act(async () => {
          await vi.advanceTimersByTimeAsync(699);
        });

        expect(searchConversationRecordsMock).not.toHaveBeenCalled();

        await act(async () => {
          await vi.advanceTimersByTimeAsync(1);
        });

        expect(searchConversationRecordsMock).toHaveBeenCalledWith({
          content_types: ["question", "answer", "tool", "command", "code", "result"],
          limit: 50,
          query: "deploy",
          record_kind: recordKind,
        });
      } finally {
        vi.useRealTimers();
      }
    },
  );

  it("submits content card search immediately from Enter or the search button", async () => {
    vi.useFakeTimers();
    try {
      renderConversationsPage("session");

      const searchInput = screen.getByPlaceholderText("Search content and jump to cards...") as HTMLInputElement;
      fireEvent.change(searchInput, { target: { value: "deploy" } });
      fireEvent.keyDown(searchInput, { key: "Enter" });

      await act(async () => undefined);

      expect(searchConversationRecordsMock).toHaveBeenCalledWith({
        content_types: ["question", "answer", "tool", "command", "code", "result"],
        limit: 50,
        query: "deploy",
        record_kind: "session",
      });

      await act(async () => {
        await vi.advanceTimersByTimeAsync(700);
      });

      expect(searchConversationRecordsMock).toHaveBeenCalledTimes(1);
      searchConversationRecordsMock.mockClear();

      fireEvent.change(searchInput, { target: { value: "rollback" } });
      fireEvent.click(screen.getByRole("button", { name: "Search content" }));

      await act(async () => undefined);

      expect(searchConversationRecordsMock).toHaveBeenCalledWith({
        content_types: ["question", "answer", "tool", "command", "code", "result"],
        limit: 50,
        query: "rollback",
        record_kind: "session",
      });
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows explicit progress while content card search is running", async () => {
    let resolveSearch: (value: Awaited<ReturnType<typeof searchConversationRecordsMock>>) => void;
    searchConversationRecordsMock.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveSearch = resolve;
      }),
    );

    renderConversationsPage("session");

    const searchInput = screen.getByPlaceholderText("Search content and jump to cards...") as HTMLInputElement;
    fireEvent.change(searchInput, { target: { value: "deploy" } });
    fireEvent.keyDown(searchInput, { key: "Enter" });

    expect(await screen.findByRole("progressbar", { name: "Searching content..." })).toBeTruthy();

    resolveSearch!({
      hits: [],
      query: "deploy",
      record_kind: "session",
      scope: {
        adapter_id: null,
        content_types: ["question", "answer", "tool", "command", "code", "result"],
        limit: 50,
        offset: 0,
        project_path: null,
        query: "deploy",
        record_kind: "session",
        since: null,
        timeline: false,
        until: null,
      },
      total_count: 0,
    });

    await waitFor(() =>
      expect(screen.queryByRole("progressbar", { name: "Searching content..." })).toBeNull(),
    );
  });

  it("groups search hits by card type and filters multiple result types from the header", async () => {
    searchConversationRecordsMock.mockResolvedValueOnce({
      hits: [
        searchHit("command-hit", "command", "command match"),
        searchHit("answer-hit-1", "answer", "answer match one"),
        searchHit("answer-hit-2", "answer", "answer match two"),
      ],
      query: "deploy",
      record_kind: "session",
      scope: searchScope("deploy", ["question", "answer", "tool", "command", "code", "result"]),
      total_count: 3,
    });

    renderConversationsPage("session");

    const searchInput = screen.getByPlaceholderText("Search content and jump to cards...") as HTMLInputElement;
    fireEvent.change(searchInput, { target: { value: "deploy" } });
    fireEvent.keyDown(searchInput, { key: "Enter" });

    const answerOne = await screen.findByText("answer match one");
    const answerTwo = screen.getByText("answer match two");
    const command = screen.getByText("command match");

    expect(answerOne.compareDocumentPosition(command) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(answerTwo.compareDocumentPosition(command) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    const commandBadge = document.querySelector('[data-search-card-type-badge="command"]');
    expect(commandBadge?.getAttribute("style")).toContain("rgb(208, 138, 25)");

    let resolveCommandSearch: (value: Awaited<ReturnType<typeof searchConversationRecordsMock>>) => void;
    searchConversationRecordsMock.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveCommandSearch = resolve;
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Commands" }));

    await waitFor(() =>
      expect(searchConversationRecordsMock).toHaveBeenLastCalledWith({
        content_types: ["command"],
        limit: 50,
        query: "deploy",
        record_kind: "session",
      }),
    );
    expect(screen.queryByText("answer match one")).toBeNull();
    expect(screen.getByText("command match")).toBeTruthy();

    let resolveCombinedSearch: (value: Awaited<ReturnType<typeof searchConversationRecordsMock>>) => void;
    searchConversationRecordsMock.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveCombinedSearch = resolve;
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "Answer text" }));

    await waitFor(() =>
      expect(searchConversationRecordsMock).toHaveBeenLastCalledWith({
        content_types: ["answer", "command"],
        limit: 50,
        query: "deploy",
        record_kind: "session",
      }),
    );
    expect(screen.getByText("answer match one")).toBeTruthy();
    expect(screen.getByText("command match")).toBeTruthy();

    resolveCommandSearch!({
      hits: [searchHit("command-hit", "command", "command match")],
      query: "deploy",
      record_kind: "session",
      scope: searchScope("deploy", ["command"]),
      total_count: 1,
    });
    resolveCombinedSearch!({
      hits: [
        searchHit("answer-hit-1", "answer", "answer match one"),
        searchHit("command-hit", "command", "command match"),
      ],
      query: "deploy",
      record_kind: "session",
      scope: searchScope("deploy", ["answer", "command"]),
      total_count: 2,
    });

    await waitFor(() => {
      expect(screen.getByText("answer match one")).toBeTruthy();
    });
    expect(screen.getByText("command match")).toBeTruthy();
  });

  it.each(["session", "web"] as const)(
    "waits for IME composition to finish before searching content cards on %s pages",
    async (recordKind) => {
      vi.useFakeTimers();
      try {
        renderConversationsPage(recordKind);

        const searchInput = screen.getByPlaceholderText("Search content and jump to cards...") as HTMLInputElement;
        fireEvent.compositionStart(searchInput);
        fireEvent.change(searchInput, {
          target: { value: "zhong" },
          nativeEvent: { isComposing: true },
        });

        expect(searchInput.value).toBe("zhong");

        await act(async () => {
          await vi.advanceTimersByTimeAsync(1000);
        });

        expect(screen.queryByText("Searching content...")).toBeNull();
        expect(searchConversationRecordsMock).not.toHaveBeenCalled();

        fireEvent.compositionEnd(searchInput, {
          data: "中",
          target: { value: "中" },
        });

        expect(searchInput.value).toBe("中");

        await act(async () => {
          await vi.advanceTimersByTimeAsync(700);
        });

        expect(searchConversationRecordsMock).toHaveBeenCalledWith({
          content_types: ["question", "answer", "tool", "command", "code", "result"],
          limit: 50,
          query: "中",
          record_kind: recordKind,
        });
      } finally {
        vi.useRealTimers();
      }
    },
  );

  it("clears session sync progress when switching to web records", async () => {
    const view = renderConversationsPage("session");

    fireEvent.click(screen.getByRole("button", { name: "Sync" }));

    expect(await screen.findByText("Reading and importing conversations")).toBeTruthy();

    view.rerender(
      <I18nProvider>
        <ConversationsPage
          appShortcuts={[]}
          onManualOpen={vi.fn()}
          onNotify={() => undefined}
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

  it("shows usage guidance when a completed web sync has failed sources", async () => {
    conversationSyncTaskMock.current = {
      adapter_id: null,
      dry_run: false,
      error: null,
      finished_at: "2026-06-15T00:00:05Z",
      id: "sync-completed-with-errors",
      record_kind: "web",
      result: {
        errors: [
          {
            adapter_id: "gemini-web",
            message: "Gemini web CSRF token SNlM0e was not found",
            source_id: "gemini-web-export",
          },
        ],
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
    expect(
      screen.getByText(
        "Some web sources failed even though the sync task completed. Successful sources were imported; refresh browser login or re-trust changed adapters, then sync the failed source again.",
      ),
    ).toBeTruthy();
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

function renderConversationsPage(
  recordKind: "session" | "web",
  options: { onNotify?: ConversationNotify } = {},
) {
  const onNotify = options.onNotify ?? (() => undefined);

  return render(
    <I18nProvider>
      <ConversationsPage
        appShortcuts={[]}
        onManualOpen={vi.fn()}
        onNotify={onNotify}
        onNotifyError={vi.fn()}
        onOpenSettings={vi.fn()}
        recordKind={recordKind}
      />
    </I18nProvider>,
  );
}

function searchScope(
  query: string,
  contentTypes: Array<"question" | "answer" | "tool" | "command" | "code" | "result">,
) {
  return {
    adapter_id: null,
    content_types: contentTypes,
    limit: 50,
    offset: 0,
    project_path: null,
    query,
    record_kind: "session" as const,
    since: null,
    timeline: false,
    until: null,
  };
}

function searchHit(
  id: string,
  cardType: "question" | "answer" | "tool" | "command" | "code" | "result",
  snippet: string,
) {
  return {
    block_id: id,
    card_type: cardType,
    part_id: `${id}-part`,
    question_id: `${id}-question`,
    question_index: 0,
    question_title: `${snippet} question`,
    score: 100,
    session: conversationSession,
    snippet,
    turn_id: `${id}-turn`,
  };
}

const conversationAdapter: ConversationAdapter = {
  capabilities: ["probe", "read_session"],
  content_hash: "adapter-hash",
  created_at: "2026-06-15T00:00:00Z",
  enabled: true,
  executable_path: "/tmp/codex-adapter",
  id: "codex",
  input_kinds: ["directory"],
  kind: "external",
  manifest_path: "/tmp/codex-adapter.json",
  name: "Codex",
  protocol_version: 1,
  trust_state: "trusted",
  trusted_hash: "adapter-hash",
  updated_at: "2026-06-15T00:00:00Z",
  version: "0.1.0",
};

const conversationSession: ConversationSessionListItem = {
  adapter_id: "codex",
  created_at: "2026-06-15T00:00:00Z",
  external_id: "external-export-target",
  id: "session-export-target",
  imported_at: "2026-06-15T00:00:00Z",
  missing: false,
  project_path: "/Users/util6/code-space/assetiweave",
  question_count: 1,
  source_id: "codex-live",
  title: "Export target",
  turn_count: 1,
};

const conversationSessionDetail: ConversationSessionDetail = {
  questions: [
    {
      parts: [
        {
          id: "part-export-target-answer",
          kind: "text",
          metadata_json: JSON.stringify({ content_card: { type: "answer", format: "markdown" } }),
          part_index: 0,
          role: "assistant",
          text: "Export-ready answer.",
          turn_id: "turn-export-target",
        },
      ],
      question: {
        answer_text: "Export-ready answer.",
        code_text: "",
        command_text: "",
        created_at: "2026-06-15T00:00:00Z",
        grouping_origin: "imported",
        id: "question-export-target",
        question_index: 0,
        question_text: "Can this be exported?",
        session_id: "session-export-target",
        title: "Export question",
        updated_at: "2026-06-15T00:00:00Z",
      },
      turns: [
        {
          external_id: "turn-export-target-external",
          fingerprint: "turn-export-target",
          id: "turn-export-target",
          imported_at: "2026-06-15T00:00:00Z",
          missing: false,
          session_id: "session-export-target",
          turn_index: 0,
          user_text: "Can this be exported?",
        },
      ],
    },
  ],
  session: conversationSession,
};
