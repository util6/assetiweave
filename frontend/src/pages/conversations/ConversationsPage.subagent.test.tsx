/* @vitest-environment jsdom */

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { defaultSettings } from "../../store/settings/settingsSchema";
import type {
  ConversationPart,
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
const parentSessionId = `conversation-session-${"a".repeat(64)}`;
const parentShortId = "aaaaaaaa";

let testQueryClient: QueryClient;

describe("ConversationsPage subagent drill-down", () => {
  beforeEach(() => {
    testQueryClient = new QueryClient({
      defaultOptions: {
        queries: { gcTime: 0, retry: false, staleTime: 0 },
      },
    });
    window.scrollTo = vi.fn();
    localStorage.setItem("assetiweave.locale", "zh");
    vi.stubGlobal(
      "ResizeObserver",
      class {
        disconnect() {}
        observe() {}
        unobserve() {}
      },
    );
    listConversationAdaptersMock.mockReset().mockResolvedValue([]);
    listConversationSessionsMock
      .mockReset()
      .mockResolvedValue([parentSessionListItem()]);
    getConversationSessionMock
      .mockReset()
      .mockImplementation(async (sessionId: string) =>
        sessionId.startsWith("child-")
          ? childSessionDetail(sessionId)
          : parentSessionDetail(),
      );
  });

  afterEach(() => {
    cleanup();
    testQueryClient.clear();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("loads only the parent session, then drills down and returns through the breadcrumb", async () => {
    const { container } = render(
      <QueryClientProvider client={testQueryClient}>
        <I18nProvider>
          <ConversationsPage
            appShortcuts={[]}
            navigationTarget={{
              nonce: "open-parent",
              recordKind: "session",
              sessionId: parentSessionId,
            }}
            onManualOpen={vi.fn()}
            onNotify={vi.fn()}
            onNotifyError={vi.fn()}
            onOpenSettings={vi.fn()}
            recordKind="session"
          />
        </I18nProvider>
      </QueryClientProvider>,
    );

    // 首屏只拉父会话详情，绝不为渲染锚点逐个子会话发请求。
    await waitFor(() => expect(screen.getByText("梳理同步链路")).toBeTruthy());
    expect(getConversationSessionMock).toHaveBeenCalledTimes(1);
    expect(getConversationSessionMock).toHaveBeenCalledWith(parentSessionId);

    // 1 问 = 1 Turn：左栏不再出现伪轮次。
    expect(screen.getByText("#1")).toBeTruthy();
    expect(screen.getByText("#2")).toBeTruthy();
    expect(container.textContent).not.toContain(
      "Continue following subagent execution",
    );

    // 第 1 轮的胶囊徽章与正文 3 张传送门卡片都已就位。
    expect(
      screen.getByRole("button", { name: "展开 3 个子代理" }),
    ).toBeTruthy();
    expect(container.querySelectorAll("[data-subagent-portal]")).toHaveLength(
      3,
    );

    // 点击正文里的传送门卡片才按需下钻，并且只拉这一个子会话。
    fireEvent.click(screen.getAllByRole("button", { name: /查看子会话/ })[0]!);
    await waitFor(() =>
      expect(getConversationSessionMock).toHaveBeenCalledWith("child-1"),
    );
    expect(getConversationSessionMock).toHaveBeenCalledTimes(2);

    // 子会话顶栏出现返回父会话的一键面包屑。
    const breadcrumb = await screen.findByRole("button", {
      name: `返回父会话 ${parentShortId} · #1`,
    });

    fireEvent.click(breadcrumb);

    await waitFor(() =>
      expect(getConversationSessionMock).toHaveBeenCalledTimes(3),
    );
    expect(getConversationSessionMock).toHaveBeenLastCalledWith(
      parentSessionId,
    );
    await waitFor(() => expect(screen.getByText("梳理同步链路")).toBeTruthy());
  });

  it("drills down from a left rail subagent chip and returns to the dispatching turn", async () => {
    render(
      <QueryClientProvider client={testQueryClient}>
        <I18nProvider>
          <ConversationsPage
            appShortcuts={[]}
            navigationTarget={{
              nonce: "open-parent",
              recordKind: "session",
              sessionId: parentSessionId,
            }}
            onManualOpen={vi.fn()}
            onNotify={vi.fn()}
            onNotifyError={vi.fn()}
            onOpenSettings={vi.fn()}
            recordKind="session"
          />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await waitFor(() => expect(screen.getByText("梳理同步链路")).toBeTruthy());

    // 展开左栏胶囊后点第 2 个芯片：只按需加载该子会话。
    fireEvent.click(screen.getByRole("button", { name: "展开 3 个子代理" }));
    fireEvent.click(
      screen.getByRole("button", {
        name: "打开子代理会话：Coder 梳理入口 B",
      }),
    );

    await waitFor(() =>
      expect(getConversationSessionMock).toHaveBeenCalledWith("child-2"),
    );

    // 面包屑回跳到父会话第 1 轮。
    fireEvent.click(
      await screen.findByRole("button", {
        name: `返回父会话 ${parentShortId} · #1`,
      }),
    );

    await waitFor(() =>
      expect(getConversationSessionMock).toHaveBeenLastCalledWith(
        parentSessionId,
      ),
    );
  });

  it("keeps an anchor without a child session visible but not clickable", async () => {
    const detail = parentSessionDetail();
    const question = detail.questions[0]!;
    question.parts = question.parts.map((part) =>
      part.id === "part-anchor-1"
        ? {
            ...part,
            metadata_json: JSON.stringify({
              agent_role: "Explore",
              title: "梳理入口 A",
            }),
          }
        : part,
    );
    question.turns = question.turns.map((turn) => {
      const process = JSON.parse(turn.process ?? "{}");
      return {
        ...turn,
        process: JSON.stringify({
          ...process,
          items: (process.items ?? []).filter(
            (item: { content?: string }) => item.content !== "child-1",
          ),
        }),
      };
    });
    getConversationSessionMock.mockImplementation(async (sessionId: string) =>
      sessionId === parentSessionId ? detail : childSessionDetail(sessionId),
    );

    const { container } = render(
      <QueryClientProvider client={testQueryClient}>
        <I18nProvider>
          <ConversationsPage
            appShortcuts={[]}
            navigationTarget={{
              nonce: "open-parent",
              recordKind: "session",
              sessionId: parentSessionId,
            }}
            onManualOpen={vi.fn()}
            onNotify={vi.fn()}
            onNotifyError={vi.fn()}
            onOpenSettings={vi.fn()}
            recordKind="session"
          />
        </I18nProvider>
      </QueryClientProvider>,
    );

    await waitFor(() => expect(screen.getByText("梳理同步链路")).toBeTruthy());

    // 三张锚点卡片都可见，其中一张降级为只读。
    expect(container.querySelectorAll("[data-subagent-portal]")).toHaveLength(
      3,
    );
    expect(
      container.querySelectorAll("[data-subagent-unresolved]"),
    ).toHaveLength(1);
    expect(screen.getAllByRole("button", { name: /查看子会话/ })).toHaveLength(
      2,
    );

    // 左栏芯片同样可见，但不可点、不发请求。
    fireEvent.click(screen.getByRole("button", { name: "展开 3 个子代理" }));
    const unresolvedChip = screen.getByRole("button", {
      name: "该锚点没有记录子会话主键，只能只读查看",
    });
    expect((unresolvedChip as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(unresolvedChip);
    expect(getConversationSessionMock).toHaveBeenCalledTimes(1);
  });
});

function parentSessionListItem(): ConversationSessionListItem {
  return {
    adapter_id: "claude-code",
    created_at: now,
    execution_origin: "user",
    external_id: "parent-external",
    id: parentSessionId,
    imported_at: now,
    missing: false,
    project_path: "/repo",
    question_count: 2,
    source_id: "source-1",
    title: "父子会话导航",
    turn_count: 2,
    updated_at: now,
  };
}

function parentSessionDetail(): ConversationSessionDetail {
  return {
    session: {
      adapter_id: "claude-code",
      created_at: now,
      execution_origin: "user",
      external_id: "parent-external",
      id: parentSessionId,
      imported_at: now,
      missing: false,
      project_path: "/repo",
      source_id: "source-1",
      title: "父子会话导航",
      updated_at: now,
    },
    questions: [
      questionWithDispatches(),
      simpleQuestion("question-2", "turn-2", "第二条问题"),
    ],
  };
}

function childSessionDetail(sessionId = "child-1"): ConversationSessionDetail {
  return {
    session: {
      adapter_id: "claude-code",
      created_at: now,
      execution_origin: "subagent",
      execution_purpose: JSON.stringify({
        agent_role: "Explore",
        parent_session_id: parentSessionId,
        spawn_anchor: { turn_index: 0 },
      }),
      external_id: `${sessionId}-external`,
      id: sessionId,
      imported_at: now,
      missing: false,
      source_id: "source-1",
      title: "梳理同步入口",
      updated_at: now,
      user_visible: false,
    },
    questions: [
      simpleQuestion("child-question-1", "child-turn-1", "子代理任务"),
    ],
  };
}

function questionWithDispatches(): ConversationQuestionDetail {
  const anchors = ["梳理入口 A", "梳理入口 B", "梳理入口 C"];
  const rootTurn = {
    id: "turn-1",
    imported_at: now,
    process: JSON.stringify({
      items: anchors.map((label, index) => ({
        content: `child-${index + 1}`,
        kind: "subagent",
        label,
        seq: index,
        status: "completed",
        summary: `${label} 的汇报结论`,
      })),
    }),
    prompt: "帮我梳理同步链路",
    session_id: parentSessionId,
    started_at: now,
    turn_index: 0,
  };
  const parts: ConversationPart[] = [
    answerPart("part-answer-1", "turn-1", "同步链路已经梳理完毕。"),
    ...anchors.map((label, index) => ({
      content_card: {
        kind: "claude-code.subagent",
        renderer: "subagent_tree" as const,
        schema_version: 1,
        semantic_role: "subagent",
      },
      id: `part-anchor-${index + 1}`,
      kind: "subagent" as const,
      metadata_json: JSON.stringify({
        agent_role: index === 1 ? "Coder" : "Explore",
        child_session_id: `child-${index + 1}`,
        status: "completed",
        step_count: 4 + index,
        title: label,
      }),
      part_index: index + 1,
      role: "assistant" as const,
      text: `${label} 的完整委派 Prompt`.repeat(20),
      turn_id: "turn-1",
    })),
  ];

  return {
    parts,
    question: {
      created_at: now,
      id: "question-1",
      session_id: parentSessionId,
      title: "梳理同步链路",
      updated_at: now,
    },
    question_turns: [
      {
        assigned_at: now,
        assignment_origin: "imported",
        question_id: "question-1",
        turn_id: "turn-1",
        turn_order: 0,
        updated_at: now,
      },
    ],
    projected_content_nodes: [
      contentNode("part-answer-node-1", "part-answer-1", "turn-1", "answer"),
      // 真实锚点不挂 content_card，后端不会产出锚点节点：
      // 正文的 3 张传送门卡片必须由前端从 Part 合成。
    ],
    turns: [rootTurn],
  };
}

function simpleQuestion(
  questionId: string,
  turnId: string,
  title: string,
): ConversationQuestionDetail {
  return {
    parts: [answerPart(`${turnId}-answer`, turnId, `${title}的回答`)],
    question: {
      created_at: now,
      id: questionId,
      session_id: parentSessionId,
      title,
      updated_at: now,
    },
    question_turns: [
      {
        assigned_at: now,
        assignment_origin: "imported",
        question_id: questionId,
        turn_id: turnId,
        turn_order: 0,
        updated_at: now,
      },
    ],
    projected_content_nodes: [
      contentNode(
        `${turnId}-answer-node`,
        `${turnId}-answer`,
        turnId,
        "answer",
      ),
    ],
    turns: [
      {
        id: turnId,
        imported_at: now,
        prompt: title,
        session_id: parentSessionId,
        started_at: now,
        turn_index: 1,
      },
    ],
  };
}

function answerPart(
  id: string,
  turnId: string,
  text: string,
): ConversationPart {
  return {
    content_card: {
      kind: "answer",
      renderer: "markdown",
      schema_version: 1,
      semantic_role: "answer",
    },
    id,
    kind: "text",
    part_index: 0,
    role: "assistant",
    text,
    turn_id: turnId,
  };
}

function contentNode(
  nodeId: string,
  partId: string,
  turnId: string,
  semanticRole: "answer" | "subagent",
): ConversationSessionDetail["questions"][number]["projected_content_nodes"][number] {
  return {
    command_label: null,
    content: semanticRole === "answer" ? "同步链路已经梳理完毕。" : "",
    cwd: null,
    exit_code: null,
    language: null,
    legacy_anchor_ids: [],
    locator: {
      node_order: 0,
      part_id: partId,
      question_id: "question-1",
      turn_id: turnId,
    },
    node_id: nodeId,
    node_order: 0,
    node_type: semanticRole === "answer" ? "answer" : "claude-code.subagent",
    part_id: partId,
    part_order: 0,
    question_id: "question-1",
    renderer: semanticRole === "answer" ? "markdown" : "subagent_tree",
    role: semanticRole === "answer" ? "assistant" : "tool",
    semantic_role: semanticRole,
    source_execution_id: null,
    status: null,
    translated_content: null,
    turn_id: turnId,
    turn_order: 0,
  };
}
