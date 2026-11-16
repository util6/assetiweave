// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Translator } from "../../i18n/I18nProvider";
import { memoryZh } from "../../i18n/resources/zh/memory";
import type {
  MemoryProjectStateView,
  RecentMemoryStateView,
} from "../../types/memory";
import { MemoryRecentWorkspace } from "./MemoryRecentWorkspace";

const mockGetRecentMemorySnapshot = vi.fn();
const mockRefreshMemoryRecentState = vi.fn();
const mockUpdateMemoryNodeStatus = vi.fn();
const mockGetMemoryProjectState = vi.fn();
const mockRebuildMemoryScope = vi.fn();
const mockSubscribeMemoryTasks = vi.fn();

vi.mock("../../services/memory", () => ({
  emptyMemoryScope: () => ({
    app_id: null,
    source_id: null,
    project_path: null,
    session_id: null,
  }),
  getRecentMemorySnapshot: () => mockGetRecentMemorySnapshot(),
  refreshMemoryRecentState: (window?: string | null) =>
    mockRefreshMemoryRecentState(window),
  updateMemoryNodeStatus: (params: unknown) =>
    mockUpdateMemoryNodeStatus(params),
  getMemoryProjectState: (slug: string) => mockGetMemoryProjectState(slug),
  rebuildMemoryScope: (scope?: unknown, options?: unknown) =>
    mockRebuildMemoryScope(scope, options),
  subscribeMemoryTasks: (cb: () => void) => mockSubscribeMemoryTasks(cb),
}));

vi.mock("../../app/backgroundTasks/MemoryTaskProvider", () => ({
  useMemoryTasks: () => ({
    task: null,
    tasks: [],
    cancelTask: vi.fn(),
    retryTask: vi.fn(),
    refresh: vi.fn(),
  }),
}));

const t: Translator = (key, vars) => {
  const dict = memoryZh as Record<string, string>;
  let value = dict[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      value = value.replace(`{${k}}`, String(v));
    }
  }
  return value;
};

function makeStateFixture(): RecentMemoryStateView {
  return {
    status: "ready",
    latestAttemptTaskId: null,
    latestAttemptError: null,
    snapshot: {
      snapshotId: "snap-1",
      sequence: 1,
      targetWatermark: "2026-11-13T10:00:00Z",
      windowStart: "2026-11-10T10:00:00Z",
      windowEnd: "2026-11-13T10:00:00Z",
      windowHours: 72,
      publicationKind: "generated",
      reusedFromSnapshotId: null,
      contentGeneratedAt: "2026-11-13T10:00:00Z",
      publishedAt: "2026-11-13T10:00:00Z",
      projects: [
        {
          projectKey: "assetiweave",
          projectTitle: "AssetIWeave",
          projectPath: "/Users/demo/assetiweave",
          summary: "",
          noMaterialChange: false,
          latestActivityAt: "2026-11-13T09:30:00Z",
          sourceSessionCount: 1,
          items: [],
          topics: [
            {
              topicId: "topic-ui",
              topic: "Memory Recent Rewrite",
              updatedAt: "2026-11-13T09:30:00Z",
              nodes: [
                {
                  id: "sess1234#T4",
                  parentId: "sess1234#T1",
                  parentSummary: "Designed topic tree data contract",
                  status: "unexplored",
                  label: "未探索分支",
                  summary: "Add Date Rail timeline switch",
                  recordedAt: "2026-11-13T09:30:00Z",
                  sourceSessionId: "session-uuid-1234",
                  turnIndex: 4,
                  sessionTitle: "Rewrite frontend around .state",
                  sourceAgent: "claude",
                  sessionAvailable: true,
                },
                {
                  id: "sess1234#T2",
                  parentId: null,
                  parentSummary: null,
                  status: "completed",
                  label: "已完成",
                  summary: "Enriched Rust recent_snapshot view",
                  recordedAt: "2026-11-12T15:00:00Z",
                  sourceSessionId: "session-uuid-1234",
                  turnIndex: 2,
                  sessionTitle: "Rewrite frontend around .state",
                  sourceAgent: "claude",
                  sessionAvailable: true,
                },
              ],
            },
          ],
        },
      ],
    },
  };
}

