/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Profiler, useState } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ConversationContentCards,
  buildConversationContentBlocks,
  type ConversationContentVisibility,
} from "../../components/conversations/ConversationContentCards";
import {
  ConversationBackgroundTaskIndicator,
  ConversationContentFilter,
  ConversationSyncProgress,
} from "../../components/conversations/ConversationToolbarControls";
import { ConversationImportDialog } from "../../components/conversations/ConversationImportDialog";
import { DebouncedToolbarSearch } from "../../components/common/DataToolbar";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Translator } from "../../i18n/I18nProvider";
import { messages, type TranslationParams } from "../../i18n/messages";
import { DEFAULT_CONVERSATION_CONTENT_CARD_COLORS } from "../../store/settings/AppSettingsProvider";
import type {
  AppShortcut,
  ConversationAdapter,
  ConversationQuestionDetail,
  ConversationSearchHit,
  ConversationSessionDetail,
} from "../../types";
import {
  AppSessionBrowser,
  ConversationContentSearchResults,
  groupConversationSessionsByApp,
  ConversationExportDialog,
  loadAllConversationSessionPages,
  MarkdownContent,
  QuestionPreview,
  SessionQuestionWorkspace,
  preferredConversationQuestionId,
} from "./ConversationsPage";

beforeEach(() => {
  vi.stubGlobal("localStorage", createMockLocalStorage());
});

