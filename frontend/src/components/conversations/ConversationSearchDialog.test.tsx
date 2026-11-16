// @vitest-environment jsdom

import { Profiler, useState } from "react";
import {
  cleanup,
  render,
  screen,
  fireEvent,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

import { DEFAULT_CONVERSATION_CONTENT_CARD_COLORS } from "../../store/settings/settingsSchema";
import type { ConversationSearchHit } from "../../types";
import type { ConversationSearchResultState } from "../../hooks/conversations/useConversationsController";
import {
  ConversationSearchDialog,
  ConversationSearchTrigger,
} from "./ConversationSearchDialog";

const dummyTranslator = (key: string, params?: Record<string, unknown>) => {
  if (key === "conversation.search.dialogTitle") return "搜索对话内容";
  if (key === "conversation.search.dialogDescription") return "全局检索会话";
  if (key === "conversation.search.dialogClose") return "关闭";
  if (key === "conversation.search.contentPlaceholder")
    return "搜索内容并定位卡片...";
  if (key === "conversation.search.clear") return "清除";
  if (key === "conversation.search.submit") return "提交搜索";
  if (key === "conversation.search.initialHint")
    return "输入关键词开始全局搜索会话内容...";
  if (key === "conversation.search.resultsCount")
    return `“${params?.query}” 命中 ${params?.count} 个卡片`;
  if (key === "conversation.search.empty") return "没有内容命中。";
  if (key === "conversation.search.type.all") return "全部";
  if (key === "conversation.search.card.question") return "用户问题";
  if (key.includes("answer")) return "回答";
  if (key === "conversation.search.groupCount") return `${params?.count} 个`;
  if (key === "conversation.search.openHit")
    return `打开${params?.type}搜索结果：${params?.title}`;
  if (key === "conversation.search.appChip") return `${params?.app}`;
  if (key === "conversation.search.sessionChip") return `${params?.sessionId}`;
  if (key === "conversation.search.previewTitle") return "卡片内容预览";
  if (key === "conversation.search.openSessionAndJump")
    return "打开会话并定位此卡片";
  if (key === "conversation.search.shortcutNav") return "切换卡片";
  if (key === "conversation.search.shortcutOpen") return "打开";
  if (key === "conversation.search.copySnippet") return "复制内容";
  if (key === "conversation.search.copied") return "已复制";
  if (key === "conversation.search.resetFilters") return "清除筛选";
  if (key === "conversation.search.noPreview")
    return "选择左侧卡片以查看完整内容";
  if (key === "conversation.search.quickHintsTitle") return "快捷搜索建议";
  if (key === "conversation.search.quickHintsDesc")
    return "支持搜索提问、代码段、命令、文件变更及 Skill 卡片";
  return key;
};

const dummyHit: ConversationSearchHit = {
  block_id: "block-1",
  card_type: "answer",
  part_id: "part-1",
  question_id: "q-1",
  question_index: 0,
  question_title: "如何配置资产",
  score: 10,
  session: {
    adapter_id: "codex",
    created_at: "2026-09-20T10:00:00Z",
    external_id: "ext-1",
    id: "session-12345678",
    imported_at: "2026-09-20T10:00:00Z",
    missing: false,
    project_path: "/Users/test/project",
    question_count: 1,
    source_id: "src-1",
    started_at: "2026-09-20T10:00:00Z",
    title: "测试会话",
    turn_count: 1,
    updated_at: "2026-09-20T10:30:00Z",
  },
  snippet: "可以使用 assetiweave 配置文件进行挂载",
  turn_id: "turn-1",
};

const dummyHit2: ConversationSearchHit = {
  block_id: "block-2",
  card_type: "code",
  part_id: "part-2",
  question_id: "q-2",
  question_index: 1,
  question_title: "第二条问题",
  score: 8,
  session: {
    adapter_id: "opencode",
    created_at: "2026-09-20T11:00:00Z",
    external_id: "ext-2",
    id: "session-87654321",
    imported_at: "2026-09-20T11:00:00Z",
    missing: false,
    project_path: "/Users/test/other",
    question_count: 2,
    source_id: "src-2",
    started_at: "2026-09-20T11:00:00Z",
    title: "第二条会话",
    turn_count: 2,
    updated_at: "2026-09-20T11:30:00Z",
  },
  snippet: "const config = { enabled: true };",
  turn_id: "turn-2",
};

describe("ConversationSearchTrigger", () => {
  it("renders placeholder and shortcut badge", () => {
    const handleClick = vi.fn();
    render(
      <ConversationSearchTrigger
        onClick={handleClick}
        placeholder="搜索内容并定位卡片..."
        shortcut="⌘K"
      />,
    );

    expect(screen.getByText("搜索内容并定位卡片...")).toBeDefined();
    expect(screen.getByText("⌘K")).toBeDefined();

    fireEvent.click(screen.getByRole("button"));
    expect(handleClick).toHaveBeenCalledTimes(1);
  });

  it("renders searching indicator and result count badge", () => {
    const { rerender } = render(
      <ConversationSearchTrigger
        onClick={vi.fn()}
        placeholder="搜索内容并定位卡片..."
        searching={true}
        searchingLabel="搜索中…"
        searchingQuery="docker"
      />,
    );

    expect(screen.getByText("docker")).toBeDefined();
    expect(screen.getByText("搜索中…")).toBeDefined();

    rerender(
      <ConversationSearchTrigger
        activeQuery="docker"
        onClick={vi.fn()}
        placeholder="搜索内容并定位卡片..."
        resultCount={12}
        resultCountLabel="12 个结果"
        searching={false}
      />,
    );

    expect(screen.getByText("docker")).toBeDefined();
    expect(screen.getByText("12 个结果")).toBeDefined();
  });
});

describe("ConversationSearchDialog", () => {
  it("does not render when closed", () => {
    const { container } = render(
      <ConversationSearchDialog
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={vi.fn()}
        onClose={vi.fn()}
        onOpenHit={vi.fn()}
        onQueryChange={vi.fn()}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={false}
        query=""
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    expect(container.firstChild).toBeNull();
  });

  it("renders search input, initial hint, and reacts to typing and clear", () => {
    const handleClose = vi.fn();
    const handleQueryChange = vi.fn();

    render(
      <ConversationSearchDialog
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={vi.fn()}
        onClose={handleClose}
        onOpenHit={vi.fn()}
        onQueryChange={handleQueryChange}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query=""
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    expect(screen.getByRole("dialog")).toBeDefined();
    expect(screen.getByText("搜索对话内容")).toBeDefined();
    expect(screen.getByText("输入关键词开始全局搜索会话内容...")).toBeDefined();

    const searchInput = screen.getByRole("searchbox");
    fireEvent.change(searchInput, { target: { value: "aiwc" } });
    fireEvent.keyDown(searchInput, { key: "Enter" });
    expect(handleQueryChange).toHaveBeenCalledWith("aiwc");

    // Clear button appears when there is draft query
    const clearButton = screen.getByLabelText("清除");
    fireEvent.click(clearButton);
    expect(handleQueryChange).toHaveBeenCalledWith("");
  });

  it("does not auto-commit while typing, and submits on Enter or button click", async () => {
    vi.useFakeTimers();
    try {
      const handleQueryChange = vi.fn();
      render(
        <ConversationSearchDialog
          commitDelayMs={700}
          contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
          includeQuestions={true}
          loading={false}
          onCardKindToggle={vi.fn()}
          onClose={vi.fn()}
          onOpenHit={vi.fn()}
          onQueryChange={handleQueryChange}
          onQuestionToggle={vi.fn()}
          onSemanticRoleToggle={vi.fn()}
          onShowAllCardTypes={vi.fn()}
          open={true}
          query=""
          result={null}
          selectedCardKinds={[]}
          selectedSemanticRoles={[]}
          t={dummyTranslator}
        />,
      );

      const searchInput = screen.getByRole("searchbox");
      fireEvent.change(searchInput, { target: { value: "dep" } });
      await vi.advanceTimersByTimeAsync(300);
      fireEvent.change(searchInput, { target: { value: "deploy" } });
      expect(handleQueryChange).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(1500);
      expect(handleQueryChange).not.toHaveBeenCalled();

      fireEvent.keyDown(searchInput, { target: { value: "deploy" }, key: "Enter" });
      expect(handleQueryChange).toHaveBeenCalledWith("deploy");

      fireEvent.change(searchInput, { target: { value: "immediate" } });
      fireEvent.keyDown(searchInput, { key: "Enter" });
      expect(handleQueryChange).toHaveBeenCalledWith("immediate");

      fireEvent.change(searchInput, { target: { value: "button" } });
      const submitBtn = screen.getByRole("button", { name: "提交搜索" });
      fireEvent.click(submitBtn);
      expect(handleQueryChange).toHaveBeenCalledWith("button");
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not auto-trigger search during or after IME composition, only commits on explicit Enter after guard window", async () => {
    vi.useFakeTimers();
    try {
      const handleQueryChange = vi.fn();
      render(
        <ConversationSearchDialog
          commitDelayMs={700}
          contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
          includeQuestions={true}
          loading={false}
          onCardKindToggle={vi.fn()}
          onClose={vi.fn()}
          onOpenHit={vi.fn()}
          onQueryChange={handleQueryChange}
          onQuestionToggle={vi.fn()}
          onSemanticRoleToggle={vi.fn()}
          onShowAllCardTypes={vi.fn()}
          open={true}
          query=""
          result={null}
          selectedCardKinds={[]}
          selectedSemanticRoles={[]}
          t={dummyTranslator}
        />,
      );

      const searchInput = screen.getByRole("searchbox");
      fireEvent.compositionStart(searchInput);
      fireEvent.change(searchInput, {
        target: { value: "zhong" },
        nativeEvent: { isComposing: true },
      });

      await vi.advanceTimersByTimeAsync(1000);
      expect(handleQueryChange).not.toHaveBeenCalled();

      fireEvent.keyDown(searchInput, {
        key: "Enter",
        nativeEvent: { isComposing: true },
      });
      expect(handleQueryChange).not.toHaveBeenCalled();

      fireEvent.compositionEnd(searchInput, { target: { value: "中" } });

      // Trailing Enter within WebKit IME confirmation window must not prematurely submit
      fireEvent.keyDown(searchInput, { key: "Enter", keyCode: 13 });
      expect(handleQueryChange).not.toHaveBeenCalled();

      // Even after long pauses post-composition, must NOT auto-search
      await vi.advanceTimersByTimeAsync(2000);
      expect(handleQueryChange).not.toHaveBeenCalled();

      // User continues typing the second character of the word
      fireEvent.compositionStart(searchInput);
      fireEvent.change(searchInput, {
        target: { value: "中wen" },
        nativeEvent: { isComposing: true },
      });
      fireEvent.compositionEnd(searchInput, { target: { value: "中文" } });

      await vi.advanceTimersByTimeAsync(1500);
      expect(handleQueryChange).not.toHaveBeenCalled();

      // Explicit Enter after guard window submits cleanly
      fireEvent.keyDown(searchInput, { key: "Enter", keyCode: 13 });
      expect(handleQueryChange).toHaveBeenCalledWith("中文");
    } finally {
      vi.useRealTimers();
    }
  });

  it("ignores Enter key with keyCode 229 during IME candidate confirmation", async () => {
    vi.useFakeTimers();
    try {
      const handleQueryChange = vi.fn();
      render(
        <ConversationSearchDialog
          commitDelayMs={700}
          contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
          includeQuestions={true}
          loading={false}
          onCardKindToggle={vi.fn()}
          onClose={vi.fn()}
          onOpenHit={vi.fn()}
          onQueryChange={handleQueryChange}
          onQuestionToggle={vi.fn()}
          onSemanticRoleToggle={vi.fn()}
          onShowAllCardTypes={vi.fn()}
          open={true}
          query=""
          result={null}
          selectedCardKinds={[]}
          selectedSemanticRoles={[]}
          t={dummyTranslator}
        />,
      );

      const searchInput = screen.getByRole("searchbox");
      fireEvent.change(searchInput, { target: { value: "pinyin" } });

      // Simulating IME confirmation Enter with keyCode 229
      fireEvent.keyDown(searchInput, { key: "Enter", keyCode: 229 });
      expect(handleQueryChange).not.toHaveBeenCalled();

      // Normal Enter submits immediately
      fireEvent.keyDown(searchInput, { key: "Enter", keyCode: 13 });
      expect(handleQueryChange).toHaveBeenCalledWith("pinyin");
    } finally {
      vi.useRealTimers();
    }
  });

  it("does not auto-commit 8-hex string when typed keystroke by keystroke, commits on explicit Enter", async () => {
    vi.useFakeTimers();
    try {
      const handleQueryChange = vi.fn();
      const isShortId = (v: string) => /^[0-9a-f]{8}$/i.test(v.trim());

      render(
        <ConversationSearchDialog
          commitDelayMs={700}
          commitImmediatelyWhen={isShortId}
          contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
          includeQuestions={true}
          loading={false}
          onCardKindToggle={vi.fn()}
          onClose={vi.fn()}
          onOpenHit={vi.fn()}
          onQueryChange={handleQueryChange}
          onQuestionToggle={vi.fn()}
          onSemanticRoleToggle={vi.fn()}
          onShowAllCardTypes={vi.fn()}
          open={true}
          query=""
          result={null}
          selectedCardKinds={[]}
          selectedSemanticRoles={[]}
          t={dummyTranslator}
        />,
      );

      const searchInput = screen.getByRole("searchbox");

      // Simulating character by character keystrokes up to 8 hex chars: "2e003a42"
      let currentVal = "";
      const targetStr = "2e003a42";
      for (let i = 0; i < targetStr.length; i++) {
        currentVal += targetStr[i];
        fireEvent.change(searchInput, { target: { value: currentVal } });
        if (i === 7) {
          expect(handleQueryChange).not.toHaveBeenCalled();
        }
      }

      // Even after debounce period, typing keystrokes does not auto-submit
      await vi.advanceTimersByTimeAsync(1000);
      expect(handleQueryChange).not.toHaveBeenCalled();

      // Explicit Enter commits cleanly
      fireEvent.keyDown(searchInput, { key: "Enter", keyCode: 13 });
      expect(handleQueryChange).toHaveBeenCalledWith("2e003a42");
    } finally {
      vi.useRealTimers();
    }
  });

  it("immediately commits recognized short ID on paste or bulk input, and displays badge", () => {
    const handleQueryChange = vi.fn();
    const isShortId = (v: string) => /^[0-9a-f]{8}$/i.test(v.trim());

    render(
      <ConversationSearchDialog
        commitDelayMs={700}
        commitImmediatelyWhen={isShortId}
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={vi.fn()}
        onClose={vi.fn()}
        onOpenHit={vi.fn()}
        onQueryChange={handleQueryChange}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query=""
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    const searchInput = screen.getByRole("searchbox");

    // Simulating paste event + bulk change
    fireEvent.paste(searchInput);
    fireEvent.change(searchInput, { target: { value: "2e003a42" } });

    // Committed immediately without waiting for timers
    expect(handleQueryChange).toHaveBeenCalledWith("2e003a42");
    expect(screen.getByText("#短 ID")).toBeDefined();
  });

  it("displays search hits with full card metadata and allows opening via click", () => {
    const handleOpenHit = vi.fn();
    const handleClose = vi.fn();

    const searchResult: ConversationSearchResultState = {
      cardKinds: ["answer"],
      hits: [dummyHit],
      includeQuestions: true,
      query: "assetiweave",
      recordKind: "session",
      semanticRoles: [],
      totalCount: 1,
    };

    render(
      <ConversationSearchDialog
        appMetaById={
          new Map([["codex", { accentColor: "#10b981", name: "Codex" }]])
        }
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={vi.fn()}
        onClose={handleClose}
        onOpenHit={handleOpenHit}
        onQueryChange={vi.fn()}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query="assetiweave"
        result={searchResult}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    // Verify rich card metadata is rendered in single-column view
    expect(screen.getByRole("button", { name: /测试会话/ })).toBeDefined();
    expect(screen.getByText("如何配置资产")).toBeDefined();
    expect(screen.getByText("~/project")).toBeDefined();

    const hitButton = screen.getByRole("button", {
      name: /测试会话/,
    });
    fireEvent.click(hitButton);

    expect(handleOpenHit).toHaveBeenCalledWith(dummyHit);
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it("supports keyboard arrow navigation and Enter to open active hit", () => {
    const handleOpenHit = vi.fn();
    const handleClose = vi.fn();

    const searchResult: ConversationSearchResultState = {
      cardKinds: ["answer", "code"],
      hits: [dummyHit, dummyHit2],
      includeQuestions: true,
      query: "config",
      recordKind: "session",
      semanticRoles: [],
      totalCount: 2,
    };

    render(
      <ConversationSearchDialog
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={vi.fn()}
        onClose={handleClose}
        onOpenHit={handleOpenHit}
        onQueryChange={vi.fn()}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query="config"
        result={searchResult}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    const searchInput = screen.getByRole("searchbox");

    // Navigate down to second hit
    fireEvent.keyDown(searchInput, { key: "ArrowDown" });

    // Press Enter to open the second hit
    fireEvent.keyDown(searchInput, { key: "Enter" });

    expect(handleOpenHit).toHaveBeenCalledWith(dummyHit2);
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it("opens hit directly when clicking hit card in single-column layout", () => {
    const handleOpenHit = vi.fn();
    const handleClose = vi.fn();

    const searchResult: ConversationSearchResultState = {
      cardKinds: ["answer"],
      hits: [dummyHit],
      includeQuestions: true,
      query: "assetiweave",
      recordKind: "session",
      semanticRoles: [],
      totalCount: 1,
    };

    render(
      <ConversationSearchDialog
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={vi.fn()}
        onClose={handleClose}
        onOpenHit={handleOpenHit}
        onQueryChange={vi.fn()}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query="assetiweave"
        result={searchResult}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    const hitBtn = screen.getByRole("button", { name: /测试会话/ });
    fireEvent.click(hitBtn);

    expect(handleOpenHit).toHaveBeenCalledWith(dummyHit);
    expect(handleClose).toHaveBeenCalledTimes(1);
  });

  it("closes dialog when pressing Escape in input or clicking close button", () => {
    const handleClose = vi.fn();

    render(
      <ConversationSearchDialog
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={vi.fn()}
        onClose={handleClose}
        onOpenHit={vi.fn()}
        onQueryChange={vi.fn()}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query=""
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    const searchInput = screen.getByRole("searchbox");
    fireEvent.keyDown(searchInput, { key: "Escape" });
    expect(handleClose).toHaveBeenCalledTimes(1);

    const closeButtons = screen.getAllByRole("button", { name: "关闭" });
    fireEvent.click(closeButtons[0]);
    expect(handleClose).toHaveBeenCalledTimes(2);
  });

  it("supports app filter and pagination load more", () => {
    const handleAdapterChange = vi.fn();
    const handleLoadMore = vi.fn();

    const searchResult: ConversationSearchResultState = {
      cardKinds: ["answer", "code"],
      hits: [dummyHit, dummyHit2],
      includeQuestions: true,
      query: "config",
      recordKind: "session",
      semanticRoles: [],
      totalCount: 50,
    };

    render(
      <ConversationSearchDialog
        appMetaById={
          new Map([
            ["codex", { name: "Codex" }],
            ["cursor", { name: "Cursor" }],
          ])
        }
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onAdapterChange={handleAdapterChange}
        onCardKindToggle={vi.fn()}
        onClose={vi.fn()}
        onLoadMore={handleLoadMore}
        onOpenHit={vi.fn()}
        onQueryChange={vi.fn()}
        onQuestionToggle={vi.fn()}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query="config"
        result={searchResult}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    // Filter app button - stages filter change without immediately triggering search
    const appBtn = screen.getByRole("button", { name: /Codex/ });
    fireEvent.click(appBtn);
    expect(handleAdapterChange).not.toHaveBeenCalled();

    // Confirm apply filters via Apply button
    const applyBtn = screen.getByRole("button", { name: "应用筛选" });
    expect(applyBtn).toBeDefined();
    fireEvent.click(applyBtn);
    expect(handleAdapterChange).toHaveBeenCalledWith("codex");

    // Pagination load more button
    const loadMoreBtn = screen.getByRole("button", {
      name: "加载更多搜索结果",
    });
    fireEvent.click(loadMoreBtn);
    expect(handleLoadMore).toHaveBeenCalledTimes(1);
  });

  it("renders quick category cards in initial welcome state and triggers filter toggles on confirmation", () => {
    const handleCardKindToggle = vi.fn();
    const handleQuestionToggle = vi.fn();

    render(
      <ConversationSearchDialog
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onCardKindToggle={handleCardKindToggle}
        onClose={vi.fn()}
        onOpenHit={vi.fn()}
        onQueryChange={vi.fn()}
        onQuestionToggle={handleQuestionToggle}
        onSemanticRoleToggle={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        open={true}
        query=""
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    const dialogs = screen.getAllByRole("dialog");
    const dialog = dialogs[dialogs.length - 1];
    const dialogWithin = within(dialog);

    expect(dialogWithin.getAllByText("代码片段").length).toBeGreaterThan(0);
    expect(dialogWithin.getAllByText("终端命令").length).toBeGreaterThan(0);

    fireEvent.click(dialogWithin.getByRole("button", { name: /代码片段/ }));
    expect(handleCardKindToggle).not.toHaveBeenCalled();

    const applyBtn = screen.getByRole("button", { name: "应用筛选" });
    fireEvent.click(applyBtn);
    expect(handleCardKindToggle).toHaveBeenCalledWith("code");
  });

  it("supports staged filter confirmation via Enter key and onApplyFilters batching", () => {
    const handleApplyFilters = vi.fn();
    const handleQueryChange = vi.fn();

    render(
      <ConversationSearchDialog
        appMetaById={new Map([["codex", { name: "Codex" }]])}
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onApplyFilters={handleApplyFilters}
        onClose={vi.fn()}
        onOpenHit={vi.fn()}
        onQueryChange={handleQueryChange}
        open={true}
        query="search term"
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    const codexBtn = screen.getByRole("button", { name: /Codex/ });
    fireEvent.click(codexBtn);
    expect(handleApplyFilters).not.toHaveBeenCalled();

    const searchInput = screen.getByRole("searchbox");
    fireEvent.keyDown(searchInput, { key: "Enter" });

    expect(handleApplyFilters).toHaveBeenCalledWith({
      adapterId: "codex",
      cardKinds: [],
      includeQuestions: true,
      semanticRoles: [],
    });
  });

  it("discards staged filter changes when Escape is pressed without search text", () => {
    const handleApplyFilters = vi.fn();

    render(
      <ConversationSearchDialog
        appMetaById={new Map([["codex", { name: "Codex" }]])}
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={false}
        onApplyFilters={handleApplyFilters}
        onClose={vi.fn()}
        onOpenHit={vi.fn()}
        onQueryChange={vi.fn()}
        open={true}
        query=""
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    const codexBtn = screen.getByRole("button", { name: /Codex/ });
    fireEvent.click(codexBtn);
    expect(screen.queryByRole("button", { name: "应用筛选" })).not.toBeNull();

    // Press Escape to discard
    const searchInput = screen.getByRole("searchbox");
    fireEvent.keyDown(searchInput, { key: "Escape" });

    // Staging changes should be discarded back to clean state
    expect(screen.queryByRole("button", { name: "应用筛选" })).toBeNull();
  });

  it("does not re-render during IME composition or overwrite/re-select draft when committed query flows back", async () => {
    vi.useFakeTimers();
    try {
      const handleQueryChange = vi.fn();
      let renderCount = 0;

      function ControlledSearchHarness() {
        const [query, setQuery] = useState("");
        return (
          <Profiler
            id="conversation-search-dialog"
            onRender={() => {
              renderCount += 1;
            }}
          >
            <ConversationSearchDialog
              commitDelayMs={700}
              contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
              includeQuestions={true}
              loading={false}
              onClose={vi.fn()}
              onOpenHit={vi.fn()}
              onQueryChange={(next) => {
                handleQueryChange(next);
                setQuery(next);
              }}
              open={true}
              query={query}
              result={null}
              selectedCardKinds={[]}
              selectedSemanticRoles={[]}
              t={dummyTranslator}
            />
          </Profiler>
        );
      }

      render(<ControlledSearchHarness />);
      const searchInput = screen.getByRole("searchbox") as HTMLInputElement;

      // Flush initial open focus timer
      await vi.advanceTimersByTimeAsync(60);
      renderCount = 0;

      // 1. IME composition must not trigger React re-renders or premature search
      fireEvent.compositionStart(searchInput);
      fireEvent.change(searchInput, {
        target: { value: "ceshi" },
        nativeEvent: { isComposing: true },
      });
      expect(searchInput.value).toBe("ceshi");
      expect(renderCount).toBe(0);

      await vi.advanceTimersByTimeAsync(1000);
      expect(handleQueryChange).not.toHaveBeenCalled();

      // Finish first word -> must NOT auto-commit without explicit Enter/Submit
      fireEvent.compositionEnd(searchInput, { target: { value: "测试" } });
      await vi.advanceTimersByTimeAsync(1200);
      expect(handleQueryChange).not.toHaveBeenCalled();

      // Explicit Enter commits "测试"
      fireEvent.keyDown(searchInput, { key: "Enter", keyCode: 13 });
      expect(handleQueryChange).toHaveBeenCalledWith("测试");

      // 2. While dialog remains open and committed query ("测试") flows back via props,
      // user immediately starts composing the next word. Must NOT reset IME or select text.
      fireEvent.compositionStart(searchInput);
      fireEvent.change(searchInput, {
        target: { value: "测试neirong" },
        nativeEvent: { isComposing: true },
      });
      await vi.advanceTimersByTimeAsync(100);

      expect(searchInput.value).toBe("测试neirong");
      expect(searchInput.selectionStart).toBe(searchInput.selectionEnd);

      // Even after 1s while composing, "测试neirong" must NOT be prematurely committed
      await vi.advanceTimersByTimeAsync(1000);
      expect(handleQueryChange).toHaveBeenCalledTimes(1);

      fireEvent.compositionEnd(searchInput, {
        target: { value: "测试内容" },
      });
      await vi.advanceTimersByTimeAsync(1200);
      expect(handleQueryChange).toHaveBeenCalledTimes(1);

      fireEvent.keyDown(searchInput, { key: "Enter", keyCode: 13 });
      expect(handleQueryChange).toHaveBeenLastCalledWith("测试内容");
      expect(handleQueryChange).toHaveBeenCalledTimes(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("renders running task banner, conflict confirmation banner, and preserves active task on draft clear", () => {
    const handleCancelActiveTask = vi.fn();
    const handleConfirmReplace = vi.fn();
    const handleKeepWaiting = vi.fn();
    const handleSubmitSearch = vi.fn();

    const { rerender } = render(
      <ConversationSearchDialog
        activeTask={{
          id: "task-search-1",
          query: "旧词",
          recordKind: "session",
          task: null,
        }}
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={true}
        onCancelActiveTask={handleCancelActiveTask}
        onClose={vi.fn()}
        onOpenHit={vi.fn()}
        onQueryChange={vi.fn()}
        onSubmitSearch={handleSubmitSearch}
        open={true}
        query="旧词"
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    expect(screen.getByRole("status")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "conversation.search.cancelTask" }));
    expect(handleCancelActiveTask).toHaveBeenCalledTimes(1);

    // Clearing draft with Escape/X while active task runs must NOT cancel or submit
    const searchInput = screen.getByRole("searchbox") as HTMLInputElement;
    fireEvent.click(screen.getByRole("button", { name: "清除" }));
    expect(searchInput.value).toBe("");
    expect(handleCancelActiveTask).toHaveBeenCalledTimes(1);
    expect(handleSubmitSearch).not.toHaveBeenCalled();

    // Render conflict confirmation banner
    rerender(
      <ConversationSearchDialog
        activeTask={{
          id: "task-search-1",
          query: "旧词",
          recordKind: "session",
          task: null,
        }}
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        includeQuestions={true}
        loading={true}
        onCancelActiveTask={handleCancelActiveTask}
        onClose={vi.fn()}
        onConfirmConflictReplace={handleConfirmReplace}
        onDismissConflictKeepWaiting={handleKeepWaiting}
        onOpenHit={vi.fn()}
        onQueryChange={vi.fn()}
        onSubmitSearch={handleSubmitSearch}
        open={true}
        pendingConflict={{
          activeTaskId: "task-search-1",
          activeQuery: "旧词",
          nextQuery: "新词",
          submission: {
            query: "新词",
            recordKind: "session",
            adapterId: null,
            cardKinds: [],
            semanticRoles: [],
            includeQuestions: true,
          },
        }}
        query="旧词"
        result={null}
        selectedCardKinds={[]}
        selectedSemanticRoles={[]}
        t={dummyTranslator}
      />,
    );

    expect(screen.getByRole("alert")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "conversation.search.conflictReplace" }),
    );
    expect(handleConfirmReplace).toHaveBeenCalledTimes(1);

    fireEvent.click(
      screen.getByRole("button", { name: "conversation.search.conflictWait" }),
    );
    expect(handleKeepWaiting).toHaveBeenCalledTimes(1);
  });
});
