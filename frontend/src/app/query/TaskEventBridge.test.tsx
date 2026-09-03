/* @vitest-environment jsdom */
import { act, render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { TaskEventBridge } from "./TaskEventBridge";

describe("TaskEventBridge", () => {
  it("卸载后到达的订阅句柄立即释放", async () => {
    const client = new QueryClient();
    const cleanup = vi.fn();
    let finish!: (cleanup: () => void) => void;
    const subscribe = vi.fn(
      () =>
        new Promise<() => void>((resolve) => {
          finish = resolve;
        }),
    );
    const view = render(
      <QueryClientProvider client={client}>
        <TaskEventBridge<number, number>
          queryKey={["task-probe"]}
          subscribe={subscribe}
          merge={(_, event) => event}
        />
      </QueryClientProvider>,
    );
    view.unmount();
    await act(async () => {
      finish(() => {
        cleanup();
      });
    });
    expect(cleanup).toHaveBeenCalledTimes(1);
    client.clear();
  });

  it("接收事件时调用 merge 并更新 query cache", async () => {
    const client = new QueryClient();
    client.setQueryData(["counter"], 10);

    let emit!: (value: number) => void;
    const subscribe = vi.fn(
      (listener: (val: number) => void) =>
        new Promise<() => void>((resolve) => {
          emit = listener;
          resolve(() => {});
        }),
    );

    render(
      <QueryClientProvider client={client}>
        <TaskEventBridge<number, number>
          queryKey={["counter"]}
          subscribe={subscribe}
          merge={(curr, event) => (curr ?? 0) + event}
        />
      </QueryClientProvider>,
    );

    await act(async () => {});

    act(() => {
      emit(5);
    });

    expect(client.getQueryData(["counter"])).toBe(15);
    client.clear();
  });

  it("无 merge 时收到事件主动 invalidate query cache", async () => {
    const client = new QueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");

    let emit!: (value: string) => void;
    const subscribe = vi.fn(
      (listener: (val: string) => void) =>
        new Promise<() => void>((resolve) => {
          emit = listener;
          resolve(() => {});
        }),
    );

    render(
      <QueryClientProvider client={client}>
        <TaskEventBridge<string, string>
          queryKey={["notifications"]}
          subscribe={subscribe}
        />
      </QueryClientProvider>,
    );

    await act(async () => {});

    act(() => {
      emit("ping");
    });

    expect(invalidateSpy).toHaveBeenCalledWith(
      { queryKey: ["notifications"], exact: true },
      { cancelRefetch: false },
    );
    client.clear();
  });

  it("订阅失败时1000ms后自动重连", async () => {
    vi.useFakeTimers();
    const client = new QueryClient();
    const subscribe = vi
      .fn()
      .mockRejectedValueOnce(new Error("network error"))
      .mockResolvedValueOnce(vi.fn());

    render(
      <QueryClientProvider client={client}>
        <TaskEventBridge<string, string>
          queryKey={["retry-test"]}
          subscribe={subscribe}
        />
      </QueryClientProvider>,
    );

    await act(async () => {});
    expect(subscribe).toHaveBeenCalledTimes(1);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });

    expect(subscribe).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
    client.clear();
  });
});
