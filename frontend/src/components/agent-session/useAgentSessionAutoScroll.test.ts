// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import {
  isNearTimelineBottom,
  useAgentSessionAutoScroll,
} from "./useAgentSessionAutoScroll";

describe("useAgentSessionAutoScroll", () => {
  it("detects whether scroll element is near bottom using threshold", () => {
    // Exactly at bottom
    expect(
      isNearTimelineBottom({
        clientHeight: 500,
        scrollHeight: 1000,
        scrollTop: 500,
      }),
    ).toBe(true);

    // Within threshold (30px from bottom <= 64)
    expect(
      isNearTimelineBottom({
        clientHeight: 500,
        scrollHeight: 1000,
        scrollTop: 470,
      }),
    ).toBe(true);

    // Beyond threshold (100px from bottom > 64)
    expect(
      isNearTimelineBottom({
        clientHeight: 500,
        scrollHeight: 1000,
        scrollTop: 400,
      }),
    ).toBe(false);
  });

  it("auto follows when near bottom and freezes when user scrolls up, showing new activity", () => {
    const scrollToMock = vi.fn();
    const fakeTimeline = {
      clientHeight: 500,
      scrollHeight: 1000,
      scrollTop: 500,
      scrollTo: scrollToMock,
    } as unknown as HTMLDivElement;

    const { rerender, result } = renderHook(
      ({ activityKey, resetKey }) =>
        useAgentSessionAutoScroll(activityKey, resetKey),
      {
        initialProps: {
          activityKey: "key-1",
          resetKey: "session-1",
        },
      },
    );

    // Attach fake timeline element
    (result.current.timelineRef as { current: HTMLDivElement | null }).current =
      fakeTimeline;

    // Initially at bottom, showNewActivity should be false
    expect(result.current.showNewActivity).toBe(false);

    // 1. New activity arrives while near bottom -> auto scrolls and no notification
    scrollToMock.mockClear();
    rerender({ activityKey: "key-2", resetKey: "session-1" });
    expect(scrollToMock).toHaveBeenCalled();
    expect(result.current.showNewActivity).toBe(false);

    // 2. User scrolls up away from bottom (scrollTop: 200, remaining: 300 > 64)
    fakeTimeline.scrollTop = 200;
    act(() => {
      result.current.onTimelineScroll();
    });

    // 3. New activity arrives while scrolled up -> stays frozen and shows new activity button
    scrollToMock.mockClear();
    rerender({ activityKey: "key-3", resetKey: "session-1" });
    expect(scrollToMock).not.toHaveBeenCalled();
    expect(result.current.showNewActivity).toBe(true);

    // 4. User clicks "scroll to latest" -> scrolls down and hides new activity button
    act(() => {
      result.current.scrollTimelineToLatest("smooth");
    });
    expect(scrollToMock).toHaveBeenCalled();
    expect(result.current.showNewActivity).toBe(false);

    // 5. Subsequent activity resumes auto-following
    scrollToMock.mockClear();
    rerender({ activityKey: "key-4", resetKey: "session-1" });
    expect(scrollToMock).toHaveBeenCalled();
    expect(result.current.showNewActivity).toBe(false);
  });

  it("resets auto scroll state when sessionResetKey changes", () => {
    const scrollToMock = vi.fn();
    const fakeTimeline = {
      clientHeight: 500,
      scrollHeight: 1000,
      scrollTop: 100,
      scrollTo: scrollToMock,
    } as unknown as HTMLDivElement;

    const { rerender, result } = renderHook(
      ({ activityKey, resetKey }) =>
        useAgentSessionAutoScroll(activityKey, resetKey),
      {
        initialProps: {
          activityKey: "key-1",
          resetKey: "session-1",
        },
      },
    );

    (result.current.timelineRef as { current: HTMLDivElement | null }).current =
      fakeTimeline;

    // Simulate scrolled up with showNewActivity
    act(() => {
      result.current.onTimelineScroll();
    });
    rerender({ activityKey: "key-2", resetKey: "session-1" });
    expect(result.current.showNewActivity).toBe(true);

    // Switch session
    act(() => {
      rerender({ activityKey: "key-1", resetKey: "session-2" });
    });

    // Reset should clear notification and scroll to bottom
    expect(result.current.showNewActivity).toBe(false);
    expect(scrollToMock).toHaveBeenCalled();
  });
});
