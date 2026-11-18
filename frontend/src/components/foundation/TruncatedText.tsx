import * as React from "react";

import { cn } from "@/lib/utils";
import {
  Tooltip,
  TooltipContent,
  type TooltipContentProps,
  TooltipTrigger,
  useTooltipTiming,
} from "../ui/tooltip";

export type TruncatedTextLines = 1 | 2 | 3 | false | null | 0;

export interface TruncatedTextOwnProps<T extends React.ElementType = "span"> {
  as?: T;
  focusable?: boolean;
  inline?: boolean;
  lines?: TruncatedTextLines;
  tooltip?: React.ReactNode | false;
  tooltipAlign?: TooltipContentProps["align"];
  tooltipClassName?: string;
  tooltipSide?: TooltipContentProps["side"];
}

export type TruncatedTextProps<T extends React.ElementType = "span"> =
  TruncatedTextOwnProps<T> &
    Omit<React.ComponentPropsWithoutRef<T>, keyof TruncatedTextOwnProps<T>>;

const TRUNCATION_CLASS_MAP: Record<1 | 2 | 3, string> = {
  1: "truncate",
  2: "line-clamp-2",
  3: "line-clamp-3",
};

const CLOSE_GRACE_MS = 80;

function isTextOverflowing(
  element: HTMLElement | null,
  lines: 1 | 2 | 3,
): boolean {
  if (!element) {
    return false;
  }
  if (!(element.clientWidth > 0 && element.clientHeight > 0)) {
    return false;
  }
  if (lines === 1) {
    return element.scrollWidth - element.clientWidth > 1;
  }
  return (
    element.scrollHeight - element.clientHeight > 1 ||
    element.scrollWidth - element.clientWidth > 1
  );
}

function assignRef<T>(ref: React.Ref<T> | undefined, value: T | null) {
  if (typeof ref === "function") {
    ref(value);
  } else if (ref && typeof ref === "object") {
    (ref as React.MutableRefObject<T | null>).current = value;
  }
}

