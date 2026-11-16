/* @vitest-environment jsdom */

import {
  act,
  cleanup,
  configure,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { defaultSettings } from "../../store/settings/settingsSchema";
import type {
  ConversationAdapter,
  ConversationQuestionDetail,
  ConversationSessionDetail,
  ConversationSessionListItem,
} from "../../types";
import { ConversationsPage } from "./ConversationsPage";

const getConversationSessionMock = vi.hoisted(() => vi.fn());
const listConversationAdaptersMock = vi.hoisted(() => vi.fn());
const listConversationSessionsMock = vi.hoisted(() => vi.fn());

vi.mock("../../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({ startSync: vi.fn(), taskFor: () => null }),
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
    listConversationAdapters: listConversationAdaptersMock,
    listConversationSessions: listConversationSessionsMock,
  };
});

const now = "2026-11-09T00:00:00Z";
const SESSION_COUNT = 12;

// 整页 + 虚拟化 + 渲染调度器在并行跑多个测试文件时可能超出默认 1s。
configure({ asyncUtilTimeout: 5_000 });
const PARENT_SESSION_ID = "alpha-session-1";
const CHILD_SESSION_ID = "child-session-1";

let testQueryClient: QueryClient;

describe("ConversationsPage returning to the browse view", () => {
  beforeEach(() => {
    testQueryClient = new QueryClient({
      defaultOptions: {
        queries: { gcTime: 0, retry: false, staleTime: 60_000 },
      },
    });
    localStorage.removeItem("assetiweave.locale");
    window.scrollTo = vi.fn();
    stubLayout();
    listConversationAdaptersMock.mockReset().mockResolvedValue([adapter]);
    listConversationSessionsMock
      .mockReset()
      .mockResolvedValue(sessionListItems());
    getConversationSessionMock
      .mockReset()
      .mockImplementation(async (sessionId: string) =>
        sessionDetail(sessionId),
      );
  });

  afterEach(() => {
    cleanup();
    testQueryClient.clear();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.clearAllMocks();
  });

  it("keeps the session list mounted with its scroll offset across the detail round trip", async () => {
    renderPage();

    const firstCard = await screen.findByRole("button", {
      name: "Open session Alpha 01",
    });
    const listSurface = firstCard.closest(
      "[data-render-safe-scroll-surface]",
    ) as HTMLElement;
    expect(listSurface).toBeTruthy();
    listSurface.scrollTop = 400;
    fireEvent.scroll(listSurface, { target: { scrollTop: 400 } });
    const cardAtOffset = await waitFor(() => {
      const cards = within(listSurface).getAllByRole("button", {
        name: /^Open session /,
      });
      expect(cards.length).toBeGreaterThan(0);
      return cards[0]!;
    });

    fireEvent.click(cardAtOffset);
    const backButton = await screen.findByRole("button", {
      name: "Back to apps / sessions",
    });
    fireEvent.click(backButton);

    const returnedCard = await screen.findByRole("button", {
      name: "Open session Alpha 05",
    });
    const returnedSurface = returnedCard.closest(
      "[data-render-safe-scroll-surface]",
    ) as HTMLElement;
    expect(returnedSurface.isConnected).toBe(true);
    expect(returnedSurface).toBe(listSurface);
    expect(returnedSurface.scrollTop).toBe(400);
  });

  it("expands the collapsible header on return and keeps it expanded on a small scroll up", async () => {
    renderPage();

    const firstCard = await screen.findByRole("button", {
      name: "Open session Alpha 01",
    });
    const listSurface = firstCard.closest(
      "[data-render-safe-scroll-surface]",
    ) as HTMLElement;
    const headerShell = document.querySelector(
      ".collapsible-header-shell",
    ) as HTMLElement;

    listSurface.scrollTop = 400;
    fireEvent.scroll(listSurface, { target: { scrollTop: 400 } });
    expect(headerShell.getAttribute("data-collapsed")).toBe("true");

    fireEvent.click(firstCard);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Back to apps / sessions",
      }),
    );

    await waitFor(() =>
      expect(headerShell.getAttribute("data-collapsed")).toBe("false"),
    );

    // 基准必须是列表当前的 400：否则 395 - 0 会被当成向下滚动而再次收起。
    act(() => {
      listSurface.scrollTop = 395;
      fireEvent.scroll(listSurface, { target: { scrollTop: 395 } });
    });
    expect(headerShell.getAttribute("data-collapsed")).toBe("false");
  });

  it("keeps the app and project selection and can reopen the same session after returning", async () => {
    renderPage();

    const betaProject = await screen.findByRole("button", {
      name: "Select project folder /repo/beta",
    });
    fireEvent.click(betaProject);
    expect(betaProject.getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("heading", { name: "/repo/beta" })).toBeTruthy();

    const betaCard = await screen.findByRole("button", {
      name: "Open session Beta 01",
    });
    fireEvent.click(betaCard);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "Back to apps / sessions",
      }),
    );

    expect(
      screen
        .getByRole("button", { name: "Select app Codex" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(
      screen
        .getByRole("button", { name: "Select project folder /repo/beta" })
        .getAttribute("aria-pressed"),
    ).toBe("true");
    expect(screen.getByRole("heading", { name: "/repo/beta" })).toBeTruthy();

    const reopenedCard = await screen.findByRole("button", {
      name: "Open session Beta 01",
    });
    fireEvent.click(reopenedCard);

    expect(
      await screen.findByRole("button", {
        name: "Back to apps / sessions",
      }),
    ).toBeTruthy();
    expect(getConversationSessionMock).toHaveBeenCalledWith("beta-session-1");
  });

  it("returns from a drilled-down subagent session with the list rendered", async () => {
    renderPage();

    const parentCard = await screen.findByRole("button", {
      name: "Open session Alpha 01",
    });
    fireEvent.click(parentCard);

    fireEvent.click(
      (
        await screen.findAllByRole("button", {
          name: /^View subagent session/,
        })
      )[0]!,
    );
    const parentBreadcrumb = await screen.findByRole("button", {
      name: /^Back to parent .+ · #1$/,
    });

    fireEvent.click(
      screen.getByRole("button", { name: "Back to apps / sessions" }),
    );

    const returnedCard = await screen.findByRole("button", {
      name: "Open session Alpha 01",
    });
    expect(
      returnedCard.closest("[data-render-safe-scroll-surface]"),
    ).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: /^View subagent session/ }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: /^Back to parent .+ · #1$/ }),
    ).toBeNull();
    expect(parentBreadcrumb).toBeTruthy();
  });

  it("gives the session list its own scroll-content base without content-visibility skipping", async () => {
    renderPage();

    const firstCard = await screen.findByRole("button", {
      name: "Open session Alpha 01",
    });
    const listSurface = firstCard.closest(
      "[data-render-safe-scroll-surface]",
    ) as HTMLElement;

    expect(
      listSurface.querySelector(".render-safe-scroll-content"),
    ).toBeTruthy();
    expect(listSurface.querySelectorAll(".render-safe-content")).toHaveLength(
      0,
    );
  });
});

