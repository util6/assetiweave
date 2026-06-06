import { Slot } from "@radix-ui/react-slot";
import * as React from "react";
import { type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";
import { surfaceButtonRecipe } from "../../theme/recipes";

export interface SurfaceButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof surfaceButtonRecipe> {
  asChild?: boolean;
}

const SurfaceButton = React.forwardRef<HTMLButtonElement, SurfaceButtonProps>(
  ({ asChild = false, className, size, variant, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return <Comp className={cn(surfaceButtonRecipe({ className, size, variant }))} ref={ref} {...props} />;
  },
);
SurfaceButton.displayName = "SurfaceButton";

export { SurfaceButton };
