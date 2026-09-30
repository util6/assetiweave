import { Columns2, Square } from "lucide-react";
import type { TeamViewMode } from "../../../types/agentWorkspace";

export interface TeamViewToggleProps {
  mode: TeamViewMode;
  onChange: (mode: TeamViewMode) => void;
  className?: string;
}

export function TeamViewToggle({
  mode,
  onChange,
  className = "",
}: TeamViewToggleProps) {
  return (
    <div
      role="group"
      aria-label="团队工作台视图切换"
      className={`inline-flex items-center rounded-xl border border-theme-border/60 bg-theme-panel/40 p-0.5 shadow-sm ${className}`}
    >
      <button
        type="button"
        onClick={() => onChange("parallel")}
        aria-pressed={mode === "parallel"}
        title="并行多列视图 (Parallel Lanes)"
        className={`flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-body-xs font-medium transition-all ${
          mode === "parallel"
            ? "bg-theme-surface text-primary-strong shadow-xs font-semibold"
            : "text-theme-muted hover:text-theme-text"
        }`}
      >
        <Columns2 className="h-3.5 w-3.5" />
        <span>并行多列</span>
      </button>

      <button
        type="button"
        onClick={() => onChange("single")}
        aria-pressed={mode === "single"}
        title="单成员聚焦视图 (Single Member Focus)"
        className={`flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-body-xs font-medium transition-all ${
          mode === "single"
            ? "bg-theme-surface text-primary-strong shadow-xs font-semibold"
            : "text-theme-muted hover:text-theme-text"
        }`}
      >
        <Square className="h-3.5 w-3.5" />
        <span>聚焦单列</span>
      </button>
    </div>
  );
}
