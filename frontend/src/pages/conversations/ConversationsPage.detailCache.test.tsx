/* @vitest-environment jsdom */

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { I18nProvider } from "../../i18n/I18nProvider";
import { defaultSettings } from "../../store/settings/settingsSchema";
import type {
  ConversationAdapter,
  ConversationSessionDetail,
  ConversationSessionListItem,
} from "../../types";
import { ConversationsPage } from "./ConversationsPage";

const getConversationSessionMock = vi.hoisted(() => vi.fn());
const getWebRecordSessionMock = vi.hoisted(() => vi.fn());
const listConversationAdaptersMock = vi.hoisted(() => vi.fn());
const listConversationSessionsMock = vi.hoisted(() => vi.fn());
const listWebRecordSessionsMock = vi.hoisted(() => vi.fn());
const searchConversationRecordsMock = vi.hoisted(() => vi.fn());
const conversationSyncTaskMock = vi.hoisted(() => ({
  current: null as null | Record<string, unknown>,
}));

vi.mock("../../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({
    startSync: vi.fn().mockResolvedValue(undefined),
    taskFor: (recordKind: "session" | "web") =>
      conversationSyncTaskMock.current?.record_kind === recordKind
        ? conversationSyncTaskMock.current
        : null,
  }),
}));

vi.mock("../../app/backgroundTasks/SearchIndexProvider", () => ({
  useSearchIndex: () => ({
    rebuild: vi.fn(),
    refresh: vi.fn(),
    status: { health: "ready" },
    task: null,
  }),
}));

vi.mock("../../store/settings/useAppSettings", async () => {
  const actual = await vi.importActual<
    typeof import("../../store/settings/useAppSettings")
  >("../../store/settings/useAppSettings");
  return {
    ...actual,
    useAppSettings: () => ({
      resetSettings: vi.fn(),
      retrySave: vi.fn(),
      setColumnLayout: vi.fn(),
      setColumnLayoutAsync: vi.fn().mockResolvedValue(undefined),
      settings: defaultSettings,
      settingsError: null,
      settingsLoaded: true,
      storageInfo: {},
      updateSetting: vi.fn(),
    }),
  };
});

vi.mock("../../services/conversations", async () => {
  const actual = await vi.importActual<
    typeof import("../../services/conversations")
  >("../../services/conversations");
  return {
    ...actual,
    getConversationSession: getConversationSessionMock,
    getWebRecordSession: getWebRecordSessionMock,
    listConversationAdapters: listConversationAdaptersMock,
    listConversationSessions: listConversationSessionsMock,
    listWebRecordSessions: listWebRecordSessionsMock,
    searchConversationRecords: searchConversationRecordsMock,
  };
});

let testQueryClient: QueryClient;
let onNotifyError: ReturnType<typeof vi.fn<(message: string) => void>>;

