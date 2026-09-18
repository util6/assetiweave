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
import { I18nProvider } from "../../i18n/I18nProvider";
import { MemoryRecentWorkspace } from "./MemoryRecentWorkspace";
import type {
  RecentMemorySnapshotView,
  RecentMemoryStateView,
} from "../../types/memory";

const memoryService = vi.hoisted(() => ({
  getRecentMemorySnapshot: vi.fn(),
  subscribeMemoryTasks: vi.fn(),
}));

vi.mock("../../services/memory", () => memoryService);

function mockSnapshot(): RecentMemorySnapshotView {
  return {
    snapshotId: "snap-1",
    sequence: 1,
    targetWatermark: "2026-09-15T14:00:00Z",
    windowStart: "2026-09-13T14:00:00Z",
    windowEnd: "2026-09-15T14:00:00Z",
    windowHours: 48,
    publicationKind: "generated",
    reusedFromSnapshotId: null,
    contentGeneratedAt: "2026-09-15T14:00:00Z",
    publishedAt: "2026-09-15T14:00:00Z",
    projects: [
      {
        projectKey: "proj-alpha",
        projectTitle: "Alpha Project",
        projectPath: "/code/alpha",
        summary: "Migrated database schema to v2",
        noMaterialChange: false,
        latestActivityAt: "2026-09-15T12:00:00Z",
        sourceSessionCount: 2,
        items: [
          {
            itemId: "item-1",
            revisionId: "rev-1",
            category: "decision",
            status: "active",
            title: "Adopt Single Layer Symlink",
            summary: "Direct symlinks from app dir",
            rationale: "Prevent complexity of middle link pool",
            occurredAt: "2026-09-15T10:00:00Z",
            recommendationRank: 1,
            sourceAvailability: "available",
            sessionReferences: [
              {
                sourceId: "src-1",
                sessionId: "sess-1",
                sessionTitle: "Symlink Architecture Discussion",
                sourceAgent: "antigravity",
                lastActivityAt: "2026-09-15T10:00:00Z",
                available: true,
                unavailableReason: null,
              },
              {
                sourceId: "src-2",
                sessionId: "sess-2",
                sessionTitle: "Old Deleted Session",
                sourceAgent: "claude-code",
                lastActivityAt: "2026-09-15T09:00:00Z",
                available: false,
                unavailableReason: "Session file missing",
              },
            ],
          },
          {
            itemId: "item-2",
            revisionId: "rev-2",
            category: "verification",
            status: "active",
            title: "Pass E2E Test Suite",
            summary: "All 5 slices verified",
            rationale: "Ensure zero regression",
            occurredAt: "2026-09-14T16:00:00Z",
            recommendationRank: 2,
            sourceAvailability: "available",
            sessionReferences: [],
          },
        ],
      },
      {
        projectKey: "proj-beta",
        projectTitle: "Beta Project",
        projectPath: "/code/beta",
        summary: "UI design system alignment",
        noMaterialChange: false,
        latestActivityAt: "2026-09-14T08:00:00Z",
        sourceSessionCount: 1,
        items: [
          {
            itemId: "item-3",
            revisionId: "rev-3",
            category: "research",
            status: "resolved",
            title: "AssetIWeave Design Benchmark",
            summary: "Examined pill tab interactions",
            rationale: "High standard aesthetic",
            occurredAt: "2026-09-14T08:00:00Z",
            recommendationRank: null,
            sourceAvailability: "available",
            sessionReferences: [],
          },
        ],
      },
    ],
  };
}

