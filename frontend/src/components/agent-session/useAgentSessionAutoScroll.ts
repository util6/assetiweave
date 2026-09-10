import { useCallback, useLayoutEffect, useRef, useState } from "react";

export function isNearTimelineBottom(
  element: Pick<HTMLElement, "clientHeight" | "scrollHeight" | "scrollTop">,
  threshold = 64,
): boolean {
  return (
    element.scrollHeight - element.scrollTop - element.clientHeight <= threshold
  );
}

export function useAgentSessionAutoScroll(
  activityDependencyKey?: string,
  sessionResetKey?: string,
) {
  const timelineRef = useRef<HTMLDivElement>(null);
  const timelineFollowingRef = useRef(true);
  const previousKeyRef = useRef<string | null>(null);
  const [showNewActivity, setShowNewActivity] = useState(false);

  const scrollTimelineToLatest = useCallback(
    (behavior: ScrollBehavior = "smooth") => {
      const timeline = timelineRef.current;
      if (!timeline) return;
      const top = Math.max(0, timeline.scrollHeight - timeline.clientHeight);
      if (typeof timeline.scrollTo === "function") {
        timeline.scrollTo({ behavior, top });
      } else {
        timeline.scrollTop = top;
      }
      timelineFollowingRef.current = true;
      setShowNewActivity(false);
    },
    [],
  );

  const onTimelineScroll = useCallback(() => {
    const timeline = timelineRef.current;
    if (!timeline) return;
    const following = isNearTimelineBottom(timeline);
    timelineFollowingRef.current = following;
    if (following) {
      setShowNewActivity(false);
    }
  }, []);

  useLayoutEffect(() => {
    previousKeyRef.current = null;
    timelineFollowingRef.current = true;
    setShowNewActivity(false);
    scrollTimelineToLatest("auto");
  }, [sessionResetKey, scrollTimelineToLatest]);

  useLayoutEffect(() => {
    if (activityDependencyKey === undefined) return;
    if (previousKeyRef.current === activityDependencyKey) return;
    previousKeyRef.current = activityDependencyKey;

    const timeline = timelineRef.current;
    if (!timeline) return;
    if (timelineFollowingRef.current || isNearTimelineBottom(timeline)) {
      scrollTimelineToLatest("auto");
    } else {
      setShowNewActivity(true);
    }
  }, [activityDependencyKey, scrollTimelineToLatest]);

  return {
    timelineRef,
    showNewActivity,
    scrollTimelineToLatest,
    onTimelineScroll,
  };
}