afterEach(() => {
  cleanup();
  globalThis.localStorage?.clear();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("MarkdownContent", () => {
  it("renders markdown headings, lists, inline code, strong text, and code fences", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "# Question summary",
          "",
          "Use `conversation.sync` with **dry run** first.",
          "",
          "- Session",
          "- Question",
          "",
          "```sh",
          "assetiweave-cli conversation sync --dry-run",
          "```",
        ].join("\n")}
      />,
    );

    expect(html).toContain("Question summary");
    expect(html).toContain("<code");
    expect(html).toContain("conversation.sync");
    expect(html).toContain("<strong");
    expect(html).toContain("dry run");
    expect(html).toContain("<li>Session</li>");
    expect(html).toContain("assetiweave-cli conversation sync --dry-run");
  });

  it("renders markdown tables and mermaid diagrams", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "| 阶段 | 状态 |",
          "| --- | --- |",
          "| 导入 | 完成 |",
          "| 渲染 | 等待 |",
          "",
          "```mermaid",
          "flowchart TD",
          "  A[导入] --> B[渲染]",
          "```",
        ].join("\n")}
      />,
    );

    expect(html).toContain("<table");
    expect(html).toContain("<th");
    expect(html).toContain("阶段");
    expect(html).toContain("<td");
    expect(html).toContain("完成");
    expect(html).toContain('data-mermaid-diagram="true"');
    expect(html).toContain("flowchart TD");
  });

  it("normalizes escaped OpenCode markdown text before rendering", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "| 管理费率 | 1\\.20%（前端） | 托管费率 | 0\\.20% |\\n| 业绩比较基准 | 沪深300指数收益率\\*95%+活期存款利率（税后）\\*5% | 跟踪标的 | 沪深300指数 |",
          "\\n\\n什么是保本基金的保本模式？ [详情]\\n(http://help.1234567.com.cn/question_795.html)",
          "\\n\\n#### 投资目标\\n本基金为指数增强型股票基金，追求超越业绩比较基准的投资回报。",
          "\\n\\n- 内地依法发行上市的股票\\n- 存托凭证",
        ].join("")}
      />,
    );

    expect(html).toContain("<table");
    expect(html).toContain("管理费率");
    expect(html).toContain("1.20%（前端）");
    expect(html).toContain("沪深300指数收益率*95%+活期存款利率（税后）*5%");
    expect(html).toContain('href="http://help.1234567.com.cn/question_795.html"');
    expect(html).toContain("投资目标");
    expect(html).toContain("<li>内地依法发行上市的股票</li>");
    expect(html).not.toContain("\\n");
    expect(html).not.toContain("\\*");
    expect(html).not.toContain("\\.");
  });

  it("renders inline and display LaTeX math in markdown previews", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "Inline math supports \\(\\alpha + \\beta\\) and $E=mc^2$.",
          "",
          "$$",
          "\\frac{a}{b} = c",
          "$$",
          "",
          "\\[",
          "\\int_0^1 x^2 dx",
          "\\]",
        ].join("\n")}
      />,
    );

    expect(html.match(/data-latex-math="inline"/g)).toHaveLength(2);
    expect(html.match(/data-latex-math="display"/g)).toHaveLength(2);
    expect(html).toContain("katex");
    expect(html).not.toContain("\\(\\alpha");
    expect(html).not.toContain("$$");
  });

  it("renders a question-based preview with markdown parts and turn split controls", () => {
    const html = renderToStaticMarkup(
      <QuestionPreview
        onExport={async () => undefined}
        onPickOutputRoot={async () => null}
        onSplit={async () => undefined}
        outputRoot="/tmp/conversation-export"
        question={questionDetail}
        session={sessionDetail}
        setOutputRoot={vi.fn()}
        t={t}
      />,
    );

    expect(html).toContain("用户问题");
    expect(html).toContain("回复与活动");
    expect(html).toContain("同步流程");
    expect(html).toContain("<li>按 Session 导入</li>");
    expect(html).toContain("assetiweave-cli conversation sync --dry-run");
    expect(html).toContain("从这里拆分");
    expect(html).toContain("导出");
  });

  it("omits project path from web record question previews", () => {
    const html = renderToStaticMarkup(
      <QuestionPreview
        onExport={async () => undefined}
        onPickOutputRoot={async () => null}
        outputRoot="/tmp/web-record-export"
        question={questionDetail}
        recordKind="web"
        session={sessionDetail}
        setOutputRoot={vi.fn()}
        t={t}
      />,
    );

    expect(html).not.toContain("code-space/assetiweave");
    expect(html).not.toContain("无项目路径");
  });

  it("selects the first question with adapter-declared content by default", () => {
    const emptyImportedQuestion: ConversationQuestionDetail = {
      question: {
        ...questionDetail.question,
        id: "empty-imported-question",
        question_index: 0,
        question_text: "Imported context without assistant content",
      },
      turns: [
        {
          ...questionDetail.turns[0],
          id: "empty-turn",
          user_text: "Imported context without assistant content",
        },
      ],
      parts: [],
    };

    expect(preferredConversationQuestionId([emptyImportedQuestion, questionDetail], null))
      .toBe(questionDetail.question.id);
    expect(preferredConversationQuestionId([emptyImportedQuestion, questionDetail], emptyImportedQuestion.question.id))
      .toBe(questionDetail.question.id);
  });

  it("lets users choose the inline question export output root", async () => {
    const setOutputRoot = vi.fn();
    const onPickOutputRoot = vi.fn(async () => "/tmp/detail-export-root");

    render(
      <QuestionPreview
        onExport={async () => undefined}
        onPickOutputRoot={onPickOutputRoot}
        onSplit={async () => undefined}
        outputRoot="/tmp/conversation-export"
        question={questionDetail}
        session={sessionDetail}
        setOutputRoot={setOutputRoot}
        t={t}
      />,
    );

    const pickButton = screen.getByRole("button", { name: "选择导出根目录" });
    expect(pickButton.textContent).toBe("");

    fireEvent.click(pickButton);

    await waitFor(() => {
      expect(onPickOutputRoot).toHaveBeenCalledTimes(1);
      expect(setOutputRoot).toHaveBeenCalledWith("/tmp/detail-export-root");
    });
  });

  it("groups sessions from the app level before browsing individual sessions", () => {
    const groups = groupConversationSessionsByApp(adapters, [
      {
        ...sessionDetail.session,
        question_count: 1,
        turn_count: 2,
      },
      {
        ...sessionDetail.session,
        id: "session-2",
        adapter_id: "opencode",
        question_count: 4,
        turn_count: 7,
      },
    ]);

    expect(groups.map((group) => [group.app.id, group.sessions.length])).toEqual([
      ["codex", 1],
      ["opencode", 1],
      ["claude-code", 0],
    ]);
    expect(groups[0].questionCount).toBe(1);
    expect(groups[1].turnCount).toBe(7);
  });

  it("groups app sessions by project folder before listing individual sessions", () => {
    const groups = groupConversationSessionsByApp(adapters, [
      {
        ...sessionDetail.session,
        id: "codex-1",
        project_path: "/Users/util6/code-space/assetiweave",
        question_count: 2,
        turn_count: 5,
      },
      {
        ...sessionDetail.session,
        id: "codex-2",
        project_path: "/Users/util6/code-space/assetiweave",
        question_count: 3,
        turn_count: 8,
      },
      {
        ...sessionDetail.session,
        id: "codex-no-project",
        project_path: null,
        question_count: 1,
        turn_count: 1,
      },
    ]);

    expect(groups[0].projectGroups.map((group) => [group.projectPath, group.sessions.length])).toEqual([
      ["/Users/util6/code-space/assetiweave", 2],
      [null, 1],
    ]);
    expect(groups[0].projectGroups[0].questionCount).toBe(5);
    expect(groups[0].projectGroups[0].turnCount).toBe(13);
  });

  it("loads every session page instead of only the first 100 records", async () => {
    const allSessions = Array.from({ length: 153 }, (_, index) => ({
      ...sessionDetail.session,
      id: `session-${index + 1}`,
      external_id: `external-${index + 1}`,
      question_count: 1,
      turn_count: 1,
    }));
    const listSessions = vi.fn(async ({ limit = 100, offset = 0 }) => allSessions.slice(offset, offset + limit));

    const sessions = await loadAllConversationSessionPages(listSessions, null);

    expect(sessions).toHaveLength(153);
    expect(sessions[sessions.length - 1]?.id).toBe("session-153");
    expect(listSessions).toHaveBeenCalledTimes(2);
    expect(listSessions.mock.calls.map(([params]) => params.offset)).toEqual([0, 100]);
  });

  it("uses shared sticky split controls for session browsing", () => {
    const html = renderToStaticMarkup(
      <AppSessionBrowser
        appShortcuts={[]}
        columnMinWidth={300}
        groups={groupConversationSessionsByApp(adapters, [
          {
            ...sessionDetail.session,
            question_count: 1,
            turn_count: 2,
          },
        ])}
        onAppSelect={vi.fn()}
        onProjectSelect={vi.fn()}
        onSessionOpen={vi.fn()}
        selectedAppId="codex"
        selectedProjectKey="/Users/util6/code-space/assetiweave"
        t={t}
      />,
    );

    expect(html).toContain("项目文件夹");
    expect(html).toContain("code-space/assetiweave");
    expect(html).toContain("水平浏览分栏");
    expect(html).toContain('role="scrollbar"');
    expect(html).toContain("sticky bottom-0");
    expect(html).toContain("min-h-[620px]");
  });

  it("shows the session summary as separate chips", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });

    render(
      <AppSessionBrowser
        appShortcuts={[]}
        columnMinWidth={300}
        groups={groupConversationSessionsByApp(adapters, [
          {
            ...sessionDetail.session,
            id: "conversation-session-abcdef1234567890abcdef1234567890",
            question_count: 1,
            turn_count: 2,
          },
        ])}
        onAppSelect={vi.fn()}
        onProjectSelect={vi.fn()}
        onSessionOpen={vi.fn()}
        selectedAppId="codex"
        selectedProjectKey="/Users/util6/code-space/assetiweave"
        t={t}
      />,
    );
    const hashId = screen.getByText("abcdef12");

    expect(hashId.className).toContain("font-mono");
    expect(screen.getByText("1 个问题").className).toContain("rounded-md");
    expect(screen.getByText("2 个 Turn").className).toContain("rounded-md");
    expect(hashId.parentElement?.getAttribute("aria-label")).toBe("1 个问题 · 2 个 Turn");
    expect(screen.queryByText(/Hash ID/)).toBeNull();
    expect(screen.queryByText(/abcdef123/)).toBeNull();
  });

  it("shows app and session id chips on content search result entries", () => {
    const hit: ConversationSearchHit = {
      block_id: "part-1-answer",
      card_type: "answer",
      part_id: "part-1",
      question_id: "question-1",
      question_index: 0,
      question_title: "同步流程",
      score: 100,
      session: {
        ...sessionDetail.session,
        id: "conversation-session-abcdef1234567890abcdef1234567890",
        question_count: 1,
        turn_count: 2,
      },
      snippet: "导入后按问题预览。",
      turn_id: "turn-1",
    };

    render(
      <ConversationContentSearchResults
        appMetaById={new Map([["codex", { accentColor: "#10b981", name: "Codex" }]])}
        contentCardColors={DEFAULT_CONVERSATION_CONTENT_CARD_COLORS}
        loading={false}
        onCardTypeToggle={vi.fn()}
        onOpenHit={vi.fn()}
        onShowAllCardTypes={vi.fn()}
        result={{
          contentTypes: ["answer"],
          hits: [hit],
          query: "导入",
          recordKind: "session",
          totalCount: 1,
        }}
        selectedCardTypes={["answer"]}
        t={t}
      />,
    );

    const appChip = screen.getByText("Codex");

    expect(appChip.className).toContain("rounded-md");
    expect(appChip.getAttribute("style")).toContain("rgb(16, 185, 129)");
    expect(screen.queryByText("APP Codex")).toBeNull();
    expect(screen.getByText("abcdef12").className).toContain("font-mono");
    expect(screen.queryByText("Session abcdef12")).toBeNull();
    expect(screen.queryByText(/abcdef123/)).toBeNull();
  });

  it("omits project path UI when browsing web record sessions", () => {
    const html = renderToStaticMarkup(
      <AppSessionBrowser
        appShortcuts={[]}
        columnMinWidth={300}
        groups={groupConversationSessionsByApp(adapters, [
          {
            ...sessionDetail.session,
            id: "web-session-1",
            project_path: "/Users/util6/code-space/assetiweave",
            question_count: 1,
            turn_count: 2,
          },
        ])}
        onAppSelect={vi.fn()}
        onProjectSelect={vi.fn()}
        onSessionOpen={vi.fn()}
        recordKind="web"
        selectedAppId="codex"
        selectedProjectKey={null}
        t={t}
      />,
    );

    expect(html).not.toContain("项目文件夹");
    expect(html).not.toContain("code-space/assetiweave");
    expect(html).toContain("Conversation fixture");
  });

  it("opens sessions only from the explicit action so card text can be selected", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const onSessionOpen = vi.fn();

    render(
      <AppSessionBrowser
        appShortcuts={[]}
        columnMinWidth={300}
        groups={groupConversationSessionsByApp(adapters, [
          {
            ...sessionDetail.session,
            id: "conversation-session-abcdef1234567890abcdef1234567890",
            question_count: 1,
            turn_count: 2,
          },
        ])}
        onAppSelect={vi.fn()}
        onProjectSelect={vi.fn()}
        onSessionOpen={onSessionOpen}
        selectedAppId="codex"
        selectedProjectKey="/Users/util6/code-space/assetiweave"
        t={t}
      />,
    );

    fireEvent.click(screen.getByText("Conversation fixture"));
    fireEvent.click(screen.getByText("abcdef12"));
    fireEvent.click(screen.getByText("1 个问题"));

    expect(onSessionOpen).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "打开 Session Conversation fixture" }));

    expect(onSessionOpen).toHaveBeenCalledTimes(1);
    expect(onSessionOpen).toHaveBeenCalledWith("conversation-session-abcdef1234567890abcdef1234567890");
  });

  it("does not re-render the session browser for unrelated parent search updates", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    const groups = groupConversationSessionsByApp(adapters, [
      {
        ...sessionDetail.session,
        question_count: 1,
        turn_count: 2,
      },
    ]);
    const onAppSelect = vi.fn();
    const onProjectSelect = vi.fn();
    const onSessionOpen = vi.fn();
    const appShortcuts: AppShortcut[] = [];
    let translateCallCount = 0;
    const countingT: Translator = (key, params) => {
      translateCallCount += 1;
      return t(key, params);
    };

    function Wrapper() {
      const [, setSearchDraft] = useState("");
      return (
        <>
          <button onClick={() => setSearchDraft("deploy")} type="button">
            Update search draft
          </button>
          <AppSessionBrowser
            appShortcuts={appShortcuts}
            columnMinWidth={300}
            groups={groups}
            onAppSelect={onAppSelect}
            onProjectSelect={onProjectSelect}
            onSessionOpen={onSessionOpen}
            selectedAppId="codex"
            selectedProjectKey="/Users/util6/code-space/assetiweave"
            t={countingT}
          />
        </>
      );
    }

    render(<Wrapper />);

    translateCallCount = 0;

    fireEvent.click(screen.getByRole("button", { name: "Update search draft" }));

    expect(translateCallCount).toBe(0);
  });

  it("does not re-render the debounced search control during IME composition", async () => {
    vi.useFakeTimers();
    try {
      const onChange = vi.fn();
      let renderCount = 0;

      render(
        <Profiler id="content-search" onRender={() => {
          renderCount += 1;
        }}>
          <DebouncedToolbarSearch
            commitDelayMs={220}
            onChange={onChange}
            placeholder="Search content"
            value=""
          />
        </Profiler>,
      );

      const searchInput = screen.getByPlaceholderText("Search content") as HTMLInputElement;
      renderCount = 0;

      fireEvent.compositionStart(searchInput);
      fireEvent.change(searchInput, {
        target: { value: "zhong" },
        nativeEvent: { isComposing: true },
      });

      expect(searchInput.value).toBe("zhong");
      expect(renderCount).toBe(0);

      await vi.advanceTimersByTimeAsync(1000);

      expect(onChange).not.toHaveBeenCalled();

      fireEvent.compositionEnd(searchInput, {
        data: "中",
        target: { value: "中" },
      });

      await vi.advanceTimersByTimeAsync(220);

      expect(onChange).toHaveBeenCalledWith("中");
    } finally {
      vi.useRealTimers();
    }
  });

  it("splits commands and execution results into independently filterable cards", () => {
    const blocks = buildConversationContentBlocks(questionDetail.parts);

    expect(blocks.map((block) => block.type)).toEqual(["answer", "command", "result"]);

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: false,
          result: true,
          tool: false,
        }}
      />,
    );

    expect(html).toContain("同步流程");
    expect(html).toContain("命令执行结果");
    expect(html).toContain("tests passed");
    expect(html).not.toContain("assetiweave-cli conversation sync --dry-run");

    const commandOnlyHtml = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{
          answer: false,
          code: false,
          command: true,
          result: false,
          tool: false,
        }}
      />,
    );

    expect(commandOnlyHtml).toContain("assetiweave-cli conversation sync --dry-run");
    expect(commandOnlyHtml).not.toContain("tests passed");
    expect(commandOnlyHtml).not.toContain("completed");
    expect(commandOnlyHtml).not.toContain("退出码");
  });

  it("copies the raw text from a content card", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    render(
      <ConversationContentCards
        blocks={buildConversationContentBlocks(questionDetail.parts)}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "复制命令执行" }));

    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("assetiweave-cli conversation sync --dry-run"),
    );
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "已复制" })).toBeTruthy();
    });
  });

  it("copies user prompt text from the user question card", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    render(
      <QuestionPreview
        onExport={async () => undefined}
        onPickOutputRoot={async () => null}
        onSplit={async () => undefined}
        outputRoot="/tmp/conversation-export"
        question={questionDetail}
        session={sessionDetail}
        setOutputRoot={vi.fn()}
        t={t}
      />,
    );

    fireEvent.click(screen.getAllByRole("button", { name: "复制用户问题" })[0]);

    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("AssetIWeave 如何同步对话记录？"),
    );
  });

  it("renders stable card anchors for search-result navigation", () => {
    const html = renderToStaticMarkup(
      <QuestionPreview
        activeSearchTarget={{
          blockId: "part-1-answer",
          cardType: "answer",
          questionId: "question-1",
          sessionId: "session-1",
        }}
        onExport={async () => undefined}
        onPickOutputRoot={async () => null}
        outputRoot="/tmp/conversation-export"
        question={questionDetail}
        session={sessionDetail}
        setOutputRoot={vi.fn()}
        t={t}
      />,
    );

    expect(html).toContain('id="conversation-card-turn-1-question"');
    expect(html).toContain('id="conversation-card-part-1-answer"');
    expect(html).toContain('data-conversation-card-id="part-1-answer"');
    expect(html).toContain("ring-2 ring-primary/70");
  });

  it("preserves and restores line breaks in command result previews", () => {
    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={[
          {
            id: "result-with-file-matches",
            role: "tool",
            text: [
              "Chunk ID: 5b951a",
              "Wall time: 0.0000 seconds",
              "Process exited with code 0",
              "Output:",
              "./specs/design.md:69:- App 快捷入口支持真实应用图标",
              "./cli/internal/errlint/legacy_exit_test.go:23: got := summarizeBySymbol(violations)",
              "./src-tauri/src/path_utils.rs:166: &[\"symbolic-ref\", \"--short\"]",
            ].join("\n"),
            type: "result",
          },
        ]}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(html).toContain("<pre");
    expect(html).toContain("whitespace-pre-wrap");
    expect(html).toContain("Output:\n./specs/design.md:69");
    expect(html).toContain("\n./cli/internal/errlint/legacy_exit_test.go:23");
    expect(html).toContain("\n./src-tauri/src/path_utils.rs:166");
  });

  it("collapses long command result previews with an expand-all action", () => {
    render(
      <ConversationContentCards
        blocks={[
          {
            id: "long-result",
            role: "tool",
            text: ["line one", "line two", "line three", "line four"].join("\n"),
            type: "result",
          },
        ]}
        resultPreviewLineLimit={2}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(screen.getByText((_content, element) =>
      element?.tagName.toLowerCase() === "code" &&
      element.textContent === "line one\nline two",
    )).toBeTruthy();
    expect(screen.queryByText(/line three/)).toBeNull();
    expect(screen.getByText("显示 2 / 4 行")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "展开全部" }));

    expect(screen.getByText((_content, element) =>
      element?.tagName.toLowerCase() === "code" &&
      element.textContent === "line one\nline two\nline three\nline four",
    )).toBeTruthy();
    expect(screen.getByText("显示 4 / 4 行")).toBeTruthy();
    expect(screen.getByRole("button", { name: "收起" })).toBeTruthy();
  });

  it("renders content switches as toolbar controls and cards for every supported type", () => {
    const visibility: ConversationContentVisibility = {
      answer: true,
      code: true,
      command: true,
      result: true,
      tool: true,
    };
    const filterHtml = renderToStaticMarkup(
      <ConversationContentFilter
        onChange={vi.fn()}
        t={t}
        visibility={visibility}
      />,
    );
    const previewHtml = renderToStaticMarkup(
      <QuestionPreview
        onExport={async () => undefined}
        onPickOutputRoot={async () => null}
        onSplit={async () => undefined}
        outputRoot="/tmp/conversation-export"
        question={richQuestionDetail}
        session={{ ...sessionDetail, questions: [richQuestionDetail] }}
        setOutputRoot={vi.fn()}
        t={t}
      />,
    );

    for (const label of ["回答文字", "工具调用", "命令执行", "代码", "执行结果"]) {
      expect(filterHtml).toContain(label);
    }
    for (const type of Object.keys(visibility)) {
      expect(previewHtml).toContain(`data-content-type="${type}"`);
    }
    expect(previewHtml).not.toContain("回答内容显示设置");
  });

  it("renders question checkboxes for batch export selection", () => {
    const html = renderToStaticMarkup(
      <SessionQuestionWorkspace
        contentCardColors={{
          answer: "#facc15",
          code: "#60a5fa",
          command: "#f59e0b",
          result: "#34d399",
          tool: "#22c55e",
        }}
        onExport={async () => undefined}
        onMerge={async () => undefined}
        onPickOutputRoot={async () => null}
        onQuestionSelect={vi.fn()}
        onQuestionSelectionChange={vi.fn()}
        onSplit={async () => undefined}
        outputRoot="/tmp/conversation-export"
        question={richQuestionDetail}
        questions={[questionDetail, richQuestionDetail]}
        selectedQuestionId={richQuestionDetail.question.id}
        selectedQuestionIds={new Set([richQuestionDetail.question.id])}
        session={{ ...sessionDetail, questions: [questionDetail, richQuestionDetail] }}
        setOutputRoot={vi.fn()}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(html).toContain('type="checkbox"');
    expect(html).toContain("选择问题");
    expect(html).toContain('checked=""');
    expect(html).toContain("水平浏览分栏");
    expect(html).toContain('role="scrollbar"');
    expect(html).toContain("sticky bottom-0");
    expect(html).toContain("flex h-48 flex-col overflow-hidden");
    expect(html).toContain("line-clamp-2 min-w-0 break-words");
  });

  it("collapses the question list so the selected question preview can use the full width", () => {
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });

    render(
      <SessionQuestionWorkspace
        contentCardColors={{
          answer: "#facc15",
          code: "#60a5fa",
          command: "#f59e0b",
          result: "#34d399",
          tool: "#22c55e",
        }}
        onExport={async () => undefined}
        onPickOutputRoot={async () => null}
        onQuestionSelect={vi.fn()}
        onQuestionSelectionChange={vi.fn()}
        outputRoot="/tmp/conversation-export"
        question={questionDetail}
        questions={[questionDetail]}
        selectedQuestionId={questionDetail.question.id}
        selectedQuestionIds={new Set()}
        session={sessionDetail}
        setOutputRoot={vi.fn()}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(screen.getByRole("heading", { name: "问题" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "预览问题 同步流程" })).toBeTruthy();
    expect(screen.getByRole("scrollbar", { name: "水平浏览分栏" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "收起问题列表" }));

    expect(screen.queryByRole("heading", { name: "问题" })).toBeNull();
    expect(screen.queryByRole("button", { name: "预览问题 同步流程" })).toBeNull();
    expect(screen.queryByRole("scrollbar", { name: "水平浏览分栏" })).toBeNull();
    expect(screen.getByRole("button", { name: "展开问题列表" })).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "展开问题列表" }));

    expect(screen.getByRole("heading", { name: "问题" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "收起问题列表" })).toBeTruthy();
  });

  it("renders an export dialog that reuses content visibility controls", () => {
    const html = renderToStaticMarkup(
      <ConversationExportDialog
        contentCardColors={{
          answer: "#facc15",
          code: "#60a5fa",
          command: "#f59e0b",
          result: "#34d399",
          tool: "#22c55e",
        }}
        exporting={false}
        mode="questions"
        onClose={vi.fn()}
        onConfirm={async () => undefined}
        onOutputRootChange={vi.fn()}
        onPickOutputRoot={async () => "/tmp/selected-export-root"}
        onVisibilityChange={vi.fn()}
        outputRoot="/tmp/conversation-export"
        questionCount={2}
        t={t}
        visibility={{
          answer: true,
          code: false,
          command: true,
          result: true,
          tool: false,
        }}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain("导出 Markdown");
    expect(html).toContain("2 个问题");
    expect(html).toContain("/tmp/conversation-export");
    expect(html).toContain('aria-label="选择导出根目录"');
    for (const label of ["回答文字", "工具调用", "命令执行", "代码", "执行结果"]) {
      expect(html).toContain(label);
    }
    expect(html).toContain("确认导出");
  });

  it("lets users choose the export output root from a filesystem picker", async () => {
    const onOutputRootChange = vi.fn();
    const onPickOutputRoot = vi.fn(async () => "/tmp/selected-export-root");

    render(
      <ConversationExportDialog
        contentCardColors={{
          answer: "#facc15",
          code: "#60a5fa",
          command: "#f59e0b",
          result: "#34d399",
          tool: "#22c55e",
        }}
        exporting={false}
        mode="session"
        onClose={vi.fn()}
        onConfirm={async () => undefined}
        onOutputRootChange={onOutputRootChange}
        onPickOutputRoot={onPickOutputRoot}
        onVisibilityChange={vi.fn()}
        outputRoot="/tmp/conversation-export"
        questionCount={3}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    const pickButton = screen.getByRole("button", { name: "选择导出根目录" });
    expect(pickButton.textContent).toBe("");

    fireEvent.click(pickButton);

    await waitFor(() => {
      expect(onPickOutputRoot).toHaveBeenCalledTimes(1);
      expect(onOutputRootChange).toHaveBeenCalledWith("/tmp/selected-export-root");
    });
  });

  it("renders explicit sync phases and accessible progress", () => {
    const html = renderToStaticMarkup(
      <ConversationSyncProgress
        state={{
          phase: "importing",
          sourceLabel: "全部来源",
        }}
        t={t}
      />,
    );

    expect(html).toContain('role="status"');
    expect(html).toContain('role="progressbar"');
    expect(html).toContain('aria-valuenow="2"');
    expect(html).toContain('aria-valuemax="4"');
    expect(html).toContain("正在读取并导入对话");
    expect(html).toContain("第 2/4 阶段");
    expect(html).toContain("全部来源");
  });

  it("renders web record sync copy without session labels", () => {
    const html = renderToStaticMarkup(
      <ConversationSyncProgress
        recordKind="web"
        state={{
          phase: "importing",
          sourceLabel: "ChatGPT Web",
          summary: "本次新增/更新 3 条网页记录、18 条内容，跳过 7 条未变化记录，覆盖 2 个来源。",
        }}
        t={t}
      />,
    );

    expect(html).toContain("正在读取并导入网页记录");
    expect(html).toContain("网页来源：ChatGPT Web");
    expect(html).toContain("3 条网页记录");
    expect(html).not.toContain("3 个 Session");
    expect(html).toContain("md:grid-cols-[minmax(0,1fr)_auto]");
    expect(html).toContain("break-words");
  });

  it("renders completed sync summary with a dismiss action", () => {
    const html = renderToStaticMarkup(
      <ConversationSyncProgress
        onDismiss={() => undefined}
        state={{
          phase: "completed",
          sourceLabel: "全部来源",
          summary: "本次新增/更新 3 个 Session、18 条内容，跳过 7 个未变化 Session，覆盖 2 个来源。",
        }}
        t={t}
      />,
    );

    expect(html).toContain("同步完成");
    expect(html).toContain("本次新增/更新 3 个 Session、18 条内容");
    expect(html).toContain("关闭同步进度");
  });

  it("collects adapter manifest and source details before importing", async () => {
    const onImport = vi.fn(async () => undefined);
    const onPickManifest = vi.fn(async () => "/tmp/adapter/conversation-adapter.json");
    const onPickSourceLocation = vi.fn(async () => "/tmp/web-records");
    localStorage.setItem("assetiweave.locale", "zh");

    render(
      <I18nProvider>
        <ConversationImportDialog
          onClose={vi.fn()}
          onImport={onImport}
          onPickManifest={onPickManifest}
          onPickSourceLocation={onPickSourceLocation}
          recordKind="web"
        />
      </I18nProvider>,
    );

    expect(screen.getByRole("tab", { name: "导入表单" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "脚本市场榜单" }));
    expect(screen.getByText("需要解析器时可先从 GitHub catalog 安装，安装完成后会自动填入插件 manifest。")).toBeTruthy();
    fireEvent.click(await screen.findByRole("tab", { name: /更新 \(/ }));
    fireEvent.click(screen.getByRole("tab", { name: "导入表单" }));
    fireEvent.click(screen.getByRole("tab", { name: "脚本市场榜单" }));
    expect(screen.getByRole("tab", { name: /更新 \(/ }).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: "导入表单" }));
    fireEvent.click(screen.getByRole("button", { name: "选择插件 manifest" }));
    await waitFor(() => expect(onPickManifest).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: "选择来源目录" }));
    await waitFor(() => expect(onPickSourceLocation).toHaveBeenCalledWith("directory"));
    fireEvent.change(screen.getByLabelText("来源名称"), {
      target: { value: "医保网页记录" },
    });
    fireEvent.click(screen.getByRole("button", { name: "开始导入" }));

    await waitFor(() => {
      expect(onImport).toHaveBeenCalledWith({
        config_json: null,
        manifest_path: "/tmp/adapter/conversation-adapter.json",
        source_kind: "directory",
        source_location: "/tmp/web-records",
        source_name: "医保网页记录",
      });
    });
  });

  it("renders a global background sync indicator without blocking other controls", () => {
    const html = renderToStaticMarkup(
      <ConversationBackgroundTaskIndicator
        task={{
          id: "sync-1",
          status: "running",
          source_id: null,
          adapter_id: null,
          dry_run: false,
          started_at: "2026-06-15T00:00:00Z",
          finished_at: null,
          result: null,
          error: null,
        }}
        t={t}
      />,
    );

    expect(html).toContain('role="status"');
    expect(html).toContain("后台同步对话记录");
    expect(html).toContain("可继续使用其他功能");
    expect(html).not.toContain("disabled");
  });
});

