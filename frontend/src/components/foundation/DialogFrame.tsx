import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as React from "react";
import { X } from "lucide-react";

import { cn } from "@/lib/utils";
import { dialogRecipe, iconButtonRecipe, panelRecipe } from "../../theme/recipes";

export type DialogSize = "sm" | "md" | "lg" | "xl" | "2xl";

const dialogSizeClasses: Record<DialogSize, string> = {
  sm: "max-w-md",
  md: "max-w-xl",
  lg: "max-w-2xl",
  xl: "max-w-4xl",
  "2xl": "max-w-5xl",
};

export interface DialogFrameProps extends Omit<React.HTMLAttributes<HTMLElement>, "title"> {
  busy?: boolean;
  closeButtonRef?: React.Ref<HTMLButtonElement>;
  closeLabel?: string;
  contentClassName?: string;
  description?: React.ReactNode;
  footer?: React.ReactNode;
  footerClassName?: string;
  headerActions?: React.ReactNode;
  headerClassName?: string;
  icon?: React.ReactNode;
  iconClassName?: string;
  initialFocusRef?: React.RefObject<HTMLElement | null>;
  onClose?: () => void;
  onBackdropClick?: () => void;
  overlayClassName?: string;
  size?: DialogSize;
  title?: React.ReactNode;
}

const DialogFrame = React.forwardRef<HTMLElement, DialogFrameProps>(
  (
    {
      busy = false,
      children,
      className,
      closeButtonRef,
      closeLabel = "Close",
      contentClassName,
      description,
      footer,
      footerClassName,
      headerActions,
      headerClassName,
      icon,
      iconClassName,
      initialFocusRef,
      onClose,
      onBackdropClick,
      overlayClassName,
      size = "md",
      title,
      ...props
    },
    ref,
  ) => {
    const hasHeader = Boolean(title || description || icon || headerActions || onClose);
    const previouslyFocusedElementRef = React.useRef<HTMLElement | null>(
      typeof document !== "undefined" && document.activeElement instanceof HTMLElement ? document.activeElement : null,
    );

    React.useEffect(
      () => () => {
        const previouslyFocusedElement = previouslyFocusedElementRef.current;
        if (!previouslyFocusedElement?.isConnected) {
          return;
        }
        window.setTimeout(() => {
          if (document.activeElement === document.body) {
            previouslyFocusedElement.focus();
          }
        }, 0);
      },
      [],
    );

    return (
      <DialogPrimitive.Root
        modal
        onOpenChange={(nextOpen) => {
          if (!nextOpen && !busy) {
            onClose?.();
          }
        }}
        open
      >
        <DialogPrimitive.Overlay asChild>
          <div className={cn(dialogRecipe(), overlayClassName)}>
            <DialogPrimitive.Content
              asChild
              onEscapeKeyDown={(event) => {
                if (busy || !onClose) {
                  event.preventDefault();
                }
              }}
              onOpenAutoFocus={(event) => {
                if (initialFocusRef?.current) {
                  event.preventDefault();
                  initialFocusRef.current.focus();
                }
              }}
              onPointerDownOutside={(event) => {
                if (busy) {
                  event.preventDefault();
                  return;
                }
                if (onBackdropClick) {
                  event.preventDefault();
                  onBackdropClick();
                }
              }}
            >
              <section
                className={cn(
                  panelRecipe({ padding: "none", variant: "default" }),
                  "flex max-h-[92vh] w-full flex-col overflow-hidden shadow-[var(--theme-shadow-dialog)]",
                  dialogSizeClasses[size],
                  className,
                )}
                ref={ref}
                {...props}
              >
                {hasHeader && (
                  <header className={cn("flex min-h-14 shrink-0 items-center gap-3 border-b border-theme-card-border bg-theme-card-header/70 px-5 py-3", headerClassName)}>
                    {icon && (
                      <span className={cn("grid size-10 shrink-0 place-items-center rounded-lg border border-theme-control-border bg-theme-control text-primary", iconClassName)}>
                        {icon}
                      </span>
                    )}
                    <div className="min-w-0 flex-1">
                      {title && (
                        <DialogPrimitive.Title asChild>
                          <h2 className="text-title-sm font-bold text-on-surface">{title}</h2>
                        </DialogPrimitive.Title>
                      )}
                      {description && (
                        <DialogPrimitive.Description asChild>
                          <p className="mt-1 text-body-sm text-on-surface-variant">{description}</p>
                        </DialogPrimitive.Description>
                      )}
                      {!description && <DialogPrimitive.Description className="sr-only">{title ?? closeLabel}</DialogPrimitive.Description>}
                    </div>
                    {headerActions}
                    {onClose && (
                      <DialogPrimitive.Close asChild disabled={busy}>
                        <button
                          aria-label={closeLabel}
                          className={cn(iconButtonRecipe({ size: "sm" }))}
                          disabled={busy}
                          ref={closeButtonRef}
                          title={closeLabel}
                          type="button"
                        >
                          <X size={17} />
                        </button>
                      </DialogPrimitive.Close>
                    )}
                  </header>
                )}
                <div className={cn("min-h-0 flex-1 overflow-y-auto px-5 py-4", contentClassName)}>{children}</div>
                {footer && (
                  <footer
                    className={cn(
                      "flex shrink-0 items-center justify-end gap-2 border-t border-theme-card-border bg-theme-card px-5 py-4",
                      footerClassName,
                    )}
                  >
                    {footer}
                  </footer>
                )}
              </section>
            </DialogPrimitive.Content>
          </div>
        </DialogPrimitive.Overlay>
      </DialogPrimitive.Root>
    );
  },
);
DialogFrame.displayName = "DialogFrame";

export { DialogFrame };
