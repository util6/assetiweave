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
});
