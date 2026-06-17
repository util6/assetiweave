/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
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
import type { Translator } from "../../i18n/I18nProvider";
import { messages, type TranslationParams } from "../../i18n/messages";
import type {
  ConversationAdapter,
  ConversationQuestionDetail,
  ConversationSessionDetail,
} from "../../types";
import {
  AppSessionBrowser,
  groupConversationSessionsByApp,
  ConversationExportDialog,
  loadAllConversationSessionPages,
  MarkdownContent,
  QuestionPreview,
  SessionQuestionWorkspace,
} from "./ConversationsPage";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
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
        onSessionOpen={vi.fn()}
        selectedAppId="codex"
        t={t}
      />,
    );

    expect(html).toContain("水平浏览分栏");
    expect(html).toContain('role="scrollbar"');
    expect(html).toContain("sticky bottom-0");
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
    expect(screen.getByRole("button", { name: "已复制" })).toBeTruthy();
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
              "Chunk ID: 5b951a Wall time: 0.0000 seconds Process exited with code 0 Output:",
              "./specs/design.md:69:- App 快捷入口支持真实应用图标",
              "./cli/internal/errlint/legacy_exit_test.go:23: got := summarizeBySymbol(violations)",
              "./src-tauri/src/path_utils.rs:166: &[\"symbolic-ref\", \"--short\"]",
            ].join(" "),
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
    for (const label of ["回答文字", "工具调用", "命令执行", "代码", "执行结果"]) {
      expect(html).toContain(label);
    }
    expect(html).toContain("确认导出");
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
    },
    {
      id: "part-2",
      turn_id: "turn-2",
      part_index: 0,
      role: "tool",
      kind: "command",
      text: "tests passed",
      command: "assetiweave-cli conversation sync --dry-run",
      status: "completed",
      exit_code: 0,
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
    },
    {
      id: "part-4",
      turn_id: "turn-1",
      part_index: 2,
      role: "tool",
      kind: "tool",
      text: "Read project files",
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
    kind: "codex",
    version: "1",
    enabled: true,
    trust_state: "built_in",
    capabilities: [],
    input_kinds: ["live"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "opencode",
    name: "OpenCode",
    kind: "opencode",
    version: "1",
    enabled: true,
    trust_state: "built_in",
    capabilities: [],
    input_kinds: ["sqlite"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "claude-code",
    name: "Claude Code",
    kind: "claude_code",
    version: "1",
    enabled: true,
    trust_state: "built_in",
    capabilities: [],
    input_kinds: ["directory"],
    created_at: now,
    updated_at: now,
  },
];
