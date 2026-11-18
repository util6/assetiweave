// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TaskNotificationBubble } from "./TaskNotificationBubble";
import type { TaskNotificationItem } from "../../app/backgroundTasks/TaskCenterProvider";
import { useAppUiStore } from "../../store/ui/appUiStore";

const mockDismiss = vi.hoisted(() => vi.fn());
const mockSetSelectedTaskId = vi.hoisted(() => vi.fn());
const mockNavigate = vi.hoisted(() => vi.fn());
const mockOpenResultForTask = vi.hoisted(() => vi.fn());
const mockGetTaskRecordKind = vi.hoisted(() => vi.fn(() => "session"));
let mockNotification: TaskNotificationItem | null = null;
let mockPathname = "/skills/overview";

vi.mock("../../app/backgroundTasks/TaskCenterProvider", () => ({
  useOptionalTaskCenter: () => ({
    currentNotification: mockNotification,
    dismissNotification: mockDismiss,
    setSelectedTaskId: mockSetSelectedTaskId,
  }),
  useTaskCenter: () => ({
    currentNotification: mockNotification,
    dismissNotification: mockDismiss,
    setSelectedTaskId: mockSetSelectedTaskId,
  }),
}));

vi.mock("../../app/backgroundTasks/ConversationSearchProvider", () => ({
  useOptionalConversationSearch: () => ({
    openResultForTask: mockOpenResultForTask,
    getTaskRecordKind: mockGetTaskRecordKind,
  }),
}));

vi.mock("@tanstack/react-router", () => ({
  useNavigate: () => mockNavigate,
  useRouterState: () => ({
    location: { pathname: mockPathname },
  }),
}));

describe("TaskNotificationBubble", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useAppUiStore.getState().setTaskCenterOpen(false);
    mockDismiss.mockReset();
    mockSetSelectedTaskId.mockReset();
    mockNavigate.mockReset();
    mockOpenResultForTask.mockReset();
    mockGetTaskRecordKind.mockReset();
    mockGetTaskRecordKind.mockReturnValue("session");
    mockPathname = "/skills/overview";
    mockNotification = {
      id: "notif-1",
      task: {
        id: "task-1",
        kind: "conversation_sync",
        title: "会话同步",
        state: "succeeded",
        outcome: "success",
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: false, retryable: false, clearable: true },
        revision: 2,
      },
      title: "会话同步 (已完成)",
      message: "同步成功",
      isFailure: false,
      durationMs: 4000,
      createdAt: Date.now(),
    };
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("renders notification and auto-dismisses after durationMs", async () => {
    render(<TaskNotificationBubble />);

    expect(screen.getByText("会话同步 (已完成)")).toBeTruthy();
    expect(screen.getByText("同步成功")).toBeTruthy();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(4000);
    });

    expect(mockDismiss).toHaveBeenCalledWith("notif-1");
  });

  it("pauses timer on mouse enter and resumes on mouse leave", async () => {
    render(<TaskNotificationBubble />);

    const bubble = screen.getByRole("status");
    fireEvent.mouseEnter(bubble);

    // Advanced past 4000ms while paused
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(mockDismiss).not.toHaveBeenCalled();

    // Mouse leave resumes
    fireEvent.mouseLeave(bubble);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4000);
    });
    expect(mockDismiss).toHaveBeenCalledWith("notif-1");
  });

  it("dismisses when clicking close button", () => {
    render(<TaskNotificationBubble />);

    fireEvent.click(screen.getByRole("button", { name: "关闭通知" }));
    expect(mockDismiss).toHaveBeenCalledWith("notif-1");
    expect(mockNavigate).not.toHaveBeenCalled();
  });

  it("opens task center modal and selects task on click", () => {
    render(<TaskNotificationBubble />);

    fireEvent.click(screen.getByRole("status"));
    expect(mockSetSelectedTaskId).toHaveBeenCalledWith("task-1");
    expect(mockDismiss).toHaveBeenCalledWith("notif-1");
    expect(useAppUiStore.getState().taskCenterOpen).toBe(true);
  });

  it("does not render when taskCenterOpen is true", () => {
    useAppUiStore.getState().setTaskCenterOpen(true);
    const { container } = render(<TaskNotificationBubble />);
    expect(container.firstChild).toBeNull();
  });

  it("navigates to record route and opens search dialog when clicking conversationcontentsearch notification", () => {
    mockGetTaskRecordKind.mockReturnValue("web");
    mockNotification = {
      id: "notif-search-1",
      task: {
        id: "task-search-1",
        kind: "conversationcontentsearch",
        title: "搜索对话内容",
        state: "succeeded",
        outcome: "success",
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: false, retryable: false, clearable: true },
        revision: 2,
      },
      title: "搜索对话内容 (已完成)",
      message: "「docker」共找到 5 个结果",
      isFailure: false,
      durationMs: 4000,
      createdAt: Date.now(),
    };

    render(<TaskNotificationBubble />);
    fireEvent.click(screen.getByRole("status"));

    expect(mockDismiss).toHaveBeenCalledWith("notif-search-1");
    expect(mockNavigate).toHaveBeenCalledWith({
      to: "/conversations/web-records",
    });
    expect(mockOpenResultForTask).toHaveBeenCalledWith("task-search-1");
    expect(useAppUiStore.getState().taskCenterOpen).toBe(false);
  });

  it("toggles TruncatedText lines between 2 and false when expanding and collapsing long messages", () => {
    const longMessage =
      "Line one error details that exceed fifty characters to trigger expand control and multiline clamping";
    mockNotification = {
      ...mockNotification!,
      message: longMessage,
      isFailure: true,
    };

    render(<TaskNotificationBubble />);

    const messageEl = screen.getByText(longMessage);
    expect(messageEl.className).toContain("line-clamp-2");
    expect(messageEl.getAttribute("title")).toBeNull();

    Object.defineProperty(messageEl, "clientWidth", {
      configurable: true,
      get: () => 200,
    });
    Object.defineProperty(messageEl, "scrollWidth", {
      configurable: true,
      get: () => 200,
    });
    Object.defineProperty(messageEl, "clientHeight", {
      configurable: true,
      get: () => 32,
    });
    Object.defineProperty(messageEl, "scrollHeight", {
      configurable: true,
      get: () => 72,
    });

    fireEvent.mouseEnter(messageEl);
    act(() => {
      vi.advanceTimersByTime(320);
    });
    expect(screen.getByRole("tooltip").textContent).toBe(longMessage);

    fireEvent.click(screen.getByRole("button", { name: "展开完整信息" }));
    expect(messageEl.className).not.toContain("line-clamp-2");
    expect(screen.queryByRole("tooltip")).toBeNull();

    fireEvent.mouseEnter(messageEl);
    act(() => {
      vi.advanceTimersByTime(320);
    });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
