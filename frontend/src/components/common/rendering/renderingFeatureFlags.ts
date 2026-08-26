export interface RenderingFeatureFlags {
  contentVisibilityContainment: boolean;
  conversationListVirtualization: boolean;
  conversationTurnVirtualization: boolean;
  deferredSkeletonRendering: boolean;
}

export const renderingFlags: RenderingFeatureFlags = {
  contentVisibilityContainment: true,
  conversationListVirtualization: true,
  conversationTurnVirtualization: true,
  deferredSkeletonRendering: true,
};
