// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryTaskProvider } from "../../app/backgroundTasks/MemoryTaskProvider";
import { createAppI18nSync } from "../../i18n/createAppI18n";
import { I18nProvider } from "../../i18n/I18nProvider";
import { MemoryPage } from "./MemoryPage";

vi.mock("../../services/memory", () => ({
  emptyMemoryScope: () => ({
    app_id: null,
    source_id: null,
    project_path: null,
    session_id: null,
  }),
  getRecentMemorySnapshot: vi.fn(async () => ({
    status: "empty",
    snapshot: null,
    latestAttemptTaskId: null,
    latestAttemptError: null,
  })),
  refreshMemoryRecentState: vi.fn(async () => ({
    status: "empty",
    snapshot: null,
    latestAttemptTaskId: null,
    latestAttemptError: null,
  })),
  updateMemoryNodeStatus: vi.fn(),
  getMemoryProjectState: vi.fn(async () => null),
  rebuildMemoryScope: vi.fn(async () => ({
    accepted: true,
    scheduledTaskIds: [],
    targetWatermark: null,
    reused: false,
  })),
  listMemoryPublicTasks: vi.fn(async () => []),
  getMemoryPublicTask: vi.fn(async () => null),
  cancelMemoryPublicTask: vi.fn(),
  retryMemoryPublicTask: vi.fn(),
  subscribeMemoryTasks: vi.fn(async () => () => {}),
  createMemoryRecallSession: vi.fn(async () => ({
    id: "recall-1",
    status: "active",
    scope: {
      app_id: null,
      source_id: null,
      project_path: null,
      session_id: null,
    },
    executionContextKey: "ctx-1",
    agentId: "claude",
    model: null,
    turnCount: 0,
    activeTurnId: null,
    lastError: null,
    createdAt: "2026-11-13T00:00:00Z",
    updatedAt: "2026-11-13T00:00:00Z",
    turns: [],
  })),
  getMemoryRecallSession: vi.fn(async () => null),
  searchMemoryRecall: vi.fn(async () => null),
  sendMemoryRecallTurn: vi.fn(),
  cancelMemoryRecallTurn: vi.fn(),
}));

function renderMemoryPage(activeSubNavId: string) {
  return render(
    <I18nProvider i18n={createAppI18nSync("zh")}>
      <MemoryTaskProvider>
        <MemoryPage activeSubNavId={activeSubNavId} />
      </MemoryTaskProvider>
    </I18nProvider>,
  );
}

describe("MemoryPage", () => {
  afterEach(() => {
    cleanup();
  });

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("only renders the Recent workspace for the default entry point", async () => {
    renderMemoryPage("recent");
    expect(await screen.findByText("当前窗口暂无近期记忆")).toBeTruthy();
    expect(screen.getByText("72h")).toBeTruthy();
    expect(screen.queryByText("今日 / 继续工作")).toBeNull();
    expect(screen.queryByText("自动 Dream")).toBeNull();
  });

  it("renders Recall in its own dedicated view", async () => {
    renderMemoryPage("recall");
    expect(
      await screen.findByRole("heading", { name: "深度回忆" }),
    ).toBeTruthy();
    expect(screen.queryByText("当前窗口暂无近期记忆")).toBeNull();
  });
});
