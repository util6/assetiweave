import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  ConversationContentCards,
  buildConversationContentBlocks,
  type ConversationContentVisibility,
} from "../../components/conversations/ConversationContentCards";
import {
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
  groupConversationSessionsByApp,
  MarkdownContent,
  QuestionPreview,
} from "./ConversationsPage";

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