beforeEach(() => {
  vi.stubGlobal("localStorage", createMockLocalStorage());
  localStorage.setItem("assetiweave.locale", "zh");
  vi.clearAllMocks();
  memoryService.subscribeMemoryTasks.mockResolvedValue(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("MemoryRecentWorkspace (M35-UI-01 through M35-UI-07)", () => {
  it("renders empty state when there are no snapshots", async () => {
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "empty",
      snapshot: null,
      latestAttemptTaskId: null,
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace();
    expect(await screen.findByText("最近 72 小时没有工作记录")).toBeTruthy();
  });

  it("renders generating skeleton when first round is generating without snapshot", async () => {
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "generating",
      snapshot: null,
      latestAttemptTaskId: "task-1",
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace();
    expect(await screen.findByText("近期记忆生成中...")).toBeTruthy();
  });

  it("defaults to time view and switches to project view with identical item sets (M35-UI-01, M35-UI-02)", async () => {
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "ready",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: null,
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace();

    // 1. 验证默认按时间
    expect(await screen.findByText("2026-09-15")).toBeTruthy();
    expect(screen.getByText("2026-09-14")).toBeTruthy();
    expect(screen.getAllByText("Alpha Project").length).toBeGreaterThanOrEqual(
      1,
    );
    expect(
      screen.getAllByText("Adopt Single Layer Symlink").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText("Pass E2E Test Suite").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText("AssetIWeave Design Benchmark").length,
    ).toBeGreaterThanOrEqual(1);

    // 初始只调用了一次 snapshot 加载
    expect(memoryService.getRecentMemorySnapshot).toHaveBeenCalledTimes(1);

    // 2. 点击 PillTab 切换至「按项目」视图
    const projectTab = screen.getByRole("button", { name: "按项目" });
    fireEvent.click(projectTab);

    // 验证切换视图不重新调用网络/后端接口 (M35-UI-02)
    expect(memoryService.getRecentMemorySnapshot).toHaveBeenCalledTimes(1);

    // 验证项目视图下依然包含相同的全部 items
    expect(screen.getByText("Alpha Project")).toBeTruthy();
    expect(screen.getByText("Beta Project")).toBeTruthy();
    expect(
      screen.getAllByText("Adopt Single Layer Symlink").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText("Pass E2E Test Suite").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText("AssetIWeave Design Benchmark").length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("allows item expansion, preserves expanded state across view toggles (M35-UI-03)", async () => {
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "ready",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: null,
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace();

    await screen.findAllByText("Adopt Single Layer Symlink");

    // 初始状态下 rationale 未展开
    expect(
      screen.queryByText("Prevent complexity of middle link pool"),
    ).toBeNull();

    // 点击展开 item-1
    const expandButton = screen.getByRole("button", {
      name: "展开条目: Adopt Single Layer Symlink",
    });
    fireEvent.click(expandButton);

    // 展开后显示 rationale、水位以及关联会话
    expect(
      screen.getByText("Prevent complexity of middle link pool"),
    ).toBeTruthy();
    expect(screen.getByText("关联会话 (2)")).toBeTruthy();

    // 切换至项目视图
    fireEvent.click(screen.getByRole("button", { name: "按项目" }));

    // 验证切换视图后，同一个 itemId 的展开状态依然保留！
    expect(
      screen.getByText("Prevent complexity of middle link pool"),
    ).toBeTruthy();
    expect(screen.getByText("关联会话 (2)")).toBeTruthy();
  });

  it("navigates on clicking available session, blocks unavailable session (M35-UI-04)", async () => {
    const onNavigateSession = vi.fn();
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "ready",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: null,
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace({ onNavigateSession });

    await screen.findAllByText("Adopt Single Layer Symlink");
    // 展开 item-1
    fireEvent.click(
      screen.getByRole("button", {
        name: "展开条目: Adopt Single Layer Symlink",
      }),
    );

    // 验证两张 Session 卡片均渲染
    const availableSession = screen.getByText(
      "Symlink Architecture Discussion",
    );
    const unavailableSession = screen.getByText("Old Deleted Session");

    expect(availableSession).toBeTruthy();
    expect(unavailableSession).toBeTruthy();

    // 1. 点击 available 卡片，触发 onNavigateSession
    fireEvent.click(availableSession);
    expect(onNavigateSession).toHaveBeenCalledTimes(1);
    expect(onNavigateSession).toHaveBeenCalledWith({
      record_kind: "session",
      source_id: "src-1",
      session_id: "sess-1",
      question_id: null,
      turn_id: null,
      part_id: null,
      block_id: null,
    });

    // 2. 点击 unavailable 卡片，不触发导航 (M35-UI-04)
    fireEvent.click(unavailableSession);
    expect(onNavigateSession).toHaveBeenCalledTimes(1);
  });

  it("DOM strictly contains NO refresh, generate, rebuild, or recall input controls (M35-UI-05)", async () => {
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "ready",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: null,
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace();
    await screen.findAllByText("Alpha Project");

    // 严格校验 M35-UI-05：无刷新、窗口修改、生成、重建、编辑、删除、反馈、置顶或 Recall 输入框
    expect(screen.queryByText("刷新")).toBeNull();
    expect(screen.queryByText("生成")).toBeNull();
    expect(screen.queryByText("立即生成")).toBeNull();
    expect(screen.queryByText("重建")).toBeNull();
    expect(screen.queryByText("编辑")).toBeNull();
    expect(screen.queryByText("删除")).toBeNull();
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("displays correct status indicators for generated, reused, and failed states (M35-UI-06)", async () => {
    // 1. generated 状态 -> 已更新
    memoryService.getRecentMemorySnapshot.mockResolvedValueOnce({
      status: "ready",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: null,
      latestAttemptError: null,
    });
    const { unmount } = renderWorkspace();
    expect(await screen.findByText("已更新")).toBeTruthy();
    unmount();

    // 2. reused 状态 -> 内容复用
    const reusedSnap = {
      ...mockSnapshot(),
      publicationKind: "reused" as const,
    };
    memoryService.getRecentMemorySnapshot.mockResolvedValueOnce({
      status: "ready",
      snapshot: reusedSnap,
      latestAttemptTaskId: null,
      latestAttemptError: null,
    });
    const { unmount: unmount2 } = renderWorkspace();
    expect(await screen.findByText("内容复用")).toBeTruthy();
    unmount2();

    // 3. update_failed 状态 -> 更新未完成 (静默展示，不弹阻断错误)
    memoryService.getRecentMemorySnapshot.mockResolvedValueOnce({
      status: "update_failed",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: "task-fail",
      latestAttemptError: {
        code: "AGENT_TIMEOUT",
        message: "Timeout",
        retryable: true,
      },
    });
    renderWorkspace();
    expect(await screen.findByText("更新未完成")).toBeTruthy();
  });

  it("renders suggested next steps and fallback message correctly (M35-PROJ-02, Section 5.5)", async () => {
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "ready",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: null,
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace();

    // Alpha Project 有建议下一步 1 和 2
    expect(
      (await screen.findAllByText("建议下一步")).length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText("Adopt Single Layer Symlink").length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText("Pass E2E Test Suite").length,
    ).toBeGreaterThanOrEqual(1);

    // Beta Project 没有建议，显示固定次要文案 (Section 5.5)
    expect(screen.getByText("本窗口没有形成明确下一步")).toBeTruthy();
  });

  it("renders Craft Date Rail layout with serif day numbers and month abbreviations", async () => {
    memoryService.getRecentMemorySnapshot.mockResolvedValue({
      status: "ready",
      snapshot: mockSnapshot(),
      latestAttemptTaskId: null,
      latestAttemptError: null,
    } satisfies RecentMemoryStateView);

    renderWorkspace();

    // 验证左侧 Date Rail 渲染大号衬线日期数字
    expect(await screen.findByText("15")).toBeTruthy();
    expect(screen.getByText("14")).toBeTruthy();

    // 验证大写月份缩写（如 SEP）
    expect(screen.getAllByText("SEP").length).toBeGreaterThanOrEqual(2);

    // 验证完整日期标识依然存在
    expect(screen.getByText("2026-09-15")).toBeTruthy();
    expect(screen.getByText("2026-09-14")).toBeTruthy();
  });
});

function renderWorkspace(
  props: Partial<Parameters<typeof MemoryRecentWorkspace>[0]> = {},
) {
  const dummyT = (key: string) => {
    const dict: Record<string, string> = {
      "common.loading": "加载中...",
      "memory.recent.timeView": "按时间",
      "memory.recent.projectView": "按项目",
      "memory.recent.emptyTitle": "最近 72 小时没有工作记录",
      "memory.recent.emptyDescription":
        "Conversation 同步后，近期工作会在这里按项目或时间展示。",
      "memory.recent.generating": "近期记忆生成中...",
      "memory.recent.items": "条目",
      "memory.recent.whatChanged": "工作进展",
      "memory.recent.suggestedNext": "建议下一步",
      "memory.recent.noSuggestions": "本窗口没有形成明确下一步",
      "memory.recent.why": "理由",
      "memory.recent.watermark": "水位",
      "memory.recent.sessions": "关联会话",
      "memory.recent.sourceAvailable": "来源可用",
      "memory.recent.sourceUnavailable": "来源不可用",
      "memory.recent.partiallyUnavailable": "部分来源不可用",
      "memory.recent.expandItem": "展开条目",
      "memory.recent.collapseItem": "收起条目",
      "memory.recent.updated": "已更新",
      "memory.recent.reused": "内容复用",
      "memory.recent.updateIncomplete": "更新未完成",
      "memory.recent.latestUpdate": "最近更新",
      "memory.recent.today": "今天",
      "memory.recent.yesterday": "昨天",
      "memory.recent.beforeYesterday": "前天",
    };
    return dict[key] ?? key;
  };

  return render(
    <I18nProvider>
      <MemoryRecentWorkspace
        onNavigateSession={props.onNavigateSession}
        t={props.t ?? (dummyT as any)}
      />
    </I18nProvider>,
  );
}

function createMockLocalStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => Array.from(values.keys())[index] ?? null,
    removeItem: (key) => values.delete(key),
    setItem: (key, value) => values.set(key, String(value)),
  };
}