const now = "2026-06-05T00:00:00Z";

const t: Translator = (key, params) => interpolate(messages.zh[key] ?? key, params);

function interpolate(template: string, params?: TranslationParams) {
  if (!params) return template;
  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(params[key] ?? ""));
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

const questionDetail: ConversationQuestionDetail = {
  question: {
    id: "question-1",
    session_id: "session-1",
    question_index: 0,
    title: "同步流程",
    question_text: "AssetIWeave 如何同步对话记录？\n\n继续",
    answer_text: "导入后按问题预览。",
    code_text: "",
    command_text: "assetiweave-cli conversation sync --dry-run",
    grouping_origin: "auto_merged",
    created_at: now,
    updated_at: now,
  },
  turns: [
    {
      id: "turn-1",
      session_id: "session-1",
      external_id: "turn-1",
      turn_index: 0,
      user_text: "AssetIWeave 如何同步对话记录？",
      fingerprint: "turn-1",
      missing: false,
      imported_at: now,
    },
    {
      id: "turn-2",
      session_id: "session-1",
      external_id: "turn-2",
      turn_index: 1,
      user_text: "继续",
      fingerprint: "turn-2",
      missing: false,
      imported_at: now,
    },
  ],
  parts: [
    {
      id: "part-1",
      turn_id: "turn-1",
      part_index: 0,
      role: "assistant",
      kind: "text",
      text: ["# 同步流程", "", "- 按 Session 导入", "- 按用户问题预览"].join("\n"),
      metadata_json: JSON.stringify({
        content_card: { type: "answer", format: "markdown" },
      }),
    },
    {
      id: "part-2",
      turn_id: "turn-2",
      part_index: 0,
      role: "tool",
      kind: "command",
      command: "assetiweave-cli conversation sync --dry-run",
      status: "completed",
      exit_code: 0,
      metadata_json: JSON.stringify({
        content_card: { type: "command" },
      }),
    },
    {
      id: "part-2-result",
      turn_id: "turn-2",
      part_index: 1,
      role: "tool",
      kind: "tool",
      text: "tests passed",
      metadata_json: JSON.stringify({
        content_card: { type: "result", format: "plain", suffix: "result" },
      }),
    },
  ],
};