describe("ConversationsPage detail query cache", () => {
  beforeEach(() => {
    // 使用真实的缓存新鲜度窗口，缓存命中才有意义（沿用生产默认的 60s）。
    testQueryClient = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: 60_000,
        },
      },
    });
    onNotifyError = vi.fn();
    conversationSyncTaskMock.current = null;
    window.scrollTo = vi.fn();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        disconnect() {}
        observe() {}
        unobserve() {}
      },
    );
    listConversationAdaptersMock.mockReset().mockResolvedValue([adapter]);
    listConversationSessionsMock
      .mockReset()
      .mockResolvedValue([sessionA, sessionB]);
    listWebRecordSessionsMock.mockReset().mockResolvedValue([]);
    searchConversationRecordsMock.mockReset().mockResolvedValue({
      hits: [],
      query: "",
      record_kind: "session",
      scope: {},
      total_count: 0,
    });
    getConversationSessionMock
      .mockReset()
      .mockImplementation((sessionId: string) =>
        Promise.resolve(detailFor(sessionId)),
      );
    getWebRecordSessionMock
      .mockReset()
      .mockImplementation((sessionId: string) =>
        Promise.resolve(detailFor(sessionId)),
      );
  });

  afterEach(() => {
    cleanup();
    testQueryClient.clear();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("reuses the cached detail without a second request when reopening the same session", async () => {
    const view = renderPage("session");

    fireEvent.click(
      await screen.findByRole("button", { name: "Open session First target" }),
    );
    expect(
      (await screen.findAllByText("Question first")).length,
    ).toBeGreaterThan(0);
    expect(getConversationSessionMock).toHaveBeenCalledTimes(1);

    fireEvent.click(
      screen.getByRole("button", { name: "Back to apps / sessions" }),
    );

    fireEvent.click(
      await screen.findByRole("button", { name: "Open session First target" }),
    );

    expect(screen.getAllByText("Question first").length).toBeGreaterThan(0);
    expect(screen.queryByText("Loading session...")).toBeNull();
    expect(getConversationSessionMock).toHaveBeenCalledTimes(1);
    view.unmount();
  });

  it("keeps showing the newly selected session when an older detail response lands late", async () => {
    const firstLoad = createDeferred<ConversationSessionDetail>();
    const secondLoad = createDeferred<ConversationSessionDetail>();
    getConversationSessionMock.mockImplementation((sessionId: string) =>
      sessionId === sessionA.id ? firstLoad.promise : secondLoad.promise,
    );

    renderPage("session");

    fireEvent.click(
      await screen.findByRole("button", { name: "Open session First target" }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Back to apps / sessions" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "Open session Second target" }),
    );

    await act(async () => {
      firstLoad.resolve(detailFor(sessionA.id));
    });
    expect(screen.queryByText("Question first")).toBeNull();

    await act(async () => {
      secondLoad.resolve(detailFor(sessionB.id));
    });
    expect(
      (await screen.findAllByText("Question second")).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText("Question first")).toBeNull();
  });

  it("refetches the open session detail after a background sync completes", async () => {
    const view = renderPage("session");

    fireEvent.click(
      await screen.findByRole("button", { name: "Open session First target" }),
    );
    expect(
      (await screen.findAllByText("Question first")).length,
    ).toBeGreaterThan(0);
    expect(getConversationSessionMock).toHaveBeenCalledTimes(1);

    act(() => {
      conversationSyncTaskMock.current = {
        adapter_id: null,
        dry_run: false,
        error: null,
        finished_at: "2026-06-15T00:00:05Z",
        id: "sync-after-open",
        record_kind: "session",
        result: { errors: [], results: [] },
        source_id: null,
        started_at: "2026-06-15T00:00:00Z",
        status: "completed",
      };
    });
    view.rerenderPage();

    await waitFor(() =>
      expect(getConversationSessionMock).toHaveBeenCalledTimes(2),
    );
    // 后台刷新期间详情继续展示缓存内容，不退化成骨架屏。
    expect(screen.getAllByText("Question first").length).toBeGreaterThan(0);
  });

  it("reports a detail load failure only once across re-renders", async () => {
    getConversationSessionMock.mockImplementation(() =>
      Promise.reject(new Error("detail unavailable")),
    );

    const view = renderPage("session");

    fireEvent.click(
      await screen.findByRole("button", { name: "Open session First target" }),
    );

    await waitFor(() =>
      expect(onNotifyError).toHaveBeenCalledWith("detail unavailable"),
    );
    expect(onNotifyError).toHaveBeenCalledTimes(1);

    view.rerenderPage();
    await act(async () => undefined);
    expect(onNotifyError).toHaveBeenCalledTimes(1);
  });
});

function renderPage(recordKind: "session" | "web") {
  function element() {
    return (
      <QueryClientProvider client={testQueryClient}>
        <I18nProvider>
          <ConversationsPage
            appShortcuts={[]}
            onManualOpen={vi.fn()}
            onNotify={vi.fn()}
            onNotifyError={onNotifyError}
            onOpenSettings={vi.fn()}
            recordKind={recordKind}
          />
        </I18nProvider>
      </QueryClientProvider>
    );
  }

  const view = render(element());
  return {
    ...view,
    rerenderPage: () => view.rerender(element()),
  };
}

function createDeferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

const adapter: ConversationAdapter = {
  capabilities: ["read_session"],
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

function sessionListItem(
  id: string,
  title: string,
): ConversationSessionListItem {
  return {
    adapter_id: "codex",
    created_at: "2026-06-15T00:00:00Z",
    external_id: id,
    id,
    imported_at: "2026-06-15T00:00:00Z",
    missing: false,
    project_path: "/Users/util6/code-space/assetiweave",
    question_count: 1,
    source_id: "codex-live",
    title,
    turn_count: 1,
  };
}

const sessionA = sessionListItem("session-first", "First target");
const sessionB = sessionListItem("session-second", "Second target");

function detailFor(sessionId: string): ConversationSessionDetail {
  const session = sessionId === sessionB.id ? sessionB : sessionA;
  const suffix = session === sessionB ? "second" : "first";
  return {
    questions: [
      {
        parts: [
          {
            content_card: {
              kind: "answer",
              renderer: "markdown",
              schema_version: 1,
            },
            id: `part-${suffix}`,
            kind: "text",
            part_index: 0,
            role: "assistant",
            text: `${suffix} answer`,
            turn_id: `turn-${suffix}`,
          },
        ],
        projected_content_nodes: [
          {
            locator: {
              node_order: 0,
              part_id: `part-${suffix}`,
              question_id: `question-${suffix}`,
              turn_id: `turn-${suffix}`,
            },
            node_id: `part-${suffix}-node-0`,
            node_order: 0,
            node_type: "answer",
            part_id: `part-${suffix}`,
            part_order: 0,
            question_id: `question-${suffix}`,
            turn_id: `turn-${suffix}`,
            turn_order: 0,
            semantic_role: "answer",
            renderer: "markdown",
            role: "assistant",
            content: `${suffix} answer`,
            language: null,
            cwd: null,
            status: null,
            exit_code: null,
            source_execution_id: null,
            command_label: null,
            translated_content: null,
            legacy_anchor_ids: [],
          },
        ],
        question: {
          created_at: "2026-06-15T00:00:00Z",
          id: `question-${suffix}`,
          session_id: session.id,
          title: `Question ${suffix}`,
          updated_at: "2026-06-15T00:00:00Z",
        },
        question_turns: [
          {
            assigned_at: "2026-06-15T00:00:00Z",
            assignment_origin: "imported",
            question_id: `question-${suffix}`,
            turn_id: `turn-${suffix}`,
            turn_order: 0,
            updated_at: "2026-06-15T00:00:00Z",
          },
        ],
        turns: [
          {
            id: `turn-${suffix}`,
            imported_at: "2026-06-15T00:00:00Z",
            session_id: session.id,
            started_at: "2026-06-15T00:00:00Z",
            turn_index: 0,
            prompt: `Prompt ${suffix}`,
          },
        ],
      },
    ],
    session,
  } as ConversationSessionDetail;
}
