import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as React from "react";
import { createPortal } from "react-dom";

import { cn } from "@/lib/utils";

export const DEFAULT_TOOLTIP_DELAY_MS = 300;
export const DEFAULT_TOOLTIP_SKIP_DELAY_MS = 150;

interface TooltipTimingContextValue {
  delayDuration: number;
  getOpenDelay: () => number;
  hasProvider: boolean;
  notifyClosed: () => void;
  skipDelayDuration: number;
}

export function resolveOpenDelay(
  lastClosedAt: number | null,
  delayDuration: number,
  skipDelayDuration: number,
): number {
  if (skipDelayDuration <= 0 || lastClosedAt === null) {
    return delayDuration;
  }
  return Date.now() - lastClosedAt <= skipDelayDuration ? 0 : delayDuration;
}

const fallbackLastClosedAt = { current: null as number | null };

const defaultTimingContext: TooltipTimingContextValue = {
  delayDuration: DEFAULT_TOOLTIP_DELAY_MS,
  skipDelayDuration: DEFAULT_TOOLTIP_SKIP_DELAY_MS,
  hasProvider: false,
  getOpenDelay: () =>
    resolveOpenDelay(
      fallbackLastClosedAt.current,
      DEFAULT_TOOLTIP_DELAY_MS,
      DEFAULT_TOOLTIP_SKIP_DELAY_MS,
    ),
  notifyClosed: () => {
    fallbackLastClosedAt.current = Date.now();
  },
};

const TooltipTimingContext =
  React.createContext<TooltipTimingContextValue>(defaultTimingContext);

export function useTooltipTiming(): TooltipTimingContextValue {
  return React.useContext(TooltipTimingContext);
}

export type TooltipProviderProps = React.ComponentPropsWithoutRef<
  typeof TooltipPrimitive.Provider
>;

const TooltipProvider: React.FC<TooltipProviderProps> = ({
  children,
  delayDuration = DEFAULT_TOOLTIP_DELAY_MS,
  skipDelayDuration = DEFAULT_TOOLTIP_SKIP_DELAY_MS,
  ...props
}) => {
  const lastClosedAtRef = React.useRef<number | null>(null);

  const contextValue = React.useMemo<TooltipTimingContextValue>(
    () => ({
      delayDuration,
      skipDelayDuration,
      hasProvider: true,
      getOpenDelay: () =>
        resolveOpenDelay(
          lastClosedAtRef.current,
          delayDuration,
          skipDelayDuration,
        ),
      notifyClosed: () => {
        lastClosedAtRef.current = Date.now();
      },
    }),
    [delayDuration, skipDelayDuration],
  );

  return (
    <TooltipTimingContext.Provider value={contextValue}>
      <TooltipPrimitive.Provider
        delayDuration={delayDuration}
        skipDelayDuration={skipDelayDuration}
        {...props}
      >
        {children}
      </TooltipPrimitive.Provider>
    </TooltipTimingContext.Provider>
  );
};
TooltipProvider.displayName = "TooltipProvider";

export type TooltipProps = React.ComponentPropsWithoutRef<
  typeof TooltipPrimitive.Root
>;

const Tooltip: React.FC<TooltipProps> = ({ onOpenChange, ...props }) => {
  const { hasProvider, notifyClosed } = useTooltipTiming();

  const handleOpenChange = React.useCallback(
    (nextOpen: boolean) => {
      if (!nextOpen) {
        notifyClosed();
      }
      onOpenChange?.(nextOpen);
    },
    [notifyClosed, onOpenChange],
  );

  const root = (
    <TooltipPrimitive.Root onOpenChange={handleOpenChange} {...props} />
  );

  if (hasProvider) {
    return root;
  }

  return <TooltipProvider>{root}</TooltipProvider>;
};
Tooltip.displayName = "Tooltip";

const TooltipTrigger = TooltipPrimitive.Trigger;
const TooltipPortal = TooltipPrimitive.Portal;
const TooltipArrow = TooltipPrimitive.Arrow;

export interface TooltipContentProps extends React.ComponentPropsWithoutRef<
  typeof TooltipPrimitive.Content
> {
  portalContainer?: HTMLElement | null;
}

const TooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  TooltipContentProps
>(
  (
    {
      align = "start",
      children,
      className,
      collisionPadding = 12,
      portalContainer,
      side = "top",
      sideOffset = 8,
      ...props
    },
    ref,
  ) => (
    <TooltipPrimitive.Portal container={portalContainer}>
      <TooltipPrimitive.Content
        align={align}
        className={cn(
          "ui-tooltip pointer-events-auto z-[110] max-w-[min(24rem,calc(100vw-2rem))] [overflow-wrap:anywhere] text-on-surface",
          className,
        )}
        collisionPadding={collisionPadding}
        ref={ref}
        side={side}
        sideOffset={sideOffset}
        {...props}
      >
        <div className="ui-tooltip-scroll">{children}</div>
        <span aria-hidden="true" className="ui-tooltip-arrow" />
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  ),
);
TooltipContent.displayName = TooltipPrimitive.Content.displayName;

export interface TooltipAnchorRect {
  height: number;
  left: number;
  top: number;
  width: number;
}

export function measureTooltipAnchorRect(
  element: HTMLElement | null,
): TooltipAnchorRect | null {
  if (!element) {
    return null;
  }
  const rect = element.getBoundingClientRect();
  return {
    top: rect.top,
    left: rect.left,
    width: rect.width,
    height: rect.height,
  };
}

export function isSameTooltipAnchorRect(
  prev: TooltipAnchorRect | null,
  next: TooltipAnchorRect | null,
): boolean {
  if (prev === next) {
    return true;
  }
  if (!prev || !next) {
    return false;
  }
  return (
    Math.abs(prev.top - next.top) <= 0.5 &&
    Math.abs(prev.left - next.left) <= 0.5 &&
    Math.abs(prev.width - next.width) <= 0.5 &&
    Math.abs(prev.height - next.height) <= 0.5
  );
}

export interface AnchoredTooltipPortalProps {
  align?: TooltipContentProps["align"];
  anchorDataAttr?: Record<string, string>;
  anchorRect: TooltipAnchorRect | null;
  ariaDescribedbyId?: string;
  children: React.ReactNode;
  className?: string;
  onContentMouseEnter?: React.MouseEventHandler<HTMLDivElement>;
  onContentMouseLeave?: React.MouseEventHandler<HTMLDivElement>;
  onContentPointerEnter?: React.PointerEventHandler<HTMLDivElement>;
  onContentPointerLeave?: React.PointerEventHandler<HTMLDivElement>;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  side?: TooltipContentProps["side"];
}

/**
 * Portals a floating Tooltip + fixed anchor to document.body so ancestors using
 * `contain: layout paint` or CSS `transform` cannot trap fixed positioning.
 */
const AnchoredTooltipPortal: React.FC<AnchoredTooltipPortalProps> = ({
  align = "start",
  anchorDataAttr,
  anchorRect,
  ariaDescribedbyId,
  children,
  className,
  onContentMouseEnter,
  onContentMouseLeave,
  onContentPointerEnter,
  onContentPointerLeave,
  onOpenChange,
  open,
  side = "top",
}) => {
  const node = (
    <Tooltip disableHoverableContent onOpenChange={onOpenChange} open={open}>
      <TooltipTrigger asChild>
        <span
          aria-hidden="true"
          className="pointer-events-none fixed"
          {...anchorDataAttr}
          style={
            anchorRect
              ? {
                  top: anchorRect.top,
                  left: anchorRect.left,
                  width: anchorRect.width,
                  height: anchorRect.height,
                }
              : undefined
          }
        />
      </TooltipTrigger>
      <TooltipContent
        align={align}
        className={className}
        id={ariaDescribedbyId}
        onMouseEnter={onContentMouseEnter}
        onMouseLeave={onContentMouseLeave}
        onPointerEnter={onContentPointerEnter}
        onPointerLeave={onContentPointerLeave}
        side={side}
      >
        {children}
      </TooltipContent>
    </Tooltip>
  );

  if (typeof document !== "undefined") {
    return createPortal(node, document.body);
  }
  return node;
};

export {
  AnchoredTooltipPortal,
  Tooltip,
  TooltipArrow,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
};