function renderPage() {
  return render(
    <QueryClientProvider client={testQueryClient}>
      <I18nProvider>
        <ConversationsPage
          appShortcuts={[]}
          onManualOpen={vi.fn()}
          onNotify={vi.fn()}
          onNotifyError={vi.fn()}
          onOpenSettings={vi.fn()}
          recordKind="session"
        />
      </I18nProvider>
    </QueryClientProvider>,
  );
}

/** jsdom 没有布局：给滚动容器一个稳定的有界视口，虚拟化才会真正生效。 */
function stubLayout() {
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get: () => 640,
  });
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get: () => 1200,
  });
  Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
    configurable: true,
    get: () => 132,
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      private readonly callback: ResizeObserverCallback;
      constructor(callback: ResizeObserverCallback) {
        this.callback = callback;
      }
      observe(target: Element) {
        if (!target.hasAttribute("data-render-safe-scroll-surface")) return;
        queueMicrotask(() =>
          this.callback(
            [
              {
                target,
                contentRect: {
                  height: (target as HTMLElement).clientHeight,
                  width: (target as HTMLElement).clientWidth,
                },
              } as ResizeObserverEntry,
            ],
            this as unknown as ResizeObserver,
          ),
        );
      }
      unobserve() {}
      disconnect() {}
    },
  );
}

const adapter: ConversationAdapter = {
  capabilities: ["probe", "read_session"],
  content_hash: "adapter-hash",
  created_at: now,
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
  updated_at: now,
  version: "0.1.0",
};

function sessionListItems(): ConversationSessionListItem[] {
  const beta: ConversationSessionListItem[] = [1, 2].map((index) => ({
    adapter_id: "codex",
    created_at: now,
    external_id: `beta-${index}`,
    id: `beta-session-${index}`,
    imported_at: now,
    missing: false,
    project_path: "/repo/beta",
    question_count: 1,
    source_id: "codex-live",
    title: `Beta 0${index}`,
    turn_count: 1,
    updated_at: now,
  }));
  return [
    ...beta,
    ...Array.from({ length: SESSION_COUNT }, (_, index) => ({
      adapter_id: "codex",
      created_at: now,
      external_id: `alpha-${index + 1}`,
      id: `alpha-session-${index + 1}`,
      imported_at: now,
      missing: false,
      project_path: "/repo/alpha",
      question_count: 1,
      source_id: "codex-live",
      title: `Alpha ${String(index + 1).padStart(2, "0")}`,
      turn_count: 1,
      updated_at: now,
    })),
  ];
}

