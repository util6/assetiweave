/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
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
import type {
  ConversationContentNode,
  ConversationPart,
  ConversationQuestionDetail,
} from "../../types";

const now = "2026-08-16T00:00:00Z";
const t: Translator = (key, params?: TranslationParams) =>
  interpolate(messages.zh[key] ?? key, params);

function interpolate(template: string, params?: TranslationParams) {
  if (!params) return template;
  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) =>
    String(params[key] ?? ""),
  );
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
      turn_index: 0,
      prompt: "First question",
      started_at: now,
      imported_at: now,
    },
    {
      id: "turn-2",
      session_id: "session-1",
      turn_index: 1,
      prompt: "Second question",
      started_at: now,
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
      content_card: {
        schema_version: 1,
        kind: "answer",
        renderer: "markdown",
      },
    },
    {
      id: "part-2",
      turn_id: "turn-2",
      part_index: 0,
      role: "tool",
      kind: "tool",
      text: "legacy tool output",
      content_card: {
        schema_version: 1,
        kind: "tool",
        renderer: "plain",
      },
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
    expect(models[0]?.displayNodes?.[0]).toMatchObject({
      type: "card",
      block: { id: "part-1" },
    });
    expect(models[1]?.displayNodes?.[0]).toMatchObject({
      type: "card",
      block: { id: "part-2" },
    });
    expect(
      collectConversationTurnBlocks(models).map((block) => block.id),
    ).toEqual(["part-1", "part-2"]);

    const index = buildConversationBlockTurnIndex(models);
    expect(index.get("turn-1-question")).toBe("turn-1");
    expect(index.get("part-2")).toBe("turn-2");
  });

  it("keeps prompt, split, empty, and content card presentation inside one memoized turn", () => {
    const model: ConversationTurnPresentation =
      buildConversationTurnPresentations(question)[0]!;

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={contentController()}
        index={0}
        model={model}
        recordKind="session"
        t={t}
        visibility={allVisible}
      />,
    );
    expect(html).toContain('data-conversation-turn-id="turn-1"');
    expect(html).toContain("First question");
    expect(html).toContain("First answer");
    expect(html).toContain('data-conversation-card-id="part-1"');
  });

  it("renders the model badge when model is present on the turn", () => {
    const questionWithModel: ConversationQuestionDetail = {
      ...question,
      turns: [
        {
          ...question.turns[0]!,
          model: "claude-3-7-sonnet",
        },
      ],
      question_turns: [question.question_turns[0]!],
    };
    const [model] = buildConversationTurnPresentations(questionWithModel);

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={contentController()}
        index={0}
        model={model!}
        recordKind="session"
        t={t}
        visibility={allVisible}
      />,
    );
    expect(html).toContain("claude-3-7-sonnet");
  });

  it("projects one raw command Part into ephemeral display nodes with one shared Part ID", () => {
    const executionQuestion: ConversationQuestionDetail = {
      ...question,
      turns: [question.turns[0]!],
      question_turns: [question.question_turns[0]!],
      projected_content_nodes: [
        executionNode(
          "part-shell",
          "shell-command",
          "command",
          "printf divider; printf one; printf two",
          0,
        ),
        executionNode("part-result", "shell-result", "result", "one\ntwo", 1),
      ],
    };

    const [model] = buildConversationTurnPresentations(executionQuestion, [
      {
        part_id: "part-shell",
        schema_version: 1,
        projector_version: "shell-projector-v1",
        nodes: [
          { display_order: 0, command: "printf one", command_label: "one" },
          { display_order: 1, command: "printf two", command_label: "two" },
        ],
      },
    ]);
    expect(model?.displayNodes).toHaveLength(1);
    expect(model?.displayNodes?.[0]).toMatchObject({
      type: "execution",
      sourceExecutionId: "shell-execution",
      commands: [
        {
          id: "part-shell::display:0",
          partId: "part-shell",
          commandLabel: "one",
        },
        {
          id: "part-shell::display:1",
          partId: "part-shell",
          commandLabel: "two",
        },
      ],
      results: [{ id: "part-result-node-1" }],
    });
    expect(
      buildConversationBlockTurnIndex([model!]).get("part-shell-node-0"),
    ).toBe("turn-1");
  });
});

