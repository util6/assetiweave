/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, beforeEach, vi } from "vitest";
import { I18nProvider } from "../../../i18n/I18nProvider";
import { TeamWorkspace } from "./TeamWorkspace";

function createMockLocalStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: vi.fn(() => values.clear()),
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    key: vi.fn((index: number) => Array.from(values.keys())[index] ?? null),
    removeItem: vi.fn((key: string) => values.delete(key)),
    setItem: vi.fn((key: string, value: string) => values.set(key, value)),
  };
}

describe("TeamWorkspace", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", createMockLocalStorage());
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders team header, member tabs, and parallel lanes", () => {
    render(
      <I18nProvider>
        <TeamWorkspace />
      </I18nProvider>,
    );

    // Team name
    expect(screen.getByText("全栈架构重构小队")).not.toBeNull();

    // Member tabs
    expect(screen.getByRole("button", { name: /系统架构师/i })).not.toBeNull();
    expect(
      screen.getByRole("button", { name: /前端界面专家/i }),
    ).not.toBeNull();
    expect(
      screen.getByRole("button", { name: /测试与代码审查员/i }),
    ).not.toBeNull();

    // Broadcast bar
    expect(
      screen.getByPlaceholderText(/向整个团队 \(3 位智能体\) 广播总目标/i),
    ).not.toBeNull();
  });

  it("switches to single focused view and back to parallel view", () => {
    render(
      <I18nProvider>
        <TeamWorkspace />
      </I18nProvider>,
    );

    const singleViewBtn = screen.getByRole("button", { name: /聚焦单列/i });
    fireEvent.click(singleViewBtn);

    expect(screen.getByText(/聚焦成员: 系统架构师/i)).not.toBeNull();

    const parallelViewBtn = screen.getByRole("button", { name: /并行多列/i });
    fireEvent.click(parallelViewBtn);

    expect(screen.queryByText(/聚焦成员: 系统架构师/i)).toBeNull();
  });

  it("opens team create modal when clicking create button", () => {
    render(
      <I18nProvider>
        <TeamWorkspace />
      </I18nProvider>,
    );

    const createBtn = screen.getByTitle("创建新团队");
    fireEvent.click(createBtn);

    expect(
      screen.getAllByText("创建协同团队与成员配额绑定").length,
    ).toBeGreaterThan(0);
  });
});
