import { PlugZap } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { Button } from "../ui/button";
import { AgentCatalogIcon } from "./AgentCatalogIcon";
import { agentCatalog } from "./agentCatalog";

export function AgentCapabilitySetting({
  agentId,
  description,
  model,
  onOpen,
}: {
  agentId: string;
  description: string;
  model?: string;
  onOpen: () => void;
}) {
  const { t } = useI18n();
  const selectedAgent = agentCatalog.find((agent) => agent.id === agentId);

  return (
    <div className="flex w-[min(38rem,52vw)] items-center justify-between gap-4">
      <div className="min-w-0">
        <p className="text-body-sm leading-5 text-on-surface-variant">{description}</p>
      </div>
      <Button
        className="max-w-[18rem] shrink-0"
        onClick={onOpen}
        title={selectedAgent?.name ?? agentId}
        type="button"
        variant="outline"
      >
        {selectedAgent ? (
          <AgentCatalogIcon agent={selectedAgent} className="size-[15px]" fallbackSize={15} />
        ) : (
          <PlugZap size={15} />
        )}
        <span className="truncate">
          {selectedAgent?.name ?? agentId}
          {model ? ` · ${model}` : ` · ${t("settings.agents.modelDefault")}`}
        </span>
      </Button>
    </div>
  );
}
