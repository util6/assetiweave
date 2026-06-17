import { FolderOpen } from "lucide-react";
import * as React from "react";

import { cn } from "@/lib/utils";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export interface PathPickerInputProps
  extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "className" | "disabled"> {
  className?: string;
  disabled?: boolean;
  inputClassName?: string;
  onPick: () => void;
  pickLabel: string;
  picking?: boolean;
}

export const PathPickerInput = React.forwardRef<HTMLInputElement, PathPickerInputProps>(
  ({ className, disabled = false, inputClassName, onPick, pickLabel, picking = false, ...props }, ref) => {
    const controlDisabled = disabled || picking;

    return (
      <div className={cn("flex gap-2", className)}>
        <Input
          className={cn("min-w-0 flex-1", inputClassName)}
          disabled={controlDisabled}
          ref={ref}
          {...props}
        />
        <Button
          aria-label={pickLabel}
          disabled={controlDisabled}
          onClick={onPick}
          size="icon"
          title={pickLabel}
          type="button"
          variant="outline"
        >
          <FolderOpen size={17} />
        </Button>
      </div>
    );
  },
);

PathPickerInput.displayName = "PathPickerInput";
