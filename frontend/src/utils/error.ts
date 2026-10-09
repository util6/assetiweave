export function formatErrorMessage(
  error: unknown,
  fallback = "未知错误",
): string {
  if (error === null || error === undefined) {
    return fallback;
  }
  if (typeof error === "string") {
    const trimmed = error.trim();
    return trimmed || fallback;
  }
  if (error instanceof Error) {
    return error.message || fallback;
  }
  if (typeof error === "object") {
    const obj = error as Record<string, unknown>;
    if (typeof obj.message === "string" && obj.message.trim()) {
      return obj.message.trim();
    }
    if (typeof obj.error === "string" && obj.error.trim()) {
      return obj.error.trim();
    }
    if (typeof obj.details === "string" && obj.details.trim()) {
      return obj.details.trim();
    }
    if (typeof obj.code === "string" && obj.code.trim()) {
      return `[${obj.code.trim()}]`;
    }
    try {
      const json = JSON.stringify(error);
      if (json && json !== "{}") {
        return json;
      }
    } catch {
      // ignore JSON stringify errors
    }
  }
  try {
    const stringified = String(error);
    if (!stringified || stringified === "[object Object]") {
      return fallback;
    }
    return stringified;
  } catch {
    return fallback;
  }
}
