import * as React from "react";

import { cn } from "@/lib/utils";
import { dialogRecipe, panelRecipe } from "../../theme/recipes";

export interface DialogFrameProps extends Omit<React.HTMLAttributes<HTMLElement>, "title"> {
  contentClassName?: string;
  description?: React.ReactNode;
  footer?: React.ReactNode;
  headerActions?: React.ReactNode;
  headerClassName?: string;
  icon?: React.ReactNode;
  iconClassName?: string;
  onBackdropClick?: () => void;
  overlayClassName?: string;
  title?: React.ReactNode;
}

const DialogFrame = React.forwardRef<HTMLElement, DialogFrameProps>(
  (
    {
      children,
      className,
      contentClassName,
      description,
      footer,
      headerActions,
      headerClassName,
      icon,
      iconClassName,
      onBackdropClick,
      overlayClassName,
      title,
      ...props
    },
    ref,
  ) => {
    const titleId = React.useId();
    const descriptionId = React.useId();
    const hasHeader = Boolean(title || description || icon);

    return (
      <div
        className={cn(dialogRecipe(), overlayClassName)}
        onMouseDown={(event) => {
          if (event.target === event.currentTarget) {
            onBackdropClick?.();
          }
        }}
      >
        <section
          aria-describedby={description ? descriptionId : undefined}
          aria-labelledby={title ? titleId : undefined}
          aria-modal="true"
          className={cn(
            panelRecipe({ padding: "none", variant: "default" }),
            "w-full max-w-lg overflow-hidden shadow-[var(--theme-shadow-dialog)]",
            className,
          )}
          ref={ref}
          role="dialog"
          {...props}
        >
          {hasHeader && (
            <header className={cn("flex items-start gap-3 border-b border-theme-card-border bg-theme-card-header/70 px-5 py-4", headerClassName)}>
              {icon && (
                <span className={cn("grid size-10 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary", iconClassName)}>
                  {icon}
                </span>
              )}
              <div className="min-w-0 flex-1">
                {title && (
                  <h2 className="text-title-sm font-bold text-on-surface" id={titleId}>
                    {title}
                  </h2>
                )}
                {description && (
                  <p className="mt-1 text-body-sm text-on-surface-variant" id={descriptionId}>
                    {description}
                  </p>
                )}
              </div>
              {headerActions}
            </header>
          )}
          <div className={cn("px-5 py-4", contentClassName)}>{children}</div>
          {footer && <footer className="flex items-center justify-end gap-2 border-t border-theme-card-border px-5 py-4">{footer}</footer>}
        </section>
      </div>
    );
  },
);
DialogFrame.displayName = "DialogFrame";

export { DialogFrame };
