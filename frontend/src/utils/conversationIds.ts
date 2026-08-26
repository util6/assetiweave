const conversationHashPattern = /(?:^|[^a-f0-9])([a-f0-9]{64})(?![a-f0-9])/i;
const legacyConversationHashPattern = /(?:^|[^a-f0-9])([a-f0-9]{12,})(?![a-f0-9])/i;

export function conversationIdFragment(value: string) {
  const normalized = value.trim().toLowerCase();
  const hash = normalized.match(conversationHashPattern)?.[1]
    ?? normalized.match(legacyConversationHashPattern)?.[1];
  const fragment = (hash ?? normalized).slice(0, 8);
  const nodeOrder = normalized.match(/-node-(\d+)$/)?.[1];
  return nodeOrder === undefined ? fragment : `${fragment}:n${nodeOrder}`;
}
