import {
  AlertCircle,
  Check,
  Copy,
  FileCode,
  Image as ImageIcon,
  MapPin,
  Package,
  Terminal as TerminalIcon,
} from "lucide-react";
import { useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  AgentSessionItemView,
  ToolContentBlock,
} from "../../types/agentSession";
import { abbreviateHomePath } from "../../utils/path";
import { stripAnsi } from "../../utils/stripAnsi";
import { ConversationDiff } from "../conversations/ConversationDiff";

export interface AgentSessionToolDetailProps {
  item: AgentSessionItemView;
  testIdPrefix?: string;
}

export function AgentSessionToolDetail({
  item,
  testIdPrefix = "agent-session",
}: AgentSessionToolDetailProps) {
  const { t } = useI18n();
  const [copiedSection, setCopiedSection] = useState<string | null>(null);

  const copyToClipboard = (text: string, section: string) => {
    void navigator.clipboard?.writeText(text);
    setCopiedSection(section);
    setTimeout(() => setCopiedSection(null), 2000);
  };

  const toolInput = item.toolInput;
  const toolOutput = item.toolOutput;

  // Extract blocks
  const inputBlocks = extractToolBlocks(toolInput);
  const outputBlocks = extractToolBlocks(toolOutput);

  // Detect command & cwd
  const commandInfo = extractCommandInfo(toolInput, inputBlocks);

  // Detect terminal stdout / stderr / exit
  const terminalInfo = extractTerminalInfo(toolOutput, outputBlocks);

  // Filter out blocks that are handled as command/terminal
  const nonCommandInputBlocks = inputBlocks.filter(
    (b) => b.type !== "command" && b.type !== "terminal",
  );
  const nonTerminalOutputBlocks = outputBlocks.filter(
    (b) => b.type !== "command" && b.type !== "terminal",
  );

  return (
    <div
      className="mt-2.5 grid gap-2.5 border-t border-theme-card-border/40 pt-2 text-body-sm"
      data-testid={`${testIdPrefix}-tool-detail-${item.id}`}
    >
      {/* Truncation indicator if present */}
      {item.truncation ? (
        <div
          className="flex items-center gap-1.5 rounded-md border border-status-warning/40 bg-status-warning/10 px-2 py-1 text-caption text-status-warning"
          data-testid={`${testIdPrefix}-tool-truncation-${item.id}`}
        >
          <span className="font-semibold">
            {t("agentSession.truncated") || "Truncated"}
          </span>
          <span>
            {t("agentSession.truncatedDetail", {
              original: item.truncation.originalBytes,
              retained: item.truncation.retainedBytes,
            }) ||
              `(${item.truncation.retainedBytes}/${item.truncation.originalBytes} bytes)`}
          </span>
        </div>
      ) : null}

      {/* Command block if detected */}
      {commandInfo ? (
        <div
          className="rounded-lg border border-theme-control-border/60 bg-theme-control/30 p-2"
          data-testid={`${testIdPrefix}-tool-command-${item.id}`}
        >
          <div className="flex items-center justify-between gap-2">
            <span className="flex items-center gap-1.5 text-label-caps uppercase font-semibold text-primary">
              <TerminalIcon size={13} />
              Command
            </span>
            <button
              aria-label="Copy command"
              className="rounded p-1 text-outline transition-colors hover:text-on-surface"
              onClick={() => copyToClipboard(commandInfo.command, "command")}
              type="button"
            >
              {copiedSection === "command" ? (
                <Check size={13} className="text-status-create" />
              ) : (
                <Copy size={13} />
              )}
            </button>
          </div>
          <pre className="mt-1 max-h-40 overflow-auto whitespace-pre-wrap break-all rounded bg-theme-panel/70 p-2 font-mono text-caption text-on-surface">
            $ {commandInfo.command}
          </pre>
          {commandInfo.cwd ? (
            <div className="mt-1 truncate text-caption font-mono text-on-surface-variant">
              cwd: {abbreviateHomePath(commandInfo.cwd)}
            </div>
          ) : null}
        </div>
      ) : null}

      {/* Typed Input Blocks */}
      {nonCommandInputBlocks.map((block, index) => (
        <ToolContentBlockRenderer
          block={block}
          copyToClipboard={copyToClipboard}
          copiedSection={copiedSection}
          index={index}
          itemId={item.id}
          key={`input-${block.type}-${index}`}
          testIdPrefix={testIdPrefix}
        />
      ))}

      {/* Generic Input if no command and no typed input blocks */}
      {!commandInfo &&
      nonCommandInputBlocks.length === 0 &&
      toolInput !== undefined &&
      toolInput !== null ? (
        <div data-testid={`${testIdPrefix}-tool-input-${item.id}`}>
          <div className="flex items-center justify-between gap-2">
            <span className="text-label-caps uppercase font-medium text-on-surface-variant">
              {t("agentSession.tool.input") || "Input"}
            </span>
            <button
              aria-label="Copy input"
              className="rounded p-1 text-outline transition-colors hover:text-on-surface"
              onClick={() => copyToClipboard(formatPayload(toolInput), "input")}
              type="button"
            >
              {copiedSection === "input" ? (
                <Check size={13} className="text-status-create" />
              ) : (
                <Copy size={13} />
              )}
            </button>
          </div>
          <pre className="mt-1 max-h-60 overflow-auto whitespace-pre-wrap break-all rounded bg-theme-control/40 p-2 font-mono text-caption text-on-surface">
            {formatPayload(toolInput)}
          </pre>
        </div>
      ) : null}

      {/* Terminal Output if detected */}
      {terminalInfo ? (
        <div
          className="grid gap-2"
          data-testid={`${testIdPrefix}-tool-terminal-${item.id}`}
        >
          {/* Stdout */}
          {terminalInfo.stdout ? (
            <div>
              <div className="flex items-center justify-between gap-2">
                <span className="text-label-caps uppercase font-semibold text-on-surface-variant">
                  stdout
                </span>
                <button
                  aria-label="Copy stdout"
                  className="rounded p-1 text-outline transition-colors hover:text-on-surface"
                  onClick={() => copyToClipboard(terminalInfo.stdout, "stdout")}
                  type="button"
                >
                  {copiedSection === "stdout" ? (
                    <Check size={13} className="text-status-create" />
                  ) : (
                    <Copy size={13} />
                  )}
                </button>
              </div>
              <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap break-all rounded bg-theme-control/50 p-2 font-mono text-caption text-on-surface">
                {terminalInfo.stdout}
              </pre>
            </div>
          ) : null}

          {/* Stderr */}
          {terminalInfo.stderr ? (
            <div>
              <div className="flex items-center justify-between gap-2">
                <span className="text-label-caps uppercase font-semibold text-status-remove">
                  stderr
                </span>
                <button
                  aria-label="Copy stderr"
                  className="rounded p-1 text-outline transition-colors hover:text-on-surface"
                  onClick={() => copyToClipboard(terminalInfo.stderr, "stderr")}
                  type="button"
                >
                  {copiedSection === "stderr" ? (
                    <Check size={13} className="text-status-create" />
                  ) : (
                    <Copy size={13} />
                  )}
                </button>
              </div>
              <pre className="mt-1 max-h-64 overflow-auto whitespace-pre-wrap break-all rounded border border-status-remove/30 bg-status-remove/10 p-2 font-mono text-caption text-status-remove">
                {terminalInfo.stderr}
              </pre>
            </div>
          ) : null}

          {/* Exit Code / Signal Badge */}
          {terminalInfo.exitCode !== null || terminalInfo.signal ? (
            <div
              className="flex items-center gap-2 text-caption font-mono"
              data-testid={`${testIdPrefix}-tool-exit-${item.id}`}
            >
              {terminalInfo.exitCode !== null ? (
                <span
                  className={`rounded px-1.5 py-0.5 ${
                    terminalInfo.exitCode === 0
                      ? "bg-theme-control/60 text-outline"
                      : "bg-status-remove/20 text-status-remove font-semibold"
                  }`}
                >
                  Exit code: {terminalInfo.exitCode}
                </span>
              ) : null}
              {terminalInfo.signal ? (
                <span className="rounded bg-status-conflict/20 px-1.5 py-0.5 text-status-conflict font-semibold">
                  Signal: {terminalInfo.signal}
                </span>
              ) : null}
            </div>
          ) : null}
        </div>
      ) : null}

      {/* Typed Output Blocks */}
      {nonTerminalOutputBlocks.map((block, index) => (
        <ToolContentBlockRenderer
          block={block}
          copyToClipboard={copyToClipboard}
          copiedSection={copiedSection}
          index={index}
          itemId={item.id}
          key={`output-${block.type}-${index}`}
          testIdPrefix={testIdPrefix}
        />
      ))}

      {/* Generic Output if no terminal and no typed output blocks */}
      {!terminalInfo &&
      nonTerminalOutputBlocks.length === 0 &&
      toolOutput !== undefined &&
      toolOutput !== null ? (
        <div data-testid={`${testIdPrefix}-tool-output-${item.id}`}>
          <div className="flex items-center justify-between gap-2">
            <span className="text-label-caps uppercase font-medium text-on-surface-variant">
              {t("agentSession.tool.output") || "Output"}
            </span>
            <button
              aria-label="Copy output"
              className="rounded p-1 text-outline transition-colors hover:text-on-surface"
              onClick={() =>
                copyToClipboard(formatPayload(toolOutput), "output")
              }
              type="button"
            >
              {copiedSection === "output" ? (
                <Check size={13} className="text-status-create" />
              ) : (
                <Copy size={13} />
              )}
            </button>
          </div>
          <pre className="mt-1 max-h-60 overflow-auto whitespace-pre-wrap break-all rounded bg-theme-control/40 p-2 font-mono text-caption text-on-surface">
            {formatPayload(toolOutput)}
          </pre>
        </div>
      ) : null}
    </div>
  );
}

