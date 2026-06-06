import * as React from "react";
import { type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";
import { panelRecipe } from "../../theme/recipes";

export interface PanelProps extends React.HTMLAttributes<HTMLDivElement>, VariantProps<typeof panelRecipe> {}

const Panel = React.forwardRef<HTMLDivElement, PanelProps>(({ className, padding, variant, ...props }, ref) => (
  <div className={cn(panelRecipe({ padding, variant }), className)} ref={ref} {...props} />
));
Panel.displayName = "Panel";

export { Panel };
