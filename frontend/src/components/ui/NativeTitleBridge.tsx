import * as React from "react";

import {
  AnchoredTooltipPortal,
  isSameTooltipAnchorRect,
  measureTooltipAnchorRect,
  type TooltipAnchorRect,
  useTooltipTiming,
} from "./tooltip";

export const NATIVE_TITLE_DATA_ATTR = "data-aw-tooltip";
export const NATIVE_TITLE_EXEMPT_ATTR = "data-native-title-exempt";

const CLOSE_GRACE_MS = 80;

function isInterceptableElement(node: unknown): node is HTMLElement {
  if (!(node instanceof HTMLElement)) {
    return false;
  }
  if (
    node.tagName === "IFRAME" ||
    node.hasAttribute(NATIVE_TITLE_EXEMPT_ATTR) ||
    node.closest(`[${NATIVE_TITLE_EXEMPT_ATTR}]`) !== null ||
    node.closest('[role="tooltip"], .ui-tooltip') !== null
  ) {
    return false;
  }
  return true;
}

function stashElementTitle(element: HTMLElement): string | null {
  if (!isInterceptableElement(element)) {
    return null;
  }
  const rawTitle = element.getAttribute("title");
  if (rawTitle === null) {
    return element.getAttribute(NATIVE_TITLE_DATA_ATTR);
  }
  element.removeAttribute("title");
  if (rawTitle.trim() === "") {
    element.removeAttribute(NATIVE_TITLE_DATA_ATTR);
    return null;
  }
  element.setAttribute(NATIVE_TITLE_DATA_ATTR, rawTitle);
  if (
    !element.hasAttribute("aria-label") &&
    (element.tagName === "BUTTON" ||
      element.tagName === "A" ||
      element.getAttribute("role") === "button") &&
    !element.textContent?.trim()
  ) {
    element.setAttribute("aria-label", rawTitle);
  }
  return rawTitle;
}

function stashSubtreeTitles(root: Node): void {
  if (root instanceof HTMLElement && root.hasAttribute("title")) {
    stashElementTitle(root);
  }
  if (root instanceof Element) {
    const descendants = root.querySelectorAll("[title]");
    for (const el of descendants) {
      if (el instanceof HTMLElement) {
        stashElementTitle(el);
      }
    }
  }
}

function isTooltipContentTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) {
    return false;
  }
  return target.closest("[data-native-title-tooltip-content]") !== null;
}

function resolveNativeTooltipHost(
  target: EventTarget | null,
  clientX?: number,
  clientY?: number,
): HTMLElement | null {
  if (target instanceof Element) {
    if (
      target.closest('[data-truncated-tooltip-active="true"]') !== null ||
      target.closest(`[${NATIVE_TITLE_EXEMPT_ATTR}]`) !== null ||
      target.closest('[role="tooltip"], .ui-tooltip') !== null
    ) {
      return null;
    }
    const direct = target.closest(`[${NATIVE_TITLE_DATA_ATTR}]`);
    if (direct instanceof HTMLElement && isInterceptableElement(direct)) {
      return direct;
    }
  }

  if (
    typeof clientX === "number" &&
    typeof clientY === "number" &&
    typeof document.elementsFromPoint === "function"
  ) {
    const stack = document.elementsFromPoint(clientX, clientY);
    for (const el of stack) {
      if (
        el instanceof HTMLElement &&
        el.hasAttribute(NATIVE_TITLE_DATA_ATTR) &&
        isInterceptableElement(el)
      ) {
        return el;
      }
    }
  }

  return null;
}

/**
 * Globally intercepts native HTML `title` attributes across `document.body`,
 * moves them to `data-aw-tooltip` so OS/WebKit dark gray tooltips never pop,
 * and renders a single shared Scheme C Tooltip Portal for hovered targets.
 */
