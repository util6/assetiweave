import type {
  AgentSessionContentBlock,
  AgentSessionItemView,
  AgentSessionStepGroupStatus,
  AgentSessionTurnView,
} from "../../types/agentSession";

export function calculateStepGroupStatus(
  items: AgentSessionItemView[],
): AgentSessionStepGroupStatus {
  if (items.some((item) => item.state === "failed")) {
    return "failed";
  }
  if (items.some((item) => item.state === "streaming" || item.state === "pending")) {
    return "running";
  }
  if (items.some((item) => item.state === "cancelled")) {
    return "cancelled";
  }
  if (items.length > 0 && items.every((item) => item.state === "succeeded" || item.state === "completed")) {
    return "succeeded";
  }
  return "pending";
}

export function isGroupDefaultExpanded(
  status: AgentSessionStepGroupStatus,
  delivery: "live" | "replay",
): boolean {
  if (status === "failed") {
    return true;
  }
  if (delivery === "live") {
    return true;
  }
  if (delivery === "replay" && status === "succeeded") {
    return false;
  }
  return false;
}

export function buildAgentSessionTurns(
  items: AgentSessionItemView[],
): AgentSessionTurnView[] {
  if (items.length === 0) {
    return [];
  }

  // 1. Group items by turnId while preserving sequence order
  const turnMap = new Map<string, AgentSessionItemView[]>();
  const turnOrder: string[] = [];

  for (const item of items) {
    const turnKey = item.turnId || "turn-default";
    if (!turnMap.has(turnKey)) {
      turnMap.set(turnKey, []);
      turnOrder.push(turnKey);
    }
    turnMap.get(turnKey)!.push(item);
  }

  // 2. Reduce each turn's items into semantic blocks
  return turnOrder.map((turnId) => {
    const turnItems = turnMap.get(turnId)!;
    const blocks: AgentSessionContentBlock[] = [];
    let currentGroupItems: AgentSessionItemView[] = [];
    let groupIndex = 0;

    const flushCurrentGroup = () => {
      if (currentGroupItems.length > 0) {
        blocks.push({
          type: "step_group",
          groupId: `group-${turnId}-${groupIndex}`,
          items: currentGroupItems,
          logicalCount: currentGroupItems.length,
          status: calculateStepGroupStatus(currentGroupItems),
        });
        currentGroupItems = [];
        groupIndex += 1;
      }
    };

    for (const item of turnItems) {
      if (item.kind === "tool") {
        currentGroupItems.push(item);
      } else {
        flushCurrentGroup();
        switch (item.kind) {
          case "user_message":
            blocks.push({ type: "user_request", item });
            break;
          case "assistant_text":
            blocks.push({ type: "assistant_text", item });
            break;
          case "thinking":
            blocks.push({ type: "thinking", item });
            break;
          case "processing":
            blocks.push({ type: "processing", item });
            break;
          case "task":
            blocks.push({ type: "task", item });
            break;
          case "notice":
            blocks.push({ type: "notice", item });
            break;
          case "final_result":
          case "cancelled":
            blocks.push({ type: "terminal", item });
            break;
          case "error":
            blocks.push({ type: "error", item });
            break;
        }
      }
    }

    flushCurrentGroup();

    return {
      turnId,
      blocks,
    };
  });
}
