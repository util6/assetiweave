import { forwardRef } from "react";
import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../../../lib/utils";

export interface RenderSafeScrollSurfaceProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

export const RenderSafeScrollSurface = forwardRef<HTMLDivElement, RenderSafeScrollSurfaceProps>(
  function RenderSafeScrollSurface({ children, className, ...props }, ref) {
    return (
      <div
        {...props}
        className={cn("render-safe-scroll-surface", className)}
        data-render-safe-scroll-surface=""
        ref={ref}
      >
        {children}
      </div>
    );
  },
);
