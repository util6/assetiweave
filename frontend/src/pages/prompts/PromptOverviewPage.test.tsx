/* @vitest-environment jsdom */

import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { AppSettingsProvider } from "../../store/settings/AppSettingsProvider";
import { defaultSettings, defaultStorageInfo } from "../../store/settings/settingsSchema";
import { parseTags, PromptOverviewPage } from "./PromptOverviewPage";

const selectTargetDirectoryMock = vi.hoisted(() => vi.fn(async () => "/picked/project"));
const copyPromptImagesToClipboardMock = vi.hoisted(() => vi.fn(async () => undefined));
const copyPromptTextToClipboardMock = vi.hoisted(() => vi.fn(async () => undefined));

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

vi.mock("../../services/catalog", () => ({
  selectTargetDirectory: selectTargetDirectoryMock,
}));

vi.mock("../../services/promptClipboard", () => ({
  copyPromptImagesToClipboard: copyPromptImagesToClipboardMock,
  copyPromptTextToClipboard: copyPromptTextToClipboardMock,
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

  it("restores an unsaved new prompt card draft after the page remounts", () => {
    seedPromptCards([
      createStoredPromptCard("Saved prompt", "already saved prompt", ["work"], "/tmp/a", "session-a", "2026-01-01T00:00:00.000Z"),
    ]);
    const view = renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "新建卡片" }));
    fireEvent.change(screen.getByPlaceholderText("粘贴一段 prompt、记录一个 feature 想法，或写下还没整理完的灵感。"), {
      target: { value: "half-written prompt draft" },
    });
    view.unmount();

    renderPromptPage();

    expect(screen.getByDisplayValue("half-written prompt draft")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "保存卡片" }));

    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0]).toMatchObject({
      content: "half-written prompt draft",
    });
    expect(localStorage.getItem("assetiweave.promptNoteDraft")).toBeNull();
  });

  it("previews, removes, and persists pasted images on prompt cards", async () => {
    renderPromptPage();

    const composer = screen.getByPlaceholderText("粘贴一段 prompt、记录一个 feature 想法，或写下还没整理完的灵感。");
    fireEvent.change(composer, {
      target: { value: "Use this screenshot in the prompt." },
    });
    fireEvent.paste(composer, {
      clipboardData: createClipboardDataWithFiles([
        createImageFile("screenshot.png"),
      ]),
    });

    expect(await screen.findByAltText("screenshot.png")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "移除图片 screenshot.png" }));
    await waitFor(() => {
      expect(screen.queryByAltText("screenshot.png")).toBeNull();
    });

    fireEvent.paste(composer, {
      clipboardData: createClipboardDataWithFiles([
        createImageFile("screenshot.png"),
      ]),
    });
    expect(await screen.findByAltText("screenshot.png")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "保存卡片" }));

    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0].attachments).toHaveLength(1);
    expect(stored[0].attachments[0]).toMatchObject({
      mimeType: "image/png",
      name: "screenshot.png",
    });
    expect(stored[0].attachments[0].dataUrl).toMatch(/^data:image\/png;base64,/);
  });

  it("copies prompt cards with images through a two-step clipboard flow", async () => {
    const attachment = createStoredPromptImageAttachment("diagram.png", "data:image/png;base64,ZGlhZ3JhbQ==");
    seedPromptCards([
      createStoredPromptCard(
        "Image prompt",
        "Use this diagram.",
        ["work"],
        "/tmp/a",
        "session-a",
        "2026-01-01T00:00:00.000Z",
        undefined,
        [attachment],
      ),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "复制图片" }));

    await waitFor(() => {
      expect(copyPromptImagesToClipboardMock).toHaveBeenCalledTimes(1);
    });
    expect(copyPromptImagesToClipboardMock).toHaveBeenCalledWith([attachment]);
    expect(screen.getByRole("button", { name: "已复制图片" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "复制文字" }));

    await waitFor(() => {
      expect(copyPromptTextToClipboardMock).toHaveBeenCalledWith("Use this diagram.");
    });
    expect(copyPromptImagesToClipboardMock).toHaveBeenCalledTimes(1);
  });

  it("counts copies for the active prompt card", async () => {
    seedPromptCards([
      createStoredPromptCard("Low use", "first prompt", ["work"], "/tmp/a", "session-a", "2026-01-01T00:00:00.000Z"),
      createStoredPromptCard("High use", "second prompt", ["ops"], "/tmp/b", "session-b", "2026-01-02T00:00:00.000Z"),
    ]);
    renderPromptPage();

    const copyButton = screen.getByRole("button", { name: "复制文字" });
    fireEvent.click(copyButton);
    fireEvent.click(copyButton);

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

  it("debounces prompt card search and filters cards by tags", async () => {
    vi.useFakeTimers();
    try {
      seedPromptCards([
        createStoredPromptCard("Feature prompt", "feature prompt", ["feature"], "/tmp/project", "s1", "2026-01-01T00:00:00.000Z"),
        createStoredPromptCard("Ops prompt", "ops prompt", ["ops"], "/tmp/project", "s2", "2026-01-02T00:00:00.000Z"),
      ]);
      renderPromptPage();

      fireEvent.change(screen.getByPlaceholderText("搜索正文、标签或翻译结果..."), {
        target: { value: "feature" },
      });

      expect(screen.getAllByText("ops prompt").length).toBeGreaterThan(0);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(700);
      });

      expect(screen.getAllByText("feature prompt").length).toBeGreaterThan(0);
      expect(screen.queryAllByText("ops prompt")).toHaveLength(0);
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows the default tag group instead of untitled and no-project fallbacks in card headers", () => {
    seedPromptCards([
      createStoredPromptCard("未命名提示词", "default grouped prompt", [], "", "", "2026-01-01T00:00:00.000Z"),
    ]);
    renderPromptPage();

    const activeCard = screen.getByTestId("prompt-active-card");
    expect(activeCard.textContent).toContain("默认");
    expect(activeCard.textContent).not.toContain("未命名提示词");
    expect(activeCard.textContent).not.toContain("未设置项目");
  });

  it("filters prompt cards from the toolbar tag group control", async () => {
    seedPromptCards([
      createStoredPromptCard("Default prompt", "default grouped prompt", [], "", "s1", "2026-01-01T00:00:00.000Z"),
      createStoredPromptCard("Ops prompt", "ops grouped prompt", ["ops"], "/tmp/project", "s2", "2026-01-02T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.pointerDown(screen.getByRole("button", { name: "标签筛选" }));
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "默认" }));

    await waitFor(() => {
      expect(screen.getByTestId("prompt-active-card").textContent).toContain("default grouped prompt");
      expect(screen.queryAllByText("ops grouped prompt")).toHaveLength(0);
    });
  });

  it("applies the active tag when creating a prompt card from a tag filter", async () => {
    seedPromptCards([
      createStoredPromptCard("Ops prompt", "ops grouped prompt", ["ops"], "/tmp/project", "s1", "2026-01-01T00:00:00.000Z"),
      createStoredPromptCard("Design prompt", "design grouped prompt", ["design"], "/tmp/project", "s2", "2026-01-02T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.pointerDown(screen.getByRole("button", { name: "标签筛选" }));
    fireEvent.click(await screen.findByRole("menuitemcheckbox", { name: "ops" }));
    fireEvent.keyDown(screen.getByRole("menu", { name: "标签筛选" }), { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "新建卡片" }));
    expect(screen.getByTestId("prompt-active-card").textContent).toContain("ops");
    fireEvent.change(screen.getByPlaceholderText("粘贴一段 prompt、记录一个 feature 想法，或写下还没整理完的灵感。"), {
      target: { value: "new ops prompt" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存卡片" }));

    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0]).toMatchObject({
      content: "new ops prompt",
      tags: ["ops"],
    });
  });

  it("shows random color swatches for toolbar tag group options", async () => {
    seedPromptCards([
      createStoredPromptCard("Ops prompt", "ops grouped prompt", ["ops"], "/tmp/project", "s1", "2026-01-01T00:00:00.000Z"),
      createStoredPromptCard("Design prompt", "design grouped prompt", ["design"], "/tmp/project", "s2", "2026-01-02T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.pointerDown(screen.getByRole("button", { name: "标签筛选" }));
    const opsOption = await screen.findByRole("menuitemcheckbox", { name: "ops" });
    const swatch = opsOption.querySelector("[data-toolbar-option-swatch]");

    expect(swatch).not.toBeNull();
    expect(swatch?.className).toContain("bg-");
  });

  it("edits prompt card metadata from the top-right info action", () => {
    seedPromptCards([
      createStoredPromptCard("Original prompt", "keep this body", ["draft"], "/tmp/old", "s1", "2026-01-01T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "编辑信息" }));
    expect(screen.queryByPlaceholderText("标题，可留空")).toBeNull();
    fireEvent.change(screen.getByPlaceholderText("项目目录路径，例如 ~/project"), {
      target: { value: "/tmp/new" },
    });
    const tagInput = screen.getByPlaceholderText("输入标签（还能添加 9 个）");
    fireEvent.change(tagInput, { target: { value: "ready" } });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    fireEvent.change(tagInput, { target: { value: "prompt" } });
    fireEvent.click(screen.getByRole("button", { name: "添加" }));
    fireEvent.click(screen.getByRole("button", { name: "保存修改" }));

    expect(screen.getByText("keep this body")).toBeTruthy();
    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0]).toMatchObject({
      content: "keep this body",
      projectPath: "/tmp/new",
      tags: ["draft", "ready", "prompt"],
      title: "Original prompt",
    });
  });

  it("picks a project directory from the edit card dialog", async () => {
    seedPromptCards([
      createStoredPromptCard("Project prompt", "keep project body", ["draft"], "/tmp/old", "s1", "2026-01-01T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "编辑信息" }));
    fireEvent.click(screen.getByRole("button", { name: "选择项目目录" }));

    await waitFor(() => {
      expect(screen.getByDisplayValue("/picked/project")).toBeTruthy();
    });
    fireEvent.click(screen.getByRole("button", { name: "保存修改" }));

    expect(selectTargetDirectoryMock).toHaveBeenCalledWith("选择项目目录");
    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0]).toMatchObject({
      projectPath: "/picked/project",
    });
  });

  it("separates the created tag library from current card tags in the edit card dialog", () => {
    seedPromptCards([
      createStoredPromptCard("Library prompt", "library prompt body", ["library-only", "shared"], "/tmp/old", "s0", "2026-01-01T00:00:00.000Z"),
      createStoredPromptCard("Current prompt", "keep current body", ["current"], "/tmp/old", "s1", "2026-01-02T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "编辑信息" }));

    const tagLibrary = screen.getByLabelText("已创建标签");
    const currentTags = screen.getByLabelText("当前卡片标签");
    expect(within(tagLibrary).getByText("library-only")).toBeTruthy();
    expect(within(tagLibrary).getByText("current")).toBeTruthy();
    expect(within(currentTags).getByText("current")).toBeTruthy();
    expect(within(currentTags).queryByText("library-only")).toBeNull();

    fireEvent.click(within(tagLibrary).getByRole("button", { name: "绑定标签 library-only" }));
    expect(within(currentTags).getByText("library-only")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "保存修改" }));

    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored.find((note: { title: string }) => note.title === "Current prompt")).toMatchObject({
      tags: ["current", "library-only"],
    });
  });

  it("shows editable colored chips for current tags in the edit card dialog", () => {
    seedPromptCards([
      createStoredPromptCard("Tagged prompt", "keep tagged body", ["draft", "ready"], "/tmp/old", "s1", "2026-01-01T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "编辑信息" }));

    expect(screen.getByText("最多 10 个标签，单个标签长度不超过 20 个字符。")).toBeTruthy();
    expect(screen.queryByText("暂无标签")).toBeNull();
    expect(screen.getByText("当前卡片标签")).toBeTruthy();
    const draftChip = screen.getByTestId("prompt-edit-tag-chip-draft");
    expect(draftChip.textContent).toContain("draft");
    expect(draftChip.className).toContain("bg-");
    expect(screen.getByPlaceholderText("输入标签（还能添加 8 个）")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "移除标签 draft" }));
    fireEvent.click(screen.getByRole("button", { name: "保存修改" }));

    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0]).toMatchObject({
      tags: ["ready"],
    });
  });

  it("adds tags from the edit card tag input and enforces tag limits", () => {
    seedPromptCards([
      createStoredPromptCard("Untagged prompt", "keep untagged body", [], "/tmp/old", "s1", "2026-01-01T00:00:00.000Z"),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "编辑信息" }));

    const addButton = screen.getByRole("button", { name: "添加" }) as HTMLButtonElement;
    const tagInput = screen.getByPlaceholderText("输入标签（还能添加 10 个）") as HTMLInputElement;
    expect(addButton.disabled).toBe(true);

    fireEvent.change(tagInput, { target: { value: "公益站" } });
    expect(addButton.disabled).toBe(false);
    fireEvent.click(addButton);

    fireEvent.change(tagInput, { target: { value: "abcdefghijklmnopqrstuvwxyz" } });
    expect(tagInput.value).toHaveLength(20);
    fireEvent.click(addButton);
    fireEvent.click(screen.getByRole("button", { name: "保存修改" }));

    const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
    expect(stored[0]).toMatchObject({
      tags: ["公益站", "abcdefghijklmnopqrst"],
    });
  });

  it("disables adding tags after reaching the tag count limit", () => {
    seedPromptCards([
      createStoredPromptCard(
        "Full tags prompt",
        "keep full tags body",
        ["t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t10"],
        "/tmp/old",
        "s1",
        "2026-01-01T00:00:00.000Z",
      ),
    ]);
    renderPromptPage();

    fireEvent.click(screen.getByRole("button", { name: "编辑信息" }));

    const addButton = screen.getByRole("button", { name: "添加" }) as HTMLButtonElement;
    const tagInput = screen.getByPlaceholderText("输入标签（还能添加 0 个）") as HTMLInputElement;
    expect(tagInput.disabled).toBe(true);
    expect(addButton.disabled).toBe(true);
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

    const translateButton = screen.getByRole("button", { name: "翻译到 简体中文" }) as HTMLButtonElement;
    await waitFor(() => {
      expect(translateButton.disabled).toBe(false);
    });
    fireEvent.click(translateButton);

    expect(await screen.findByText("为提示词卡片编写功能规格。")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "优化" }));

    await waitFor(() => {
      expect(screen.getByText("Write a concise implementation plan for prompt cards.")).toBeTruthy();
    });
    await waitFor(() => {
      const stored = JSON.parse(localStorage.getItem("assetiweave.promptNotes") ?? "[]");
      expect(stored[0]).toMatchObject({
        content: "make prompt card feature",
        optimizedText: "Write a concise implementation plan for prompt cards.",
      });
    });
    expect(translator).toHaveBeenCalledTimes(2);
  });

  it("flips to an existing optimized prompt without running optimization again", async () => {
    const translator = vi.fn(async () => ({ translated_text: "should not run" }));
    seedPromptCards([
      createStoredPromptCard(
        "Optimized prompt",
        "raw prompt",
        ["feature"],
        "/tmp/project",
        "s1",
        "2026-01-01T00:00:00.000Z",
        "stored optimized prompt",
      ),
    ]);
    renderPromptPage({ translator });

    const showOptimizedButton = await screen.findByRole("button", { name: "查看优化稿" }) as HTMLButtonElement;
    await waitFor(() => {
      expect(showOptimizedButton.disabled).toBe(false);
    });
    fireEvent.click(showOptimizedButton);

    expect(screen.getByText("stored optimized prompt")).toBeTruthy();
    expect(translator).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "再次优化" })).toBeTruthy();
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
  optimizedText?: string,
  attachments = [] as ReturnType<typeof createStoredPromptImageAttachment>[],
) {
  return {
    attachments,
    content,
    copyCount: 0,
    createdAt: timestamp,
    id: `prompt-${title}`,
    projectPath,
    sessionName,
    tags,
    title,
    optimizedText,
    updatedAt: timestamp,
  };
}

function createStoredPromptImageAttachment(name: string, dataUrl = "data:image/png;base64,c2NyZWVuc2hvdA==") {
  return {
    createdAt: "2026-01-01T00:00:00.000Z",
    dataUrl,
    id: `image-${name}`,
    mimeType: "image/png",
    name,
    size: 10,
  };
}

function createImageFile(name: string) {
  return new File(["screenshot"], name, { type: "image/png" });
}

function createClipboardDataWithFiles(files: File[]) {
  return {
    files,
    getData: vi.fn(() => ""),
    items: files.map((file) => ({
      getAsFile: () => file,
      kind: "file",
      type: file.type,
    })),
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
