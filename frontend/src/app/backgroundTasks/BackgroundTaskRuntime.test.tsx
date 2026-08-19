// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useBackgroundTaskRuntime, type BackgroundTaskRuntimeAdapter } from "./BackgroundTaskRuntime";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("useBackgroundTaskRuntime", () => {
  it("merges refreshes and events without disabling unrelated controls", async () => {
    const listeners: Array<(event: number) => void> = [];
    const unsubscribe = vi.fn();
    const adapter: BackgroundTaskRuntimeAdapter<number, number> = {
      initialState: 0,
      isRunning: (state) => state < 3,
      merge: (_current, incoming) => incoming,
      refresh: vi.fn().mockResolvedValue(1),
      subscribe: async (listener) => {
        listeners.push(listener);
        return unsubscribe;
      },
    };

    render(
      <RuntimeHarness adapter={adapter} />,
    );

    await act(async () => {});
    expect(screen.getByTestId("state").textContent).toBe("1");
    expect((screen.getByRole("button", { name: "Other feature" }) as HTMLButtonElement).disabled).toBe(false);

    act(() => listeners[0]?.(2));
    expect(screen.getByTestId("state").textContent).toBe("2");
  });

  it("polls active work and removes the subscription on unmount", async () => {
    vi.useFakeTimers();
    const unsubscribe = vi.fn();
    const refresh = vi.fn()
      .mockResolvedValueOnce(1)
      .mockResolvedValueOnce(3);
    const adapter: BackgroundTaskRuntimeAdapter<number, number> = {
      initialState: 0,
      isRunning: (state) => state < 3,
      merge: (_current, incoming) => incoming,
      refresh,
      subscribe: async () => unsubscribe,
      pollIntervalMs: 1000,
    };

    const { unmount } = render(<RuntimeHarness adapter={adapter} />);
    await act(async () => {});
    expect(screen.getByTestId("state").textContent).toBe("1");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("state").textContent).toBe("3");
    unmount();
    expect(unsubscribe).toHaveBeenCalledTimes(1);
  });

  it("reconnects the event subscription after a transport failure", async () => {
    vi.useFakeTimers();
    const subscribe = vi.fn()
      .mockRejectedValueOnce(new Error("disconnected"))
      .mockResolvedValue(vi.fn());
    const adapter: BackgroundTaskRuntimeAdapter<number, number> = {
      initialState: 3,
      isRunning: () => false,
      merge: (_current, incoming) => incoming,
      refresh: vi.fn().mockResolvedValue(3),
      subscribe,
      reconnectDelayMs: 50,
    };

    render(<RuntimeHarness adapter={adapter} />);
    await act(async () => {});
    expect(subscribe).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(50);
    });
    expect(subscribe).toHaveBeenCalledTimes(2);
  });
});

function RuntimeHarness({ adapter }: { adapter: BackgroundTaskRuntimeAdapter<number, number> }) {
  const { state } = useBackgroundTaskRuntime(adapter);
  return (
    <>
      <output data-testid="state">{state}</output>
      <button type="button">Other feature</button>
      <button onClick={() => fireEvent.click(screen.getByRole("button", { name: "Other feature" }))} type="button">
        Trigger
      </button>
    </>
  );
}
