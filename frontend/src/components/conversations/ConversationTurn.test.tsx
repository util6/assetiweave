/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  buildConversationBlockTurnIndex,
  buildConversationTurnPresentations,
  collectConversationTurnBlocks,
  conversationTurnAncestorIds,
  ConversationTurn,
  type ConversationTurnPresentation,
  visibleConversationTurnPresentations,
} from "./ConversationTurn";
import type { Translator } from "../../i18n/I18nProvider";
import { messages, type TranslationParams } from "../../i18n/messages";
import type { ConversationContentController } from "./useConversationContentController";
import type {
  ConversationContentNode,
  ConversationQuestionDetail,
  ConversationTurn as ConversationTurnRecord,
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
      external_id: "turn-1",
      turn_index: 0,
      prompt: "First question",
      fingerprint: "turn-1",
      missing: false,
      imported_at: now,
    },
    {
      id: "turn-2",
      session_id: "session-1",
      external_id: "turn-2",
      turn_index: 1,
      prompt: "Second question",
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
    const controller: ConversationContentController = {
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

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={controller}
        index={0}
        model={model}
        recordKind="session"
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
    const controller: ConversationContentController = {
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

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={controller}
        index={0}
        model={model!}
        recordKind="session"
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
    expect(html).toContain("claude-3-7-sonnet");
  });

  it("renders a specialized subagent turn with subagent task header, agent role badge, and model badge", () => {
    const subagentTurnQuestion: ConversationQuestionDetail = {
      ...question,
      turns: [
        {
          ...question.turns[0]!,
          turn_kind: "subagent",
          agent_role: "Code Reviewer",
          model: "gemini-1.5-pro",
          prompt: "Please review the diff between branch A and branch B",
        },
      ],
      projected_content_nodes: [
        projectedNode(
          "part-ans-1",
          "turn-1",
          "answer",
          "Review complete. All good.",
        ),
      ],
    };

    const [model] = buildConversationTurnPresentations(subagentTurnQuestion);
    const controller: ConversationContentController = {
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
    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={controller}
        index={0}
        model={model!}
        recordKind="session"
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
    expect(html).toContain("子代理任务");
    expect(html).toContain("Code Reviewer");
    expect(html).toContain("gemini-1.5-pro");
    expect(html).toContain("Please review the diff");
    expect(html).toContain("复制子代理提示词");
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

describe("ConversationTurn subagent tree", () => {
  it("nests turns by parent_turn_id with depth and direct child counts", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());

    expect(models.map((model) => model.turn.id)).toEqual([
      "turn-root",
      "turn-child",
      "turn-grandchild",
      "turn-great-grandchild",
      "turn-orphan",
    ]);
    const byId = new Map(models.map((model) => [model.turn.id, model]));
    expect(byId.get("turn-root")).toMatchObject({
      childCount: 1,
      depth: 0,
      parentTurnId: null,
    });
    expect(byId.get("turn-child")).toMatchObject({
      childCount: 1,
      depth: 1,
      isSubagent: true,
      parentTurnId: "turn-root",
    });
    expect(byId.get("turn-grandchild")).toMatchObject({
      childCount: 1,
      depth: 2,
      parentTurnId: "turn-child",
    });
    expect(byId.get("turn-great-grandchild")).toMatchObject({
      childCount: 0,
      depth: 3,
      parentTurnId: "turn-grandchild",
    });
  });

  it("degrades orphans of an out-of-question parent to flat roots", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());
    const orphan = models.find((model) => model.turn.id === "turn-orphan");

    expect(orphan).toMatchObject({ depth: 0, parentTurnId: null });
    expect(
      visibleConversationTurnPresentations(models, new Set()).map(
        (model) => model.turn.id,
      ),
    ).toContain("turn-orphan");
  });

  it("keeps mutually-parented turns from vanishing or looping", () => {
    const question = subagentTreeQuestion();
    const cyclic: ConversationQuestionDetail = {
      ...question,
      turns: [
        conversationTurnRecord("turn-a", {
          parent_turn_id: "turn-b",
          turn_kind: "subagent",
        }),
        conversationTurnRecord("turn-b", {
          parent_turn_id: "turn-a",
          turn_kind: "subagent",
        }),
      ],
      question_turns: [],
      projected_content_nodes: [],
    };

    const models = buildConversationTurnPresentations(cyclic);

    expect(models.map((model) => model.turn.id).sort()).toEqual([
      "turn-a",
      "turn-b",
    ]);
    expect(models.every((model) => model.depth === 0)).toBe(true);
    expect(models.every((model) => model.parentTurnId === null)).toBe(true);
  });

  it("collapses subagent branches by default and reveals them one level at a time", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());

    expect(
      visibleConversationTurnPresentations(models, new Set()).map(
        (model) => model.turn.id,
      ),
    ).toEqual(["turn-root", "turn-child", "turn-orphan"]);
    expect(
      visibleConversationTurnPresentations(models, new Set(["turn-child"])).map(
        (model) => model.turn.id,
      ),
    ).toEqual(["turn-root", "turn-child", "turn-grandchild", "turn-orphan"]);
    expect(
      visibleConversationTurnPresentations(
        models,
        new Set(["turn-child", "turn-grandchild", "turn-great-grandchild"]),
      ).map((model) => model.turn.id),
    ).toEqual([
      "turn-root",
      "turn-child",
      "turn-grandchild",
      "turn-great-grandchild",
      "turn-orphan",
    ]);
  });

  it("resolves the ancestor chain of a deep turn for deep-link expansion", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());

    expect(
      conversationTurnAncestorIds(models, "turn-great-grandchild"),
    ).toEqual(["turn-grandchild", "turn-child", "turn-root"]);
    expect(conversationTurnAncestorIds(models, "turn-root")).toEqual([]);
  });

  it("renders a collapsed subagent header with role, type, model, child count, and status", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());
    const child = models.find((model) => model.turn.id === "turn-child")!;

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={contentController()}
        expanded={false}
        index={0}
        model={child}
        recordKind="session"
        t={t}
        visibility={allVisible}
      />,
    );

    expect(html).toContain('data-conversation-turn-depth="1"');
    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("Teamwork Orchestrator");
    expect(html).toContain("teamwork_preview_orchestrator");
    expect(html).toContain("gemini-1.5-pro");
    expect(html).toContain("1 个子代理");
    expect(html).toContain("进行中");
    expect(html).toContain("Coordinate the rollout plan");
    expect(html).not.toContain("Nested worker answer");
  });

  it("renders the reused content cards once the subagent is expanded", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());
    const child = models.find((model) => model.turn.id === "turn-child")!;

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={contentController()}
        expanded
        index={0}
        model={child}
        recordKind="session"
        t={t}
        visibility={allVisible}
      />,
    );

    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain("Nested worker answer");
    expect(html).toContain('data-conversation-card-id="part-child"');
  });

  it("shows the explicit empty record notice for subagents without parts", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());
    const grandchild = models.find(
      (model) => model.turn.id === "turn-grandchild",
    )!;

    const html = renderToStaticMarkup(
      <ConversationTurn
        controller={contentController()}
        expanded
        index={0}
        model={grandchild}
        recordKind="session"
        t={t}
        visibility={allVisible}
      />,
    );

    expect(html).toContain("无执行记录");
    expect(html).not.toContain("conversation-empty-state");
  });

  it("toggles a subagent branch through the collapse control", () => {
    const models = buildConversationTurnPresentations(subagentTreeQuestion());
    const child = models.find((model) => model.turn.id === "turn-child")!;
    const onToggleExpanded = vi.fn();

    render(
      <ConversationTurn
        controller={contentController()}
        expanded={false}
        index={0}
        model={child}
        onToggleExpanded={onToggleExpanded}
        recordKind="session"
        t={t}
        visibility={allVisible}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "展开子代理" }));

    expect(onToggleExpanded).toHaveBeenCalledWith("turn-child");
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

