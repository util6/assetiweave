import { Check, Copy, Terminal as TerminalIcon } from "lucide-react";
import { useState } from "react";
import { useI18n } from "../../i18n/I18nProvider";
import type { AgentSessionItemView } from "../../types/agentSession";
import { stripAnsi } from "../../utils/stripAnsi";

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

  // 1. Detect command & cwd
  const commandInfo = extractCommandInfo(toolInput);

  // 2. Detect terminal stdout / stderr / exit
  const terminalInfo = extractTerminalInfo(toolOutput);

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

      {/* Command block or Generic Input */}
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
              cwd: {commandInfo.cwd}
            </div>
          ) : null}
        </div>
      ) : toolInput !== undefined && toolInput !== null ? (
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

      {/* Terminal Output or Generic Output */}
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
      ) : toolOutput !== undefined && toolOutput !== null ? (
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
): { command: string; cwd?: string } | null {
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

function extractTerminalInfo(output: unknown): {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  signal: string | null;
} | null {
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
