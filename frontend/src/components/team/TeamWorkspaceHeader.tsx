import { MessageSquare, Settings2, XCircle } from "lucide-react";
import React from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { TeamDetail } from "../../types/team";
import { Button } from "../ui/button";
import { TeamViewToggle, type TeamViewMode } from "./TeamViewToggle";

export interface TeamWorkspaceHeaderProps {
  team: TeamDetail;
  viewMode: TeamViewMode;
  onViewModeChange: (mode: TeamViewMode) => void;
  onOpenDetails: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

export function TeamWorkspaceHeader({
  onDelete,
  onEdit,
  onOpenDetails,
  onViewModeChange,
  team,
  viewMode,
}: TeamWorkspaceHeaderProps) {
  const { t } = useI18n();

  return (
    <header className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-theme-card-border/65 bg-theme-card-header/55 px-4 py-3 sm:px-5">
      <div className="flex min-w-0 items-start gap-3">
        <span className="grid size-10 shrink-0 place-items-center rounded-xl border border-theme-nav-active-border/50 bg-theme-nav-active/20 text-primary">
          <MessageSquare size={19} />
        </span>
        <div className="min-w-0">
          <p className="text-label-caps uppercase text-status-update">
            {t("team.chat.eyebrow")}
          </p>
          <h2 className="truncate text-title-lg font-bold text-on-surface">
            {team.name}
          </h2>
          <p className="mt-0.5 truncate text-body-sm text-on-surface-variant">
            {team.description || t("team.chat.description")}
          </p>
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-end gap-2">
        <TeamViewToggle value={viewMode} onChange={onViewModeChange} />
        <Button onClick={onOpenDetails} size="sm" type="button" variant="ghost">
          <Settings2 size={14} />
          {t("team.chat.details")}
        </Button>
        <Button onClick={onEdit} size="sm" type="button" variant="outline">
          {t("team.action.edit")}
        </Button>
        <Button
          aria-label={t("team.action.delete")}
          onClick={onDelete}
          size="icon-sm"
          type="button"
          variant="destructive"
        >
          <XCircle size={15} />
        </Button>
      </div>
    </header>
  );
}
