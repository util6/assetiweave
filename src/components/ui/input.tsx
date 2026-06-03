import * as React from "react";

import { cn } from "@/lib/utils";
import { controlRecipe } from "../../theme/recipes";

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, ...props }, ref) => {
    return (
      <input
        className={cn(
          controlRecipe({ variant: "input" }),
          "flex w-full file:border-0 file:bg-transparent file:text-body-sm file:font-semibold focus:border-primary-strong/65",
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