function conversationTurnRecord(
  id: string,
  overrides: Partial<ConversationTurnRecord> = {},
): ConversationTurnRecord {
  return {
    id,
    session_id: "session-1",
    external_id: id,
    turn_index: 0,
    prompt: `Prompt for ${id}`,
    fingerprint: id,
    missing: false,
    imported_at: now,
    ...overrides,
  };
}

function subagentTreeQuestion(): ConversationQuestionDetail {
  const turns: ConversationTurnRecord[] = [
    conversationTurnRecord("turn-root", {
      prompt: "Ship the feature",
      turn_index: 0,
    }),
    conversationTurnRecord("turn-child", {
      agent_role: "Teamwork Orchestrator",
      ended_at: null,
      model: "gemini-1.5-pro",
      parent_turn_id: "turn-root",
      prompt: "Coordinate the rollout plan",
      started_at: now,
      title: "teamwork_preview_orchestrator",
      turn_index: 1,
      turn_kind: "subagent",
    }),
    conversationTurnRecord("turn-grandchild", {
      agent_role: "Project Orchestrator",
      ended_at: now,
      parent_turn_id: "turn-child",
      prompt: "Break down the rollout",
      started_at: now,
      turn_index: 2,
      turn_kind: "subagent",
    }),
    conversationTurnRecord("turn-great-grandchild", {
      agent_role: "Rust Core Reviewer 2",
      ended_at: now,
      parent_turn_id: "turn-grandchild",
      prompt: "Review the patch",
      started_at: now,
      turn_index: 3,
      turn_kind: "subagent",
    }),
    conversationTurnRecord("turn-orphan", {
      agent_role: "Auditor",
      ended_at: now,
      parent_turn_id: "turn-missing",
      prompt: "Audit leftovers",
      started_at: now,
      turn_index: 4,
      turn_kind: "subagent",
    }),
  ];

  return {
    question: {
      id: "question-tree",
      session_id: "session-1",
      title: "Subagent tree",
      created_at: now,
      updated_at: now,
    },
    turns,
    parts: [],
    question_turns: turns.map((turn, turn_order) => ({
      question_id: "question-tree",
      turn_id: turn.id,
      turn_order,
      assignment_origin: "auto_merged",
      assigned_at: now,
      updated_at: now,
    })),
    projected_content_nodes: [
      projectedNode("part-root", "turn-root", "answer", "Root answer"),
      projectedNode(
        "part-child",
        "turn-child",
        "answer",
        "Nested worker answer",
      ),
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
