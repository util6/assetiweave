import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useSyncExternalStore,
} from "react";
import type { ReactNode, RefObject } from "react";
import {
  createScrollActivityController,
  type ScrollActivityController,
  type ScrollActivitySnapshot,
} from "./ScrollActivityController";
import { createRenderScheduler, type RenderScheduler } from "./RenderScheduler";
import { createRenderVisibilityRegistry, type RenderVisibilityRegistry } from "./RenderVisibilityRegistry";

export interface RenderActivityProviderProps {
  children: ReactNode;
  scrollElementRef: RefObject<HTMLElement | null>;
}

interface RenderActivityContextValue {
  controller: ScrollActivityController;
  scheduler: RenderScheduler;
  visibility: RenderVisibilityRegistry;
}

const RenderActivityContext = createContext<RenderActivityContextValue | null>(null);

export function RenderActivityProvider({ children, scrollElementRef }: RenderActivityProviderProps): React.ReactElement {
  const controllerRef = useRef<ScrollActivityController | null>(null);
  const schedulerRef = useRef<RenderScheduler | null>(null);
  const visibilityRef = useRef<RenderVisibilityRegistry | null>(null);
  const disposeTimerRef = useRef<number | null>(null);
  if (!controllerRef.current) controllerRef.current = createScrollActivityController();
  if (!schedulerRef.current) schedulerRef.current = createRenderScheduler();
  if (!visibilityRef.current) visibilityRef.current = createRenderVisibilityRegistry();
  const value = useMemo(
    () => ({
      controller: controllerRef.current!,
      scheduler: schedulerRef.current!,
      visibility: visibilityRef.current!,
    }),
    [],
  );

  useEffect(() => {
    if (disposeTimerRef.current != null) {
      window.clearTimeout(disposeTimerRef.current);
      disposeTimerRef.current = null;
    }
    const element = scrollElementRef.current;
    if (!element) return undefined;
    const unsubscribe = value.controller.subscribe(() => {
      const snapshot = value.controller.getSnapshot();
      value.scheduler.setPhase(snapshot.phase);
      element.dataset.scrollPhase = snapshot.phase;
    });
    const detach = value.controller.attach(element);
    const detachVisibility = value.visibility.attach(
      element,
      () => value.controller.getSnapshot().direction,
    );
    const snapshot = value.controller.getSnapshot();
    value.scheduler.setPhase(snapshot.phase);
    element.dataset.scrollPhase = snapshot.phase;
    return () => {
      unsubscribe();
      detach();
      detachVisibility();
      disposeTimerRef.current = window.setTimeout(() => {
        disposeTimerRef.current = null;
        value.scheduler.dispose();
      }, 0);
    };
  }, [scrollElementRef, value]);

  return <RenderActivityContext.Provider value={value}>{children}</RenderActivityContext.Provider>;
}

export function useRenderActivity(): RenderActivityContextValue {
  const value = useContext(RenderActivityContext);
  if (!value) throw new Error("useRenderActivity must be used within RenderActivityProvider");
  return value;
}

export function useRenderVisibilityRegistry(): RenderVisibilityRegistry {
  return useRenderActivity().visibility;
}

export function useScrollActivitySnapshot(): ScrollActivitySnapshot {
  const { controller } = useRenderActivity();
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
}
