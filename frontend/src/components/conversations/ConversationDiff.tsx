import parseDiff from "parse-diff";
import { useMemo } from "react";

type ConversationDiffLineType = "addition" | "context" | "deletion" | "metadata";

interface ConversationDiffLine {
  content: string;
  marker: "+" | " " | "-";
  newLineNumber: number | null;
  oldLineNumber: number | null;
  type: ConversationDiffLineType;
}

interface ConversationDiffHunk {
  header: string;
  lines: ConversationDiffLine[];
}

interface ConversationDiffFile {
  additions: number;
  deletions: number;
  displayPath: string;
  hunks: ConversationDiffHunk[];
  newPath: string | null;
  oldPath: string | null;
  primaryPath: string;
}

export function ConversationDiff({ value }: { value: string }) {
  const files = useMemo(() => parseConversationDiff(value), [value]);

  return (
    <div className="grid gap-3" data-conversation-diff="unified">
      {files.map((file, fileIndex) => (
        <section
          className="overflow-hidden rounded-lg border border-theme-card-border bg-theme-control/55"
          data-diff-file={file.primaryPath}
          data-diff-new-file={file.newPath ?? undefined}
          data-diff-old-file={file.oldPath ?? undefined}
          key={`${file.primaryPath}-${fileIndex}`}
        >
          <header className="flex min-h-9 flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-theme-card-border bg-theme-card-header/80 px-3 py-2">
            <span className="min-w-0 truncate font-mono text-code-sm font-semibold text-on-surface" title={file.displayPath}>
              {file.displayPath}
            </span>
            <span className="flex shrink-0 items-center gap-2 font-mono text-code-sm" aria-label={`${file.additions} additions, ${file.deletions} deletions`}>
              <span className="text-status-create">+{file.additions}</span>
              <span className="text-status-remove">-{file.deletions}</span>
            </span>
          </header>
          <div className="overflow-x-auto">
            <table className="w-max min-w-full border-collapse font-mono text-code-sm leading-6">
              <colgroup>
                <col className="w-12" />
                <col className="w-12" />
                <col />
              </colgroup>
              <tbody>
                {file.hunks.flatMap((hunk, hunkIndex) => [
                  <tr className="bg-status-update/10 text-status-update" data-diff-line-type="hunk" key={`hunk-${hunkIndex}`}>
                    <td className="border-b border-theme-card-border/70 px-3 py-1 font-medium" colSpan={3}>
                      <span className="whitespace-pre">{hunk.header}</span>
                    </td>
                  </tr>,
                  ...hunk.lines.map((line, lineIndex) => (
                    <DiffLine key={`${hunkIndex}-${lineIndex}`} line={line} />
                  )),
                ])}
              </tbody>
            </table>
          </div>
        </section>
      ))}
    </div>
  );
}

function DiffLine({ line }: { line: ConversationDiffLine }) {
  const rowClass = line.type === "addition"
    ? "bg-status-create/10"
    : line.type === "deletion"
      ? "bg-status-remove/10"
      : line.type === "metadata"
        ? "bg-theme-card/45 text-on-surface-muted"
        : "text-on-surface";
  const markerClass = line.type === "addition"
    ? "text-status-create"
    : line.type === "deletion"
      ? "text-status-remove"
      : "text-on-surface-muted";

  return (
    <tr className={rowClass} data-diff-line-type={line.type}>
      <DiffLineNumber value={line.oldLineNumber} />
      <DiffLineNumber value={line.newLineNumber} />
      <td className="min-w-[32rem] border-b border-theme-card-border/45 px-3 align-top">
        <span aria-hidden="true" className={`inline-block w-4 select-none ${markerClass}`}>
          {line.marker}
        </span>
        <span className="whitespace-pre">{line.content}</span>
      </td>
    </tr>
  );
}

function DiffLineNumber({ value }: { value: number | null }) {
  return (
    <td
      aria-label={value == null ? undefined : `Line ${value}`}
      className="select-none border-b border-r border-theme-card-border/55 bg-theme-card/35 px-2 text-right align-top tabular-nums text-on-surface-muted"
    >
      {value ?? ""}
    </td>
  );
}

