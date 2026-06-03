import * as React from "react";

import { cn } from "@/lib/utils";
import { controlRecipe } from "../../theme/recipes";

export interface FieldFrameProps extends React.HTMLAttributes<HTMLDivElement> {
  description?: React.ReactNode;
  error?: React.ReactNode;
  label?: React.ReactNode;
}

const FieldFrame = React.forwardRef<HTMLDivElement, FieldFrameProps>(
  ({ children, className, description, error, label, ...props }, ref) => (
    <div className="flex flex-col gap-2">
      {(label || description) && (
        <div className="min-w-0">
          {label && <div className="text-label-caps uppercase text-outline">{label}</div>}
          {description && <div className="mt-1 text-body-sm text-on-surface-variant">{description}</div>}
        </div>
      )}
      <div className={cn(controlRecipe({ variant: "frame" }), className)} ref={ref} {...props}>
        {children}
      </div>
      {error && <div className="text-body-sm text-status-remove">{error}</div>}
    </div>
  ),
);
FieldFrame.displayName = "FieldFrame";

export { FieldFrame };