function sessionDetail(sessionId: string): ConversationSessionDetail {
  if (sessionId === CHILD_SESSION_ID) return childSessionDetail();
  const session = sessionListItems().find((item) => item.id === sessionId);
  return {
    questions:
      sessionId === PARENT_SESSION_ID
        ? [parentQuestionWithSubagent()]
        : [plainQuestion(sessionId)],
    session: session ?? sessionListItems()[0]!,
  };
}

function plainQuestion(sessionId: string): ConversationQuestionDetail {
  return {
    parts: [],
    question: {
      created_at: now,
      id: `question-${sessionId}`,
      session_id: sessionId,
      title: `Question of ${sessionId}`,
      updated_at: now,
    },
    projected_content_nodes: [],
    question_turns: [],
    turns: [],
  };
}

function parentQuestionWithSubagent(): ConversationQuestionDetail {
  const turn = {
    id: "parent-turn-1",
    imported_at: now,
    process: JSON.stringify({
      items: [
        {
          content: CHILD_SESSION_ID,
          kind: "subagent",
          label: "Explore child",
          seq: 0,
          status: "completed",
          summary: "child summary",
        },
      ],
    }),
    prompt: "Parent prompt",
    session_id: PARENT_SESSION_ID,
    started_at: now,
    turn_index: 0,
  };
  return {
    parts: [
      {
        content_card: {
          kind: "answer",
          renderer: "markdown" as const,
          schema_version: 1,
          semantic_role: "answer",
        },
        id: "parent-answer",
        kind: "text" as const,
        part_index: 0,
        role: "assistant" as const,
        text: "Parent answer.",
        turn_id: "parent-turn-1",
      },
      {
        content_card: {
          kind: "claude-code.subagent",
          renderer: "subagent_tree" as const,
          schema_version: 1,
          semantic_role: "subagent",
        },
        id: "parent-anchor",
        kind: "subagent" as const,
        metadata_json: JSON.stringify({
          agent_role: "Explore",
          child_session_id: CHILD_SESSION_ID,
          status: "completed",
          step_count: 4,
          title: "Explore child",
        }),
        part_index: 1,
        role: "assistant" as const,
        text: "child prompt",
        turn_id: "parent-turn-1",
      },
    ],
    question: {
      created_at: now,
      id: "question-parent",
      session_id: PARENT_SESSION_ID,
      title: "Parent question",
      updated_at: now,
    },
    question_turns: [
      {
        assigned_at: now,
        assignment_origin: "imported" as const,
        question_id: "question-parent",
        turn_id: "parent-turn-1",
        turn_order: 0,
        updated_at: now,
      },
    ],
    projected_content_nodes: [
      {
        command_label: null,
        content: "Parent answer.",
        cwd: null,
        exit_code: null,
        language: null,
        legacy_anchor_ids: [],
        locator: {
          node_order: 0,
          part_id: "parent-answer",
          question_id: "question-parent",
          turn_id: "parent-turn-1",
        },
        node_id: "parent-answer-node",
        node_order: 0,
        node_type: "answer",
        part_id: "parent-answer",
        part_order: 0,
        question_id: "question-parent",
        renderer: "markdown",
        role: "assistant",
        semantic_role: "answer",
        source_execution_id: null,
        status: null,
        translated_content: null,
        turn_id: "parent-turn-1",
        turn_order: 0,
      },
    ],
    turns: [turn],
  };
}

function childSessionDetail(): ConversationSessionDetail {
  return {
    questions: [plainQuestion(CHILD_SESSION_ID)],
    session: {
      adapter_id: "codex",
      created_at: now,
      execution_origin: "subagent",
      execution_purpose: JSON.stringify({
        agent_role: "Explore",
        parent_session_id: PARENT_SESSION_ID,
        spawn_anchor: { turn_index: 0 },
      }),
      external_id: CHILD_SESSION_ID,
      id: CHILD_SESSION_ID,
      imported_at: now,
      missing: false,
      project_path: "/repo/alpha",
      source_id: "codex-live",
      title: "Child session",
      updated_at: now,
      user_visible: false,
    },
  };
}
