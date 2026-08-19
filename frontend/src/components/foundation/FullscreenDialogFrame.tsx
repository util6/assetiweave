import { cn } from "@/lib/utils";
import { DialogFrame, type DialogFrameProps } from "./DialogFrame";

export type FullscreenDialogFrameProps = Omit<DialogFrameProps, "size">;

export function FullscreenDialogFrame({
  className,
  containerClassName,
  contentClassName,
  overlayClassName,
  ...props
}: FullscreenDialogFrameProps) {
  return (
    <DialogFrame
      {...props}
      className={cn("h-full max-h-none max-w-none rounded-none", className)}
      containerClassName={cn("top-[var(--app-window-titlebar-height)] items-stretch p-0", containerClassName)}
      contentClassName={cn("overflow-hidden p-0", contentClassName)}
      overlayClassName={cn("top-[var(--app-window-titlebar-height)]", overlayClassName)}
      size="2xl"
    />
  );
}
