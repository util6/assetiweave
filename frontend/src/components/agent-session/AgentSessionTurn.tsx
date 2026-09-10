import type { ReactNode } from "react";
import type {
  AgentSessionTurnView,
} from "../../types/agentSession";
import { isGroupDefaultExpanded } from "./agentSessionReducer";
import { AgentSessionAssistantText } from "./AgentSessionAssistantText";
import { AgentSessionItem } from "./AgentSessionItem";
import { AgentSessionStepGroup } from "./AgentSessionStepGroup";
import { AgentSessionThinking } from "./AgentSessionThinking";
import { AgentSessionUserRequest } from "./AgentSessionUserRequest";
import type { useAgentSessionExpansion } from "./useAgentSessionExpansion";

export interface AgentSessionTurnProps {
  turn: AgentSessionTurnView;
  expansion: ReturnType<typeof useAgentSessionExpansion>;
  testIdPrefix?: string;
}

export function AgentSessionTurn({
  expansion,
  testIdPrefix = "agent-session",
  turn,
}: AgentSessionTurnProps) {
  return (
    <div
      className="contents"
      data-testid={`${testIdPrefix}-turn-${turn.turnId}`}
    >
      {turn.blocks.map((block, index) => {
        let content: ReactNode = null;
        switch (block.type) {
          case "user_request":
            content = (
              <AgentSessionUserRequest
                item={block.item}
                testIdPrefix={testIdPrefix}
              />
            );
            break;
          case "assistant_text":
            content = (
              <AgentSessionAssistantText
                item={block.item}
                testIdPrefix={testIdPrefix}
              />
            );
            break;
          case "thinking":
          case "processing": {
            const isLive = block.item.delivery === "live";
            const isRunning =
              block.item.state === "streaming" || block.item.state === "pending";
            const defaultThinkingExpanded = isLive && isRunning;
            const expanded = expansion.isExpanded(
              `thinking-${block.item.id}`,
              defaultThinkingExpanded,
            );
            content = (
              <AgentSessionThinking
                expanded={expanded}
                item={block.item}
                onToggle={() =>
                  expansion.toggleExpanded(
                    `thinking-${block.item.id}`,
                    defaultThinkingExpanded,
                  )
                }
                testIdPrefix={testIdPrefix}
              />
            );
            break;
          }
          case "step_group": {
            const defaultGroupExpanded = isGroupDefaultExpanded(
              block.status,
              block.items[0]?.delivery || "live",
            );
            const expanded = expansion.isExpanded(
              block.groupId,
              defaultGroupExpanded,
            );
            content = (
              <AgentSessionStepGroup
                expanded={expanded}
                groupId={block.groupId}
                items={block.items}
                logicalCount={block.logicalCount}
                onToggle={() =>
                  expansion.toggleExpanded(block.groupId, defaultGroupExpanded)
                }
                status={block.status}
                testIdPrefix={testIdPrefix}
              />
            );
            break;
          }
          case "task":
          case "notice":
          case "terminal":
          case "error":
            content = (
              <AgentSessionItem
                item={block.item}
                testIdPrefix={testIdPrefix}
              />
            );
            break;
        }

        return (
          <li
            className="list-none py-1"
            key={`${turn.turnId}-${block.type}-${index}`}
            role="listitem"
          >
            {content}
          </li>
        );
      })}
    </div>
  );
}
