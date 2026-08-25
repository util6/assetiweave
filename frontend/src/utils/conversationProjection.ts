import type { ConversationContentNode, ConversationQuestionDetail } from "../types";

const CONTENT_PREVIEW_ROLES = new Set(["answer", "result", "tool", "command", "code"]);

export function conversationQuestionTitle(detail: ConversationQuestionDetail): string | null {
  const title = detail.question.title?.trim();
  if (title) return title;

  for (const turn of detail.turns) {
    const firstLine = firstNonEmptyLine(turn.user_text);
    if (firstLine) return firstLine;
  }

  return null;
}

export function conversationQuestionPreview(detail: ConversationQuestionDetail): string | null {
  const node = detail.projected_content_nodes.find((candidate) => {
    const role = candidate.semantic_role ?? contentNodeKindSuffix(candidate.node_type);
    return CONTENT_PREVIEW_ROLES.has(role) && candidate.content.trim();
  }) ?? detail.projected_content_nodes.find((candidate) => candidate.content.trim());

  if (node) return firstNonEmptyLine(node.content);
  for (const turn of detail.turns) {
    const firstLine = firstNonEmptyLine(turn.user_text);
    if (firstLine) return firstLine;
  }
  return null;
}

export function conversationContentNodePresentationKind(node: ConversationContentNode): string {
  const role = node.semantic_role?.trim();
  if (role && CONTENT_PREVIEW_ROLES.has(role)) return role;
  return contentNodeKindSuffix(node.node_type);
}

function contentNodeKindSuffix(kind: string) {
  return kind.includes(".") ? kind.slice(kind.lastIndexOf(".") + 1) : kind;
}

function firstNonEmptyLine(value: string) {
  return value.split(/\r?\n/).find((line) => line.trim())?.trim() ?? null;
}
