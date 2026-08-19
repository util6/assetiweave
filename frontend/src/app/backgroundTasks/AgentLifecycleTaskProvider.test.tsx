// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";
import {
  mergeAgentLifecycleTaskSnapshots,
} from "./AgentLifecycleTaskProvider";
import type { AgentLifecycleTaskSnapshot } from "../../services/agentRuntime";

vi.mock("../../services/agentRuntime", () => ({
  cancelAgentLifecycleTask: vi.fn(),
  listAgentLifecycleTasks: vi.fn().mockResolvedValue([]),
  subscribeAgentLifecycleTasks: vi.fn().mockResolvedValue(vi.fn()),
}));

function task(overrides: Partial<AgentLifecycleTaskSnapshot> = {}): AgentLifecycleTaskSnapshot {
  return {
    id: "task-1",
    agentId: "agent",
    action: "install",
    state: "running",
    phase: "installing",
    catalogVersion: "2026.08.1",
    agentVersion: "1.0.0",
    distributionId: "system",
    distributionType: "system",
    ownership: "system",
    progress: {
      completedUnits: 3,
      totalUnits: 5,
      downloadedBytes: null,
      totalBytes: null,
    },
    cancellable: true,
    createdAt: "2026-08-17T00:00:00Z",
    updatedAt: "2026-08-17T00:00:01Z",
    finishedAt: null,
    result: null,
    error: null,
    warnings: [],
    ...overrides,
  };
}

describe("AgentLifecycleTaskProvider merge", () => {
  it("keeps a terminal snapshot when an older running event arrives", () => {
    const terminal = task({
      state: "succeeded",
      phase: "succeeded",
      cancellable: false,
      updatedAt: "2026-08-17T00:00:03Z",
      finishedAt: "2026-08-17T00:00:03Z",
    });
    const merged = mergeAgentLifecycleTaskSnapshots([terminal], [task()]);
    expect(merged).toEqual([terminal]);
  });

  it("retains active tasks and caps terminal history", () => {
    const terminal = Array.from({ length: 105 }, (_, index) => task({
      id: `terminal-${index}`,
      state: "failed",
      phase: "failed",
      cancellable: false,
      updatedAt: `2026-08-17T00:01:${String(index).padStart(2, "0")}Z`,
      finishedAt: `2026-08-17T00:01:${String(index).padStart(2, "0")}Z`,
    }));
    const merged = mergeAgentLifecycleTaskSnapshots([], [task(), ...terminal]);
    expect(merged.some((entry) => entry.state === "running")).toBe(true);
    expect(merged.filter((entry) => entry.state === "failed")).toHaveLength(100);
  });
});
