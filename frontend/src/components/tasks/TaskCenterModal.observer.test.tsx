// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { TaskCenterModal } from "./TaskCenterModal";
import type { TaskView } from "../../types/taskCenter";
import type {
  AgentSessionGetResult,
  AgentSessionRef,
  AgentSessionView,
} from "../../types/agentSession";

const mockCancelTask = vi.hoisted(() => vi.fn());
const mockRetryTask = vi.hoisted(() => vi.fn());
const mockClearTerminal = vi.hoisted(() => vi.fn());
const mockSetSelectedTaskId = vi.hoisted(() => vi.fn());
const mockGetAgentSession = vi.hoisted(() => vi.fn());
const mockSubscribeAgentSessionUpdated = vi.hoisted(() =>
  vi.fn().mockResolvedValue(() => {}),
);

let mockTasks: TaskView[] = [];
let mockSelectedTaskId: string | null = null;
let mockActiveCount = 0;
let mockFailureCount = 0;

vi.mock("../../app/backgroundTasks/TaskCenterProvider", () => ({
  useTaskCenter: () => ({
    tasks: mockTasks,
    activeTasks: mockTasks.filter((t) => t.state === "running"),
    terminalTasks: mockTasks.filter(
      (t) =>
        t.state === "succeeded" ||
        t.state === "failed" ||
        t.state === "canceled",
    ),
    selectedTaskId: mockSelectedTaskId,
    setSelectedTaskId: mockSetSelectedTaskId,
    cancelTask: mockCancelTask,
    retryTask: mockRetryTask,
    clearTerminal: mockClearTerminal,
    activeCount: mockActiveCount,
    failureCount: mockFailureCount,
  }),
}));

vi.mock("../../services/agentSessionService", () => ({
  getAgentSession: (params: unknown) => mockGetAgentSession(params),
  subscribeAgentSessionUpdated: (ref: unknown, cb: unknown) =>
    mockSubscribeAgentSessionUpdated(ref, cb),
}));

