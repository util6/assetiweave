import * as React from "react";

import { cn } from "@/lib/utils";

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        className={cn(
          "flex h-9 w-full rounded-lg border border-border bg-surface-high px-3 py-2 text-body-sm text-on-surface outline-none transition-colors file:border-0 file:bg-transparent file:text-body-sm file:font-semibold placeholder:text-outline focus:border-primary-strong/60 disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        ref={ref}
        type={type}
        {...props}
      />
    );
  },
);
Input.displayName = "Input";

export { Input };
