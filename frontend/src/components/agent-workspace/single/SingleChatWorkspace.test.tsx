/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, beforeEach, vi } from "vitest";
import { I18nProvider } from "../../../i18n/I18nProvider";
import { SingleChatWorkspace } from "./SingleChatWorkspace";

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

describe("SingleChatWorkspace", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", createMockLocalStorage());
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("renders single chat header with agent selector, model, and account pill", () => {
    render(
      <I18nProvider>
        <SingleChatWorkspace />
      </I18nProvider>,
    );

    expect(screen.getByText("Codex")).not.toBeNull();
    expect(screen.getByText("o3-mini (推荐)")).not.toBeNull();
    expect(screen.getByText("Codex 工作主号")).not.toBeNull();
  });

  it("switches agent and updates model and account accordingly", () => {
    render(
      <I18nProvider>
        <SingleChatWorkspace />
      </I18nProvider>,
    );

    // Open Agent Menu
    const agentBtn = screen.getByText("Codex");
    fireEvent.click(agentBtn);

    // Select Antigravity
    const agyBtn = screen.getByText("Antigravity (AGY / ACP)");
    fireEvent.click(agyBtn);

    // Header updates to Antigravity and its default model
    expect(screen.getByText("Antigravity")).not.toBeNull();
    expect(screen.getByText("Gemini 2.5 Pro (推荐)")).not.toBeNull();
  });

  it("allows selecting a recommended quick prompt and populates stream", async () => {
    render(
      <I18nProvider>
        <SingleChatWorkspace />
      </I18nProvider>,
    );

    const quickPromptBtn = screen.getByText("免代理切号测试");
    fireEvent.click(quickPromptBtn);

    // Expect user message to appear
    expect(screen.getByText(/模拟调用 Codex 与 Antigravity/i)).not.toBeNull();
  });
});
