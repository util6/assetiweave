import { useMemo } from "react";
import { Diff, Hunk, parseDiff, type FileData } from "react-diff-view";
import "react-diff-view/style/index.css";

export function ConversationDiff({
  value,
  summary,
}: {
  value: string;
  summary?: ConversationDiffSummary;
}) {
  const parsed = useMemo(() => parseConversationDiff(value), [value]);

  if (!parsed.files.length) {
    return <PlainDiffFallback value={value} />;
  }

  return (
    <div className="grid gap-3" data-conversation-diff="unified">
      {parsed.files.map((file, index) => {
        const fileSummary = summary?.files[index];
        const additions = fileSummary?.additions ?? countChanges(file, "insert");
        const deletions = fileSummary?.deletions ?? countChanges(file, "delete");
        return (
          <section
          className="overflow-hidden rounded-xl border border-theme-card-border bg-theme-control/55"
          data-diff-file={displayPath(file)}
          key={`${displayPath(file)}-${index}`}
        >
          <header className="flex min-h-9 flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-theme-card-border bg-theme-card-header/80 px-3 py-2">
            <span className="min-w-0 truncate font-mono text-code-sm font-semibold text-on-surface" title={displayPath(file)}>
              {displayPath(file)}
            </span>
            <span className="flex shrink-0 items-center gap-2 font-mono text-code-sm" aria-label={`${additions} additions, ${deletions} deletions`}>
              <span className="text-status-create">+{additions}</span>
              <span className="text-status-remove">-{deletions}</span>
            </span>
          </header>
          {file.isBinary || file.hunks.length === 0 ? (
            <PlainDiffFallback value={file.raw} />
          ) : (
            <div className="conversation-diff-view overflow-x-auto p-2">
              <Diff
                className="min-w-[42rem] font-mono text-code-sm"
                diffType={file.type}
                gutterType="default"
                hunks={file.hunks}
                viewType="unified"
              >
                {(hunks) => hunks.map((hunk) => <Hunk hunk={hunk} key={hunk.content} />)}
              </Diff>
            </div>
          )}
          </section>
        );
      })}
    </div>
  );
}

interface ParsedDiffFile extends FileData {
  raw: string;
}

export interface ConversationDiffFileSummary {
  path: string;
  additions: number;
  deletions: number;
  status: "added" | "deleted" | "modified" | "renamed";
  binary: boolean;
}

export interface ConversationDiffSummary {
  files: ConversationDiffFileSummary[];
  additions: number;
  deletions: number;
}

export function summarizeConversationDiff(value: string): ConversationDiffSummary {
  const parsed = parseConversationDiff(value);
  const files = parsed.files.map((file) => ({
    path: displayPath(file),
    additions: countChanges(file, "insert"),
    deletions: countChanges(file, "delete"),
    status: diffStatus(file),
    binary: /^(?:Binary files|GIT binary patch)/m.test(file.raw),
  }));

  return {
    files,
    additions: files.reduce((total, file) => total + file.additions, 0),
    deletions: files.reduce((total, file) => total + file.deletions, 0),
  };
}

function parseConversationDiff(value: string): { files: ParsedDiffFile[] } {
  const normalized = value.replace(/\r\n?/g, "\n").trimEnd();
  try {
    const parserSource = prepareDiffForParser(normalized);
    const files = parseDiff(parserSource, { nearbySequences: "zip" });
    if (files.length > 0) {
      return {
        files: files.map((file, index) => ({
          ...file,
          raw: rawFileForIndex(parserSource, index, files.length),
        })),
      };
    }
  } catch {
    // Keep the original payload visible when a source emits an invalid diff.
  }
  return { files: [] };
}

function prepareDiffForParser(value: string) {
  const lines = value.split("\n");
  if (lines[0]?.startsWith("@@")) {
    return ["diff --git a/patch b/patch", "--- a/patch", "+++ b/patch", ...lines].join("\n");
  }
  const prepared: string[] = [];
  for (let index = 0; index < lines.length; index += 1) {
    const oldHeader = lines[index];
    if (oldHeader == null) break;
    const newHeader = lines[index + 1];
    if (oldHeader?.startsWith("--- ") && newHeader?.startsWith("+++ ") && !lines[index - 1]?.startsWith("diff --git ")) {
      const oldPath = oldHeader.slice(4).split("\t", 1)[0];
      const newPath = newHeader.slice(4).split("\t", 1)[0];
      prepared.push(`diff --git ${oldPath} ${newPath}`);
    }
    prepared.push(oldHeader);
  }
  return prepared.join("\n");
}

function rawFileForIndex(value: string, index: number, fileCount: number) {
  const starts = [...value.matchAll(/^diff --git .*$/gm)].map((match) => match.index ?? 0);
  if (starts.length !== fileCount || starts[index] == null) return value;
  return value.slice(starts[index], starts[index + 1] ?? value.length);
}

function displayPath(file: FileData) {
  const oldPath = visiblePath(file.oldPath);
  const newPath = visiblePath(file.newPath);
  if (oldPath && newPath && oldPath !== newPath) return `${oldPath} → ${newPath}`;
  return newPath ?? oldPath ?? "diff";
}

function diffStatus(file: FileData): ConversationDiffFileSummary["status"] {
  if (file.type === "add") return "added";
  if (file.type === "delete") return "deleted";
  const oldPath = visiblePath(file.oldPath);
  const newPath = visiblePath(file.newPath);
  return oldPath && newPath && oldPath !== newPath ? "renamed" : "modified";
}

function visiblePath(value?: string | null) {
  return value && value !== "/dev/null" ? value : null;
}

function countChanges(file: FileData, type: "insert" | "delete") {
  return file.hunks.reduce(
    (total, hunk) => total + hunk.changes.filter((change) => change.type === type).length,
    0,
  );
}

function PlainDiffFallback({ value }: { value: string }) {
  return (
    <pre className="max-h-[38rem] overflow-auto whitespace-pre-wrap break-words p-3 font-mono text-code-sm leading-6 text-on-surface" data-diff-fallback="plain">
      <code>{value}</code>
    </pre>
  );
}
