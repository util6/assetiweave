/* @vitest-environment jsdom */
import {
  QueryClient,
  QueryClientProvider,
  useQuery,
} from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  mergeSearchIndexQueryState,
  searchIndexQueryOptions,
  type SearchIndexQueryState,
} from "./searchIndexQueries";
import type { QueryScope } from "../query/catalogQueries";
import type { ConversationSearchIndexStatus } from "../../services/conversations";

const testScope: QueryScope = { tenantId: "default", epoch: 1 };

const mockGetStatus = vi.hoisted(() => vi.fn());
const mockGetTask = vi.hoisted(() => vi.fn());

vi.mock("../../services/conversations", () => ({
  getConversationSearchIndexStatus: mockGetStatus,
  getConversationSearchIndexTask: mockGetTask,
}));

beforeEach(() => {
  vi.clearAllMocks();
});

function createTestWrapper(client: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
  };
}

function createMockStatus(
  overrides?: Partial<ConversationSearchIndexStatus>,
): ConversationSearchIndexStatus {
  return {
    health: "ready",
    schema_version: 1,
    tokenizer_version: "v1",
    source_revision: 0,
    indexed_revision: 0,
    active_generation: "gen-1",
    document_count: 10,
    size_bytes: 1024,
    last_built_at: "2026-07-22T00:00:00Z",
    last_error: null,
    lease_owner: null,
    lease_expires_at: null,
    is_rebuilding: false,
    updated_at: "2026-07-22T00:00:00Z",
    supported_modes: ["lexical"],
    ...overrides,
  };
}

describe("mergeSearchIndexQueryState", () => {
  it("同 ID 终态不回退 running", () => {
    const previous: SearchIndexQueryState = {
      status: createMockStatus({ health: "ready", source_revision: 2 }),
      task: {
        id: "task-1",
        status: "completed",
        started_at: "2026-07-22T00:00:00Z",
        finished_at: "2026-07-22T00:00:05Z",
        result: null,
        error: null,
      },
    };

    const staleIncoming: SearchIndexQueryState = {
      status: createMockStatus({ health: "ready", source_revision: 2 }),
      task: {
        id: "task-1",
        status: "running",
        started_at: "2026-07-22T00:00:00Z",
        finished_at: null,
        result: null,
        error: null,
      },
    };

    const merged = mergeSearchIndexQueryState(previous, staleIncoming);
    expect(merged.task?.status).toBe("completed");
  });

  it("不同 ID 比较 started_at 保留较新的任务", () => {
    const taskOld: SearchIndexQueryState = {
      status: null,
      task: {
        id: "task-1",
        status: "completed",
        started_at: "2026-07-22T00:00:00Z",
        finished_at: "2026-07-22T00:00:05Z",
        result: null,
        error: null,
      },
    };

    const taskNew: SearchIndexQueryState = {
      status: null,
      task: {
        id: "task-2",
        status: "running",
        started_at: "2026-07-22T00:01:00Z",
        finished_at: null,
        result: null,
        error: null,
      },
    };

    const merged = mergeSearchIndexQueryState(taskOld, taskNew);
    expect(merged.task?.id).toBe("task-2");

    // Reversing order still keeps newer
    const mergedReverse = mergeSearchIndexQueryState(taskNew, taskOld);
    expect(mergedReverse.task?.id).toBe("task-2");
  });

  it("null 不擦掉尚在执行的当前任务", () => {
    const running: SearchIndexQueryState = {
      status: createMockStatus({ health: "stale", source_revision: 1 }),
      task: {
        id: "task-1",
        status: "running",
        started_at: "2026-07-22T00:00:00Z",
        finished_at: null,
        result: null,
        error: null,
      },
    };

    const nullIncoming: SearchIndexQueryState = {
      status: createMockStatus({ health: "stale", source_revision: 1 }),
      task: null,
    };

    const merged = mergeSearchIndexQueryState(running, nullIncoming);
    expect(merged.task?.id).toBe("task-1");
    expect(merged.task?.status).toBe("running");
  });

  it("status 用 source_revision 保留最新快照", () => {
    const current: SearchIndexQueryState = {
      status: createMockStatus({ health: "ready", source_revision: 5 }),
      task: null,
    };

    const olderStatus: SearchIndexQueryState = {
      status: createMockStatus({ health: "ready", source_revision: 3 }),
      task: null,
    };

    const merged = mergeSearchIndexQueryState(current, olderStatus);
    expect(merged.status?.source_revision).toBe(5);
  });

  it("复现 poll 已发出→收到终态事件→旧 running poll 回来，最终保持终态", () => {
    // 1. Initial running state
    let state: SearchIndexQueryState | undefined = {
      status: createMockStatus({ health: "stale", source_revision: 1 }),
      task: {
        id: "task-race",
        status: "running",
        started_at: "2026-07-22T00:00:00Z",
        finished_at: null,
        result: null,
        error: null,
      },
    };

    // 2. Terminal event arrives via WebSocket / IPC
    const terminalEvent: SearchIndexQueryState = {
      status: createMockStatus({ health: "ready", source_revision: 2 }),
      task: {
        id: "task-race",
        status: "completed",
        started_at: "2026-07-22T00:00:00Z",
        finished_at: "2026-07-22T00:00:10Z",
        result: null,
        error: null,
      },
    };
    state = mergeSearchIndexQueryState(state, terminalEvent);
    expect(state.task?.status).toBe("completed");

    // 3. Stale poll HTTP/IPC response arrives late with "running"
    const stalePoll: SearchIndexQueryState = {
      status: createMockStatus({ health: "stale", source_revision: 1 }),
      task: {
        id: "task-race",
        status: "running",
        started_at: "2026-07-22T00:00:00Z",
        finished_at: null,
        result: null,
        error: null,
      },
    };
    state = mergeSearchIndexQueryState(state, stalePoll);
    expect(state.task?.status).toBe("completed");
    expect(state.status?.health).toBe("ready");
  });
});

