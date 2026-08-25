import type {
  ConversationCard,
  LegacyConversationContentNode,
  ConversationQuestionDetail,
  ConversationTurn,
} from "../../../../types";

const FIXTURE_TIMESTAMP = "2026-08-16T00:00:00.000Z";
const TURN_COUNT = 80;

export interface RenderingStressFixture extends ConversationQuestionDetail {
  cards: ConversationCard[];
  content_nodes: LegacyConversationContentNode[];
}

export function createConversationRenderingStressFixture(): RenderingStressFixture {
  const turns = Array.from({ length: TURN_COUNT }, (_, index) => createTurn(index));
  const cards: ConversationCard[] = [];
  const contentNodes: LegacyConversationContentNode[] = [];

  const appendCard = (turnId: string, card: Omit<ConversationCard, "card_id" | "part_id" | "legacy_anchor_ids">) => {
    const cardIndex = cards.length;
    const cardId = `stress-card-${String(cardIndex + 1).padStart(3, "0")}`;
    cards.push({
      ...card,
      card_id: cardId,
      part_id: `stress-part-${String(cardIndex + 1).padStart(3, "0")}`,
      legacy_anchor_ids: [`${cardId}-legacy`],
    });
    contentNodes.push({ card_index: cardIndex, turn_id: turnId, type: "card" });
  };

  turns.forEach((turn, index) => {
    appendCard(turn.id, {
      adapter_id: "fixture",
      body: createMarkdownAnswer(index),
      kind: "fixture.answer",
      renderer: "markdown",
      role: "assistant",
      semantic_role: "answer",
    });

    if (index < 24) {
      appendCard(turn.id, {
        adapter_id: "fixture",
        body: ["const renderTurn = () => {", `  return \"turn ${index + 1}\";`, "};"].join("\n"),
        kind: "fixture.code",
        language: "ts",
        renderer: "code",
        role: "assistant",
        semantic_role: "code",
      });
      appendCard(turn.id, {
        adapter_id: "fixture",
        body: `assetiweave-cli conversation render --turn ${index + 1}`,
        command_label: `render-${index + 1}`,
        kind: "fixture.command",
        renderer: "command",
        role: "tool",
        semantic_role: "command",
      });
    }

    if (index < 32) {
      appendCard(turn.id, {
        adapter_id: "fixture",
        body: index < 12 ? createDiff(index) : createResult(index),
        kind: "fixture.result",
        renderer: index < 12 ? "diff" : "terminal_output",
        role: "tool",
        semantic_role: "result",
        status: "completed",
      });
    }
  });

  return {
    cards,
    content_nodes: contentNodes,
    parts: [],
    question: {
      answer_text: "Deterministic rendering stress fixture",
      code_text: "",
      command_text: "",
      created_at: FIXTURE_TIMESTAMP,
      grouping_origin: "imported",
      id: "rendering-stress-question",
      question_index: 0,
      question_text: "Render the deterministic conversation stress fixture.",
      session_id: "rendering-stress-session",
      title: "Rendering stress fixture",
      updated_at: FIXTURE_TIMESTAMP,
    },
    turns,
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
