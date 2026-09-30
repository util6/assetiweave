/* @vitest-environment jsdom */

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { AgentWorkspacePage } from "./AgentWorkspacePage";

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

describe("AgentWorkspacePage", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", createMockLocalStorage());
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("renders page header with tabs and account library button", () => {
    render(
      <I18nProvider>
        <AgentWorkspacePage />
      </I18nProvider>,
    );

    expect(screen.getByText("智能体工作台")).toBeTruthy();
    expect(screen.getByText("智能体单聊 (Single ACP)")).toBeTruthy();
    expect(screen.getByText("团队协作 (Team Workspace)")).toBeTruthy();
    expect(screen.getByRole("button", { name: /智能体账号库/ })).toBeTruthy();
  });

  it("defaults to single chat workspace", () => {
    render(
      <I18nProvider>
        <AgentWorkspacePage />
      </I18nProvider>,
    );

    expect(screen.getByText("Codex")).toBeTruthy();
    expect(screen.getByText("o3-mini (推荐)")).toBeTruthy();
  });

  it("switches to team workspace when team tab is clicked", async () => {
    render(
      <I18nProvider>
        <AgentWorkspacePage />
      </I18nProvider>,
    );

    const teamTab = screen.getByText("团队协作 (Team Workspace)");
    fireEvent.click(teamTab);

    await waitFor(() => {
      expect(screen.getByText("全栈架构重构小队")).toBeTruthy();
      expect(screen.getByText("并行多列")).toBeTruthy();
      expect(screen.getByText("聚焦单列")).toBeTruthy();
      expect(
        screen.getByPlaceholderText(/向整个团队 \(3 位智能体\) 广播总目标/),
      ).toBeTruthy();
    });
  });

  it("opens and closes account manager modal", async () => {
    render(
      <I18nProvider>
        <AgentWorkspacePage />
      </I18nProvider>,
    );

    const accountLibraryBtn = screen.getByRole("button", {
      name: /智能体账号库/,
    });
    fireEvent.click(accountLibraryBtn);

    await waitFor(() => {
      expect(screen.getByText("智能体账号凭据管理")).toBeTruthy();
    });

    const closeBtn = screen.getByRole("button", { name: "Close" });
    fireEvent.click(closeBtn);

    await waitFor(() => {
      expect(screen.queryByText("智能体账号凭据管理")).toBeNull();
    });
  });
});
