import { afterEach, describe, expect, it, vi } from "vitest";
import {
  clearSharedResourceCache,
  loadSharedResource,
  readSharedResource,
  writeSharedResource,
} from "./asyncCache";

describe("asyncCache", () => {
  afterEach(() => {
    clearSharedResourceCache();
  });

  it("deduplicates concurrent loads and keeps the resolved value", async () => {
    const loader = vi.fn(async () => ["groups"]);
    const first = loadSharedResource("groups", loader);
    const second = loadSharedResource("groups", loader);

    expect(first).toBe(second);
    await expect(first).resolves.toEqual(["groups"]);
    expect(loader).toHaveBeenCalledTimes(1);
    expect(readSharedResource<string[]>("groups")).toEqual(["groups"]);
  });

  it("serves cached data while a forced refresh is in flight", async () => {
    writeSharedResource("sources", ["cached"]);
    let resolveRefresh: ((value: string[]) => void) | undefined;
    const refreshing = new Promise<string[]>((resolve) => {
      resolveRefresh = resolve;
    });

    const request = loadSharedResource("sources", () => refreshing, { force: true });
    expect(readSharedResource<string[]>("sources")).toEqual(["cached"]);
    resolveRefresh?.(["fresh"]);
    await expect(request).resolves.toEqual(["fresh"]);
    expect(readSharedResource<string[]>("sources")).toEqual(["fresh"]);
  });
});