describe("searchIndexQueryOptions polling & owner behavior", () => {
  it("两页面 observer 只有根任务组件作为单一 poll owner，页面从 cache 观察", async () => {
    vi.useFakeTimers();
    mockGetStatus.mockResolvedValue(
      createMockStatus({ health: "ready", source_revision: 1 }),
    );
    mockGetTask.mockResolvedValue(null);

    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = createTestWrapper(client);

    // Root owner hook with refetchInterval
    const { result: rootOwner } = renderHook(
      () =>
        useQuery({
          ...searchIndexQueryOptions(testScope),
          refetchInterval: (query) =>
            query.state.data?.task?.status === "running" ? 1000 : 10000,
          refetchIntervalInBackground: true,
        }),
      { wrapper },
    );

    // Page observer hook: enabled: false (no independent polling)
    const { result: pageA } = renderHook(
      () =>
        useQuery({
          ...searchIndexQueryOptions(testScope),
          enabled: false,
        }),
      { wrapper },
    );
    const { result: pageB } = renderHook(
      () =>
        useQuery({
          ...searchIndexQueryOptions(testScope),
          enabled: false,
        }),
      { wrapper },
    );

    await act(async () => {});

    expect(mockGetStatus).toHaveBeenCalledTimes(1);

    // Advance 5 seconds: no extra poll since interval is 10000ms when not running
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(mockGetStatus).toHaveBeenCalledTimes(1);

    // Advance to 10 seconds: root owner polls once
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5000);
    });
    expect(mockGetStatus).toHaveBeenCalledTimes(2);

    // Both pages observe updated cache without starting duplicate poll timers
    expect(pageA.current.data?.status?.health).toBe("ready");
    expect(pageB.current.data?.status?.health).toBe("ready");

    vi.useRealTimers();
    client.clear();
  });

  it("running 状态时按 1000ms 高频轮询，terminal snapshot 后自动降为 10000ms", async () => {
    vi.useFakeTimers();
    const runningTask = {
      id: "task-fast",
      status: "running" as const,
      started_at: "2026-07-22T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    };
    mockGetStatus.mockResolvedValue(
      createMockStatus({ health: "stale", source_revision: 1 }),
    );
    mockGetTask.mockResolvedValue(runningTask);

    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const wrapper = createTestWrapper(client);

    renderHook(
      () =>
        useQuery({
          ...searchIndexQueryOptions(testScope),
          refetchInterval: (query) =>
            query.state.data?.task?.status === "running" ? 1000 : 10000,
          refetchIntervalInBackground: true,
        }),
      { wrapper },
    );

    await act(async () => {});
    expect(mockGetTask).toHaveBeenCalledTimes(1);

    // Running -> 1000ms poll
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(mockGetTask).toHaveBeenCalledTimes(2);

    // Now task completes
    mockGetTask.mockResolvedValue({
      ...runningTask,
      status: "completed",
      finished_at: "2026-07-22T00:00:02Z",
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(mockGetTask).toHaveBeenCalledTimes(3);

    // Now that status is completed, next poll should not occur at 1000ms
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(mockGetTask).toHaveBeenCalledTimes(3);

    // It waits for 10000ms
    await act(async () => {
      await vi.advanceTimersByTimeAsync(9000);
    });
    expect(mockGetTask).toHaveBeenCalledTimes(4);

    vi.useRealTimers();
    client.clear();
  });
});
