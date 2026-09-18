import {
  AppSkeleton,
  type SkeletonLayoutName,
} from "../components/foundation/skeleton";
import { cn } from "../lib/utils";

export type RouteTransitionKind = SkeletonLayoutName;

export interface RouteTransitionState {
  id: number;
  layout: RouteTransitionKind;
  label: string;
  phase: "enter" | "exit";
}

export function RouteTransitionOverlay({
  transition,
}: {
  transition: RouteTransitionState | null;
}) {
  if (!transition) {
    return null;
  }

  return (
    <div
      className={cn(
        "ui-route-transition pointer-events-none absolute inset-0 z-20 overflow-auto",
        transition.phase === "exit" && "ui-route-transition-exit",
      )}
      data-route-transition={transition.phase}
      data-route-transition-id={transition.id}
    >
      <div className="ui-route-progress" />
      <AppSkeleton label={transition.label} layout={transition.layout} />
    </div>
  );
}
