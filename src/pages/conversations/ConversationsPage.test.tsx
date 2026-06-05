import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { Translator } from "../../i18n/I18nProvider";
import { messages, type TranslationParams } from "../../i18n/messages";
import type { ConversationQuestionDetail, ConversationSessionDetail } from "../../types";
import { MarkdownContent, QuestionPreview } from "./ConversationsPage";

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
      text: "",
      command: "assetiweave-cli conversation sync --dry-run",
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
