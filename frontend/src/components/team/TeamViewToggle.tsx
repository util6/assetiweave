import { Columns, Square } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";

export type TeamViewMode = "parallel" | "single";

export interface TeamViewToggleProps {
  value: TeamViewMode;
  onChange: (mode: TeamViewMode) => void;
  disabled?: boolean;
}

export function TeamViewToggle({
  value,
  onChange,
  disabled = false,
}: TeamViewToggleProps) {
  const { t } = useI18n();

  return (
    <div
      className="flex items-center gap-1 rounded-lg border border-theme-control-border/60 bg-theme-control/30 p-0.5"
      data-testid="team-view-toggle"
      role="group"
      aria-label={t("team.view.label") || "View"}
    >
      <button
        type="button"
        data-testid="team-view-toggle-parallel"
        aria-pressed={value === "parallel"}
        aria-label={t("team.view.parallel") || "Parallel"}
        disabled={disabled}
        onClick={() => onChange("parallel")}
        className={`flex items-center gap-1.5 rounded-md px-2 py-1 text-caption font-semibold transition-colors ${
          value === "parallel"
            ? "bg-theme-panel text-primary shadow-sm border border-theme-control-border/40"
            : "text-on-surface-variant hover:text-on-surface hover:bg-theme-control-hover/40"
        }`}
      >
        <Columns size={13} />
        <span>{t("team.view.parallel") || "Parallel"}</span>
      </button>
      <button
        type="button"
        data-testid="team-view-toggle-single"
        aria-pressed={value === "single"}
        aria-label={t("team.view.single") || "Single"}
        disabled={disabled}
        onClick={() => onChange("single")}
        className={`flex items-center gap-1.5 rounded-md px-2 py-1 text-caption font-semibold transition-colors ${
          value === "single"
            ? "bg-theme-panel text-primary shadow-sm border border-theme-control-border/40"
            : "text-on-surface-variant hover:text-on-surface hover:bg-theme-control-hover/40"
        }`}
      >
        <Square size={13} />
        <span>{t("team.view.single") || "Single"}</span>
      </button>
    </div>
  );
}