describe("ConversationTurn subagent dispatch anchors", () => {
  it("keeps exactly one human turn per question and drops legacy pseudo turns", () => {
    const portalQuestion = questionWithAnchors();
    portalQuestion.turns = [
      ...portalQuestion.turns,
      {
        ...portalQuestion.turns[0]!,
        id: "turn-subagent",
        parent_turn_id: "turn-1",
        prompt: "Continue following subagent...",
        turn_index: 2,
        turn_kind: "subagent",
      },
      {
        ...portalQuestion.turns[0]!,
        id: "turn-continuation",
        prompt: "Continue following subagent execution",
        turn_index: 3,
        turn_kind: "continuation",
      },
    ];

    const models = buildConversationTurnPresentations(portalQuestion);

    expect(models.map((model) => model.turn.id)).toEqual(["turn-1"]);
  });

  it("synthesizes a portal card for anchors that carry no projected node", () => {
    // 真实锚点不挂 content_card，后端不会为它产出投影节点。
    const [model] = buildConversationTurnPresentations(questionWithAnchors());
    const portalNode = model?.displayNodes?.[1];

    expect(portalNode).toMatchObject({
      type: "card",
      block: {
        id: "part-anchor",
        partId: "part-anchor",
        renderer: "subagent_portal",
      },
    });
    if (portalNode?.type !== "card") throw new Error("expected portal card");
    expect(portalNode.block.portal).toMatchObject({
      childSessionId: "child-1",
      reportSummary: "定位到三处同步入口",
      title: "梳理同步入口",
    });
    // 锚点节点不再平铺原始委派 Prompt。
    expect(portalNode.block.text).toBe("");
  });

  it("rewrites an existing anchor node in place without duplicating the card", () => {
    const questionWithNode = questionWithAnchors();
    questionWithNode.projected_content_nodes = [
      projectedNode("part-1", "turn-1", "answer", "First answer"),
      {
        ...projectedNode("part-anchor", "turn-1", "tool", ""),
        node_id: "part-anchor-node",
        node_type: "claude-code.subagent",
        renderer: "subagent_tree",
        semantic_role: "subagent",
      },
    ];

    const [model] = buildConversationTurnPresentations(questionWithNode);

    expect(
      model?.displayNodes?.filter(
        (node) => node.type === "card" && node.block.partId === "part-anchor",
      ),
    ).toHaveLength(1);
    expect(model?.displayNodes?.[1]).toMatchObject({
      type: "card",
      block: { id: "part-anchor-node", renderer: "subagent_portal" },
    });
  });

  it("keeps anchors with no child session as read-only portal cards", () => {
    const legacyQuestion = questionWithAnchors();
    legacyQuestion.parts = legacyQuestion.parts.map((part) =>
      part.id === "part-anchor"
        ? { ...part, metadata_json: null, text: "long delegation prompt" }
        : part,
    );
    legacyQuestion.turns = legacyQuestion.turns.map((turn) => ({
      ...turn,
      process: null,
    }));

    const [model] = buildConversationTurnPresentations(legacyQuestion);
    const node = model?.displayNodes?.find(
      (item) => item.type === "card" && item.block.partId === "part-anchor",
    );

    expect(node).toMatchObject({
      type: "card",
      block: { renderer: "subagent_portal" },
    });
    if (node?.type !== "card") throw new Error("expected portal card");
    expect(node.block.portal).toMatchObject({
      childSessionId: "",
      unresolved: true,
    });
    // 降级卡片同样不回灌原始委派 Prompt。
    expect(node.block.text).toBe("");
  });

  it("opens the child session from the portal card without extra fetches", () => {
    const questionWithPortal = questionWithAnchors();
    const [model] = buildConversationTurnPresentations(questionWithPortal);
    const onOpenSubagent = vi.fn();

    render(
      <ConversationTurn
        controller={contentController()}
        index={0}
        model={model!}
        onOpenSubagent={onOpenSubagent}
        recordKind="session"
        t={t}
        visibility={allVisible}
      />,
    );

    fireEvent.click(
      screen.getByRole("button", { name: "查看子会话：梳理同步入口" }),
    );

    expect(onOpenSubagent).toHaveBeenCalledWith(
      expect.objectContaining({ childSessionId: "child-1" }),
      expect.objectContaining({ id: "turn-1" }),
    );
  });
});

const allVisible = {
  answer: true,
  code: true,
  command: true,
  result: true,
  tool: true,
};

function contentController(): ConversationContentController {
  return {
    cancelTranslation: async () => undefined,
    copyBlock: async () => undefined,
    expandedBlockIds: new Set(),
    getTranslatedText: () => undefined,
    getTranslationPhase: () => undefined,
    isCopied: () => false,
    isTranslating: () => false,
    toggleExpanded: () => undefined,
    translateBlock: async () => undefined,
    translationAvailability: "unavailable",
  };
}

function questionWithAnchors(): ConversationQuestionDetail {
  const turns = [
    {
      ...question.turns[0]!,
      process: JSON.stringify({
        items: [
          {
            content: "child-1",
            kind: "subagent",
            label: "梳理同步入口",
            seq: 1,
            status: "completed",
            summary: "定位到三处同步入口",
          },
        ],
      }),
    },
  ];
  const anchorPart: ConversationPart = {
    id: "part-anchor",
    turn_id: "turn-1",
    part_index: 1,
    role: "assistant",
    kind: "subagent",
    text: "请梳理同步入口，不要只列文件……".repeat(40),
    // 真实锚点只有 metadata_json，不挂 content_card。
    metadata_json: JSON.stringify({
      agent_role: "Explore",
      child_external_id: "child-1-external",
      child_session_id: "child-1",
      status: "completed",
      title: "梳理同步入口",
    }),
  };

  return {
    ...question,
    turns,
    question_turns: [question.question_turns[0]!],
    parts: [question.parts[0]!, anchorPart],
    projected_content_nodes: [
      projectedNode("part-1", "turn-1", "answer", "First answer"),
    ],
  };
}

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