const richQuestionDetail: ConversationQuestionDetail = {
  ...questionDetail,
  parts: [
    ...questionDetail.parts,
    {
      id: "part-3",
      turn_id: "turn-1",
      part_index: 1,
      role: "assistant",
      kind: "code_block",
      language: "ts",
      text: "const synced = true;",
      metadata_json: JSON.stringify({
        content_card: { type: "code", language: "ts" },
      }),
    },
    {
      id: "part-4",
      turn_id: "turn-1",
      part_index: 2,
      role: "tool",
      kind: "tool",
      text: "Read project files",
      metadata_json: JSON.stringify({
        content_card: { type: "tool" },
      }),
    },
  ],
};

const sessionDetail: ConversationSessionDetail = {
  session: {
    id: "session-1",
    source_id: "codex-live",
    adapter_id: "codex",
    external_id: "external-session-1",
    title: "Conversation fixture",
    project_path: "/Users/util6/code-space/assetiweave",
    missing: false,
    created_at: now,
    imported_at: now,
  },
  questions: [questionDetail],
};

const adapters: ConversationAdapter[] = [
  {
    id: "codex",
    name: "Codex",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/codex/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/codex/adapter.mjs",
    trust_state: "built_in",
    capabilities: [],
    input_kinds: ["live"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "opencode",
    name: "OpenCode",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/opencode/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/opencode/adapter.mjs",
    trust_state: "built_in",
    capabilities: [],
    input_kinds: ["sqlite"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "claude-code",
    name: "Claude Code",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/claude-code/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/claude-code/adapter.mjs",
    trust_state: "built_in",
    capabilities: [],
    input_kinds: ["directory"],
    created_at: now,
    updated_at: now,
  },
];
