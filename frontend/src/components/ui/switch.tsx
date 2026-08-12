import * as SwitchPrimitives from "@radix-ui/react-switch";
import * as React from "react";

import { cn } from "@/lib/utils";
import { switchRecipe, switchThumbRecipe } from "../../theme/recipes";

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn("aurora-switch", switchRecipe(), className)}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb className={cn("aurora-switch-thumb", switchThumbRecipe())} />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;

export { Switch };
