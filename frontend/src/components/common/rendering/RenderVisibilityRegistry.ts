import type { RenderPriority } from "./RenderScheduler";
import type { RenderVisibilityRegistration } from "./renderingTypes";
import type { ScrollActivitySnapshot } from "./ScrollActivityController";

export interface RenderVisibilityRegistry {
  attach(root: HTMLElement, getDirection: () => ScrollActivitySnapshot["direction"]): () => void;
  register(registration: RenderVisibilityRegistration): () => void;
}

interface ObserverEntry extends IntersectionObserverEntry {
  target: HTMLElement;
}

export function createRenderVisibilityRegistry(): RenderVisibilityRegistry {
  let root: HTMLElement | null = null;
  let getDirection: (() => ScrollActivitySnapshot["direction"]) | null = null;
  let observer: IntersectionObserver | null = null;
  let resizeObserver: ResizeObserver | null = null;
  const registrations = new Map<string, RenderVisibilityRegistration>();

  const disconnectObserver = () => {
    observer?.disconnect();
    observer = null;
  };

  const priorityForEntry = (entry: ObserverEntry): RenderPriority | null => {
    if (!root || !entry.isIntersecting) return null;
    const rootRect = root.getBoundingClientRect();
    const rect = entry.boundingClientRect;
    const isInViewport = rect.top < rootRect.bottom && rect.bottom > rootRect.top;
    if (isInViewport) return 0;

    const direction = getDirection?.();
    const isAhead = direction === "backward"
      ? rect.bottom <= rootRect.top
      : rect.top >= rootRect.bottom;
    return isAhead ? 1 : 2;
  };

  const handleEntries = (entries: IntersectionObserverEntry[]) => {
    entries.forEach((entry) => {
      const registration = [...registrations.values()].find((candidate) => candidate.element === entry.target);
      if (!registration) return;
      registration.onPriorityChange(priorityForEntry(entry as ObserverEntry));
    });
  };

  const rebuildObserver = () => {
    disconnectObserver();
    if (!root || typeof IntersectionObserver === "undefined") return;
    observer = new IntersectionObserver(handleEntries, {
      root,
      rootMargin: `${root.clientHeight}px 0px`,
      threshold: 0,
    });
    registrations.forEach((registration) => observer?.observe(registration.element));
  };

  const detach = () => {
    disconnectObserver();
    resizeObserver?.disconnect();
    resizeObserver = null;
    registrations.forEach((registration) => registration.onPriorityChange(null));
    root = null;
    getDirection = null;
  };

  return {
    attach(nextRoot, nextGetDirection) {
      detach();
      root = nextRoot;
      getDirection = nextGetDirection;
      rebuildObserver();
      if (typeof ResizeObserver !== "undefined") {
        resizeObserver = new ResizeObserver(() => rebuildObserver());
        resizeObserver.observe(nextRoot);
      }
      return detach;
    },
    register(registration) {
      registrations.set(registration.key, registration);
      observer?.observe(registration.element);
      return () => {
        const current = registrations.get(registration.key);
        if (current !== registration) return;
        registrations.delete(registration.key);
        observer?.unobserve(registration.element);
      };
    },
  };
}
