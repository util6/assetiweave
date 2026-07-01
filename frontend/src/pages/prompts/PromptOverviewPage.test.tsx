/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { AppSettingsProvider } from "../../store/settings/AppSettingsProvider";
import { defaultSettings, defaultStorageInfo } from "../../store/settings/settingsSchema";
import { parseTags, PromptOverviewPage } from "./PromptOverviewPage";

vi.mock("../../services/appSettings", () => ({
  getAppSettings: vi.fn(async () => ({
    config_dir: defaultStorageInfo.configDir,
    config_path: defaultStorageInfo.configPath,
    conversation_adapter_dir: defaultStorageInfo.conversationAdapterDir,
    settings: defaultSettings,
  })),
  saveAppSettings: vi.fn(async () => ({
    config_dir: defaultStorageInfo.configDir,
    config_path: defaultStorageInfo.configPath,
    conversation_adapter_dir: defaultStorageInfo.conversationAdapterDir,
    settings: defaultSettings,
  })),
}));

beforeEach(() => {
  vi.stubGlobal("localStorage", createMockLocalStorage());
  localStorage.setItem("assetiweave.locale", "zh");
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: {
      writeText: vi.fn(async () => undefined),
    },
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

describe("PromptOverviewPage", () => {
  it("creates prompt cards and persists them locally", () => {
    renderPromptPage();

    fireEvent.change(screen.getByPlaceholderText("粘贴一段 prompt、记录一个 feature 想法，或写下还没整理完的灵感。"), {
      target: { value: "Draft a feature spec for prompt cards." },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存卡片" }));

    expect(screen.getByText("Draft a feature spec for prompt cards.")).toBeTruthy();

    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored).toHaveLength(1);
    expect(stored[0]).toMatchObject({
      content: "Draft a feature spec for prompt cards.",
      copyCount: 0,
      projectPath: "",
      sessionName: "",
      tags: [],
      title: "未命名提示词",
    });
  });

  it("counts copies and can sort prompt cards by usage", async () => {
    seedPromptCards([
      createStoredPromptCard("Low use", "first prompt", ["work"], "/tmp/a", "session-a", "2026-01-01T00:00:00.000Z"),
      createStoredPromptCard("High use", "second prompt", ["ops"], "/tmp/b", "session-b", "2026-01-02T00:00:00.000Z"),
    ]);
    renderPromptPage();

    const copyButton = screen.getByRole("button", { name: "复制" });
    fireEvent.click(copyButton);
    fireEvent.click(copyButton);
    fireEvent.change(screen.getByLabelText("提示词排序"), {
      target: { value: "copy-count" },
    });

    await waitFor(() => {
      expect(screen.getByTestId("prompt-active-card").textContent).toContain("second prompt");
    });
    expect(screen.queryByTestId("prompt-card-list")).toBeNull();
    expect(screen.getAllByText("复制 2 次").length).toBeGreaterThan(0);

    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
      expect(stored.find((note: { title: string }) => note.title === "High use")).toMatchObject({
        copyCount: 2,
      });
    });
  });

  it("filters the card list by tag group", () => {
    seedPromptCards([
      createStoredPromptCard("Feature prompt", "feature prompt", ["feature"], "/tmp/project", "s1", "2026-01-01T00:00:00.000Z"),
      createStoredPromptCard("Ops prompt", "ops prompt", ["ops"], "/tmp/project", "s2", "2026-01-02T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "标签 feature，1 张卡片" }));

    expect(screen.getAllByText("feature prompt").length).toBeGreaterThan(0);
    expect(screen.queryAllByText("ops prompt")).toHaveLength(0);
  });

  it("edits prompt card metadata from the top-right info action", () => {
    seedPromptCards([
      createStoredPromptCard("Original prompt", "keep this body", ["draft"], "/tmp/old", "s1", "2026-01-01T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "编辑信息" }));
    expect(screen.queryByPlaceholderText("标题，可留空")).toBeNull();
    fireEvent.change(screen.getByPlaceholderText("项目目录路径，例如 /Users/me/project"), {
      target: { value: "/tmp/new" },
    });
    fireEvent.change(screen.getByPlaceholderText("标签，用空格或逗号分隔"), {
      target: { value: "ready prompt" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存修改" }));

    expect(screen.getByText("keep this body")).toBeTruthy();
    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0]).toMatchObject({
      content: "keep this body",
      projectPath: "/tmp/new",
      tags: ["ready", "prompt"],
      title: "Original prompt",
    });
  });

  it("translates and optimizes a prompt through the injected CLI translator", async () => {
    const translator = vi.fn(async (request) => ({
      translated_text: request.promptTemplate?.includes("expert prompt editor")
        ? "Write a concise implementation plan for prompt cards."
        : "为提示词卡片编写功能规格。",
    }));

    renderPromptPage({ translator });
    fireEvent.change(screen.getByPlaceholderText("粘贴一段 prompt、记录一个 feature 想法，或写下还没整理完的灵感。"), {
      target: { value: "make prompt card feature" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存卡片" }));

    await screen.findByText("可用，可执行翻译和优化");
    fireEvent.click(screen.getByRole("button", { name: "翻译到 简体中文" }));

    expect(await screen.findByText("为提示词卡片编写功能规格。")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "优化" }));

    await waitFor(() => {
      expect(screen.getByText("Write a concise implementation plan for prompt cards.")).toBeTruthy();
    });
    expect(translator).toHaveBeenCalledTimes(2);
  });

  it("deduplicates comma and space separated tags", () => {
    expect(parseTags(" prompt,feature，prompt  idea ")).toEqual(["prompt", "feature", "idea"]);
  });
});

function seedPromptCards(cards: ReturnType<typeof createStoredPromptCard>[]) {
  localStorage.setItem("assetiweave.promptNotes", JSON.stringify(cards));
}

function createStoredPromptCard(
  title: string,
  content: string,
  tags: string[],
  projectPath: string,
  sessionName: string,
  timestamp: string,
) {
  return {
    content,
    copyCount: 0,
    createdAt: timestamp,
    id: `prompt-${title}`,
    projectPath,
    sessionName,
    tags,
    title,
    updatedAt: timestamp,
  };
}

function renderPromptPage({
  translator = vi.fn(async () => ({ translated_text: "" })),
}: {
  translator?: ComponentProps<typeof PromptOverviewPage>["translator"];
} = {}) {
  return render(
    <I18nProvider>
      <AppSettingsProvider>
        <PromptOverviewPage
          availabilityChecker={async () => ({ available: true, error: null, version: "test" })}
          onManualOpen={vi.fn()}
          translator={translator}
        />
      </AppSettingsProvider>
    </I18nProvider>,
  );
}

function createMockLocalStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: vi.fn(() => values.clear()),
    getItem: vi.fn((key: string) => values.get(key) ?? null),
    key: vi.fn((index: number) => Array.from(values.keys())[index] ?? null),
    removeItem: vi.fn((key: string) => {
      values.delete(key);
    }),
    setItem: vi.fn((key: string, value: string) => {
      values.set(key, value);
    }),
  };
}
