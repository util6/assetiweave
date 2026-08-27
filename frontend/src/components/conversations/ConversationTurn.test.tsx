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
import type { ConversationContentNode, ConversationQuestionDetail } from "../../types";

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
    title: "Turn preview",
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
  question_turns: [
    {
      question_id: "question-1",
      turn_id: "turn-1",
      turn_order: 0,
      assignment_origin: "auto_merged",
      assigned_at: now,
      updated_at: now,
    },
    {
      question_id: "question-1",
      turn_id: "turn-2",
      turn_order: 1,
      assignment_origin: "auto_merged",
      assigned_at: now,
      updated_at: now,
    },
  ],
  projected_content_nodes: [
    projectedNode("part-1", "turn-1", "answer", "First answer"),
    projectedNode("part-2", "turn-2", "tool", "legacy tool output"),
  ],
};

describe("ConversationTurn", () => {
  it("builds stable per-turn presentations and deep-link indexes", () => {
    const models = buildConversationTurnPresentations(question);
    expect(models).toHaveLength(2);
    expect(models[0]?.promptBlockId).toBe("turn-1-question");
    expect(models[0]?.displayNodes?.[0]).toMatchObject({ type: "card", block: { id: "part-1" } });
    expect(models[1]?.displayNodes?.[0]).toMatchObject({ type: "card", block: { id: "part-2" } });
    expect(collectConversationTurnBlocks(models).map((block) => block.id)).toEqual(["part-1", "part-2"]);

    const index = buildConversationBlockTurnIndex(models);
    expect(index.get("turn-1-question")).toBe("turn-1");
    expect(index.get("part-2")).toBe("turn-2");
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
    expect(html).toContain('data-conversation-card-id="part-1"');
  });

  it("projects one raw command Part into ephemeral display nodes with one shared Part ID", () => {
    const executionQuestion: ConversationQuestionDetail = {
      ...question,
      turns: [question.turns[0]!],
      question_turns: [question.question_turns[0]!],
      projected_content_nodes: [
        executionNode("part-shell", "shell-command", "command", "printf divider; printf one; printf two", 0),
        executionNode("part-result", "shell-result", "result", "one\ntwo", 1),
      ],
    };

    const [model] = buildConversationTurnPresentations(executionQuestion, [{
      part_id: "part-shell",
      schema_version: 1,
      projector_version: "shell-projector-v1",
      nodes: [
        { display_order: 0, command: "printf one", command_label: "one" },
        { display_order: 1, command: "printf two", command_label: "two" },
      ],
    }]);
    expect(model?.displayNodes).toHaveLength(1);
    expect(model?.displayNodes?.[0]).toMatchObject({
      type: "execution",
      sourceExecutionId: "shell-execution",
      commands: [
        { id: "part-shell::display:0", partId: "part-shell", commandLabel: "one" },
        { id: "part-shell::display:1", partId: "part-shell", commandLabel: "two" },
      ],
      results: [{ id: "part-result-node-1" }],
    });
    expect(buildConversationBlockTurnIndex([model!]).get("part-shell-node-0")).toBe("turn-1");
  });
});

function projectedNode(
  partId: string,
  turnId: string,
  semanticRole: "answer" | "tool",
  content: string,
): ConversationContentNode {
  return {
    node_id: partId,
    locator: {
      question_id: "question-1",
      turn_id: turnId,
      part_id: partId,
      node_order: 0,
    },
    question_id: "question-1",
    turn_id: turnId,
    part_id: partId,
    turn_order: turnId === "turn-1" ? 0 : 1,
    part_order: 0,
    node_order: 0,
    node_type: semanticRole,
    semantic_role: semanticRole,
    renderer: semanticRole === "answer" ? "markdown" : "plain",
    role: semanticRole === "answer" ? "assistant" : "tool",
    content,
    language: null,
    cwd: null,
    status: null,
    exit_code: null,
    source_execution_id: null,
    command_label: null,
    translated_content: null,
    legacy_anchor_ids: [],
  };
}

function executionNode(
  partId: string,
  _label: string,
  semanticRole: "command" | "result",
  content: string,
  nodeOrder: number,
): ConversationContentNode {
  return {
    node_id: `${partId}-node-${nodeOrder}`,
    locator: {
      question_id: "question-1",
      turn_id: "turn-1",
      part_id: partId,
      node_order: nodeOrder,
    },
    question_id: "question-1",
    turn_id: "turn-1",
    part_id: partId,
    turn_order: 0,
    part_order: 0,
    node_order: nodeOrder,
    node_type: semanticRole,
    semantic_role: semanticRole,
    renderer: semanticRole === "command" ? "command" : "terminal_output",
    role: "tool",
    content,
    language: null,
    cwd: null,
    status: null,
    exit_code: semanticRole === "result" ? 0 : null,
    source_execution_id: "shell-execution",
    command_label: null,
    translated_content: null,
    legacy_anchor_ids: [],
  };
}
