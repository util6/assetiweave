const conversationHashPattern = /(?:^|[^a-f0-9])([a-f0-9]{64})(?![a-f0-9])/i;
const legacyConversationHashPattern = /(?:^|[^a-f0-9])([a-f0-9]{12,})(?![a-f0-9])/i;

export function conversationIdFragment(value: string) {
  const normalized = value.trim().toLowerCase();
  const hash = normalized.match(conversationHashPattern)?.[1]
    ?? normalized.match(legacyConversationHashPattern)?.[1];
  return (hash ?? normalized).slice(0, 8);
}
