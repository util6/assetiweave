// @vitest-environment jsdom

import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import {
  ConversationSearchProvider,
  useConversationSearch,
} from "./ConversationSearchProvider";
import { TaskCenterProvider, useTaskCenter } from "./TaskCenterProvider";
import { useAppUiStore } from "../../store/ui/appUiStore";
import type { TaskView } from "../../types/taskCenter";
import type { ConversationSearchResult } from "../../types";

let taskUpdatedHandler: ((task: TaskView) => void) | null = null;
const listPublicTasksMock = vi.hoisted(() => vi.fn(async () => []));
const cancelPublicTaskMock = vi.hoisted(() => vi.fn());
const startSearchTaskMock = vi.hoisted(() => vi.fn());
const getSearchTaskResultMock = vi.hoisted(() => vi.fn());
const searchRecordsMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/taskCenterService", () => ({
  isTauriRuntime: () => true,
  listPublicTasks: listPublicTasksMock,
  cancelPublicTask: cancelPublicTaskMock,
  retryPublicTask: vi.fn(),
  clearTerminalTasks: vi.fn(),
  subscribeTaskUpdated: vi.fn(async (cb: (task: TaskView) => void) => {
    taskUpdatedHandler = cb;
    return () => {
      taskUpdatedHandler = null;
    };
  }),
}));

vi.mock("../../services/conversations", () => ({
  startConversationContentSearchTask: startSearchTaskMock,
  getConversationContentSearchTaskResult: getSearchTaskResultMock,
  searchConversationRecords: searchRecordsMock,
}));

vi.mock("../../store/settings/useAppSettings", () => ({
  useAppSettings: () => ({
    settings: { showTaskNotifications: true },
  }),
}));

function makeTask(overrides: Partial<TaskView>): TaskView {
  return {
    id: "task-1",
    kind: "conversationcontentsearch",
    title: "搜索对话内容",
    state: "running",
    outcome: null,
    stages: [],
    metrics: [],
    failures: [],
    capabilities: { cancellable: true, retryable: false, clearable: false },
    revision: 1,
    ...overrides,
  };
}

function makeResult(query: string, recordKind: "session" | "web"): ConversationSearchResult {
  return {
    query,
    record_kind: recordKind,
    scope: {
      record_kind: recordKind,
      query,
      content_types: [],
      card_kinds: [],
      semantic_roles: [],
      include_questions: true,
      include_cards: true,
      timeline: false,
      limit: 50,
      offset: 0,
    },
    total_count: 3,
    hits: [],
  };
}

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <TaskCenterProvider>
          <ConversationSearchProvider>{children}</ConversationSearchProvider>
        </TaskCenterProvider>
      </QueryClientProvider>
    );
  };
}

describe("ConversationSearchProvider", () => {
  beforeEach(() => {
    taskUpdatedHandler = null;
    useAppUiStore.getState().setConversationSearchDialogOpen(false);
    listPublicTasksMock.mockResolvedValue([]);
    cancelPublicTaskMock.mockReset();
    startSearchTaskMock.mockReset();
    getSearchTaskResultMock.mockReset();
    searchRecordsMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("submits background search task, handles conflict decisions, and fetches cached result", async () => {
    const task1 = makeTask({ id: "task-old", state: "running" });
    const task2 = makeTask({ id: "task-new", state: "running" });
    startSearchTaskMock
      .mockResolvedValueOnce(task1)
      .mockResolvedValueOnce(task2);
    cancelPublicTaskMock.mockResolvedValueOnce(
      makeTask({ id: "task-old", state: "canceled", outcome: "canceled", revision: 2 }),
    );
    getSearchTaskResultMock.mockResolvedValueOnce(makeResult("new-query", "web"));

    const { result } = renderHook(
      () => ({
        search: useConversationSearch(),
        tasks: useTaskCenter(),
      }),
      { wrapper: createWrapper() },
    );

    await act(async () => {
      await result.current.search.submitSearch({
        query: "old-query",
        recordKind: "session",
        adapterId: null,
        cardKinds: [],
        semanticRoles: [],
        includeQuestions: true,
      });
    });

    expect(result.current.search.activeTask?.id).toBe("task-old");

    // Submitting a second search while activeTask runs triggers conflict instead of auto-cancel
    await act(async () => {
      await result.current.search.submitSearch({
        query: "new-query",
        recordKind: "web",
        adapterId: null,
        cardKinds: [],
        semanticRoles: [],
        includeQuestions: true,
      });
    });

    expect(cancelPublicTaskMock).not.toHaveBeenCalled();
    expect(result.current.search.pendingConflict?.activeQuery).toBe("old-query");
    expect(result.current.search.pendingConflict?.nextQuery).toBe("new-query");

    // Confirm replace -> cancels old (with suppressed notification) and launches new
    await act(async () => {
      await result.current.search.confirmConflictReplace();
    });

    expect(cancelPublicTaskMock).toHaveBeenCalledWith("task-old");
    expect(result.current.search.activeTask?.id).toBe("task-new");
    expect(result.current.tasks.currentNotification).toBeNull();

    // Emit running -> succeeded for task-new while dialog is closed
    act(() => {
      taskUpdatedHandler?.(makeTask({ id: "task-new", state: "running", revision: 1 }));
    });
    act(() => {
      taskUpdatedHandler?.(
        makeTask({
          id: "task-new",
          state: "succeeded",
          outcome: "success",
          result_summary: "「new-query」共找到 3 个结果",
          revision: 2,
        }),
      );
    });

    await waitFor(() => {
      expect(result.current.search.result?.query).toBe("new-query");
    });
    expect(result.current.search.result?.recordKind).toBe("web");
    expect(result.current.tasks.currentNotification?.task.id).toBe("task-new");
  });

  it("suppresses notification when dialog is open or task is canceled", async () => {
    const task1 = makeTask({ id: "task-open", state: "running" });
    startSearchTaskMock.mockResolvedValueOnce(task1);
    getSearchTaskResultMock.mockResolvedValueOnce(makeResult("open-q", "session"));

    const { result } = renderHook(
      () => ({
        search: useConversationSearch(),
        tasks: useTaskCenter(),
      }),
      { wrapper: createWrapper() },
    );

    act(() => {
      result.current.search.setDialogOpen(true);
    });

    await act(async () => {
      await result.current.search.submitSearch({
        query: "open-q",
        recordKind: "session",
        adapterId: null,
        cardKinds: [],
        semanticRoles: [],
        includeQuestions: true,
      });
    });

    act(() => {
      taskUpdatedHandler?.(makeTask({ id: "task-open", state: "running", revision: 1 }));
    });
    act(() => {
      taskUpdatedHandler?.(
        makeTask({
          id: "task-open",
          state: "succeeded",
          outcome: "success",
          revision: 2,
        }),
      );
    });

    await waitFor(() => {
      expect(result.current.search.result?.query).toBe("open-q");
    });
    expect(result.current.tasks.currentNotification).toBeNull();
  });
});
