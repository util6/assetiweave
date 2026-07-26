export interface ConversationNavigationTarget {
  blockId?: string;
  nonce: string;
  questionId?: string;
  recordKind: "session" | "web";
  sessionId: string;
}

let conversationNavigationSequence = 0;

export function createConversationNavigationTarget(
  target: Omit<ConversationNavigationTarget, "nonce">,
): ConversationNavigationTarget {
  conversationNavigationSequence += 1;
  return {
    ...target,
    nonce: `${Date.now().toString(36)}-${conversationNavigationSequence.toString(36)}`,
  };
}

export function conversationSubNavId(recordKind: ConversationNavigationTarget["recordKind"]) {
  return recordKind === "web" ? "web-records" : "sessions";
}
