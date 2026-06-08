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
    <header className={cn("flex items-start justify-between gap-4 max-[920px]:flex-col", className)}>
      <div className="min-w-0">
        {eyebrow ? (
          <div className="flex items-center gap-2 text-status-update">
            {icon ? <span className="shrink-0">{icon}</span> : null}
            <span className="text-label-caps uppercase">{eyebrow}</span>
          </div>
        ) : null}
        <div className="mt-1 flex min-w-0 items-center gap-3">
          <h1 className="min-w-0 text-h2 text-on-surface">{title}</h1>
          {titleAction ? <div className="mt-1 shrink-0">{titleAction}</div> : null}
        </div>
        {description ? <p className="mt-2 max-w-3xl text-body-sm text-on-surface-variant">{description}</p> : null}
      </div>
      {actions ? <div className={cn("w-full max-w-3xl shrink-0", actionsClassName)}>{actions}</div> : null}
    </header>
  );
}
