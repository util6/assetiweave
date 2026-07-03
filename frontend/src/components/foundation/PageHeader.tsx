import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export function PageHeader({
  actions,
  actionsClassName,
  className,
  description,
  eyebrow,
  icon,
  title,
  titleAction,
}: {
  actions?: ReactNode;
  actionsClassName?: string;
  className?: string;
  description?: ReactNode;
  eyebrow?: ReactNode;
  icon?: ReactNode;
  title: ReactNode;
  titleAction?: ReactNode;
}) {
  return (
    <header className={cn("flex min-w-0 flex-nowrap items-start justify-between gap-4 overflow-hidden", className)}>
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {eyebrow ? (
          <div className="flex min-w-0 items-center gap-2 text-status-update">
            {icon ? <span className="shrink-0">{icon}</span> : null}
            <span className="truncate whitespace-nowrap text-label-caps uppercase">{eyebrow}</span>
          </div>
        ) : null}
        <div className="mt-1 flex min-w-0 items-center gap-3 overflow-hidden">
          <h1 className="min-w-0 truncate whitespace-nowrap text-h2 text-on-surface">{title}</h1>
          {titleAction ? <div className="mt-1 shrink-0">{titleAction}</div> : null}
        </div>
        {description ? <p className="mt-2 max-w-3xl truncate whitespace-nowrap text-body-sm text-on-surface-variant">{description}</p> : null}
      </div>
      {actions ? (
        <div className={cn("flex min-w-0 max-w-[min(48rem,50%)] shrink justify-end overflow-hidden", actionsClassName)}>
          {actions}
        </div>
      ) : null}
    </header>
  );
}
