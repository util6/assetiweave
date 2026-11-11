import { describe, expect, it } from "vitest";
import type {
  ConversationContentNode,
  ConversationPart,
  ConversationQuestionDetail,
  ConversationSession,
  ConversationTurn,
} from "../../types";
import {
  collectSubagentPortals,
  groupSubagentPortalsByTurn,
  isLegacyPseudoTurn,
  mainConversationTurns,
  parentSessionBreadcrumb,
  subagentPortalsForTurn,
} from "./conversationSubagents";

const now = "2026-11-09T00:00:00Z";
const parentSessionId = `conversation-session-${"a".repeat(64)}`;

describe("subagent dispatch portals", () => {
  it("merges anchor metadata, process chain and node locator without touching child sessions", () => {
    const question = questionWithAnchors();

    const portals = collectSubagentPortals(question);

    expect(portals).toHaveLength(2);
    expect(portals[0]).toMatchObject({
      agentRole: "Explore",
      childSessionId: "child-1",
      locator: `question-1:turn-1:part-anchor-1:0`,
      model: "gemini-2.0-flash",
      partId: "part-anchor-1",
      reportSummary: "定位到三处同步入口",
      status: "completed",
      stepCount: 12,
      title: "梳理同步入口",
      turnId: "turn-1",
    });
    // 第二个锚点只带子会话主键，标题、状态与摘要从过程链补齐。
    expect(portals[1]).toMatchObject({
      childSessionId: "child-2",
      partId: "part-anchor-2",
      reportSummary: "补充了缺失索引",
      status: "running",
      title: "补齐索引",
      turnId: "turn-2",
    });
  });

  it("falls back to the JSON payload carried by legacy anchor parts", () => {
    const question = questionWithAnchors();
    question.parts = [
      anchorPart({
        id: "part-anchor-legacy",
        metadata_json: JSON.stringify({ type: "subagent" }),
        part_index: 0,
        text: JSON.stringify({
          agent_role: "Reviewer",
          child_session_id: "child-legacy",
          status: "success",
          task: "review diff",
        }),
        turn_id: "turn-1",
      }),
    ];
    question.projected_content_nodes = [];

    const [portal] = collectSubagentPortals(question);

    expect(portal).toMatchObject({
      agentRole: "Reviewer",
      childSessionId: "child-legacy",
      partId: "part-anchor-legacy",
      status: "completed",
      title: "review diff",
    });
  });

  it("keeps anchors without a child session as unresolved cards instead of dropping them", () => {
    const question = questionWithAnchors();
    question.parts = question.parts.map((part) => ({
      ...part,
      metadata_json: JSON.stringify({ agent_role: "Explore" }),
    }));
    question.turns = question.turns.map((turn) => ({ ...turn, process: null }));

    const portals = collectSubagentPortals(question);

    expect(portals).toHaveLength(2);
    expect(portals[0]).toMatchObject({
      agentRole: "Explore",
      childSessionId: "",
      partId: "part-anchor-1",
      unresolved: true,
    });
  });

  it("aligns folded anchors by child session id instead of seq", () => {
    // 折算产物：Part 与 ProcessItem 各占一个序号（part@21 / item@22），
    // 且 attrs 只保留子会话溯源，标题与状态要回落到过程链。
    const question = questionWithAnchors();
    question.turns = [
      turn("turn-fold", {
        process: JSON.stringify({
          items: [
            {
              content: "child-fold",
              kind: "subagent",
              label: "折算任务",
              seq: 22,
              status: "running",
              summary: "折算摘要",
            },
          ],
        }),
        turn_index: 0,
      }),
    ];
    question.parts = [
      anchorPart({
        id: "part-fold",
        metadata_json: JSON.stringify({
          child_external_id: "codex:subagent:1",
          child_session_id: "child-fold",
        }),
        part_index: 21,
        text: "[委派任务] folded",
        turn_id: "turn-fold",
      }),
    ];
    question.projected_content_nodes = [];

    const [portal] = collectSubagentPortals(question);

    expect(portal).toMatchObject({
      childSessionId: "child-fold",
      partId: "part-fold",
      reportSummary: "折算摘要",
      status: "running",
      title: "折算任务",
      unresolved: false,
    });
  });

  it("groups portals by parent turn for the left rail", () => {
    const question = questionWithAnchors();

    const grouped = groupSubagentPortalsByTurn(question);

    expect(grouped.get("turn-1")?.map((portal) => portal.partId)).toEqual([
      "part-anchor-1",
    ]);
    expect(grouped.get("turn-2")?.map((portal) => portal.partId)).toEqual([
      "part-anchor-2",
    ]);
    expect(subagentPortalsForTurn(question, "turn-3")).toEqual([]);
  });

  it("keeps one human turn per question and drops pseudo turns", () => {
    const question = questionWithAnchors();
    question.turns = [
      ...question.turns,
      turn("turn-subagent", {
        parent_turn_id: "turn-1",
        turn_index: 3,
        turn_kind: "subagent",
      }),
      turn("turn-continuation", { turn_index: 4, turn_kind: "continuation" }),
    ];

    expect(mainConversationTurns(question).map((item) => item.id)).toEqual([
      "turn-1",
      "turn-2",
    ]);
    expect(isLegacyPseudoTurn({ turn_kind: "subagent" })).toBe(true);
    expect(
      isLegacyPseudoTurn({ parent_turn_id: "turn-1", turn_kind: null }),
    ).toBe(true);
    expect(isLegacyPseudoTurn({ turn_kind: "user" })).toBe(false);
  });
});

