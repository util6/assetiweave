import clsx from "clsx";
import type { HeaderTabItem } from "../../navigation/types";

export function HeaderTabs({ activeId, tabs }: { activeId: string; tabs: HeaderTabItem[] }) {
  return (
    <div
      className="absolute left-1/2 flex -translate-x-1/2 gap-0.5 rounded-full border border-border bg-surface-low/90 p-1"
      role="tablist"
      aria-label="资产类型"
    >
      {tabs
        .filter((tab) => tab.enabled)
        .map((tab) => (
          <button
            className={clsx(
              "min-w-24 rounded-full px-5 py-2 text-label-caps text-on-surface-variant transition-colors hover:text-on-surface",
              tab.id === activeId && "bg-surface-highest text-primary shadow-lg",
            )}
            key={tab.id}
            role="tab"
            aria-selected={tab.id === activeId}
          >
            {tab.label}
          </button>
        ))}
    </div>
  );
}
