export const UPDATE_CHECK_RETRY_DELAYS_MS = [800, 2000, 5000] as const;
export const UPDATE_DOWNLOAD_RETRY_DELAYS_MS = [1000, 2500, 5000] as const;

const URL_PATTERN = /https?:\/\/[^\s)]+/gi;
const NON_RETRYABLE_HINTS = [
  "signature",
  "checksum",
  "hash mismatch",
  "invalid type",
  "invalid value",
  "no matching platform",
  "permission denied",
  "no space left",
  "disk full",
];
const RETRYABLE_HINTS = [
  "error sending request",
  "failed to send request",
  "timeout",
  "timed out",
  "network",
  "dns",
  "tls",
  "ssl",
  "connection reset",
  "connection refused",
  "connection aborted",
  "unexpected eof",
  "temporarily unavailable",
  "temporary failure",
  "no route to host",
  "unreachable",
];

export interface RetryContext {
  attempt: number;
  delayMs: number;
  error: unknown;
  totalRetries: number;
}

export interface RetryOptions {
  delaysMs: readonly number[];
  onRetry?: (context: RetryContext) => void;
  shouldRetry: (error: unknown) => boolean;
}

export function normalizeUpdaterError(error: unknown): string {
  if (error instanceof Error && error.message) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

export function sanitizeUpdaterError(error: unknown, maxLength = 220): string {
  const compact = normalizeUpdaterError(error).replace(URL_PATTERN, "[URL]").replace(/\s+/g, " ").trim();
  return compact.length > maxLength ? `${compact.slice(0, maxLength)}...` : compact;
}

export function isRetryableUpdaterError(error: unknown): boolean {
  const message = normalizeUpdaterError(error).toLowerCase();
  if (!message) {
    return false;
  }
  if (NON_RETRYABLE_HINTS.some((hint) => message.includes(hint))) {
    return false;
  }

  const statusCode = parseHttpStatusCode(message);
  if (statusCode !== null) {
    if (statusCode >= 500 || statusCode === 408 || statusCode === 429) {
      return true;
    }
    if (statusCode >= 400) {
      return false;
    }
  }

  return RETRYABLE_HINTS.some((hint) => message.includes(hint));
}

export async function retryWithBackoff<T>(operation: () => Promise<T>, options: RetryOptions): Promise<T> {
  const maxAttempts = Math.max(1, options.delaysMs.length + 1);

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    try {
      return await operation();
    } catch (error) {
      if (!options.shouldRetry(error) || attempt >= maxAttempts) {
        throw error;
      }

      const delayMs = withJitter(options.delaysMs[Math.min(attempt - 1, options.delaysMs.length - 1)]);
      options.onRetry?.({
        attempt,
        delayMs,
        error,
        totalRetries: maxAttempts - 1,
      });
      await sleep(delayMs);
    }
  }

  throw new Error("Retry failed without an explicit error");
}

function parseHttpStatusCode(message: string) {
  const statusMatch = message.match(/\bstatus(?:\s+code)?[:=\s]+(\d{3})\b/i) ?? message.match(/\bhttp\s*(\d{3})\b/i);
  return statusMatch?.[1] ? Number(statusMatch[1]) : null;
}

function withJitter(delayMs: number) {
  return delayMs + Math.floor(Math.random() * 350);
}

function sleep(ms: number) {
  return new Promise((resolve) => {
    globalThis.setTimeout(resolve, ms);
  });
}