describe("parent session breadcrumb", () => {
  it("derives the breadcrumb from the subagent session execution purpose", () => {
    const breadcrumb = parentSessionBreadcrumb(
      subagentSession({
        execution_purpose: JSON.stringify({
          agent_role: "Explore",
          parent_session_id: parentSessionId,
          spawn_anchor: { turn_index: 2 },
        }),
      }),
    );

    expect(breadcrumb).toEqual({
      agentRole: "Explore",
      parentSessionId,
      parentShortId: "aaaaaaaa",
      parentTurnIndex: 2,
      parentTurnNumber: 3,
    });
  });

  it("ignores sessions that are not subagent sessions or miss the parent pointer", () => {
    expect(parentSessionBreadcrumb(null)).toBeNull();
    expect(
      parentSessionBreadcrumb(subagentSession({ execution_origin: "user" })),
    ).toBeNull();
    expect(
      parentSessionBreadcrumb(
        subagentSession({ execution_purpose: JSON.stringify({}) }),
      ),
    ).toBeNull();
    expect(
      parentSessionBreadcrumb(
        subagentSession({ execution_purpose: "subagent" }),
      ),
    ).toBeNull();
  });
});

function questionWithAnchors(): ConversationQuestionDetail {
  const turns = [
    turn("turn-1", {
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
      turn_index: 0,
    }),
    turn("turn-2", {
      process: JSON.stringify({
        items: [
          {
            content: "child-2",
            kind: "subagent",
            label: "补齐索引",
            seq: 0,
            status: "running",
            summary: "补充了缺失索引",
          },
        ],
      }),
      turn_index: 1,
    }),
  ];

  return {
    question: {
      created_at: now,
      id: "question-1",
      session_id: parentSessionId,
      title: "同步链路",
      updated_at: now,
    },
    parts: [
      anchorPart({
        id: "part-anchor-1",
        metadata_json: JSON.stringify({
          agent_role: "Explore",
          child_session_id: "child-1",
          model: "gemini-2.0-flash",
          status: "completed",
          step_count: 12,
          title: "梳理同步入口",
        }),
        part_index: 1,
        text: "请梳理同步入口，不要只列文件……".repeat(50),
        turn_id: "turn-1",
      }),
      anchorPart({
        id: "part-anchor-2",
        metadata_json: JSON.stringify({
          agent_role: "Coder",
          child_session_id: "child-2",
        }),
        part_index: 0,
        text: null,
        turn_id: "turn-2",
      }),
    ],
    projected_content_nodes: [
      contentNode("part-anchor-1", "turn-1"),
      contentNode("part-anchor-2", "turn-2"),
    ],
    question_turns: turns.map((record, turn_order) => ({
      assigned_at: now,
      assignment_origin: "imported" as const,
      question_id: "question-1",
      turn_id: record.id,
      turn_order,
      updated_at: now,
    })),
    turns,
  };
}

function turn(
  id: string,
  overrides: Partial<ConversationTurn> = {},
): ConversationTurn {
  return {
    id,
    imported_at: now,
    prompt: `Prompt for ${id}`,
    session_id: parentSessionId,
    started_at: now,
    turn_index: 0,
    ...overrides,
  };
}

function anchorPart(overrides: Partial<ConversationPart>): ConversationPart {
  return {
    content_card: {
      kind: "claude-code.subagent",
      renderer: "subagent_tree",
      schema_version: 1,
      semantic_role: "subagent",
    },
    id: "part-anchor",
    kind: "subagent",
    part_index: 0,
    role: "assistant",
    turn_id: "turn-1",
    ...overrides,
  };
}

function contentNode(partId: string, turnId: string): ConversationContentNode {
  return {
    command_label: null,
    content: "",
    cwd: null,
    exit_code: null,
    language: null,
    legacy_anchor_ids: [],
    locator: {
      node_order: 0,
      part_id: partId,
      question_id: "question-1",
      turn_id: turnId,
    },
    node_id: partId,
    node_order: 0,
    node_type: "claude-code.subagent",
    part_id: partId,
    part_order: 0,
    question_id: "question-1",
    renderer: "subagent_tree",
    role: "assistant",
    semantic_role: "subagent",
    source_execution_id: null,
    status: null,
    translated_content: null,
    turn_id: turnId,
    turn_order: 0,
  };
}

function subagentSession(
  overrides: Partial<ConversationSession> = {},
): ConversationSession {
  return {
    adapter_id: "claude-code",
    created_at: now,
    execution_origin: "subagent",
    execution_purpose: null,
    external_id: "child-external",
    id: `conversation-session-${"b".repeat(64)}`,
    imported_at: now,
    missing: false,
    source_id: "source-1",
    title: "子代理会话",
    user_visible: false,
    ...overrides,
  };
}
