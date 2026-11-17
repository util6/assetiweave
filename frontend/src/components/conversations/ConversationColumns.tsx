import type { ReactNode } from "react";
import { useCollapsibleHeader } from "../common/CollapsibleHeader";
import { RenderSafeScrollSurface } from "../common/rendering";

export function ColumnPanel({
  actions,
  badge,
  children,
  className = "",
  icon,
  scrollRef,
  title,
}: {
  actions?: ReactNode;
  badge?: ReactNode;
  children: ReactNode;
  className?: string;
  icon: ReactNode;
  scrollRef?: React.Ref<HTMLDivElement>;
  title: string;
}) {
  const collapsible = useCollapsibleHeader();

  return (
    <section
      className={`conversation-column flex h-full min-h-0 flex-col ${className}`}
    >
      <header className="conversation-column-header flex h-12 shrink-0 items-center justify-between gap-2 px-4">
        <div className="flex min-w-0 items-center gap-2">
          <span className="text-primary">{icon}</span>
          <h2 className="truncate text-label-caps text-on-surface-variant">
            {title}
          </h2>
          {badge ? <span className="shrink-0">{badge}</span> : null}
        </div>
        {actions ? (
          <div className="flex shrink-0 items-center gap-1">{actions}</div>
        ) : null}
      </header>
      <RenderSafeScrollSurface
        className="conversation-column-scroll min-h-0 flex-1"
        ref={scrollRef}
        tabIndex={0}
        aria-label={title}
        onScroll={collapsible?.onScroll}
      >
        {children}
      </RenderSafeScrollSurface>
    </section>
  );
}

export function EmptyPanel({ children }: { children: ReactNode }) {
  return (
    <div className="conversation-empty-state m-2 rounded-2xl p-6 text-center text-body-sm text-on-surface-variant">
      {children}
    </div>
  );
}