function TruncatedTextInner<T extends React.ElementType = "span">(
  {
    "aria-describedby": ariaDescribedby,
    as,
    children,
    className,
    focusable = false,
    inline = false,
    lines = 1,
    onBlur,
    onFocus,
    onKeyDown,
    onMouseEnter,
    onMouseLeave,
    onPointerDown,
    onPointerEnter,
    onPointerLeave,
    tabIndex,
    tooltip,
    tooltipAlign,
    tooltipClassName,
    tooltipSide,
    ...restProps
  }: TruncatedTextProps<T>,
  forwardedRef: React.ForwardedRef<HTMLElement>,
) {
  const Component = (as ?? "span") as React.ElementType;
  const hostRef = React.useRef<HTMLElement | null>(null);
  const { getOpenDelay, notifyClosed } = useTooltipTiming();

  const [mounted, setMounted] = React.useState(false);
  const [open, setOpen] = React.useState(false);
  const [anchorRect, setAnchorRect] = React.useState<{
    height: number;
    left: number;
    top: number;
    width: number;
  } | null>(null);
  const tooltipId = React.useId();

  const openRef = React.useRef(false);
  const isTriggerHoveredRef = React.useRef(false);
  const isContentHoveredRef = React.useRef(false);
  const isFocusedRef = React.useRef(false);
  const isPointerDownRef = React.useRef(false);
  const openTimerRef = React.useRef<number | null>(null);
  const closeTimerRef = React.useRef<number | null>(null);
  const pointerUpCleanupRef = React.useRef<(() => void) | null>(null);

  const activeLines: 1 | 2 | 3 | null =
    lines === 1 || lines === 2 || lines === 3 ? lines : null;

  const resolvedTooltip = tooltip === undefined ? children : tooltip;
  const canShowTooltip =
    activeLines !== null &&
    tooltip !== false &&
    resolvedTooltip !== null &&
    resolvedTooltip !== undefined &&
    resolvedTooltip !== "";

  const clearOpenTimer = React.useCallback(() => {
    if (openTimerRef.current !== null) {
      window.clearTimeout(openTimerRef.current);
      openTimerRef.current = null;
    }
  }, []);

  const clearCloseTimer = React.useCallback(() => {
    if (closeTimerRef.current !== null) {
      window.clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  const syncAnchorRect = React.useCallback(() => {
    const el = hostRef.current;
    if (!el) {
      return;
    }
    const rect = el.getBoundingClientRect();
    setAnchorRect({
      top: rect.top,
      left: rect.left,
      width: rect.width,
      height: rect.height,
    });
  }, []);

  const openTooltipNow = React.useCallback(() => {
    clearOpenTimer();
    clearCloseTimer();
    syncAnchorRect();
    openRef.current = true;
    setMounted(true);
    setOpen(true);
  }, [clearCloseTimer, clearOpenTimer, syncAnchorRect]);

  const closeTooltip = React.useCallback(() => {
    clearOpenTimer();
    clearCloseTimer();
    if (openRef.current) {
      openRef.current = false;
      notifyClosed();
    }
    setOpen(false);
    setMounted(false);
  }, [clearCloseTimer, clearOpenTimer, notifyClosed]);

  const scheduleCloseCheck = React.useCallback(() => {
    clearCloseTimer();
    closeTimerRef.current = window.setTimeout(() => {
      closeTimerRef.current = null;
      if (
        !isTriggerHoveredRef.current &&
        !isContentHoveredRef.current &&
        !(focusable && isFocusedRef.current)
      ) {
        closeTooltip();
      }
    }, CLOSE_GRACE_MS);
  }, [clearCloseTimer, closeTooltip, focusable]);

  React.useEffect(() => {
    if (!canShowTooltip && (mounted || openRef.current)) {
      closeTooltip();
    }
  }, [canShowTooltip, closeTooltip, mounted]);

  React.useEffect(() => {
    if (!open) {
      return;
    }
    const handleScroll = (event: Event) => {
      if (
        hostRef.current &&
        event.target instanceof Node &&
        event.target.contains(hostRef.current)
      ) {
        closeTooltip();
      }
    };
    window.addEventListener("scroll", handleScroll, { capture: true });
    return () => {
      window.removeEventListener("scroll", handleScroll, { capture: true });
    };
  }, [closeTooltip, open]);

  React.useEffect(() => {
    return () => {
      clearOpenTimer();
      clearCloseTimer();
      pointerUpCleanupRef.current?.();
      pointerUpCleanupRef.current = null;
    };
  }, [clearCloseTimer, clearOpenTimer]);

  const composedRef = React.useCallback(
    (node: HTMLElement | null) => {
      hostRef.current = node;
      assignRef(forwardedRef, node);
    },
    [forwardedRef],
  );

  const checkOverflow = React.useCallback(() => {
    if (activeLines === null) {
      return false;
    }
    return isTextOverflowing(hostRef.current, activeLines);
  }, [activeLines]);

  const handleTriggerEnter = React.useCallback(() => {
    clearCloseTimer();
    const wasHovered = isTriggerHoveredRef.current;
    isTriggerHoveredRef.current = true;

    if (!canShowTooltip || !checkOverflow()) {
      closeTooltip();
      return;
    }

    if (openRef.current) {
      return;
    }

    if (wasHovered && openTimerRef.current !== null) {
      return;
    }

    setMounted(true);
    const delay = getOpenDelay();
    if (delay <= 0) {
      openTooltipNow();
      return;
    }

    clearOpenTimer();
    openTimerRef.current = window.setTimeout(() => {
      openTimerRef.current = null;
      if (
        (isTriggerHoveredRef.current || isContentHoveredRef.current) &&
        checkOverflow()
      ) {
        openTooltipNow();
      } else {
        closeTooltip();
      }
    }, delay);
  }, [
    canShowTooltip,
    checkOverflow,
    clearCloseTimer,
    clearOpenTimer,
    closeTooltip,
    getOpenDelay,
    openTooltipNow,
  ]);

  const handleTriggerLeave = React.useCallback(() => {
    isTriggerHoveredRef.current = false;
    clearOpenTimer();

    if (!openRef.current) {
      if (!(focusable && isFocusedRef.current)) {
        setMounted(false);
      }
      return;
    }

    if (focusable && isFocusedRef.current) {
      return;
    }

    scheduleCloseCheck();
  }, [clearOpenTimer, focusable, scheduleCloseCheck]);

  const handleContentEnter = React.useCallback(() => {
    isContentHoveredRef.current = true;
    clearCloseTimer();
  }, [clearCloseTimer]);

  const handleContentLeave = React.useCallback(() => {
    isContentHoveredRef.current = false;
    if (
      openRef.current &&
      !isTriggerHoveredRef.current &&
      !(focusable && isFocusedRef.current)
    ) {
      scheduleCloseCheck();
    }
  }, [focusable, scheduleCloseCheck]);

  const handleHostPointerEnter = React.useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      onPointerEnter?.(event);
      handleTriggerEnter();
    },
    [handleTriggerEnter, onPointerEnter],
  );

  const handleHostMouseEnter = React.useCallback(
    (event: React.MouseEvent<HTMLElement>) => {
      onMouseEnter?.(event);
      handleTriggerEnter();
    },
    [handleTriggerEnter, onMouseEnter],
  );

  const handleHostPointerLeave = React.useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      onPointerLeave?.(event);
      handleTriggerLeave();
    },
    [handleTriggerLeave, onPointerLeave],
  );

  const handleHostMouseLeave = React.useCallback(
    (event: React.MouseEvent<HTMLElement>) => {
      onMouseLeave?.(event);
      handleTriggerLeave();
    },
    [handleTriggerLeave, onMouseLeave],
  );

  const handleHostPointerDown = React.useCallback(
    (event: React.PointerEvent<HTMLElement>) => {
      onPointerDown?.(event);
      isPointerDownRef.current = true;
      pointerUpCleanupRef.current?.();
      const handlePointerUp = () => {
        isPointerDownRef.current = false;
        pointerUpCleanupRef.current = null;
      };
      document.addEventListener("pointerup", handlePointerUp, { once: true });
      pointerUpCleanupRef.current = () => {
        document.removeEventListener("pointerup", handlePointerUp);
      };
      clearOpenTimer();
      if (openRef.current || mounted) {
        closeTooltip();
      }
    },
    [clearOpenTimer, closeTooltip, mounted, onPointerDown],
  );

  const handleHostFocus = React.useCallback(
    (event: React.FocusEvent<HTMLElement>) => {
      onFocus?.(event);
      if (!focusable) {
        return;
      }
      isFocusedRef.current = true;
      if (isPointerDownRef.current) {
        return;
      }
      if (!canShowTooltip || !checkOverflow()) {
        return;
      }
      openTooltipNow();
    },
    [canShowTooltip, checkOverflow, focusable, onFocus, openTooltipNow],
  );

  const handleHostBlur = React.useCallback(
    (event: React.FocusEvent<HTMLElement>) => {
      onBlur?.(event);
      if (!focusable) {
        return;
      }
      isFocusedRef.current = false;
      if (!isTriggerHoveredRef.current && !isContentHoveredRef.current) {
        closeTooltip();
      }
    },
    [closeTooltip, focusable, onBlur],
  );

  const handleHostKeyDown = React.useCallback(
    (event: React.KeyboardEvent<HTMLElement>) => {
      onKeyDown?.(event);
      if (event.key === "Escape" && (openRef.current || mounted)) {
        closeTooltip();
      }
    },
    [closeTooltip, mounted, onKeyDown],
  );

  const handleRadixOpenChange = React.useCallback(
    (nextOpen: boolean) => {
      if (!nextOpen) {
        isContentHoveredRef.current = false;
        closeTooltip();
      }
    },
    [closeTooltip],
  );

  const truncationClass =
    activeLines !== null ? TRUNCATION_CLASS_MAP[activeLines] : undefined;
  const resolvedTabIndex = focusable ? (tabIndex ?? 0) : tabIndex;
  const isTooltipVisible = mounted && canShowTooltip && open;

  const hostElement = (
    <Component
      {...restProps}
      aria-describedby={
        isTooltipVisible
          ? [ariaDescribedby, tooltipId].filter(Boolean).join(" ")
          : ariaDescribedby
      }
      className={cn(
        inline ? "inline-block min-w-0 max-w-full align-bottom" : "block min-w-0",
        truncationClass,
        className,
      )}
      onBlur={handleHostBlur}
      onFocus={handleHostFocus}
      onKeyDown={handleHostKeyDown}
      onMouseEnter={handleHostMouseEnter}
      onMouseLeave={handleHostMouseLeave}
      onPointerDown={handleHostPointerDown}
      onPointerEnter={handleHostPointerEnter}
      onPointerLeave={handleHostPointerLeave}
      ref={composedRef}
      tabIndex={resolvedTabIndex}
    >
      {children}
    </Component>
  );

  return (
    <>
      {hostElement}
      {mounted && canShowTooltip ? (
        <Tooltip
          disableHoverableContent
          onOpenChange={handleRadixOpenChange}
          open={open}
        >
          <TooltipTrigger asChild>
            <span
              aria-hidden="true"
              className="pointer-events-none fixed"
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
            align={tooltipAlign}
            className={tooltipClassName}
            id={tooltipId}
            onMouseEnter={handleContentEnter}
            onMouseLeave={handleContentLeave}
            onPointerEnter={handleContentEnter}
            onPointerLeave={handleContentLeave}
            side={tooltipSide}
          >
            {resolvedTooltip}
          </TooltipContent>
        </Tooltip>
      ) : null}
    </>
  );
}

type TruncatedTextComponent = (<T extends React.ElementType = "span">(
  props: TruncatedTextProps<T> & {
    ref?: React.ComponentPropsWithRef<T>["ref"];
  },
) => React.ReactElement | null) & {
  displayName?: string;
};

export const TruncatedText = React.forwardRef(
  TruncatedTextInner,
) as TruncatedTextComponent;

TruncatedText.displayName = "TruncatedText";