function parseConversationDiff(value: string): ConversationDiffFile[] {
  const normalizedValue = value.replace(/\r\n?/g, "\n").trimEnd();
  const parsedFiles = splitDiffSections(normalizedValue).flatMap((section) => parseDiff(section));
  const filesWithHunks = parsedFiles.filter((file) => file.chunks.length > 0);
  if (filesWithHunks.length > 0) {
    return filesWithHunks.map(normalizeDiffFile);
  }

  return [parseDiffFragment(normalizedValue)];
}

function splitDiffSections(value: string) {
  const lines = value.split("\n");
  const boundaries = lines.flatMap((line, index) => line.startsWith("diff ") ? [index] : []);
  if (boundaries.length < 2) return [value];

  return boundaries.map((boundary, index) => {
    const start = index === 0 ? 0 : boundary;
    const end = boundaries[index + 1] ?? lines.length;
    return lines.slice(start, end).join("\n");
  });
}

function normalizeDiffFile(file: parseDiff.File): ConversationDiffFile {
  const oldPath = visibleDiffPath(file.from);
  const newPath = visibleDiffPath(file.to);
  const primaryPath = newPath ?? oldPath ?? "diff";
  const displayPath = oldPath && newPath && oldPath !== newPath
    ? `${oldPath} -> ${newPath}`
    : primaryPath;

  return {
    additions: file.additions,
    deletions: file.deletions,
    displayPath,
    hunks: file.chunks.map((chunk) => ({
      header: chunk.content,
      lines: chunk.changes.map(normalizeDiffChange),
    })),
    newPath,
    oldPath,
    primaryPath,
  };
}

function normalizeDiffChange(change: parseDiff.Change): ConversationDiffLine {
  if (change.content === "\\ No newline at end of file") {
    return {
      content: change.content,
      marker: " ",
      newLineNumber: null,
      oldLineNumber: null,
      type: "metadata",
    };
  }
  if (change.type === "add") {
    return {
      content: change.content.slice(1),
      marker: "+",
      newLineNumber: change.ln,
      oldLineNumber: null,
      type: "addition",
    };
  }
  if (change.type === "del") {
    return {
      content: change.content.slice(1),
      marker: "-",
      newLineNumber: null,
      oldLineNumber: change.ln,
      type: "deletion",
    };
  }
  return {
    content: change.content.slice(1),
    marker: " ",
    newLineNumber: change.ln2,
    oldLineNumber: change.ln1,
    type: "context",
  };
}

function parseDiffFragment(value: string): ConversationDiffFile {
  const lines = value.split("\n");
  let additions = 0;
  let deletions = 0;
  let oldLineNumber = 1;
  let newLineNumber = 1;
  const normalizedLines = lines.map((line): ConversationDiffLine => {
    if (line.startsWith("+") && !line.startsWith("+++")) {
      additions += 1;
      const parsedLine = {
        content: line.slice(1),
        marker: "+" as const,
        newLineNumber,
        oldLineNumber: null,
        type: "addition" as const,
      };
      newLineNumber += 1;
      return parsedLine;
    }
    if (line.startsWith("-") && !line.startsWith("---")) {
      deletions += 1;
      const parsedLine = {
        content: line.slice(1),
        marker: "-" as const,
        newLineNumber: null,
        oldLineNumber,
        type: "deletion" as const,
      };
      oldLineNumber += 1;
      return parsedLine;
    }
    if (line === "\\ No newline at end of file") {
      return {
        content: line,
        marker: " ",
        newLineNumber: null,
        oldLineNumber: null,
        type: "metadata",
      };
    }
    const parsedLine = {
      content: line.startsWith(" ") ? line.slice(1) : line,
      marker: " " as const,
      newLineNumber,
      oldLineNumber,
      type: "context" as const,
    };
    oldLineNumber += 1;
    newLineNumber += 1;
    return parsedLine;
  });

  return {
    additions,
    deletions,
    displayPath: "diff",
    hunks: [{ header: "@@", lines: normalizedLines }],
    newPath: null,
    oldPath: null,
    primaryPath: "diff",
  };
}

function visibleDiffPath(path?: string) {
  return path && path !== "/dev/null" ? path : null;
}
