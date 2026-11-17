export interface RenderingFeatureFlags {
  contentVisibilityContainment: boolean;
  conversationListVirtualization: boolean;
  conversationTurnVirtualization: boolean;
  deferredSkeletonRendering: boolean;
  /**
   * 会话卡片自身开关。卡片是虚拟化绝对定位项，且属于轻量内容；再叠加
   * `content-visibility: auto` 只会让 WebKit 在重新可见的首帧跳过绘制，
   * 表现为返回浏览视图时列表空白。详情 Turn 列表仍使用全局开关。
   */
  sessionCardContentVisibilityContainment: boolean;
}

export const renderingFlags: RenderingFeatureFlags = {
  contentVisibilityContainment: true,
  conversationListVirtualization: true,
  conversationTurnVirtualization: true,
  deferredSkeletonRendering: true,
  sessionCardContentVisibilityContainment: false,
};
