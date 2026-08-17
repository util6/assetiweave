import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as React from "react";
import { X } from "lucide-react";

import { cn } from "@/lib/utils";
import { iconButtonRecipe, panelRecipe } from "../../theme/recipes";

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
  container?: HTMLElement | null;
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
  portal?: boolean;
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
      container,
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
      portal = true,
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

    const [mounted, setMounted] = React.useState(false);

    React.useEffect(() => {
      setMounted(true);
      return () => {
        const previouslyFocusedElement = previouslyFocusedElementRef.current;
        if (!previouslyFocusedElement?.isConnected) {
          return;
        }
        window.setTimeout(() => {
          if (document.activeElement === document.body) {
            previouslyFocusedElement.focus();
          }
        }, 0);
      };
    }, []);

    const isClient = typeof document !== "undefined";
    const usePortal = portal && isClient && mounted;

    const dialogContent = (
      <>
        <DialogPrimitive.Overlay
          className={cn(
            "fixed inset-0 z-50 bg-[rgb(var(--theme-scrim)/0.62)] backdrop-blur-md transition-opacity duration-200",
            overlayClassName,
          )}
        />
        <div className="fixed inset-0 z-50 grid place-items-center overflow-y-auto p-4 sm:p-6">
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
                "aurora-dialog flex max-h-[92vh] w-full flex-col overflow-hidden shadow-[var(--theme-shadow-dialog)]",
                dialogSizeClasses[size],
                className,
              )}
              ref={ref}
              {...props}
            >
              {hasHeader && (
                <header className={cn("aurora-dialog-header flex min-h-14 shrink-0 items-center gap-3 border-b border-theme-card-border/55 bg-theme-card-header/55 px-5 py-3", headerClassName)}>
                  {icon && (
                    <span className={cn("grid size-10 shrink-0 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary", iconClassName)}>
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
                    "aurora-dialog-footer flex shrink-0 items-center justify-end gap-2 border-t border-theme-card-border/55 bg-theme-card/55 px-5 py-4",
                    footerClassName,
                  )}
                >
                  {footer}
                </footer>
              )}
            </section>
          </DialogPrimitive.Content>
        </div>
      </>
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
        {usePortal ? (
          <DialogPrimitive.Portal container={container}>
            {dialogContent}
          </DialogPrimitive.Portal>
        ) : (
          dialogContent
        )}
      </DialogPrimitive.Root>
    );
  },
);
DialogFrame.displayName = "DialogFrame";

export { DialogFrame };
