import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as React from "react";

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

  const root = <TooltipPrimitive.Root onOpenChange={handleOpenChange} {...props} />;

  if (hasProvider) {
    return root;
  }

  return <TooltipProvider>{root}</TooltipProvider>;
};
Tooltip.displayName = "Tooltip";

const TooltipTrigger = TooltipPrimitive.Trigger;
const TooltipPortal = TooltipPrimitive.Portal;

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
      className,
      collisionPadding = 12,
      portalContainer,
      sideOffset = 6,
      ...props
    },
    ref,
  ) => (
    <TooltipPrimitive.Portal container={portalContainer}>
      <TooltipPrimitive.Content
        className={cn(
          "ui-menu pointer-events-auto z-[110] max-h-[min(16rem,var(--radix-tooltip-content-available-height,16rem))] max-w-[min(28rem,calc(100vw-2rem))] overflow-y-auto whitespace-pre-wrap break-words [overflow-wrap:anywhere] rounded-xl border border-theme-card-border/80 bg-theme-card/95 px-3 py-2 text-caption leading-relaxed text-on-surface shadow-[var(--theme-shadow-panel)] backdrop-blur-xl",
          className,
        )}
        collisionPadding={collisionPadding}
        ref={ref}
        sideOffset={sideOffset}
        {...props}
      />
    </TooltipPrimitive.Portal>
  ),
);
TooltipContent.displayName = TooltipPrimitive.Content.displayName;

export {
  Tooltip,
  TooltipContent,
  TooltipPortal,
  TooltipProvider,
  TooltipTrigger,
};
