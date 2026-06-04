import { describe, expect, it } from "vitest";
import { isRetryableUpdaterError, sanitizeUpdaterError } from "./updaterRetry";

describe("updaterRetry", () => {
  it("retries transient network and server failures", () => {
    expect(isRetryableUpdaterError(new Error("timeout while checking update"))).toBe(true);
    expect(isRetryableUpdaterError("HTTP 503 from update endpoint")).toBe(true);
    expect(isRetryableUpdaterError("status code 429")).toBe(true);
  });

  it("does not retry permanent updater failures", () => {
    expect(isRetryableUpdaterError("signature verification failed")).toBe(false);
    expect(isRetryableUpdaterError("HTTP 404 latest.json")).toBe(false);
    expect(isRetryableUpdaterError("no matching platform found")).toBe(false);
  });

  it("sanitizes URLs from error messages", () => {
    expect(sanitizeUpdaterError("failed to fetch https://example.com/releases/latest.json token=secret")).toBe(
      "failed to fetch [URL] token=secret",
    );
  });
});
