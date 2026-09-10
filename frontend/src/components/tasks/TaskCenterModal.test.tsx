// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { TaskCenterModal } from "./TaskCenterModal";
import type { TaskView } from "../../types/taskCenter";

const mockCancelTask = vi.hoisted(() => vi.fn());
const mockRetryTask = vi.hoisted(() => vi.fn());
const mockClearTerminal = vi.hoisted(() => vi.fn());
const mockSetSelectedTaskId = vi.hoisted(() => vi.fn());

let mockTasks: TaskView[] = [];
let mockSelectedTaskId: string | null = null;
let mockActiveCount = 0;
let mockFailureCount = 0;

vi.mock("../../app/backgroundTasks/TaskCenterProvider", () => ({
  useTaskCenter: () => ({
    tasks: mockTasks,
    activeTasks: mockTasks.filter((t) => t.state === "running"),
    terminalTasks: mockTasks.filter(
      (t) => t.state === "succeeded" || t.state === "failed" || t.state === "canceled",
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

describe("TaskCenterModal", () => {
  const mockOnClose = vi.fn();

  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
    mockCancelTask.mockReset();
    mockRetryTask.mockReset();
    mockClearTerminal.mockReset();
    mockSetSelectedTaskId.mockReset();
    mockOnClose.mockReset();
    mockSelectedTaskId = null;
    mockActiveCount = 0;
    mockFailureCount = 0;
    mockTasks = [
      {
        id: "task-sync-1",
        kind: "ConversationSync",
        title: "会话同步",
        state: "running",
        outcome: null,
        stages: [
          {
            id: "claim",
            name: "认领任务",
            status: "succeeded",
            current_activities: [],
            metrics: [],
            failures: [],
            skipped: [],
          },
          {
            id: "fetch_batch",
            name: "抓取批次",
            status: "running",
            progress: { current: 5, total: 10, note: "50%" },
            current_activities: [
              {
                stage_id: "fetch_batch",
                worker_id: "worker-1",
                operation: "同步会话批次",
                started_at: "2026-09-10T03:00:00Z",
                current: 5,
                total: 10,
              },
            ],
            metrics: [],
            failures: [],
            skipped: [],
          },
        ],
        metrics: [
          { code: "synced_count", value: 5 },
        ],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 2,
        started_at: "2026-09-10T03:00:00Z",
        updated_at: "2026-09-10T03:01:00Z",
      },
      {
        id: "task-failed-2",
        kind: "Memory",
        title: "会话记忆提取",
        state: "failed",
        outcome: "failure",
        stages: [
          {
            id: "agent_execution",
            name: "Agent 执行",
            status: "failed",
            current_activities: [],
            metrics: [],
            failures: [
              {
                code: "agent_execution_failed",
                message: "模型上下文超长导致执行失败",
                stage: "agent_execution",
                retryable: true,
                timestamp: "2026-09-10T02:05:00Z",
              },
            ],
            skipped: [],
          },
        ],
        metrics: [],
        failures: [
          {
            code: "agent_execution_failed",
            message: "模型上下文超长导致执行失败",
            stage: "agent_execution",
            retryable: true,
            timestamp: "2026-09-10T02:05:00Z",
          },
        ],
        capabilities: { cancellable: false, retryable: true, clearable: true },
        revision: 3,
        started_at: "2026-09-10T02:00:00Z",
        updated_at: "2026-09-10T02:05:00Z",
        finished_at: "2026-09-10T02:05:00Z",
        error_summary: "模型执行失败",
      },
    ];
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("当 open=false 时不渲染任何内容", () => {
    const { container } = render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={false} />
      </I18nProvider>,
    );
    expect(container.firstChild).toBeNull();
  });

  it("当 open=true 时渲染弹窗标题与任务列表", async () => {
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getByText("任务中心")).toBeTruthy();
      const taskList = screen.getByTestId("task-list");
      expect(within(taskList).getByText("会话同步")).toBeTruthy();
      expect(within(taskList).getByText("会话记忆提取")).toBeTruthy();
    });
  });

  it("点击任务项可切换选中的任务", async () => {
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const taskList = screen.getByTestId("task-list");
    const taskItem = within(taskList).getByText("会话记忆提取");
    fireEvent.click(taskItem);
    expect(mockSetSelectedTaskId).toHaveBeenCalledWith("task-failed-2");
  });

  it("支持按关键词搜索过滤任务", async () => {
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const searchInput = screen.getByPlaceholderText(/搜索/i);
    fireEvent.change(searchInput, { target: { value: "记忆" } });

    await waitFor(() => {
      const taskList = screen.getByTestId("task-list");
      expect(within(taskList).getByText("会话记忆提取")).toBeTruthy();
      expect(within(taskList).queryByText("会话同步")).toBeNull();
    });
  });

  it("支持按运行状态过滤任务", async () => {
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const runningFilterBtn = screen.getByRole("button", { name: "进行中" });
    fireEvent.click(runningFilterBtn);

    await waitFor(() => {
      const taskList = screen.getByTestId("task-list");
      expect(within(taskList).getByText("会话同步")).toBeTruthy();
      expect(within(taskList).queryByText("会话记忆提取")).toBeNull();
    });
  });

  it("支持取消运行中任务", async () => {
    mockCancelTask.mockResolvedValueOnce(true);
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const cancelBtn = screen.getByRole("button", { name: /取消任务/i });
    fireEvent.click(cancelBtn);

    await waitFor(() => {
      expect(mockCancelTask).toHaveBeenCalledWith("task-sync-1");
    });
  });

  it("支持清空已完成终态任务", async () => {
    mockClearTerminal.mockResolvedValueOnce(1);
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const clearBtn = screen.getByRole("button", { name: /清空已完成/i });
    fireEvent.click(clearBtn);

    await waitFor(() => {
      expect(mockClearTerminal).toHaveBeenCalled();
    });
  });

  it("点击关闭按钮调用 onClose 回调", async () => {
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const closeButtons = screen.getAllByRole("button", { name: "关闭" });
    fireEvent.click(closeButtons[0]);
    expect(mockOnClose).toHaveBeenCalled();
  });

  it("完美兼容纯 camelCase 格式任务数据与活跃 worker 列表，不产生 undefined 异常", async () => {
    mockTasks = [
      {
        id: "task-camel-1",
        kind: "ConversationSync",
        title: "驼峰数据任务",
        state: "running",
        outcome: null,
        startedAt: "2026-09-10T04:00:00Z",
        updatedAt: "2026-09-10T04:01:00Z",
        stages: [
          {
            id: "stage-camel",
            name: "阶段驼峰",
            status: "running",
            durationMs: 1500,
            currentActivities: [
              {
                stageId: "stage-camel",
                workerId: "worker-camel-99",
                operation: "处理中",
                path: "/path/to/source",
              },
            ],
            metrics: [{ code: "processed", value: 100 }],
            failures: [],
            skipped: [],
          },
        ],
        metrics: [{ code: "total", value: 100 }],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 1,
      } as unknown as TaskView,
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getAllByText("驼峰数据任务").length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText("活跃 Workers (1)")).toBeTruthy();
      expect(screen.getByText("[worker-camel-99]")).toBeTruthy();
      expect(screen.getByText("耗时: 1.5s")).toBeTruthy();
    });
  });

  it("即使任务 stages 或 activities 为 null/undefined 也安全降级而不白屏崩溃", async () => {
    mockTasks = [
      {
        id: "task-edge-case",
        kind: "Scan",
        title: "极端异常字段任务",
        state: "failed",
        outcome: "failure",
        stages: undefined,
        metrics: undefined,
        failures: undefined,
        capabilities: undefined,
        revision: 1,
      } as unknown as TaskView,
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getAllByText("极端异常字段任务").length).toBeGreaterThanOrEqual(1);
      expect(screen.getByText("该任务尚未上报阶段划分")).toBeTruthy();
    });
  });

  it("任务列表项统一采用 conversation-row 与 data-selected，并具备 ChevronRight 指示与平滑切换过渡类", async () => {
    mockSelectedTaskId = "task-sync-1";
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      const taskList = screen.getByTestId("task-list");
      expect(taskList.className).toContain("aurora-view-transition");

      const selectedItem = within(taskList).getByRole("button", { pressed: true });
      expect(selectedItem.className).toContain("conversation-row");
      expect(selectedItem.getAttribute("data-selected")).toBe("true");

      const unselectedItem = within(taskList).getByRole("button", { pressed: false });
      expect(unselectedItem.className).toContain("conversation-row");
      expect(unselectedItem.getAttribute("data-selected")).toBe("false");

      // 验证右侧详情具有 aurora-view-transition 平滑过渡类
      const detailContainer = document.querySelector("main .aurora-view-transition");
      expect(detailContainer).toBeTruthy();
    });
  });
});
