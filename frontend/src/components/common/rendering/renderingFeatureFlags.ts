export interface RenderingFeatureFlags {
  contentVisibilityContainment: boolean;
  conversationTurnVirtualization: boolean;
  deferredSkeletonRendering: boolean;
}

export const renderingFlags: RenderingFeatureFlags = {
  contentVisibilityContainment: true,
  conversationTurnVirtualization: true,
  deferredSkeletonRendering: true,
};