describe("TaskCenterModal - Agent Session Observer (T09)", () => {
  const sampleSessionRef: AgentSessionRef = {
    schemaVersion: 1,
    value: "agent-session://team-1/memory-job-1/turn-1",
  };

  const sampleSessionView: AgentSessionView = {
    schemaVersion: 1,
    sessionRef: sampleSessionRef,
    executionId: "exec-1",
    purpose: "session_memory",
    mode: "observer",
    agent: {
      id: "agent-memory-1",
      displayName: "Memory Extraction Agent",
      model: "gpt-4o",
      protocol: "acp",
    },
    context: {
      memoryScope: "session",
      memoryJobId: "mem-job-1",
      taskId: "task-mem-1",
    },
    state: "active",
    capabilities: {
      read: true,
      send: false,
      stop: false,
      retry: false,
      queue: false,
      interrupt: false,
      attach: false,
      mention: false,
      slashCommand: false,
    },
    revision: 2,
    eventCount: 3,
    items: [
      {
        identity: {
          session_id: "s-1",
          turn_id: "t-1",
          item_id: "item-req-1",
          member_id: "m-1",
          execution_id: "exec-1",
        },
        kind: "user_message",
        sequence: 1,
        delivery: "live",
        state: "completed",
        text: "请提取本次会话的长期记忆要点",
        status: null,
        code: null,
      },
      {
        identity: {
          session_id: "s-1",
          turn_id: "t-1",
          item_id: "item-text-1",
          member_id: "m-1",
          execution_id: "exec-1",
        },
        kind: "assistant_text",
        sequence: 2,
        delivery: "live",
        state: "completed",
        text: "正在分析对话关键决策和技术选型...",
        status: null,
        code: null,
      },
    ],
    retention: {
      maxItems: 50,
      maxEvents: 200,
      maxBytes: 1048576,
      truncated: false,
      evictedItemCount: 0,
      rejectedEventCount: 0,
    },
    updatedAt: "2026-09-10T15:00:00Z",
  };

  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
    mockGetAgentSession.mockReset();
    mockSubscribeAgentSessionUpdated.mockClear();

    mockSelectedTaskId = "task-mem-1";
    mockActiveCount = 1;
    mockFailureCount = 0;
    mockTasks = [
      {
        id: "task-mem-1",
        kind: "Memory",
        title: "Session 记忆提取",
        tenantId: "default",
        state: "running",
        startedAt: "2026-09-10T14:59:00Z",
        updatedAt: "2026-09-10T15:00:00Z",
        stages: [
          {
            id: "stage-extract",
            name: "增量消息归纳",
            status: "succeeded",
            metrics: [],
            failures: [],
          },
          {
            id: "stage-agent",
            name: "Agent 执行与反思",
            status: "running",
            metrics: [],
            failures: [],
            agentSessionRef: sampleSessionRef,
          },
        ],
        metrics: [],
        failures: [],
        capabilities: {
          cancellable: true,
          retryable: false,
          clearable: false,
        },
        revision: 1,
        agentSessionRef: sampleSessionRef,
      },
    ];
  });

  afterEach(() => {
    cleanup();
  });

  it("renders '查看执行现场' button on stages having agentSessionRef", () => {
    render(
      <I18nProvider>
        <TaskCenterModal onClose={vi.fn()} open={true} />
      </I18nProvider>,
    );

    // Stage 1 has no agentSessionRef
    expect(
      screen.queryByTestId("task-stage-view-session-stage-extract"),
    ).toBeNull();

    // Stage 2 has agentSessionRef
    const btn = screen.getByTestId("task-stage-view-session-stage-agent");
    expect(btn).toBeTruthy();
    expect(btn.textContent).toContain("查看执行现场");
  });

  it("navigates to observer view on click, fetches session and renders read-only workspace", async () => {
    mockGetAgentSession.mockResolvedValueOnce(sampleSessionView);

    render(
      <I18nProvider>
        <TaskCenterModal onClose={vi.fn()} open={true} />
      </I18nProvider>,
    );

    const btn = screen.getByTestId("task-stage-view-session-stage-agent");
    fireEvent.click(btn);

    // Observer container should be rendered
    expect(screen.getByTestId("task-agent-session-observer")).toBeTruthy();
    expect(screen.getByText("只读观察模式")).toBeTruthy();

    // Session service should have been called
    await waitFor(() => {
      expect(mockGetAgentSession).toHaveBeenCalledWith({
        sessionRef: sampleSessionRef,
      });
    });

    // Content should be rendered in AgentSessionWorkspace
    await waitFor(() => {
      expect(screen.getByText("Memory Extraction Agent")).toBeTruthy();
      expect(screen.getByText("请提取本次会话的长期记忆要点")).toBeTruthy();
      expect(
        screen.getByText("正在分析对话关键决策和技术选型..."),
      ).toBeTruthy();
    });

    // Back button returns to task detail
    const backBtn = screen.getByTestId("task-observer-back-btn");
    fireEvent.click(backBtn);

    expect(screen.queryByTestId("task-agent-session-observer")).toBeNull();
    expect(
      screen.getByTestId("task-stage-view-session-stage-agent"),
    ).toBeTruthy();
  });

  it("gracefully shows unavailable empty state when session is expired or not found", async () => {
    const unavailableResult: AgentSessionGetResult = {
      schemaVersion: 1,
      sessionRef: sampleSessionRef,
      state: "unavailable",
      reason: "notFoundOrExpired",
    };
    mockGetAgentSession.mockResolvedValueOnce(unavailableResult);

    render(
      <I18nProvider>
        <TaskCenterModal onClose={vi.fn()} open={true} />
      </I18nProvider>,
    );

    const btn = screen.getByTestId("task-stage-view-session-stage-agent");
    fireEvent.click(btn);

    await waitFor(() => {
      expect(
        screen.getByTestId("task-agent-observer-unavailable"),
      ).toBeTruthy();
      expect(screen.getByText(/该阶段执行现场已过期/)).toBeTruthy();
    });
  });
});