function ToolContentBlockRenderer({
  block,
  itemId,
  index,
  testIdPrefix,
  copyToClipboard,
  copiedSection,
}: {
  block: ToolContentBlock;
  itemId: string;
  index: number;
  testIdPrefix: string;
  copyToClipboard: (text: string, section: string) => void;
  copiedSection: string | null;
}) {
  switch (block.type) {
    case "diff": {
      const displayPath = abbreviateHomePath(block.path);
      const diffValue =
        block.unifiedDiff ??
        buildUnifiedDiff(displayPath, block.oldText, block.newText);
      const copyKey = `diff-${itemId}-${index}`;

      return (
        <div
          className="rounded-lg border border-theme-control-border/60 bg-theme-control/20 p-2.5"
          data-testid={`${testIdPrefix}-tool-diff-${itemId}`}
        >
          <div className="flex min-w-0 items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-1.5 text-label-caps font-mono font-medium text-on-surface">
              <FileCode size={13} className="shrink-0 text-primary" />
              <span className="truncate" title={block.path}>
                {displayPath}
              </span>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {block.isTruncated ? (
                <span
                  className="rounded bg-status-warning/15 px-1.5 py-0.5 text-caption font-semibold text-status-warning"
                  data-testid={`${testIdPrefix}-diff-truncated-${itemId}-${index}`}
                >
                  Truncated Diff
                </span>
              ) : null}
              <button
                aria-label="Copy diff"
                className="rounded p-1 text-outline transition-colors hover:text-on-surface"
                onClick={() => copyToClipboard(diffValue, copyKey)}
                type="button"
              >
                {copiedSection === copyKey ? (
                  <Check size={13} className="text-status-create" />
                ) : (
                  <Copy size={13} />
                )}
              </button>
            </div>
          </div>
          <div className="mt-2 overflow-x-auto">
            <ConversationDiff value={diffValue} />
          </div>
        </div>
      );
    }

    case "location": {
      const displayPath = abbreviateHomePath(block.path);
      const locator = `${displayPath}${block.line != null ? `:${block.line}` : ""}${block.column != null ? `:${block.column}` : ""}`;
      const copyKey = `loc-${itemId}-${index}`;

      return (
        <div
          className="flex items-center justify-between gap-2 rounded-lg border border-theme-control-border/60 bg-theme-control/25 px-2.5 py-1.5"
          data-testid={`${testIdPrefix}-tool-location-${itemId}`}
        >
          <div className="flex min-w-0 items-center gap-1.5 font-mono text-caption text-on-surface">
            <MapPin size={13} className="shrink-0 text-primary" />
            <span className="truncate" title={locator}>
              {locator}
            </span>
          </div>
          <button
            aria-label="Copy location"
            className="rounded p-1 text-outline transition-colors hover:text-on-surface"
            onClick={() => copyToClipboard(locator, copyKey)}
            type="button"
          >
            {copiedSection === copyKey ? (
              <Check size={13} className="text-status-create" />
            ) : (
              <Copy size={13} />
            )}
          </button>
        </div>
      );
    }

    case "image": {
      const displayPath = abbreviateHomePath(block.path);
      const copyKey = `img-${itemId}-${index}`;

      return (
        <div
          className="grid gap-2 rounded-lg border border-theme-control-border/60 bg-theme-control/20 p-2.5"
          data-testid={`${testIdPrefix}-tool-image-${itemId}`}
        >
          <div className="flex items-center justify-between gap-2">
            <div className="flex min-w-0 items-center gap-1.5 text-label-caps font-mono font-medium text-on-surface">
              <ImageIcon size={13} className="shrink-0 text-primary" />
              <span className="truncate">{displayPath}</span>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {block.mimeType ? (
                <span className="rounded bg-theme-panel/70 px-1.5 py-0.5 text-caption font-mono text-outline">
                  {block.mimeType}
                </span>
              ) : null}
              <button
                aria-label="Copy image path"
                className="rounded p-1 text-outline transition-colors hover:text-on-surface"
                onClick={() => copyToClipboard(block.path, copyKey)}
                type="button"
              >
                {copiedSection === copyKey ? (
                  <Check size={13} className="text-status-create" />
                ) : (
                  <Copy size={13} />
                )}
              </button>
            </div>
          </div>
          <div className="flex justify-center overflow-hidden rounded bg-theme-panel/50 p-2">
            <img
              alt={block.alt || "Generated tool image"}
              className="max-h-80 max-w-full rounded object-contain border border-theme-card-border/40"
              src={block.path}
            />
          </div>
        </div>
      );
    }

    case "artifact": {
      const copyKey = `art-${itemId}-${index}`;

      return (
        <div
          className="flex items-center justify-between gap-2 rounded-lg border border-theme-control-border/60 bg-theme-control/25 p-2.5"
          data-testid={`${testIdPrefix}-tool-artifact-${itemId}`}
        >
          <div className="flex min-w-0 items-center gap-2">
            <Package size={15} className="shrink-0 text-primary" />
            <div className="min-w-0">
              <div className="truncate font-semibold text-body-sm text-on-surface">
                {block.title || block.artifactId}
              </div>
              <div className="flex items-center gap-2 font-mono text-caption text-outline">
                <span>{block.artifactId}</span>
                <span className="rounded bg-theme-panel/70 px-1 py-0.5">
                  {block.renderer}
                </span>
              </div>
            </div>
          </div>
          <button
            aria-label="Copy artifact id"
            className="rounded p-1 text-outline transition-colors hover:text-on-surface"
            onClick={() => copyToClipboard(block.artifactId, copyKey)}
            type="button"
          >
            {copiedSection === copyKey ? (
              <Check size={13} className="text-status-create" />
            ) : (
              <Copy size={13} />
            )}
          </button>
        </div>
      );
    }

    case "unknown": {
      return (
        <div
          className="grid max-h-64 gap-1.5 overflow-auto rounded-lg border border-status-warning/40 bg-status-warning/5 p-2.5 text-caption"
          data-testid={`${testIdPrefix}-tool-unknown-${itemId}`}
        >
          <div className="flex items-center gap-1.5 font-semibold text-status-warning">
            <AlertCircle size={13} />
            <span>Unknown Content ({block.providerType})</span>
          </div>
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap break-all rounded bg-theme-panel/70 p-2 font-mono text-on-surface">
            {block.display}
          </pre>
        </div>
      );
    }

    default:
      return null;
  }
}