export const GlobalNativeTitleBridge: React.FC = () => {
  const { getOpenDelay, notifyClosed } = useTooltipTiming();
  const tooltipId = React.useId();

  const [mounted, setMounted] = React.useState(false);
  const [open, setOpen] = React.useState(false);
  const [tooltipText, setTooltipText] = React.useState<string>("");
  const [anchorRect, setAnchorRect] = React.useState<TooltipAnchorRect | null>(
    null,
  );

  const activeHostRef = React.useRef<HTMLElement | null>(null);
  const activeTextRef = React.useRef<string>("");
  const mountedRef = React.useRef(false);
  const openRef = React.useRef(false);
  const isTriggerHoveredRef = React.useRef(false);
  const isContentHoveredRef = React.useRef(false);
  const openTimerRef = React.useRef<number | null>(null);
  const closeTimerRef = React.useRef<number | null>(null);

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

  const syncAnchorRect = React.useCallback((host: HTMLElement | null) => {
    const next = measureTooltipAnchorRect(host);
    if (!next) {
      return;
    }
    setAnchorRect((prev) =>
      isSameTooltipAnchorRect(prev, next) ? prev : next,
    );
  }, []);

  const closeTooltip = React.useCallback(() => {
    clearOpenTimer();
    clearCloseTimer();
    isTriggerHoveredRef.current = false;
    isContentHoveredRef.current = false;
    activeHostRef.current = null;
    activeTextRef.current = "";
    if (openRef.current) {
      openRef.current = false;
      notifyClosed();
    }
    mountedRef.current = false;
    setOpen(false);
    setMounted(false);
    setTooltipText("");
  }, [clearCloseTimer, clearOpenTimer, notifyClosed]);

  const scheduleCloseCheck = React.useCallback(() => {
    clearCloseTimer();
    closeTimerRef.current = window.setTimeout(() => {
      closeTimerRef.current = null;
      if (!isTriggerHoveredRef.current && !isContentHoveredRef.current) {
        closeTooltip();
      }
    }, CLOSE_GRACE_MS);
  }, [clearCloseTimer, closeTooltip]);

  const openTooltipNow = React.useCallback(() => {
    const host = activeHostRef.current;
    if (
      !host ||
      !host.isConnected ||
      host.querySelector('[data-truncated-tooltip-active="true"]') !== null
    ) {
      closeTooltip();
      return;
    }
    clearOpenTimer();
    clearCloseTimer();
    syncAnchorRect(host);
    mountedRef.current = true;
    openRef.current = true;
    setMounted(true);
    setOpen(true);
  }, [clearCloseTimer, clearOpenTimer, closeTooltip, syncAnchorRect]);

  const activateHost = React.useCallback(
    (host: HTMLElement, text: string) => {
      clearCloseTimer();
      const isSameHost = activeHostRef.current === host;
      const isSameText = activeTextRef.current === text;
      if (isSameHost && isSameText && isTriggerHoveredRef.current) {
        return;
      }

      activeHostRef.current = host;
      activeTextRef.current = text;
      isTriggerHoveredRef.current = true;
      setTooltipText(text);
      syncAnchorRect(host);

      if (isSameHost && openRef.current) {
        return;
      }

      if (openRef.current) {
        openTooltipNow();
        return;
      }

      mountedRef.current = true;
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
          activeHostRef.current === host &&
          host.isConnected
        ) {
          openTooltipNow();
        } else if (!isContentHoveredRef.current) {
          closeTooltip();
        }
      }, delay);
    },
    [
      clearCloseTimer,
      clearOpenTimer,
      closeTooltip,
      getOpenDelay,
      openTooltipNow,
      syncAnchorRect,
    ],
  );

  React.useEffect(() => {
    if (typeof document === "undefined" || !document.body) {
      return;
    }

    stashSubtreeTitles(document.body);

    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (mutation.type === "attributes") {
          const target = mutation.target;
          if (target instanceof HTMLElement && target.hasAttribute("title")) {
            const stashed = stashElementTitle(target);
            if (target === activeHostRef.current) {
              if (stashed) {
                activeTextRef.current = stashed;
                setTooltipText(stashed);
              } else {
                closeTooltip();
              }
            }
          }
          continue;
        }

        if (mutation.type === "childList") {
          for (const addedNode of mutation.addedNodes) {
            stashSubtreeTitles(addedNode);
          }
          if (activeHostRef.current && !activeHostRef.current.isConnected) {
            closeTooltip();
          }
        }
      }
    });

    observer.observe(document.body, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["title"],
    });

    const handlePointerMoveOrOver = (event: MouseEvent) => {
      if (isTooltipContentTarget(event.target)) {
        return;
      }
      if (
        event.target instanceof HTMLElement &&
        event.target.hasAttribute("title")
      ) {
        stashElementTitle(event.target);
      }
      const nextHost = resolveNativeTooltipHost(
        event.target,
        event.clientX,
        event.clientY,
      );
      if (!nextHost) {
        if (activeHostRef.current) {
          isTriggerHoveredRef.current = false;
          clearOpenTimer();
          if (openRef.current) {
            scheduleCloseCheck();
          } else {
            closeTooltip();
          }
        }
        return;
      }
      const nextText = nextHost.getAttribute(NATIVE_TITLE_DATA_ATTR);
      if (!nextText) {
        closeTooltip();
        return;
      }
      activateHost(nextHost, nextText);
    };

    const handleMouseOut = (event: MouseEvent) => {
      const host = activeHostRef.current;
      if (!host) {
        return;
      }
      const related = event.relatedTarget;
      if (isTooltipContentTarget(related)) {
        isTriggerHoveredRef.current = false;
        return;
      }
      if (related instanceof Node && host.contains(related)) {
        return;
      }
      if (
        related === null ||
        event.target === host ||
        (event.target instanceof Node && host.contains(event.target))
      ) {
        isTriggerHoveredRef.current = false;
        clearOpenTimer();
        if (openRef.current) {
          scheduleCloseCheck();
        } else {
          closeTooltip();
        }
      }
    };

    const handlePointerDown = () => {
      if (activeHostRef.current || openRef.current || mountedRef.current) {
        closeTooltip();
      }
    };

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && (openRef.current || mountedRef.current)) {
        closeTooltip();
      }
    };

    const handleScroll = (event: Event) => {
      const host = activeHostRef.current;
      if (host && event.target instanceof Node && event.target.contains(host)) {
        closeTooltip();
      }
    };

    const handleResize = () => {
      if (openRef.current && activeHostRef.current) {
        syncAnchorRect(activeHostRef.current);
      }
    };

    document.addEventListener("mouseover", handlePointerMoveOrOver, true);
    document.addEventListener("mousemove", handlePointerMoveOrOver, {
      passive: true,
    });
    document.addEventListener("mouseout", handleMouseOut, true);
    document.addEventListener("pointerdown", handlePointerDown, true);
    document.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("scroll", handleScroll, { capture: true });
    window.addEventListener("resize", handleResize);

    return () => {
      observer.disconnect();
      document.removeEventListener("mouseover", handlePointerMoveOrOver, true);
      document.removeEventListener("mousemove", handlePointerMoveOrOver);
      document.removeEventListener("mouseout", handleMouseOut, true);
      document.removeEventListener("pointerdown", handlePointerDown, true);
      document.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("scroll", handleScroll, { capture: true });
      window.removeEventListener("resize", handleResize);
      clearOpenTimer();
      clearCloseTimer();
    };
  }, [
    activateHost,
    clearCloseTimer,
    clearOpenTimer,
    closeTooltip,
    scheduleCloseCheck,
    syncAnchorRect,
  ]);

  React.useEffect(() => {
    if (!open || !activeHostRef.current) {
      return;
    }
    let rafId: number | null = null;
    let remainingFrames = 16;
    const syncFrames = () => {
      if (activeHostRef.current && activeHostRef.current.isConnected) {
        syncAnchorRect(activeHostRef.current);
      }
      remainingFrames -= 1;
      if (remainingFrames > 0) {
        rafId = window.requestAnimationFrame(syncFrames);
      }
    };
    if (typeof window.requestAnimationFrame === "function") {
      rafId = window.requestAnimationFrame(syncFrames);
    }
    return () => {
      if (rafId !== null && typeof window.cancelAnimationFrame === "function") {
        window.cancelAnimationFrame(rafId);
      }
    };
  }, [open, syncAnchorRect]);

  const handleContentEnter = React.useCallback(() => {
    isContentHoveredRef.current = true;
    clearCloseTimer();
  }, [clearCloseTimer]);

  const handleContentLeave = React.useCallback(() => {
    isContentHoveredRef.current = false;
    if (openRef.current && !isTriggerHoveredRef.current) {
      scheduleCloseCheck();
    }
  }, [scheduleCloseCheck]);

  const handleRadixOpenChange = React.useCallback(
    (nextOpen: boolean) => {
      if (!nextOpen) {
        closeTooltip();
      }
    },
    [closeTooltip],
  );

  if (!mounted || !tooltipText) {
    return null;
  }

  return (
    <AnchoredTooltipPortal
      anchorDataAttr={{ "data-native-title-tooltip-anchor": "" }}
      anchorRect={anchorRect}
      ariaDescribedbyId={tooltipId}
      className="whitespace-pre-line"
      onContentMouseEnter={handleContentEnter}
      onContentMouseLeave={handleContentLeave}
      onContentPointerEnter={handleContentEnter}
      onContentPointerLeave={handleContentLeave}
      onOpenChange={handleRadixOpenChange}
      open={open}
    >
      <span data-native-title-tooltip-content="">{tooltipText}</span>
    </AnchoredTooltipPortal>
  );
};
GlobalNativeTitleBridge.displayName = "GlobalNativeTitleBridge";
