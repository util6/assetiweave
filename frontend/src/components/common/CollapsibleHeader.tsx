import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
  type ReactNode,
  type UIEvent,
} from "react";
import { cn } from "../../lib/utils";

export interface CollapsibleHeaderContextValue {
  visible: boolean;
  setVisible: (visible: boolean) => void;
  onScroll: (event: UIEvent<HTMLElement>) => void;
  reset: () => void;
}

const CollapsibleHeaderContext =
  createContext<CollapsibleHeaderContextValue | null>(null);

export function useCollapsibleHeader() {
  return useContext(CollapsibleHeaderContext);
}

export interface UseCollapsibleHeaderOptions {
  /** 滚动距离达到多少像素时触发折叠或展开 (默认 8px) */
  threshold?: number;
  /** 距离顶部多少像素以内强制保持展开状态 (默认 10px) */
  topThreshold?: number;
  /** 初始是否展开 (默认 true) */
  initialVisible?: boolean;
}

export function useCollapsibleHeaderState({
  threshold = 8,
  topThreshold = 10,
  initialVisible = true,
}: UseCollapsibleHeaderOptions = {}) {
  const [visible, setVisible] = useState(initialVisible);
  const lastScrollTopRef = useRef(0);

  const reset = useCallback(() => {
    setVisible(true);
    lastScrollTopRef.current = 0;
  }, []);

  const onScroll = useCallback(
    (e: UIEvent<HTMLElement>) => {
      // 规避 Mac 触控板顶部弹性负值
      const currentTop = Math.max(0, e.currentTarget.scrollTop);
      const diff = currentTop - lastScrollTopRef.current;

      // 1. 如果处于顶部安全区内，强制保持展开
      if (currentTop <= topThreshold) {
        setVisible(true);
        lastScrollTopRef.current = currentTop;
        return;
      }

      // 2. 忽略微小的浮点抖动
      if (Math.abs(diff) < 2) {
        lastScrollTopRef.current = currentTop;
        return;
      }

      // 3. 向下滚动（浏览深层内容）：平滑收起头部
      if (diff > 0) {
        setVisible(false);
      } else if (diff < 0) {
        // 4. 向上滚动（试图回看或使用顶栏）：平滑展开头部
        setVisible(true);
      }

      lastScrollTopRef.current = currentTop;
    },
    [topThreshold],
  );

  return {
    visible,
    setVisible,
    onScroll,
    reset,
  };
}

export function CollapsibleHeaderProvider({
  children,
  ...options
}: {
  children: ReactNode;
} & UseCollapsibleHeaderOptions) {
  const state = useCollapsibleHeaderState(options);

  return (
    <CollapsibleHeaderContext.Provider value={state}>
      {children}
    </CollapsibleHeaderContext.Provider>
  );
}

export function CollapsibleHeader({
  children,
  className,
  visible: controlledVisible,
}: {
  children: ReactNode;
  className?: string;
  visible?: boolean;
}) {
  const context = useCollapsibleHeader();
  const visible = controlledVisible ?? context?.visible ?? true;

  return (
    <div
      className={cn("collapsible-header-shell shrink-0", className)}
      data-collapsed={!visible}
      aria-hidden={!visible}
    >
      <div className="min-h-0 overflow-hidden">{children}</div>
    </div>
  );
}
