import { invoke } from "@tauri-apps/api/core";

export interface ManagedLogFile {
  log_file_path: string;
  log_file_name: string;
  file_size: number;
  modified_at_ms: number | null;
}

export interface LogSnapshot {
  log_dir_path: string;
  log_file_path: string;
  log_file_name: string;
  content: string;
  line_limit: number;
  file_size: number;
  modified_at_ms: number | null;
  available_files: ManagedLogFile[];
}

export type OperationLogLevel = "INFO" | "WARN" | "ERROR";

const fallbackLogFiles = [
  {
    log_file_path: "/preview/AssetIWeave/logs/app.log",
    log_file_name: "app.log",
    file_size: 596,
    modified_at_ms: Date.now(),
  },
  {
    log_file_path: "/preview/AssetIWeave/logs/codex-api.log",
    log_file_name: "codex-api.log",
    file_size: 394,
    modified_at_ms: Date.now() - 45000,
  },
] satisfies ManagedLogFile[];

const fallbackLogContent: Record<string, string> = {
  "app.log": [
    "2026-06-01T15:14:00+08:00 INFO AssetIWeave log viewer initialized",
    "2026-06-01T15:14:01+08:00 INFO Loaded 4 skill sources from preview catalog",
    "2026-06-01T15:14:02+08:00 WARN Browser preview is using fallback Tauri data",
    "2026-06-01T15:14:04+08:00 INFO Mount state refresh completed",
  ].join("\n"),
  "codex-api.log": [
    "2026-06-01T15:13:38+08:00 INFO Local gateway request accepted",
    "2026-06-01T15:13:39+08:00 ERROR Upstream provider unavailable",
    "request_id=preview-0001",
    "2026-06-01T15:13:41+08:00 INFO Local gateway recovered",
  ].join("\n"),
};

export async function getLogSnapshot(fileName?: string, lineLimit?: number): Promise<LogSnapshot> {
  if (!isTauriRuntime()) {
    return getFallbackLogSnapshot(fileName, lineLimit);
  }

  return await invoke<LogSnapshot>("logs_get_snapshot", { fileName: fileName ?? null, lineLimit });
}

export async function openLogDirectory(): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await invoke<void>("logs_open_log_directory");
}

export async function writeOperationLog(
  level: OperationLogLevel,
  operation: string,
  message: string,
  fields?: Record<string, string | number | boolean | null | undefined>,
): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }

  await invoke<void>("logs_write_operation", {
    level,
    operation,
    message,
    fields: normalizeLogFields(fields),
  });
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function normalizeLogFields(fields?: Record<string, string | number | boolean | null | undefined>) {
  if (!fields) {
    return null;
  }

  return Object.fromEntries(
    Object.entries(fields)
      .filter((entry): entry is [string, string | number | boolean] => entry[1] !== null && entry[1] !== undefined)
      .map(([key, value]) => [key, String(value)]),
  );
}

function getFallbackLogSnapshot(fileName?: string, lineLimit = 200): LogSnapshot {
  const selectedFile = fallbackLogFiles.find((file) => file.log_file_name === fileName) ?? fallbackLogFiles[0];
  const content = fallbackLogContent[selectedFile.log_file_name] ?? "";
  const lines = content.split("\n");
  const limitedContent = lines.slice(Math.max(0, lines.length - lineLimit)).join("\n");

  return {
    log_dir_path: "/preview/AssetIWeave/logs",
    log_file_path: selectedFile.log_file_path,
    log_file_name: selectedFile.log_file_name,
    content: limitedContent,
    line_limit: lineLimit,
    file_size: selectedFile.file_size,
    modified_at_ms: selectedFile.modified_at_ms,
    available_files: fallbackLogFiles,
  };
}
