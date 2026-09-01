import { describe, expect, it } from "vitest";
import { memoryRecallSessionSchema } from "./memory";

const session = {
  id: "recall-1",
  status: "active",
  scope: { app_id: null, source_id: null, project_path: null, session_id: null },
  executionContextKey: "memory-recall:recall-1",
  agentId: "opencode",
  model: null,
  turnCount: 1,
  activeTurnId: null,
  lastError: null,
  createdAt: "2026-09-01T00:00:00Z",
  updatedAt: "2026-09-01T00:00:00Z",
  turns: [{
    id: "turn-1",
    sessionId: "recall-1",
    sequence: 0,
    conversationSessionId: "conversation-1",
    conversationTurnId: "conversation-turn-1",
    status: "completed",
    userText: "Why?",
    structuredOutput: {
      answer: "Because the shared service is the workflow boundary.",
      sessionReferences: [{ recordKind: "session", sessionId: "source-session", questionId: "question-1" }],
      contentReferences: [{ recordKind: "session", sessionId: "source-session", questionId: "question-1", turnId: "turn-1", partId: null, blockId: "block-1" }],
      followUpSuggestions: ["Show the implementation."],
    },
    lastError: null,
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-01T00:00:00Z",
  }],
} as const;

describe("Memory public schemas", () => {
  it("parses a persistent structured Recall session", () => {
    expect(memoryRecallSessionSchema.parse(session).turns[0].structuredOutput?.answer).toContain("shared service");
  });

  it("rejects unknown structured output fields", () => {
    expect(() => memoryRecallSessionSchema.parse({
      ...session,
      turns: [{ ...session.turns[0], structuredOutput: { ...session.turns[0].structuredOutput, hidden: true } }],
    })).toThrow();
  });
});
