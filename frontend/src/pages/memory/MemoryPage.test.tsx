/* @vitest-environment jsdom */

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { MemoryPage } from "./MemoryPage";

const memoryService = vi.hoisted(() => ({
  cancelMemoryRecallTurn: vi.fn(),
  createMemoryRecallSession: vi.fn(),
  emptyMemoryScope: vi.fn(() => ({ app_id: null, source_id: null, project_path: null, session_id: null })),
  getMemoryRecentEventTarget: vi.fn(),
  getMemoryRecallSession: vi.fn(),
  listMemoryRecent: vi.fn(),
  searchMemoryRecall: vi.fn(),
  sendMemoryRecallTurn: vi.fn(),
}));
vi.mock("../../services/memory", () => memoryService);

beforeEach(() => {
  vi.stubGlobal("localStorage", createMockLocalStorage());
  localStorage.setItem("assetiweave.locale", "zh");
  vi.clearAllMocks();
  memoryService.emptyMemoryScope.mockReturnValue({ app_id: null, source_id: null, project_path: null, session_id: null });
  memoryService.listMemoryRecent.mockResolvedValue([]);
  memoryService.createMemoryRecallSession.mockResolvedValue(session());
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("MemoryPage", () => {
  it("only renders the Recent workspace for the default entry point", async () => {
    renderMemoryPage("recent");
    expect(await screen.findByText("最近 72 小时没有工作记录")).toBeTruthy();
    expect(screen.queryByText("今日 / 继续工作")).toBeNull();
    expect(screen.queryByText("自动 Dream")).toBeNull();
  });

  it("opens one persistent Recall session and sends sequential turns", async () => {
    renderMemoryPage("recall");
    expect(await screen.findByRole("heading", { level: 1, name: "深度回忆" })).toBeTruthy();
    expect(memoryService.createMemoryRecallSession).toHaveBeenCalledTimes(1);
    fireEvent.change(screen.getByRole("textbox", { name: "回忆问题" }), { target: { value: "为什么？" } });
    memoryService.sendMemoryRecallTurn.mockResolvedValue(session({ turnCount: 1, activeTurnId: "turn-1" }));
    fireEvent.click(screen.getByRole("button", { name: "发送" }));
    await act(async () => {});
    expect(memoryService.sendMemoryRecallTurn).toHaveBeenCalledWith(expect.any(String), "为什么？");
  });
});

function renderMemoryPage(activeSubNavId: string) {
  return render(<I18nProvider><MemoryPage activeSubNavId={activeSubNavId} /></I18nProvider>);
}

function session(overrides: Record<string, unknown> = {}) {
  return {
    id: "recall-1",
    status: "active",
    scope: { app_id: null, source_id: null, project_path: null, session_id: null },
    executionContextKey: "memory-recall:recall-1",
    agentId: "opencode",
    model: null,
    turnCount: 0,
    activeTurnId: null,
    lastError: null,
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
    turns: [],
    ...overrides,
  };
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
