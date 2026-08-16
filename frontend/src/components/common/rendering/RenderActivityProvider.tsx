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

export interface RenderActivityProviderProps {
  children: ReactNode;
  scrollElementRef: RefObject<HTMLElement | null>;
}

interface RenderActivityContextValue {
  controller: ScrollActivityController;
  scheduler: RenderScheduler;
}

const RenderActivityContext = createContext<RenderActivityContextValue | null>(null);

export function RenderActivityProvider({ children, scrollElementRef }: RenderActivityProviderProps): React.ReactElement {
  const controllerRef = useRef<ScrollActivityController | null>(null);
  const schedulerRef = useRef<RenderScheduler | null>(null);
  if (!controllerRef.current) controllerRef.current = createScrollActivityController();
  if (!schedulerRef.current) schedulerRef.current = createRenderScheduler();
  const value = useMemo(
    () => ({ controller: controllerRef.current!, scheduler: schedulerRef.current! }),
    [],
  );

  useEffect(() => {
    const element = scrollElementRef.current;
    if (!element) return undefined;
    const unsubscribe = value.controller.subscribe(() => {
      const snapshot = value.controller.getSnapshot();
      value.scheduler.setPhase(snapshot.phase);
      element.dataset.scrollPhase = snapshot.phase;
    });
    const detach = value.controller.attach(element);
    const snapshot = value.controller.getSnapshot();
    value.scheduler.setPhase(snapshot.phase);
    element.dataset.scrollPhase = snapshot.phase;
    return () => {
      unsubscribe();
      detach();
      value.scheduler.dispose();
    };
  }, [scrollElementRef, value]);

  return <RenderActivityContext.Provider value={value}>{children}</RenderActivityContext.Provider>;
}

export function useRenderActivity(): RenderActivityContextValue {
  const value = useContext(RenderActivityContext);
  if (!value) throw new Error("useRenderActivity must be used within RenderActivityProvider");
  return value;
}

export function useScrollActivitySnapshot(): ScrollActivitySnapshot {
  const { controller } = useRenderActivity();
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
}