describe("MemoryRecentWorkspace", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    vi.clearAllMocks();
    mockSubscribeMemoryTasks.mockResolvedValue(() => {});
    mockGetRecentMemorySnapshot.mockResolvedValue(makeStateFixture());
    mockRefreshMemoryRecentState.mockResolvedValue(makeStateFixture());
    mockRebuildMemoryScope.mockResolvedValue({
      accepted: true,
      scheduledTaskIds: ["t1"],
      targetWatermark: null,
      reused: false,
    });
    mockUpdateMemoryNodeStatus.mockResolvedValue({
      id: "sess1234#T4",
      parent_id: "sess1234#T1",
      topic_id: "topic-ui",
      summary: "Add Date Rail timeline switch",
      status: "completed",
      source_session_id: "session-uuid-1234",
      turn_index: 4,
      turn_started_at: "2026-11-13T09:30:00Z",
      created_at: "2026-11-13T09:30:00Z",
      updated_at: "2026-11-13T10:00:00Z",
    });
  });

  it("defaults to project tree view with unexplored focus and switches to time view", async () => {
    render(<MemoryRecentWorkspace t={t} />);

    expect(await screen.findByText("未探索分支焦点")).toBeTruthy();
    expect(screen.getByTestId("recent-topic-tree")).toBeTruthy();
    expect(
      screen.getAllByText("Add Date Rail timeline switch").length,
    ).toBeGreaterThanOrEqual(1);

    fireEvent.click(screen.getByRole("button", { name: /按时间线/ }));

    expect(await screen.findByTestId("memory-date-rail-view")).toBeTruthy();
    expect(screen.queryByTestId("recent-topic-tree")).toBeNull();
  });

  it("updates node status, refreshes snapshot, and allows undo after dismiss", async () => {
    const afterDismiss = makeStateFixture();
    afterDismiss.snapshot!.projects[0].topics![0].nodes[0].status = "completed";
    mockGetRecentMemorySnapshot
      .mockResolvedValueOnce(makeStateFixture())
      .mockResolvedValueOnce(afterDismiss)
      .mockResolvedValueOnce(makeStateFixture());

    render(<MemoryRecentWorkspace t={t} />);
    await screen.findByText("未探索分支焦点");

    const dismissButtons = screen.getAllByRole("button", {
      name: "忽略: sess1234#T4",
    });
    fireEvent.click(dismissButtons[0]);

    await waitFor(() => {
      expect(mockUpdateMemoryNodeStatus).toHaveBeenCalledWith({
        nodeId: "sess1234#T4",
        status: "dismissed",
        projectSlug: "assetiweave",
        window: "72h",
      });
    });

    expect(await screen.findByText(/撤销忽略/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "撤销忽略" }));

    await waitFor(() => {
      expect(mockUpdateMemoryNodeStatus).toHaveBeenLastCalledWith({
        nodeId: "sess1234#T4",
        status: "unexplored",
        projectSlug: "assetiweave",
        window: "72h",
      });
    });
  });

  it("loads full project history on demand and handles reslice & extract actions", async () => {
    const fullProjectState: MemoryProjectStateView = {
      projectKey: "assetiweave",
      projectTitle: "AssetIWeave",
      projectPath: "/Users/demo/assetiweave",
      updatedAt: "2026-11-13T10:00:00Z",
      watermarkCount: 14,
      topics: [
        {
          topicId: "topic-ui",
          topic: "Memory Recent Rewrite",
          updatedAt: "2026-11-13T10:00:00Z",
          nodes: [
            {
              id: "sess1234#T1",
              parentId: null,
              status: "continued",
              label: "延续中",
              summary: "Initial architecture brainstorm",
              recordedAt: "2026-11-10T07:55:00Z",
              sourceSessionId: "session-uuid-1234",
              turnIndex: 1,
              sessionTitle: "Rewrite frontend around .state",
              sourceAgent: "claude",
              sessionAvailable: true,
            },
          ],
        },
      ],
    };
    mockGetMemoryProjectState.mockResolvedValue(fullProjectState);

    render(<MemoryRecentWorkspace t={t} />);
    await screen.findByTestId("recent-topic-tree");

    fireEvent.click(screen.getByRole("button", { name: "查看完整历史树" }));

    await waitFor(() => {
      expect(mockGetMemoryProjectState).toHaveBeenCalledWith("assetiweave");
    });
    expect(
      await screen.findByText("Initial architecture brainstorm"),
    ).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", {
        name: "根据 .state/projects 重新裁剪当前时间窗口",
      }),
    );
    await waitFor(() => {
      expect(mockRefreshMemoryRecentState).toHaveBeenCalledWith("72h");
    });

    fireEvent.click(screen.getByRole("button", { name: "AI 提炼" }));
    await waitFor(() => {
      expect(mockRebuildMemoryScope).toHaveBeenCalledWith(
        {
          app_id: null,
          source_id: null,
          project_path: null,
          session_id: null,
        },
        { target: "recent", reason: "manual" },
      );
    });
  });

  it("navigates with full sourceSessionId and turn_id when clicking Jump to Turn", async () => {
    const onNavigateSession = vi.fn();
    render(
      <MemoryRecentWorkspace onNavigateSession={onNavigateSession} t={t} />,
    );

    const jumpButtons = await screen.findAllByRole("button", {
      name: "跳回 T4: sess1234#T4",
    });
    fireEvent.click(jumpButtons[0]);

    expect(onNavigateSession).toHaveBeenCalledWith({
      record_kind: "session",
      source_id: null,
      session_id: "session-uuid-1234",
      question_id: null,
      part_id: null,
      turn_id: "4",
      block_id: "4",
    });
  });

  it("filters by search query and resets via clear filters CTA", async () => {
    render(<MemoryRecentWorkspace t={t} />);
    await screen.findByTestId("recent-topic-tree");

    const searchInput = screen.getByPlaceholderText(
      "搜索主题、摘要、会话标题或短 ID...",
    );
    fireEvent.change(searchInput, { target: { value: "non-existent-token" } });
    fireEvent.keyDown(searchInput, { key: "Enter" });

    expect(await screen.findByText("没有匹配的主题或节点")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "清除筛选" }));

    expect(
      await screen.findByText("Enriched Rust recent_snapshot view"),
    ).toBeTruthy();
  });

  it("renders empty state CTA when recent.json has no projects", async () => {
    mockGetRecentMemorySnapshot.mockResolvedValue({
      status: "empty",
      snapshot: null,
      latestAttemptTaskId: null,
      latestAttemptError: null,
    });

    render(<MemoryRecentWorkspace t={t} />);
    expect(await screen.findByText("当前窗口暂无近期记忆")).toBeTruthy();

    fireEvent.click(
      screen.getByRole("button", { name: "立即 AI 提炼记忆" }),
    );
    await waitFor(() => {
      expect(mockRebuildMemoryScope).toHaveBeenCalledWith(
        {
          app_id: null,
          source_id: null,
          project_path: null,
          session_id: null,
        },
        { target: "recent", reason: "manual" },
      );
    });
  });
});
