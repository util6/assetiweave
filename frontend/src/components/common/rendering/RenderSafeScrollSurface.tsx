import { forwardRef } from "react";
import type { HTMLAttributes, ReactNode, UIEvent } from "react";
import { cn } from "../../../lib/utils";
import { useCollapsibleHeader } from "../CollapsibleHeader";

export interface RenderSafeScrollSurfaceProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export const RenderSafeScrollSurface = forwardRef<
  HTMLDivElement,
  RenderSafeScrollSurfaceProps
>(function RenderSafeScrollSurface(
  { children, className, onScroll, ...props },
  ref,
) {
  const collapsible = useCollapsibleHeader();

  const handleScroll = (event: UIEvent<HTMLDivElement>) => {
    onScroll?.(event);
    collapsible?.onScroll(event);
  };

  return (
    <div
      {...props}
      className={cn("render-safe-scroll-surface", className)}
      data-render-safe-scroll-surface=""
      onScroll={collapsible ? handleScroll : onScroll}
      ref={ref}
    >
      {children}
    </div>
  );
});