function buildUnifiedDiff(
  path: string,
  oldText: string | null,
  newText: string | null,
): string {
  const normPath = path || "diff";
  const oldHeader = oldText !== null ? `--- a/${normPath}` : "--- /dev/null";
  const newHeader = newText !== null ? `+++ b/${normPath}` : "+++ /dev/null";

  if (oldText === null && newText !== null) {
    const lines = newText.split("\n");
    return `${oldHeader}\n${newHeader}\n@@ -0,0 +1,${lines.length} @@\n${lines.map((l) => `+${l}`).join("\n")}`;
  }
  if (oldText !== null && newText === null) {
    const lines = oldText.split("\n");
    return `${oldHeader}\n${newHeader}\n@@ -1,${lines.length} +0,0 @@\n${lines.map((l) => `-${l}`).join("\n")}`;
  }
  if (oldText !== null && newText !== null) {
    const oldLines = oldText.split("\n");
    const newLines = newText.split("\n");
    return `${oldHeader}\n${newHeader}\n@@ -1,${oldLines.length} +1,${newLines.length} @@\n${oldLines.map((l) => `-${l}`).join("\n")}\n${newLines.map((l) => `+${l}`).join("\n")}`;
  }
  return `${oldHeader}\n${newHeader}\n`;
}

function extractToolBlocks(payload: unknown): ToolContentBlock[] {
  if (!payload) return [];

  if (Array.isArray(payload)) {
    return payload
      .flatMap((item) => extractToolBlocks(item))
      .filter((b): b is ToolContentBlock => b !== null);
  }

  if (typeof payload !== "object") return [];
  const record = payload as Record<string, unknown>;

  // Check type tag
  if (typeof record.type === "string") {
    switch (record.type) {
      case "diff":
        return [
          {
            type: "diff",
            path: String(record.path || record.file_name || "diff"),
            oldText:
              typeof record.oldText === "string"
                ? record.oldText
                : typeof record.old_text === "string"
                  ? record.old_text
                  : null,
            newText:
              typeof record.newText === "string"
                ? record.newText
                : typeof record.new_text === "string"
                  ? record.new_text
                  : null,
            unifiedDiff:
              typeof record.unifiedDiff === "string"
                ? record.unifiedDiff
                : typeof record.unified_diff === "string"
                  ? record.unified_diff
                  : typeof record.diff === "string"
                    ? record.diff
                    : null,
            isTruncated: Boolean(record.isTruncated || record.truncated),
          },
        ];

      case "location":
        return [
          {
            type: "location",
            path: String(record.path || ""),
            line: typeof record.line === "number" ? record.line : null,
            column: typeof record.column === "number" ? record.column : null,
          },
        ];

      case "image":
        return [
          {
            type: "image",
            path: String(record.path || record.url || record.data || ""),
            mimeType:
              typeof record.mimeType === "string"
                ? record.mimeType
                : typeof record.mime_type === "string"
                  ? record.mime_type
                  : null,
            alt: typeof record.alt === "string" ? record.alt : null,
          },
        ];

      case "artifact":
        return [
          {
            type: "artifact",
            artifactId: String(
              record.artifactId || record.artifact_id || record.id || "",
            ),
            renderer: String(record.renderer || "plain"),
            title: typeof record.title === "string" ? record.title : null,
          },
        ];

      case "unknown":
        return [
          {
            type: "unknown",
            providerType: String(
              record.providerType || record.provider_type || "unknown",
            ),
            display:
              typeof record.display === "string"
                ? record.display
                : formatPayload(record.display ?? record),
          },
        ];

      case "command":
        return [
          {
            type: "command",
            command: String(record.command || record.cmd || ""),
            cwd: typeof record.cwd === "string" ? record.cwd : null,
          },
        ];

      case "terminal":
        return [
          {
            type: "terminal",
            stdout: stripAnsi(String(record.stdout || "")),
            stderr: stripAnsi(String(record.stderr || "")),
            ansiStripped: true,
          },
        ];

      case "text":
        return [
          {
            type: "text",
            text: String(record.text || ""),
            language:
              typeof record.language === "string" ? record.language : null,
          },
        ];

      case "json":
        return [
          {
            type: "json",
            value: record.value,
            formatted: formatPayload(record.value ?? record),
          },
        ];

      default:
        // Unsupported provider block
        return [
          {
            type: "unknown",
            providerType: record.type,
            display: formatPayload(record),
          },
        ];
    }
  }

  // Check ACP / legacy shapes without type tag
  if (
    typeof record.file_diff === "string" ||
    (typeof record.diff === "string" &&
      (record.diff.startsWith("---") ||
        record.diff.startsWith("diff --git") ||
        record.file_name ||
        record.path))
  ) {
    return [
      {
        type: "diff",
        path: String(record.file_name || record.path || "diff"),
        oldText: null,
        newText: null,
        unifiedDiff: String(record.file_diff || record.diff),
        isTruncated: Boolean(record.isTruncated || record.truncated),
      },
    ];
  }

  if (
    typeof record.path === "string" &&
    typeof record.line === "number" &&
    !record.cmd &&
    !record.command
  ) {
    return [
      {
        type: "location",
        path: record.path,
        line: record.line,
        column: typeof record.column === "number" ? record.column : null,
      },
    ];
  }

  if (Array.isArray(record.content)) {
    return extractToolBlocks(record.content);
  }

  return [];
}

