import * as React from "react";
import { Wrench } from "lucide-react";

import { cn } from "@/lib/utils";
import { Panel } from "./Panel";

export interface UnderConstructionStateProps extends Omit<React.HTMLAttributes<HTMLElement>, "title"> {
  actions?: React.ReactNode;
  description?: React.ReactNode;
  eyebrow?: React.ReactNode;
  icon?: React.ReactNode;
  title: React.ReactNode;
}

const UnderConstructionState = React.forwardRef<HTMLElement, UnderConstructionStateProps>(
  (
    {
      actions,
      "aria-labelledby": ariaLabelledBy,
      className,
      description,
      eyebrow,
      icon = <Wrench aria-hidden="true" size={22} />,
      title,
      ...props
    },
    ref,
  ) => {
    const titleId = React.useId();

    return (
      <section
        aria-labelledby={ariaLabelledBy ?? titleId}
        className={cn("flex min-h-[360px] flex-col", className)}
        ref={ref}
        {...props}
      >
        <Panel
          className="flex flex-1 flex-col items-center justify-center px-5 py-14 text-center"
          padding="none"
          variant="muted"
        >
          {icon && (
            <div className="mb-4 grid size-12 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary shadow-[var(--theme-shadow-control-inset)]">
              {icon}
            </div>
          )}
          {eyebrow && <p className="text-label-caps uppercase text-primary">{eyebrow}</p>}
          <h1 className="mt-2 text-h2 text-on-surface" id={titleId}>
            {title}
          </h1>
          {description && <p className="mt-3 max-w-lg text-body-sm text-on-surface-variant">{description}</p>}
          {actions && <div className="mt-5 flex flex-wrap items-center justify-center gap-2">{actions}</div>}
        </Panel>
      </section>
    );
  },
);
UnderConstructionState.displayName = "UnderConstructionState";

export { UnderConstructionState };
