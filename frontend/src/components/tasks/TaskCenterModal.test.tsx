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

describe("TaskCenterModal", () => {
  const mockOnClose = vi.fn();

  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        disconnect() {}
        observe() {}
        unobserve() {}
      },
    );
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
        metrics: [{ code: "synced_count", value: 5 }],
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
      {
        id: "task-open-cat-3",
        kind: "Other",
        category: "custom/pipeline-run",
        title: "自定义流水线",
        state: "succeeded",
        outcome: "success",
        stages: [
          {
            id: "step_1",
            name: "初始分析",
            status: "succeeded",
            current_activities: [],
            metrics: [],
            failures: [],
            skipped: [],
          },
          {
            id: "step_2",
            name: "可选优化",
            status: "skipped",
            current_activities: [],
            metrics: [],
            failures: [],
            skipped: [],
          },
        ],
        metrics: [],
        failures: [],
        capabilities: { cancellable: false, retryable: false, clearable: true },
        revision: 4,
        started_at: "2026-09-10T04:00:00Z",
        updated_at: "2026-09-10T04:02:00Z",
        finished_at: "2026-09-10T04:02:00Z",
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
      expect(screen.getAllByText("驼峰数据任务").length).toBeGreaterThanOrEqual(
        1,
      );
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
      expect(
        screen.getAllByText("极端异常字段任务").length,
      ).toBeGreaterThanOrEqual(1);
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
      expect(taskList.className).toContain("ui-view-transition");

      const selectedItem = within(taskList).getByRole("button", {
        pressed: true,
      });
      expect(selectedItem.className).toContain("conversation-row");
      expect(selectedItem.getAttribute("data-selected")).toBe("true");

      const unselectedItems = within(taskList).getAllByRole("button", {
        pressed: false,
      });
      expect(unselectedItems.length).toBeGreaterThan(0);
      expect(unselectedItems[0].className).toContain("conversation-row");
      expect(unselectedItems[0].getAttribute("data-selected")).toBe("false");

      // 验证右侧详情具有 ui-view-transition 平滑过渡类
      const detailContainer = document.querySelector(
        "main .ui-view-transition",
      );
      expect(detailContainer).toBeTruthy();
    });
  });

  it("支持展示开放任务分类 category 与跳过阶段状态 skipped", async () => {
    mockSelectedTaskId = "task-open-cat-3";
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    // 验证分类文本在列表项与详情头部均正确渲染
    expect(
      screen.getAllByText("custom/pipeline-run").length,
    ).toBeGreaterThanOrEqual(1);

    // 验证 skipped 徽标
    expect(screen.getByText("已跳过")).toBeTruthy();

    // 验证按 category 搜索
    const searchInput = screen.getByPlaceholderText(/搜索/i);
    fireEvent.change(searchInput, { target: { value: "pipeline-run" } });

    await waitFor(() => {
      const taskList = screen.getByTestId("task-list");
      expect(within(taskList).getByText("自定义流水线")).toBeTruthy();
      expect(within(taskList).queryByText("会话同步")).toBeNull();
    });
  });

  it("在任务完成终态后常驻展示流程节点，并支持展开回放详细历史步骤 steps", async () => {
    mockSelectedTaskId = "task-replay-1";
    mockTasks = [
      {
        id: "task-replay-1",
        kind: "ConversationSync",
        title: "会话同步历史回放",
        state: "succeeded",
        outcome: "success",
        stages: [
          {
            id: "scan",
            name: "扫描会话源",
            status: "succeeded",
            startedAt: 1000,
            endedAt: 2000,
            progress: {
              current: 12,
              total: 12,
              note: "扫描完成，发现 12 个会话",
            },
            steps: [
              {
                operation: "scan",
                detail: "发现会话 session-001",
                timestamp: 1200,
              },
              {
                operation: "scan",
                detail: "发现会话 session-002",
                timestamp: 1400,
              },
            ],
          },
          {
            id: "sync",
            name: "解析与同步会话",
            status: "succeeded",
            startedAt: 2000,
            endedAt: 5000,
            progress: {
              current: 12,
              total: 12,
              note: "所有会话解析完成",
            },
            steps: [
              {
                operation: "sync_session",
                detail: "同步 session-001 (10 msgs)",
                timestamp: 2500,
              },
            ],
          },
          {
            id: "persist",
            name: "数据入库与索引",
            status: "succeeded",
            startedAt: 5000,
            endedAt: 6000,
            progress: {
              current: 1,
              total: 1,
              note: "全文索引已就绪",
            },
            steps: [],
          },
        ],
        capabilities: { cancellable: false, retryable: false, clearable: true },
        revision: 1,
      } as unknown as TaskView,
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    // 验证所有阶段节点常驻展示，包括已完成状态
    expect(screen.getByText("扫描会话源")).toBeTruthy();
    expect(screen.getByText("解析与同步会话")).toBeTruthy();
    expect(screen.getByText("数据入库与索引")).toBeTruthy();
    expect(screen.getByText("扫描完成，发现 12 个会话")).toBeTruthy();
    expect(screen.getAllByText("已完成").length).toBeGreaterThanOrEqual(3);

    // 默认收起流程明细
    expect(screen.queryByText("发现会话 session-001")).toBeNull();

    // 点击展开第一阶段的流程明细
    const expandButtons = screen.getAllByRole("button", {
      name: /查看详细流程/i,
    });
    expect(expandButtons.length).toBeGreaterThanOrEqual(1);
    fireEvent.click(expandButtons[0]);

    // 验证 steps 展开回放
    await waitFor(() => {
      expect(screen.getByText("发现会话 session-001")).toBeTruthy();
      expect(screen.getByText("发现会话 session-002")).toBeTruthy();
      expect(screen.getByText("收起流程明细")).toBeTruthy();
    });

    // 再次点击收起
    fireEvent.click(screen.getByRole("button", { name: "收起流程明细" }));
    await waitFor(() => {
      expect(screen.queryByText("发现会话 session-001")).toBeNull();
    });
  });

  it("渲染可调整分栏的拖拽分隔手柄与左右面板", () => {
    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const separator = screen.getByRole("separator", { name: "调整分栏宽度" });
    expect(separator).toBeTruthy();
    expect(separator.getAttribute("id")).toBe("task-center-separator");
    expect(document.querySelector("#task-center-left-panel")).toBeTruthy();
    expect(document.querySelector("#task-center-right-panel")).toBeTruthy();
  });

  it("当 Stage 包含 failures 异常时内联渲染阶段报错面板、错误码与路径", async () => {
    mockTasks = [
      {
        id: "task-failed-stage",
        kind: "MemoryExtraction",
        title: "全局记忆提取任务",
        state: "failed",
        outcome: "failure",
        stages: [
          {
            id: "session_facts",
            name: "提取会话事实",
            status: "failed",
            durationMs: 3200,
            progress: { current: 0, total: 10, note: "处理中遇不可恢复错误" },
            failures: [
              {
                code: "agent_not_found",
                message: "未找到配置的 Agent [codebuddy]，无法派发提取任务",
                stage: "session_facts",
                identity: "session-123",
                path: "/path/to/session-123.json",
                retryable: false,
                timestamp: "2026-10-14T10:00:00.000Z",
              },
            ],
            steps: [
              {
                timestamp: "2026-10-14T10:00:00.000Z",
                operation: "提取失败",
                detail: "Agent 缺失错误导致中断",
              },
            ],
            metrics: [],
          },
        ],
        metrics: [],
        failures: [],
        capabilities: { cancellable: false, retryable: true, clearable: true },
        revision: 1,
      } as unknown as TaskView,
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getByText("阶段异常报错 (1)")).toBeTruthy();
      expect(screen.getByText("agent_not_found")).toBeTruthy();
      expect(screen.getByText("致命异常")).toBeTruthy();
      expect(
        screen.getByText("未找到配置的 Agent [codebuddy]，无法派发提取任务"),
      ).toBeTruthy();
      expect(screen.getByText("/path/to/session-123.json")).toBeTruthy();
    });
  });

  it("当任务失败时隐藏绿色执行结果并常驻渲染进度条，无步骤阶段展示友好空态提示", async () => {
    mockTasks = [
      {
        id: "task-memory-failed",
        kind: "Memory",
        title: "Memory 知识流水线",
        state: "failed",
        outcome: "failure",
        errorSummary: "近期记忆快照生成失败: 重试达到上限",
        resultSummary:
          "Memory 流水线执行完毕 (会话成功: 0, 失败: 0, 快照已更新: false)",
        stages: [
          {
            id: "session_facts",
            name: "提取会话事实",
            status: "succeeded",
            steps: [],
            metrics: [],
            failures: [],
          },
          {
            id: "recent_snapshot",
            name: "近期记忆快照",
            status: "failed",
            steps: [],
            metrics: [],
            failures: [],
          },
        ],
        metrics: [],
        failures: [],
        capabilities: { cancellable: false, retryable: true, clearable: true },
        revision: 1,
      } as unknown as TaskView,
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      // 绿色的执行结果应当被屏蔽，展示红色的异常信息
      expect(screen.queryByText("执行结果")).toBeNull();
      expect(
        screen.getAllByText("近期记忆快照生成失败: 重试达到上限").length,
      ).toBeGreaterThanOrEqual(1);

      // 执行进度条常驻展示 (1/2 完成 = 50%)
      expect(screen.getByText("执行进度")).toBeTruthy();
      expect(screen.getByText("50%")).toBeTruthy();

      // 阶段失败时若没有具体 failures 数组项，展示阶段失败并中断提示
      expect(screen.getByText("该阶段执行失败并中断")).toBeTruthy();

      // 无步骤记录时展示空态提示
      const emptyStepHints = screen.getAllByText("暂无子节点明细步骤记录");
      expect(emptyStepHints.length).toBeGreaterThanOrEqual(1);
    });
  });

  it("兼容后端全小写 kind 并渲染阶段 skipped 跳过分组明细", async () => {
    mockSelectedTaskId = "task-usage-lowercase";
    mockTasks = [
      {
        id: "task-usage-lowercase",
        kind: "conversationusagescan",
        title: "扫描会话用量",
        state: "succeeded",
        outcome: "success",
        stages: [
          {
            id: "persist",
            name: "汇总持久化",
            status: "succeeded",
            metrics: [],
            failures: [],
            skipped: [
              {
                reasonCode: "unchanged_hash",
                count: 4,
                samples: ["session-a.jsonl", "session-b.jsonl"],
              },
            ],
            steps: [],
          },
        ],
        metrics: [],
        failures: [],
        capabilities: { cancellable: false, retryable: false, clearable: true },
        revision: 2,
      },
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getAllByText("用量统计扫描").length).toBeGreaterThanOrEqual(
        1,
      );
      expect(screen.getByText(/unchanged_hash/)).toBeTruthy();
      expect(screen.getByText(/4/)).toBeTruthy();
      expect(screen.getByText(/session-a\.jsonl/)).toBeTruthy();
    });
  });

  it("区分 pending 排队状态与运行中状态，并展示资源域标签", async () => {
    mockSelectedTaskId = "task-queued-sqlite";
    mockTasks = [
      {
        id: "task-queued-sqlite",
        kind: "backup",
        resourcePool: "sqlite_write",
        title: "数据库增量备份",
        state: "pending",
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 1,
      },
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getAllByText("排队中").length).toBeGreaterThanOrEqual(1);
      expect(screen.queryByText("运行中")).toBeNull();
      expect(
        screen.getAllByText(/数据库写入|sqlite_write/).length,
      ).toBeGreaterThanOrEqual(1);
    });
  });

  it("右侧详情将固定摘要顶栏与独立阶段时间线滚动区分离，并采用紧凑单行元信息与指标条带", async () => {
    mockSelectedTaskId = "task-layout-1";
    mockTasks = [
      {
        id: "task-layout-1",
        kind: "ConversationSync",
        category: "conversation/session_sync",
        resourcePool: "network",
        tenantId: "default",
        title: "固定顶栏布局验证任务",
        state: "succeeded",
        outcome: "success",
        startedAt: "2026-09-10T03:00:00Z",
        finishedAt: "2026-09-10T03:05:30Z",
        resultSummary: "已完成全部 120 个会话的同步与索引写入，无任何失败记录",
        metrics: [
          { code: "m1", value: 10 },
          { code: "m2", value: 20 },
          { code: "m3", value: 30 },
          { code: "m4", value: 40 },
          { code: "m5", value: 50 },
          { code: "m6", value: 60 },
          { code: "m7", value: 70 },
        ],
        stages: [
          {
            id: "s1",
            name: "阶段一",
            status: "succeeded",
            durationMs: 1200,
            metrics: [],
            failures: [],
            skipped: [],
          },
        ],
        failures: [],
        capabilities: { cancellable: false, retryable: false, clearable: true },
        revision: 1,
      },
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const summaryHeader = await screen.findByTestId(
      "task-detail-summary-header",
    );
    const timelineScroll = screen.getByTestId("task-detail-timeline-scroll");

    expect(summaryHeader.className).toContain("shrink-0");
    expect(timelineScroll.className).toContain("flex-1");
    expect(timelineScroll.className).toContain("min-h-0");
    expect(timelineScroll.className).toContain("overflow-y-auto");
    expect(summaryHeader.contains(timelineScroll)).toBe(false);

    // 阶段时间线位于独立滚动区内，而非顶部摘要卡内
    expect(within(timelineScroll).getByText("阶段一")).toBeTruthy();

    // 顶部元信息单行防换行且时间携带完整 tooltip
    const metaRow = screen.getByTestId("task-detail-meta-row");
    expect(metaRow.className).toContain("whitespace-nowrap");
    const startTimeEl = within(metaRow).getByTitle(
      new Date("2026-09-10T03:00:00Z").toLocaleString(),
    );
    expect(startTimeEl.className).toContain("tabular-nums");

    // 指标条带为单行横向滚动容器且数字等宽
    const metricsStrip = screen.getByTestId("task-detail-metrics-strip");
    expect(metricsStrip.className).toContain("flex-nowrap");
    expect(metricsStrip.className).toContain("overflow-x-auto");
    expect(within(metricsStrip).getByText("70").className).toContain(
      "tabular-nums",
    );
  });

  it("左侧任务列表行跨状态维持一致的两行固定骨架，并将运行期进度轨贴合底边绝对定位而不撑高行高", async () => {
    mockSelectedTaskId = "task-row-running-det";
    mockTasks = [
      {
        id: "task-row-pending",
        kind: "ConversationSync",
        title: "排队中任务",
        state: "pending",
        startedAt: "2026-09-10T03:00:00Z",
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 1,
      },
      {
        id: "task-row-running-det",
        kind: "ConversationSync",
        title: "运行中确定进度任务",
        state: "running",
        startedAt: "2026-09-10T03:01:00Z",
        progress: { current: 3, total: 10, note: "正在同步第 3/10 批" },
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 2,
      },
      {
        id: "task-row-running-indet",
        kind: "Memory",
        title: "运行中不确定进度任务",
        state: "running",
        startedAt: "2026-09-10T03:02:00Z",
        progress: { current: 4, total: null, note: "正在分析上下文" },
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 2,
      },
      {
        id: "task-row-done",
        kind: "Backup",
        title: "已完成任务",
        state: "succeeded",
        outcome: "success",
        startedAt: "2026-09-10T02:50:00Z",
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: false, retryable: false, clearable: true },
        revision: 3,
      },
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const taskList = await screen.findByTestId("task-list");
    const rows = within(taskList).getAllByRole("button");
    expect(rows).toHaveLength(4);

    for (const row of rows) {
      expect(row.className).toContain("overflow-hidden");
      expect(row.className).toContain("min-h-[68px]");
    }

    // 运行中确定进度行：百分比采用 tabular-nums 并入固定元信息行，底边存在绝对定位进度轨
    const detRow = within(taskList)
      .getByText("运行中确定进度任务")
      .closest("button")!;
    const detPercent = within(detRow).getByText("30%");
    expect(detPercent.className).toContain("tabular-nums");

    const detRail = within(detRow).getByTestId(
      "task-row-progress-rail-task-row-running-det",
    );
    expect(detRail.className).toContain("absolute");
    expect(detRail.className).toContain("bottom-0");

    // 运行中不确定进度行：同样渲染底边绝对定位不确定进度轨
    const indetRow = within(taskList)
      .getByText("运行中不确定进度任务")
      .closest("button")!;
    const indetRail = within(indetRow).getByTestId(
      "task-row-progress-rail-task-row-running-indet",
    );
    expect(indetRail.className).toContain("absolute");
    expect(indetRail.className).toContain("bottom-0");
    expect(indetRail.querySelector("[data-indeterminate='true']")).toBeTruthy();

    // 终态行与排队行：不渲染底边进度轨
    const doneRow = within(taskList).getByText("已完成任务").closest("button")!;
    expect(
      within(doneRow).queryByTestId("task-row-progress-rail-task-row-done"),
    ).toBeNull();
  });

  it("运行中与失败阶段默认折叠 steps，运行态展示单行最新步骤预览与总数并支持手动展开收起", async () => {
    const runningTask: TaskView = {
      id: "task-steps-collapse",
      kind: "ConversationSync",
      title: "步骤默认折叠测试任务",
      state: "running",
      stages: [
        {
          id: "sync-stage",
          name: "增量同步阶段",
          status: "running",
          progress: { current: 2, total: 5, note: "同步中" },
          steps: [
            {
              timestamp: "2026-09-10T03:00:01Z",
              operation: "fetch",
              detail: "拉取历史消息批次 #1",
              current: 1,
              total: 5,
            },
            {
              timestamp: "2026-09-10T03:00:02Z",
              operation: "parse",
              detail: "解析会话 /very/long/path/session-latest.jsonl 的内容",
              current: 2,
              total: 5,
            },
          ],
          metrics: [],
          failures: [],
          skipped: [],
        },
        {
          id: "failed-stage",
          name: "失败阶段",
          status: "failed",
          steps: [
            {
              timestamp: "2026-09-10T03:00:03Z",
              operation: "index",
              detail: "失败阶段历史步骤明细不应自动展开",
            },
          ],
          metrics: [],
          failures: [],
          skipped: [],
        },
      ],
      metrics: [],
      failures: [],
      capabilities: { cancellable: true, retryable: false, clearable: false },
      revision: 1,
    };

    mockSelectedTaskId = "task-steps-collapse";
    mockTasks = [runningTask];

    const { rerender } = render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    // 1. running 与 failed 阶段均默认折叠，完整历史步骤不渲染
    expect(screen.queryByText("拉取历史消息批次 #1")).toBeNull();
    expect(screen.queryByText("失败阶段历史步骤明细不应自动展开")).toBeNull();

    // 2. running 阶段在折叠态展示总步数与最新一条步骤的单行截断预览
    expect(
      screen.getByRole("button", { name: /查看详细流程 \(2 个子节点步骤\)/ }),
    ).toBeTruthy();
    const preview = screen.getByTestId("stage-latest-step-preview-sync-stage");
    expect(within(preview).getByText("parse")).toBeTruthy();
    const previewDetail = within(preview).getByText(
      "解析会话 /very/long/path/session-latest.jsonl 的内容",
    );
    expect(previewDetail.className).toContain("truncate");
    expect(preview.getAttribute("title")).toBeNull();
    expect(previewDetail.getAttribute("title")).toBeNull();
    Object.defineProperty(previewDetail, "clientWidth", {
      configurable: true,
      get: () => 120,
    });
    Object.defineProperty(previewDetail, "scrollWidth", {
      configurable: true,
      get: () => 320,
    });
    Object.defineProperty(previewDetail, "clientHeight", {
      configurable: true,
      get: () => 20,
    });
    Object.defineProperty(previewDetail, "scrollHeight", {
      configurable: true,
      get: () => 20,
    });
    fireEvent.mouseEnter(previewDetail);
    await waitFor(() =>
      expect(screen.getByRole("tooltip").textContent).toBe(
        "解析会话 /very/long/path/session-latest.jsonl 的内容",
      ),
    );
    fireEvent.mouseLeave(previewDetail);

    // 3. 当 running 阶段更新更多步骤或状态变为 failed 时，不会自动强行展开
    mockTasks = [
      {
        ...runningTask,
        state: "failed",
        outcome: "failure",
        stages: [
          {
            ...runningTask.stages[0],
            status: "failed",
            steps: [
              ...(runningTask.stages[0].steps ?? []),
              {
                timestamp: "2026-09-10T03:00:04Z",
                operation: "abort",
                detail: "阶段异常中断第 3 步",
              },
            ],
          },
          runningTask.stages[1],
        ],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    expect(screen.queryByText("拉取历史消息批次 #1")).toBeNull();
    expect(screen.queryByText("阶段异常中断第 3 步")).toBeNull();
    // 终态/failed 折叠态不再显示运行中单行预览，保持与历史回放视图一致
    expect(
      screen.queryByTestId("stage-latest-step-preview-sync-stage"),
    ).toBeNull();

    // 4. 用户手动点击展开后可查看全部步骤，再次点击可收起
    fireEvent.click(
      screen.getByRole("button", { name: /查看详细流程 \(3 个子节点步骤\)/ }),
    );
    await waitFor(() => {
      expect(screen.getByText("拉取历史消息批次 #1")).toBeTruthy();
      expect(screen.getByText("阶段异常中断第 3 步")).toBeTruthy();
    });

    fireEvent.click(screen.getByRole("button", { name: "收起流程明细" }));
    await waitFor(() => {
      expect(screen.queryByText("拉取历史消息批次 #1")).toBeNull();
    });
  });

  it("展开后的步骤列表使用固定高度滚动容器，仅在滚动条贴近底部时自动跟随新增步骤", async () => {
    const baseTask: TaskView = {
      id: "task-steps-scroll",
      kind: "ConversationSync",
      title: "步骤定高智能滚动测试",
      state: "running",
      stages: [
        {
          id: "scroll-stage",
          name: "流式步骤阶段",
          status: "running",
          steps: [
            {
              timestamp: "2026-09-10T03:00:01Z",
              operation: "step-1",
              detail: "步骤一",
            },
            {
              timestamp: "2026-09-10T03:00:02Z",
              operation: "step-2",
              detail: "步骤二",
            },
          ],
          metrics: [],
          failures: [],
          skipped: [],
        },
      ],
      metrics: [],
      failures: [],
      capabilities: { cancellable: true, retryable: false, clearable: false },
      revision: 1,
    };

    mockSelectedTaskId = "task-steps-scroll";
    mockTasks = [baseTask];

    const { rerender } = render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    // 点击展开
    fireEvent.click(
      screen.getByRole("button", { name: /查看详细流程 \(2 个子节点步骤\)/ }),
    );

    const scrollBox = await screen.findByTestId(
      "stage-steps-list-scroll-stage",
    );
    expect(scrollBox.className).toContain("h-56");
    expect(scrollBox.className).toContain("overflow-y-auto");
    expect(scrollBox.className).not.toContain("max-h-64");

    // 模拟滚动几何：容器可视高度 224，总高 800，当前贴近底部 (scrollTop = 576)
    let currentScrollHeight = 800;
    let currentScrollTop = 576;
    Object.defineProperty(scrollBox, "clientHeight", {
      configurable: true,
      get: () => 224,
    });
    Object.defineProperty(scrollBox, "scrollHeight", {
      configurable: true,
      get: () => currentScrollHeight,
    });
    Object.defineProperty(scrollBox, "scrollTop", {
      configurable: true,
      get: () => currentScrollTop,
      set: (val: number) => {
        currentScrollTop = val;
      },
    });

    fireEvent.scroll(scrollBox);

    // 新增步骤 3：处于底部阈值内，应自动滚动到新底部
    currentScrollHeight = 860;
    mockTasks = [
      {
        ...baseTask,
        revision: 2,
        stages: [
          {
            ...baseTask.stages[0],
            steps: [
              ...(baseTask.stages[0].steps ?? []),
              {
                timestamp: "2026-09-10T03:00:03Z",
                operation: "step-3",
                detail: "步骤三",
              },
            ],
          },
        ],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getByText("步骤三")).toBeTruthy();
      expect(currentScrollTop).toBe(860);
    });

    // 用户向上回看早期步骤（远离底部）
    currentScrollTop = 40;
    fireEvent.scroll(scrollBox);

    // 新增步骤 4：用户不在底部，不应强行拉回底部
    currentScrollHeight = 920;
    mockTasks = [
      {
        ...baseTask,
        revision: 3,
        stages: [
          {
            ...baseTask.stages[0],
            steps: [
              ...(mockTasks[0].stages[0].steps ?? []),
              {
                timestamp: "2026-09-10T03:00:04Z",
                operation: "step-4",
                detail: "步骤四",
              },
            ],
          },
        ],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    await waitFor(() => {
      expect(screen.getByText("步骤四")).toBeTruthy();
    });
    expect(currentScrollTop).toBe(40);
  });

  it("运行态阶段无 total 时常驻渲染不确定进度条槽位，note 与 worker 路径保持单行定高截断且活跃 Workers 容器在 0→多→0 波动时不卸载", async () => {
    const stageBase: TaskView["stages"][number] = {
      id: "ingest-stage",
      name: "并发抓取阶段",
      status: "running",
      durationMs: 2400,
      progress: {
        current: 3,
        total: null,
        note: "正在扫描 /Users/demo/workspace/very-long-directory/sub-module/file.jsonl",
      },
      currentActivities: [],
      metrics: [],
      failures: [],
      skipped: [],
      steps: [],
    };

    const taskFrame1: TaskView = {
      id: "task-stage-stability",
      kind: "ConversationSync",
      title: "阶段防抖稳定性测试任务",
      state: "running",
      stages: [stageBase],
      metrics: [],
      failures: [],
      capabilities: { cancellable: true, retryable: false, clearable: false },
      revision: 1,
    };

    mockSelectedTaskId = "task-stage-stability";
    mockTasks = [taskFrame1];

    const { rerender } = render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    // 1. progress.note 单行截断并带 TruncatedText 悬停提示
    const noteEl = await screen.findByTestId("stage-note-ingest-stage");
    expect(noteEl.className).toContain("truncate");
    expect(noteEl.className).toContain("h-5");
    expect(noteEl.getAttribute("title")).toBeNull();
    Object.defineProperty(noteEl, "clientWidth", {
      configurable: true,
      get: () => 120,
    });
    Object.defineProperty(noteEl, "scrollWidth", {
      configurable: true,
      get: () => 360,
    });
    Object.defineProperty(noteEl, "clientHeight", {
      configurable: true,
      get: () => 20,
    });
    Object.defineProperty(noteEl, "scrollHeight", {
      configurable: true,
      get: () => 20,
    });
    fireEvent.mouseEnter(noteEl);
    await waitFor(() =>
      expect(screen.getByRole("tooltip").textContent).toBe(
        "正在扫描 /Users/demo/workspace/very-long-directory/sub-module/file.jsonl",
      ),
    );
    fireEvent.mouseLeave(noteEl);

    // 2. 耗时数字使用 tabular-nums
    const durationEl = screen.getByText("耗时: 2.4s");
    expect(durationEl.className).toContain("tabular-nums");

    // 3. total 缺失时仍常驻渲染阶段进度条占位（不确定态）
    const progressSlot = screen.getByTestId("stage-progress-ingest-stage");
    expect(
      progressSlot.querySelector("[data-indeterminate='true']"),
    ).toBeTruthy();

    // 4. currentActivities=0 时，活跃 Workers 固定高度容器仍常驻并展示静态等待提示
    expect(screen.getByText("活跃 Workers (0)")).toBeTruthy();
    const workersContainer = screen.getByTestId(
      "stage-activities-container-ingest-stage",
    );
    expect(workersContainer.className).toContain("h-28");
    expect(workersContainer.className).toContain("overflow-y-auto");
    expect(
      within(workersContainer).getByText(/暂无并发执行中的 Worker/),
    ).toBeTruthy();

    // 5. 帧 2：total 到达 + 2 个 Workers（一个带长 path，一个无 path）+ note 临时为空
    mockTasks = [
      {
        ...taskFrame1,
        revision: 2,
        stages: [
          {
            ...stageBase,
            progress: { current: 5, total: 10, note: "" },
            currentActivities: [
              {
                stageId: "ingest-stage",
                workerId: "w-with-path",
                operation: "解析超长会话文件操作名称",
                path: "/Users/demo/very/long/path/to/session-01.jsonl",
                current: 5,
                total: 10,
              },
              {
                stageId: "ingest-stage",
                workerId: "w-no-path",
                operation: "等待下一批任务",
                path: null,
              },
            ],
          },
        ],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    // note 为空时仍保留 h-5 槽位
    const noteElFrame2 = screen.getByTestId("stage-note-ingest-stage");
    expect(noteElFrame2.className).toContain("h-5");

    // 进度条平滑切换为确定态 50%，数值使用 tabular-nums
    const progressFrame2 = screen.getByTestId("stage-progress-ingest-stage");
    expect(
      progressFrame2.querySelector("[data-indeterminate='true']"),
    ).toBeNull();
    expect(within(progressFrame2).getByText("50%")).toBeTruthy();
    expect(within(progressFrame2).getByText("5 / 10")).toBeTruthy();

    // 活跃 Workers 为 2，带路径与无路径子卡均保留固定单行 path 槽位
    expect(screen.getByText("活跃 Workers (2)")).toBeTruthy();
    const withPathSlot = screen.getByTestId(
      "stage-activity-path-slot-w-with-path",
    );
    expect(withPathSlot.className).toContain("h-5");
    const pathText = within(withPathSlot).getByText(
      "~/very/long/path/to/session-01.jsonl",
    );
    expect(pathText.className).toContain("truncate");
    expect(pathText.getAttribute("title")).toBeNull();
    Object.defineProperty(pathText, "clientWidth", {
      configurable: true,
      get: () => 100,
    });
    Object.defineProperty(pathText, "scrollWidth", {
      configurable: true,
      get: () => 280,
    });
    Object.defineProperty(pathText, "clientHeight", {
      configurable: true,
      get: () => 20,
    });
    Object.defineProperty(pathText, "scrollHeight", {
      configurable: true,
      get: () => 20,
    });
    fireEvent.mouseEnter(pathText);
    await waitFor(() =>
      expect(screen.getByRole("tooltip").textContent).toBe(
        "/Users/demo/very/long/path/to/session-01.jsonl",
      ),
    );
    fireEvent.mouseLeave(pathText);

    const noPathSlot = screen.getByTestId("stage-activity-path-slot-w-no-path");
    expect(noPathSlot.className).toContain("h-5");
    expect(within(noPathSlot).getByText("—")).toBeTruthy();

    // 6. 帧 3：currentActivities 回落到 0，容器保持存在而不卸载
    mockTasks = [
      {
        ...taskFrame1,
        revision: 3,
        stages: [
          {
            ...stageBase,
            progress: { current: 10, total: 10, note: "收尾中" },
            currentActivities: [],
          },
        ],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    expect(screen.getByText("活跃 Workers (0)")).toBeTruthy();
    expect(
      screen.getByTestId("stage-activities-container-ingest-stage"),
    ).toBeTruthy();
  });

  it("在右侧摘要卡标题旁展示状态徽章，并在运行中 0 指标时预留等高 h-7 指标占位条带而终态收起", async () => {
    const runningNoMetrics: TaskView = {
      id: "task-summary-stability",
      kind: "ConversationSync",
      title: "摘要卡状态与指标稳定性任务",
      state: "running",
      stages: [],
      metrics: [],
      failures: [],
      capabilities: { cancellable: true, retryable: false, clearable: false },
      revision: 1,
    };

    mockSelectedTaskId = "task-summary-stability";
    mockTasks = [runningNoMetrics];

    const { rerender } = render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const summaryHeader = await screen.findByTestId(
      "task-detail-summary-header",
    );
    expect(within(summaryHeader).getByText("运行中")).toBeTruthy();

    const reservedStrip = within(summaryHeader).getByTestId(
      "task-detail-metrics-strip",
    );
    expect(reservedStrip.className).toContain("h-7");
    expect(within(reservedStrip).getByText("等待阶段指标上报...")).toBeTruthy();

    // 指标上报后保持 h-7，不触发跳动
    mockTasks = [
      {
        ...runningNoMetrics,
        revision: 2,
        metrics: [{ code: "synced", value: 12 }],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const populatedStrip = within(summaryHeader).getByTestId(
      "task-detail-metrics-strip",
    );
    expect(populatedStrip.className).toContain("h-7");
    expect(
      within(populatedStrip).queryByText("等待阶段指标上报..."),
    ).toBeNull();
    expect(within(populatedStrip).getByText("12")).toBeTruthy();

    // 终态且无指标时收起指标条带，并更新标题旁状态徽章
    mockTasks = [
      {
        ...runningNoMetrics,
        state: "succeeded",
        outcome: "success",
        revision: 3,
        metrics: [],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    expect(within(summaryHeader).getByText("成功")).toBeTruthy();
    expect(
      within(summaryHeader).queryByTestId("task-detail-metrics-strip"),
    ).toBeNull();
  });

  it("将 progress.total === 0 视为确定态 0% 而非不确定态或 NaN", async () => {
    mockSelectedTaskId = "task-zero-total";
    mockTasks = [
      {
        id: "task-zero-total",
        kind: "Scan",
        title: "零总数任务",
        state: "running",
        progress: { current: 0, total: 0, note: "空目录扫描" },
        stages: [
          {
            id: "scan-stage",
            name: "扫描文件",
            status: "running",
            progress: { current: 0, total: 0, note: "待发现项目" },
            steps: [],
            metrics: [],
            failures: [],
            skipped: [],
          },
        ],
        metrics: [],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 1,
      },
    ];

    render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const taskList = await screen.findByTestId("task-list");
    const row = within(taskList).getByText("零总数任务").closest("button")!;
    expect(within(row).getByText("0%")).toBeTruthy();
    expect(within(row).queryByText("NaN%")).toBeNull();

    const rail = within(row).getByTestId(
      "task-row-progress-rail-task-zero-total",
    );
    expect(rail.querySelector("[data-indeterminate='true']")).toBeNull();

    const stageProgress = screen.getByTestId("stage-progress-scan-stage");
    expect(within(stageProgress).getByText("0 / 0")).toBeTruthy();
    expect(within(stageProgress).getByText("0%")).toBeTruthy();
    expect(
      stageProgress.querySelector("[data-indeterminate='true']"),
    ).toBeNull();
  });

  it("约束运行态最新步骤预览中超长 operation 宽度，保持 0→1 步骤容器高度一致且数字对齐", async () => {
    const longOp =
      "very_long_operation_identifier_that_should_be_clamped_within_preview_pill";
    const baseStage: TaskView["stages"][number] = {
      id: "step-parity-stage",
      name: "步骤稳态阶段",
      status: "running",
      steps: [],
      metrics: [],
      failures: [
        {
          code: "warn_code",
          message: "模拟报错",
          stage: "step-parity-stage",
          path: "/err/path.ts",
          retryable: true,
          timestamp: "2026-09-10T03:00:00Z",
        },
      ],
      currentActivities: [
        {
          stageId: "step-parity-stage",
          workerId: "w-parity",
          operation: "scan",
          path: "/worker/path.ts",
        },
      ],
      skipped: [{ reasonCode: "dup", count: 2, samples: ["a"] }],
    };

    mockSelectedTaskId = "task-step-parity";
    mockTasks = [
      {
        id: "task-step-parity",
        kind: "ConversationSync",
        title: "步骤稳态验证",
        state: "running",
        stages: [baseStage],
        metrics: [],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 1,
      },
    ];

    const { rerender } = render(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    // 0 步骤态：外层 pt-1 与内层 h-7 分离（h-7 元素自身不带 pt-1）
    const emptyWrapper = await screen.findByTestId(
      "stage-steps-wrapper-step-parity-stage",
    );
    expect(emptyWrapper.className).toContain("pt-1");
    const emptyNotice =
      within(emptyWrapper).getByText("暂无子节点明细步骤记录");
    const emptyRow = emptyNotice.closest("div")!;
    expect(emptyRow.className).toContain("h-7");
    expect(emptyRow.className).not.toContain("pt-1");

    // 报错和跳过计数均使用 tabular-nums，复制路径按钮使用 Button 原语及微动效
    expect(screen.getByText("阶段异常报错 (1)").className).toContain(
      "tabular-nums",
    );
    expect(screen.getByText("已跳过项目分组 (1)").className).toContain(
      "tabular-nums",
    );
    const failCopyBtn = screen.getByRole("button", { name: "复制路径/标识" });
    expect(failCopyBtn.className).toContain("rounded-full");
    expect(failCopyBtn.className).toContain("duration-200");
    expect(failCopyBtn.className).toContain("hover:-translate-y-px");
    const workerCopyBtn = screen.getByRole("button", { name: "复制路径" });
    expect(workerCopyBtn.className).toContain("rounded-full");
    expect(workerCopyBtn.className).toContain("duration-200");
    expect(workerCopyBtn.className).toContain("hover:-translate-y-px");

    // 1 步骤态：外层保持同一 pt-1 + 内层 h-7，且长 operation 被 max-w-[40%] truncate 约束
    mockTasks = [
      {
        ...mockTasks[0],
        revision: 2,
        stages: [
          {
            ...baseStage,
            steps: [
              {
                timestamp: "2026-09-10T03:00:01Z",
                operation: longOp,
                detail: "不应被超长 operation 挤出视野的关键明细",
                current: 1,
                total: 3,
              },
            ],
          },
        ],
      },
    ];
    rerender(
      <I18nProvider>
        <TaskCenterModal onClose={mockOnClose} open={true} />
      </I18nProvider>,
    );

    const populatedWrapper = screen.getByTestId(
      "stage-steps-wrapper-step-parity-stage",
    );
    expect(populatedWrapper.className).toContain("pt-1");
    const toggleBtn = within(populatedWrapper).getByRole("button", {
      name: /查看详细流程 \(1 个子节点步骤\)/,
    });
    expect(toggleBtn.className).toContain("duration-200");
    expect(toggleBtn.className).toContain("hover:-translate-y-px");

    const preview = screen.getByTestId(
      "stage-latest-step-preview-step-parity-stage",
    );
    const opEl = within(preview).getByText(longOp);
    expect(opEl.className).toContain("max-w-[40%]");
    expect(opEl.className).toContain("truncate");
    expect(opEl.className).toContain("shrink-0");
    expect(opEl.getAttribute("title")).toBeNull();
    Object.defineProperty(opEl, "clientWidth", {
      configurable: true,
      get: () => 80,
    });
    Object.defineProperty(opEl, "scrollWidth", {
      configurable: true,
      get: () => 300,
    });
    Object.defineProperty(opEl, "clientHeight", {
      configurable: true,
      get: () => 20,
    });
    Object.defineProperty(opEl, "scrollHeight", {
      configurable: true,
      get: () => 20,
    });
    fireEvent.mouseEnter(opEl);
    await waitFor(() =>
      expect(screen.getByRole("tooltip").textContent).toBe(longOp),
    );
    fireEvent.mouseLeave(opEl);
  });
});
