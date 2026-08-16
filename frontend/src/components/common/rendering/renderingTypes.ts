import type { RenderPriority } from "./RenderScheduler";

export const SKELETON_BLOCK_SIZE_PX = {
  compact: 96,
  regular: 224,
  tall: 420,
} as const;

export type SkeletonBlockSize = keyof typeof SKELETON_BLOCK_SIZE_PX;
export type DeferredRenderState = "skeleton" | "queued" | "ready";

export interface RenderVisibilityRegistration {
  element: HTMLElement;
  key: string;
  onPriorityChange: (priority: RenderPriority | null) => void;
}
