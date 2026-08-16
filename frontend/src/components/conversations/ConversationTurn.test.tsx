/* @vitest-environment jsdom */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  buildConversationBlockTurnIndex,
  buildConversationTurnPresentations,
  collectConversationTurnBlocks,
  ConversationTurn,
  type ConversationTurnPresentation,
} from "./ConversationTurn";
import type { Translator } from "../../i18n/I18nProvider";
import { messages, type TranslationParams } from "../../i18n/messages";
import type { ConversationContentController } from "./useConversationContentController";
import type { ConversationQuestionDetail } from "../../types";

const now = "2026-08-16T00:00:00Z";
const t: Translator = (key, params?: TranslationParams) => interpolate(messages.zh[key] ?? key, params);

function interpolate(template: string, params?: TranslationParams) {
  if (!params) return template;
  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(params[key] ?? ""));
}

const question: ConversationQuestionDetail = {
  question: {
    id: "question-1",
    session_id: "session-1",
    question_index: 0,
    title: "Turn preview",
    question_text: "First question",
    answer_text: "Answer",
    code_text: "",
    command_text: "",
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
      user_text: "First question",
      fingerprint: "turn-1",
      missing: false,
      imported_at: now,
    },
    {
      id: "turn-2",
      session_id: "session-1",
      external_id: "turn-2",
      turn_index: 1,
      user_text: "Second question",
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
      text: "First answer",
      metadata_json: JSON.stringify({ content_card: { type: "answer" } }),
    },
    {
      id: "part-2",
      turn_id: "turn-2",
      part_index: 0,
      role: "tool",
      kind: "tool",
      text: "legacy tool output",
      metadata_json: JSON.stringify({ content_card: { type: "tool" } }),
    },
  ],
};

describe("ConversationTurn", () => {
  it("builds stable per-turn presentations and deep-link indexes", () => {
    const models = buildConversationTurnPresentations(question);
    expect(models).toHaveLength(2);
    expect(models[0]?.promptBlockId).toBe("turn-1-question");
    expect(models[0]?.blocks[0]?.id).toBe("part-1-answer");
    expect(models[1]?.blocks[0]?.id).toBe("part-2-tool");
    expect(collectConversationTurnBlocks(models).map((block) => block.id)).toEqual(["part-1-answer", "part-2-tool"]);

    const index = buildConversationBlockTurnIndex(models);
    expect(index.get("turn-1-question")).toBe("turn-1");
    expect(index.get("part-2-tool")).toBe("turn-2");
  });

  it("keeps prompt, split, empty, and content card presentation inside one memoized turn", () => {
    const model: ConversationTurnPresentation = buildConversationTurnPresentations(question)[0]!;
    const controller: ConversationContentController = {
      cancelTranslation: async () => undefined,
      copyBlock: async () => undefined,
      expandedBlockIds: new Set(),
      getTranslatedText: () => undefined,
      getTranslationError: () => undefined,
      getTranslationPhase: () => undefined,
      isCopied: () => false,
      isTranslating: () => false,
      toggleExpanded: () => undefined,
      translateBlock: async () => undefined,
      translationAvailability: "unavailable",
    };

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={controller}
        index={0}
        model={model}
        onSplit={() => undefined}
        recordKind="session"
        t={t}
        visibility={{ answer: true, code: true, command: true, result: true, tool: true }}
      />,
    );
    expect(html).toContain('data-conversation-turn-id="turn-1"');
    expect(html).toContain("First question");
    expect(html).toContain("First answer");
    expect(html).toContain('data-conversation-card-id="part-1-answer"');
  });
});
