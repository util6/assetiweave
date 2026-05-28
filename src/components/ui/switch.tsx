import * as SwitchPrimitives from "@radix-ui/react-switch";
import * as React from "react";

import { cn } from "@/lib/utils";

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cn(
      "peer inline-flex h-7 w-12 shrink-0 cursor-pointer items-center rounded-full border border-border bg-surface-highest p-0.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:border-status-create/70 data-[state=checked]:bg-status-create/30",
      className,
    )}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb
      className={cn(
        "pointer-events-none grid size-5 place-items-center rounded-full bg-outline-variant transition-transform data-[state=checked]:translate-x-5 data-[state=checked]:bg-status-create",
      )}
    />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;

export { Switch };
