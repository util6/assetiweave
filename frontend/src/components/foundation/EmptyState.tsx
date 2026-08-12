import * as React from "react";

import { cn } from "@/lib/utils";
import { Panel } from "./Panel";

export interface EmptyStateProps extends Omit<React.HTMLAttributes<HTMLDivElement>, "title"> {
  actions?: React.ReactNode;
  description?: React.ReactNode;
  icon?: React.ReactNode;
  title: React.ReactNode;
}

const EmptyState = React.forwardRef<HTMLDivElement, EmptyStateProps>(
  ({ actions, className, description, icon, title, ...props }, ref) => (
    <Panel className={cn("aurora-empty-state flex min-h-44 flex-col items-center justify-center text-center", className)} padding="lg" ref={ref} variant="muted" {...props}>
      {icon && <div className="mb-3 grid size-11 place-items-center rounded-2xl border border-theme-control-border/70 bg-theme-control/70 text-primary shadow-[0_8px_20px_rgb(var(--theme-glow)/0.12),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.2)]">{icon}</div>}
      <h3 className="text-title-sm font-bold text-on-surface">{title}</h3>
      {description && <p className="mt-2 max-w-md text-body-sm text-on-surface-variant">{description}</p>}
      {actions && <div className="mt-4 flex items-center justify-center gap-2">{actions}</div>}
    </Panel>
  ),
);
EmptyState.displayName = "EmptyState";

export { EmptyState };
