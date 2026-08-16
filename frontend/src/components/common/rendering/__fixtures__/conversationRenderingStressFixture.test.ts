import { describe, expect, it } from "vitest";
import { createConversationRenderingStressFixture } from "./conversationRenderingStressFixture";

describe("createConversationRenderingStressFixture", () => {
  it("creates 80 stable turns with the required complex content mix", () => {
    const fixture = createConversationRenderingStressFixture();
    const cards = fixture.cards ?? [];

    expect(fixture.turns).toHaveLength(80);
    expect(fixture.turns.map((turn) => turn.id)).toEqual(
      Array.from({ length: 80 }, (_, index) => `stress-turn-${String(index + 1).padStart(2, "0")}`),
    );
    expect(new Set(fixture.turns.map((turn) => turn.id)).size).toBe(80);
    expect(cards.filter((card) => card.renderer === "code")).toHaveLength(24);
    expect(cards.filter((card) => card.renderer === "command")).toHaveLength(24);
    expect(cards.filter((card) => card.kind === "fixture.result")).toHaveLength(32);
    expect(cards.filter((card) => card.renderer === "diff")).toHaveLength(12);
    expect(cards.filter((card) => card.body.includes("| 指标 | 值 |"))).toHaveLength(8);
    expect(cards.filter((card) => card.body.includes("```mermaid"))).toHaveLength(4);
    expect(cards.filter((card) => card.body.includes("$$"))).toHaveLength(8);
    expect(cards.filter((card) => card.kind === "fixture.answer")).toHaveLength(80);
  });

  it("is deterministic and keeps every card connected to a turn node", () => {
    const first = createConversationRenderingStressFixture();
    const second = createConversationRenderingStressFixture();

    expect(second).toEqual(first);
    expect(first.content_nodes).toHaveLength(first.cards?.length ?? 0);
    expect(first.content_nodes?.every((node) => (
      first.turns.some((turn) => turn.id === node.turn_id)
    ))).toBe(true);
  });
});
