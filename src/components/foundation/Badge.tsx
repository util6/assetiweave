import * as React from "react";
import { type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";
import { badgeRecipe } from "../../theme/recipes";

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement>, VariantProps<typeof badgeRecipe> {}

const Badge = React.forwardRef<HTMLSpanElement, BadgeProps>(({ className, tone, ...props }, ref) => (
  <span className={cn(badgeRecipe({ tone }), className)} ref={ref} {...props} />
));
Badge.displayName = "Badge";

export { Badge };
