import { describe, expect, it } from "vitest";
import { formatErrorMessage } from "./error";

describe("formatErrorMessage", () => {
  it("formats string errors", () => {
    expect(formatErrorMessage("network timeout")).toBe("network timeout");
    expect(formatErrorMessage("   trimmed error   ")).toBe("trimmed error");
  });

  it("formats Error instances", () => {
    expect(formatErrorMessage(new Error("standard error"))).toBe(
      "standard error",
    );
  });

  it("extracts message from Tauri AppErrorView plain object", () => {
    const tauriError = {
      code: "storage_error",
      message: "error returned from database: (code: 5) database is locked",
      retryable: false,
      details: null,
    };
    expect(formatErrorMessage(tauriError)).toBe(
      "error returned from database: (code: 5) database is locked",
    );
  });

  it("extracts error field when message is missing", () => {
    expect(formatErrorMessage({ error: "permission denied" })).toBe(
      "permission denied",
    );
  });

  it("extracts details field when message and error are missing", () => {
    expect(formatErrorMessage({ details: "failed to open socket" })).toBe(
      "failed to open socket",
    );
  });

  it("formats code field when text messages are missing", () => {
    expect(formatErrorMessage({ code: "ERR_ABORTED" })).toBe("[ERR_ABORTED]");
  });

  it("serializes other object payloads if json serializable", () => {
    expect(formatErrorMessage({ status: 500, reason: "internal" })).toBe(
      '{"status":500,"reason":"internal"}',
    );
  });

  it("uses fallback on empty string, null, undefined or empty object", () => {
    expect(formatErrorMessage("")).toBe("未知错误");
    expect(formatErrorMessage(null)).toBe("未知错误");
    expect(formatErrorMessage(undefined)).toBe("未知错误");
    expect(formatErrorMessage({}, "请求失败")).toBe("请求失败");
  });

  it("never returns [object Object]", () => {
    expect(formatErrorMessage({})).not.toBe("[object Object]");
    expect(formatErrorMessage(Object.create(null))).not.toBe("[object Object]");
  });
});
