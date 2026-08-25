import type {
  ConversationContentNode,
  ConversationQuestionDetail,
  ConversationTurn,
} from "../../../../types";

const FIXTURE_TIMESTAMP = "2026-08-16T00:00:00.000Z";
const TURN_COUNT = 80;

export interface RenderingStressFixture extends ConversationQuestionDetail {
  projected_content_nodes: ConversationContentNode[];
}

export function createConversationRenderingStressFixture(): RenderingStressFixture {
  const turns = Array.from({ length: TURN_COUNT }, (_, index) => createTurn(index));
  const projectedContentNodes: ConversationContentNode[] = [];

  const appendNode = (turnId: string, node: Omit<ConversationContentNode, "node_id" | "locator" | "question_id" | "turn_id" | "part_id" | "turn_order" | "node_order" | "legacy_anchor_ids">) => {
    const nodeIndex = projectedContentNodes.length;
    const partId = `stress-part-${String(nodeIndex + 1).padStart(3, "0")}`;
    const nodeId = `${partId}-node-0`;
    const turn = turns.find((candidate) => candidate.id === turnId)!;
    projectedContentNodes.push({
      ...node,
      node_id: nodeId,
      locator: {
        question_id: "rendering-stress-question",
        turn_id: turnId,
        part_id: partId,
        node_order: 0,
      },
      question_id: "rendering-stress-question",
      turn_id: turnId,
      part_id: partId,
      node_order: 0,
      turn_order: turn.turn_index,
      legacy_anchor_ids: [`${nodeId}-legacy`],
    });
  };

  turns.forEach((turn, index) => {
    appendNode(turn.id, {
      node_type: "fixture.answer",
      semantic_role: "answer",
      renderer: "markdown",
      role: "assistant",
      content: createMarkdownAnswer(index),
      part_order: 0,
      language: null,
      cwd: null,
      status: null,
      exit_code: null,
      source_execution_id: null,
      command_label: null,
      translated_content: null,
    });

    if (index < 24) {
      appendNode(turn.id, {
        node_type: "fixture.code",
        semantic_role: "code",
        language: "ts",
        renderer: "code",
        role: "assistant",
        content: ["const renderTurn = () => {", `  return \"turn ${index + 1}\";`, "};"].join("\n"),
        part_order: 1,
        cwd: null,
        status: null,
        exit_code: null,
        source_execution_id: null,
        command_label: null,
        translated_content: null,
      });
      appendNode(turn.id, {
        node_type: "fixture.command",
        semantic_role: "command",
        renderer: "command",
        role: "tool",
        content: `assetiweave-cli conversation render --turn ${index + 1}`,
        part_order: 2,
        language: null,
        cwd: null,
        status: null,
        exit_code: null,
        source_execution_id: null,
        command_label: `render-${index + 1}`,
        translated_content: null,
      });
    }

    if (index < 32) {
      appendNode(turn.id, {
        node_type: "fixture.result",
        semantic_role: "result",
        renderer: index < 12 ? "diff" : "terminal_output",
        role: "tool",
        content: index < 12 ? createDiff(index) : createResult(index),
        part_order: 3,
        language: null,
        cwd: null,
        status: "completed",
        exit_code: null,
        source_execution_id: null,
        command_label: null,
        translated_content: null,
      });
    }
  });

  return {
    parts: [],
    question: {
      created_at: FIXTURE_TIMESTAMP,
      id: "rendering-stress-question",
      session_id: "rendering-stress-session",
      title: "Rendering stress fixture",
      updated_at: FIXTURE_TIMESTAMP,
    },
    question_turns: turns.map((turn, turnOrder) => ({
      question_id: "rendering-stress-question",
      turn_id: turn.id,
      turn_order: turnOrder,
      assignment_origin: "imported",
      assigned_at: FIXTURE_TIMESTAMP,
      updated_at: FIXTURE_TIMESTAMP,
    })),
    turns,
    projected_content_nodes: projectedContentNodes,
  };
}

export async function translateStressFixtureBlock(text: string) {
  return { translated_text: `[mock translation] ${text}` };
}

function createTurn(index: number): ConversationTurn {
  const number = index + 1;
  const size = index % 3;
  const userText = size === 0
    ? `Short rendering prompt ${number}.`
    : size === 1
      ? `Regular rendering prompt ${number} with enough detail to form a normal preview block.`
      : Array.from(
        { length: 12 },
        (_, paragraph) => `Long rendering prompt ${number}, paragraph ${paragraph + 1}, exercises an unusually large Turn.`,
      ).join("\n\n");

  return {
    external_id: `stress-external-turn-${String(number).padStart(2, "0")}`,
    fingerprint: `stress-fingerprint-${String(number).padStart(2, "0")}`,
    id: `stress-turn-${String(number).padStart(2, "0")}`,
    imported_at: FIXTURE_TIMESTAMP,
    missing: false,
    session_id: "rendering-stress-session",
    turn_index: index,
    user_text: userText,
  };
}

function createMarkdownAnswer(index: number) {
  const paragraphs = Array.from(
    { length: 8 },
    (_, paragraph) => `Turn ${index + 1} markdown paragraph ${paragraph + 1} is deliberately deterministic for scroll rendering.`,
  );

  if (index < 8) {
    paragraphs.push("| 指标 | 值 |\n| --- | --- |\n| 挂载 Turn | 受控 |");
  }
  if (index < 4) {
    paragraphs.push("```mermaid\nflowchart LR\n  A[Scroll] --> B[Skeleton]\n```");
  }
  if (index < 8) {
    paragraphs.push("$$\\sum_{n=1}^{8} n = 36$$");
  }

  return paragraphs.join("\n\n");
}

function createDiff(index: number) {
  return [
    "diff --git a/frontend/src/preview.tsx b/frontend/src/preview.tsx",
    "--- a/frontend/src/preview.tsx",
    "+++ b/frontend/src/preview.tsx",
    `@@ -${index + 1},2 +${index + 1},2 @@`,
    "-return <OldPreview />;",
    "+return <RenderSafePreview />;",
  ].join("\n");
}

function createResult(index: number) {
  return Array.from(
    { length: 12 },
    (_, line) => `rendering result ${index + 1}, output line ${line + 1}`,
  ).join("\n");
}