function formatPayload(value: unknown): string {
  if (typeof value === "string") return stripAnsi(value);
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function extractCommandInfo(
  input: unknown,
  inputBlocks: ToolContentBlock[],
): { command: string; cwd?: string } | null {
  const blockCmd = inputBlocks.find((b) => b.type === "command");
  if (blockCmd && blockCmd.type === "command" && blockCmd.command) {
    return {
      command: blockCmd.command,
      cwd: blockCmd.cwd ?? undefined,
    };
  }

  if (!input || typeof input !== "object") return null;
  const record = input as Record<string, unknown>;
  const cmd = record.cmd ?? record.command;
  if (typeof cmd === "string" && cmd.trim()) {
    return {
      command: cmd.trim(),
      cwd: typeof record.cwd === "string" ? record.cwd : undefined,
    };
  }
  return null;
}

function extractTerminalInfo(
  output: unknown,
  outputBlocks: ToolContentBlock[],
): {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  signal: string | null;
} | null {
  const blockTerminal = outputBlocks.find((b) => b.type === "terminal");
  if (blockTerminal && blockTerminal.type === "terminal") {
    return {
      stdout: blockTerminal.stdout,
      stderr: blockTerminal.stderr,
      exitCode: null,
      signal: null,
    };
  }

  if (!output || typeof output !== "object") return null;
  const record = output as Record<string, unknown>;
  const hasStdout = "stdout" in record && typeof record.stdout === "string";
  const hasStderr = "stderr" in record && typeof record.stderr === "string";
  const hasExit = "exitCode" in record || "exit_code" in record;
  const hasSignal = "signal" in record;

  if (!hasStdout && !hasStderr && !hasExit && !hasSignal) {
    return null;
  }

  const exitCode =
    typeof record.exitCode === "number"
      ? record.exitCode
      : typeof record.exit_code === "number"
        ? record.exit_code
        : null;

  const signal = typeof record.signal === "string" ? record.signal : null;

  return {
    stdout: stripAnsi(hasStdout ? (record.stdout as string) : ""),
    stderr: stripAnsi(hasStderr ? (record.stderr as string) : ""),
    exitCode,
    signal,
  };
}
