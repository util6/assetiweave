// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { UsageDashboardPage } from "./UsageDashboardPage";
import type { TaskView } from "../../types/taskCenter";
import type { UsageDashboardDto } from "../../types/usage";

/**
 * 该测试聚焦页面与工具栏之间的扫描契约：模式、实例筛选、运行态来源与终态提示。
 * 工具栏自身交互由 usageComponents.test.tsx 覆盖，这里用轻量替身暴露回调。
 */
vi.mock("../../components/conversations/usage/UsageToolbar", () => ({
  UsageToolbar: (props: {
    scanMode: string;
    scanning: boolean;
    onInstanceChange: (id: string) => void;
    onScanModeChange: (mode: "incremental" | "full") => void;
    onScan: () => void;
  }) => (
    <div>
      <span data-testid="scan-mode">{props.scanMode}</span>
      <span data-testid="scanning">{String(props.scanning)}</span>
      <button onClick={() => props.onInstanceChange("src-1")} type="button">
        选择实例
      </button>
      <button onClick={() => props.onScanModeChange("full")} type="button">
        切换到全量
      </button>
      <button onClick={() => props.onScan()} type="button">
        开始扫描
      </button>
    </div>
  ),
}));

const taskCenterState = vi.hoisted(() => ({
  current: null as unknown,
}));

vi.mock("../../app/backgroundTasks/TaskCenterProvider", () => ({
  useOptionalTaskCenter: () => taskCenterState.current,
}));

const dashboardMock = vi.hoisted(() => vi.fn());
const scanStatusMock = vi.hoisted(() => vi.fn());
const scanMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/usage", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../services/usage")>();
  return {
    ...actual,
    getConversationUsageDashboard: dashboardMock,
    getConversationUsageScanStatus: scanStatusMock,
    scanConversationUsage: scanMock,
  };
});

const baseScanStatus = {
  scannedSourcesCount: 3,
  totalEventsCount: 1200,
  lastScannedAt: "2026-01-02T03:04:05Z",
  activeScanTaskId: null,
  sourceDiagnostics: [],
};

function buildDashboard(): UsageDashboardDto {
  return {
    hero: {
      totalTokens: 0,
      totalInputTokens: 0,
      totalOutputTokens: 0,
      cacheReadTokens: 0,
      cacheWriteTokens: 0,
      reasoningTokens: 0,
      requestsCount: 0,
      costsByCurrency: [],
      costCoveredTokens: 0,
      costCoverageRatio: 0,
      costCoveredRequests: 0,
    },
    dailyTrend: [],
    models: [],
    sources: [],
    projects: [],
    dates: [],
    scanStatus: baseScanStatus,
  };
}

function buildScanTask(overrides: Partial<TaskView> = {}): TaskView {
  return {
    id: "task-usage-scan",
    kind: "conversationusagescan",
    category: "conversation/usage_scan",
    title: "用量统计扫描",
    state: "succeeded",
    outcome: "success",
    stages: [],
    metrics: [],
    failures: [],
    capabilities: { cancellable: false, retryable: false, clearable: true },
    revision: 1,
    ...overrides,
  };
}

describe("UsageDashboardPage scan orchestration", () => {
  beforeEach(() => {
    dashboardMock.mockReset();
    scanStatusMock.mockReset();
    scanMock.mockReset();
    dashboardMock.mockResolvedValue(buildDashboard());
    scanStatusMock.mockResolvedValue({ ...baseScanStatus });
    scanMock.mockResolvedValue({ id: "task-usage-scan", mode: "incremental" });
    taskCenterState.current = {
      tasks: [],
      refresh: vi.fn(),
    };
  });

  afterEach(() => {
    cleanup();
  });

  it("triggers full rescan with the selected instance source id", async () => {
    render(<UsageDashboardPage />);
    await waitFor(() => expect(dashboardMock).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("button", { name: "切换到全量" }));
    fireEvent.click(screen.getByRole("button", { name: "选择实例" }));
    fireEvent.click(screen.getByRole("button", { name: "开始扫描" }));

    await waitFor(() =>
      expect(scanMock).toHaveBeenCalledWith({
        mode: "full",
        sourceId: "src-1",
      }),
    );
  });

  it("keeps incremental mode by default and omits source filter for all instances", async () => {
    render(<UsageDashboardPage />);
    await waitFor(() => expect(dashboardMock).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("button", { name: "开始扫描" }));

    await waitFor(() =>
      expect(scanMock).toHaveBeenCalledWith({
        mode: "incremental",
        sourceId: undefined,
      }),
    );
  });

  it("drives running state and terminal copy from the task center", async () => {
    const { rerender } = render(<UsageDashboardPage />);
    await waitFor(() => expect(dashboardMock).toHaveBeenCalled());

    fireEvent.click(screen.getByRole("button", { name: "切换到全量" }));
    fireEvent.click(screen.getByRole("button", { name: "开始扫描" }));
    await waitFor(() =>
      expect(screen.getByTestId("scanning").textContent).toBe("true"),
    );

    taskCenterState.current = {
      tasks: [
        buildScanTask({
          state: "succeeded",
          result: {
            mode: "full",
            total_sources: 3,
            completed_sources: 3,
            successful_sources: 3,
            failed_sources: 0,
            total_events_ingested: 42,
            recalculated_events_count: 42,
          },
        }),
      ],
      refresh: vi.fn(),
    };
    rerender(<UsageDashboardPage />);

    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain(
        "已全量刷新 42 条用量事件",
      ),
    );
  });

  it("surfaces failed scan summary from task center failures", async () => {
    taskCenterState.current = {
      tasks: [buildScanTask({ state: "running" })],
      refresh: vi.fn(),
    };
    const { rerender } = render(<UsageDashboardPage />);
    await waitFor(() =>
      expect(screen.getByTestId("scanning").textContent).toBe("true"),
    );

    taskCenterState.current = {
      tasks: [
        buildScanTask({
          state: "failed",
          outcome: "failure",
          errorSummary: null,
          failures: [
            {
              code: "usage_source_read_failed",
              message: "数据源【实例 A】读取用量失败",
              stage: "collect_usage",
              retryable: true,
              timestamp: "2026-01-02T00:00:00Z",
            },
          ],
        }),
      ],
      refresh: vi.fn(),
    };
    rerender(<UsageDashboardPage />);

    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain(
        "数据源【实例 A】读取用量失败",
      ),
    );
  });
});
