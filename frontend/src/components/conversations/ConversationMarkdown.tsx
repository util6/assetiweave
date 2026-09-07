import { useMemo } from "react";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import {
  CodeBlockRenderer,
  InlineCodeRenderer,
  rehypeLatexMathDataAttr,
} from "./ConversationMarkdownRenderers";

export function convertLatexDelimiters(source: string): string {
  const parts = source.split(/(```[\s\S]*?```)/g);
  return parts
    .map((part, index) => {
      // Keep code fences unchanged
      if (index % 2 === 1) return part;
      return part
        .replace(/\\\[([\s\S]*?)\\\]/g, (_, math) => `$$\n${math.trim()}\n$$`)
        .replace(/\\\(([\s\S]*?)\\\)/g, (_, math) => `$${math.trim()}$`);
    })
    .join("");
}

export function shouldDecodeEscapedLineBreaks(value: string): boolean {
  const escapedLineBreakCount = value.match(/\\n/g)?.length ?? 0;
  return (
    escapedLineBreakCount >= 2 ||
    /\\n\s*(?:[#>|*-]|\|)/.test(value) ||
    /\|\s*\\n\s*\|/.test(value)
  );
}

export function isTableLikeRow(line: string): boolean {
  const trimmed = line.trim();
  if (!trimmed.includes("|")) return false;
  const parts = trimmed.split("|");
  const cells = parts.filter((_, idx, arr) => {
    if (
      (idx === 0 && trimmed.startsWith("|")) ||
      (idx === arr.length - 1 && trimmed.endsWith("|"))
    ) {
      return false;
    }
    return true;
  });
  return cells.length >= 2;
}

export function countTableColumns(line: string): number {
  const trimmed = line.trim();
  const parts = trimmed.split("|");
  const cells = parts.filter((_, idx, arr) => {
    if (
      (idx === 0 && trimmed.startsWith("|")) ||
      (idx === arr.length - 1 && trimmed.endsWith("|"))
    ) {
      return false;
    }
    return true;
  });
  return cells.length;
}

export function isTableDividerRow(line: string): boolean {
  const trimmed = line.trim();
  if (!trimmed.includes("|")) return false;
  const parts = trimmed.split("|");
  const cells = parts.filter((_, idx, arr) => {
    if (
      (idx === 0 && trimmed.startsWith("|")) ||
      (idx === arr.length - 1 && trimmed.endsWith("|"))
    ) {
      return false;
    }
    return true;
  });
  return (
    cells.length >= 2 && cells.every((cell) => /^:?-{3,}:?$/.test(cell.trim()))
  );
}

export function repairMarkdownTables(content: string): string {
  const lines = content.split("\n");
  const result: string[] = [];
  let inTable = false;

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    const isRow = isTableLikeRow(line);

    if (!isRow) {
      inTable = false;
      result.push(line);
      continue;
    }

    if (!inTable) {
      inTable = true;
      result.push(line);

      const nextLine = lines[i + 1];
      if (nextLine && isTableLikeRow(nextLine)) {
        if (!isTableDividerRow(nextLine)) {
          const colCount = countTableColumns(line);
          if (colCount >= 2) {
            result.push(`| ${Array(colCount).fill("---").join(" | ")} |`);
          }
        }
      }
      continue;
    }

    result.push(line);
  }

  return result.join("\n");
}

export function normalizeMarkdownSource(value: string): string {
  let normalized = value.replace(/\r\n/g, "\n");
  if (shouldDecodeEscapedLineBreaks(normalized)) {
    normalized = normalized
      .replace(/\\r\\n/g, "\n")
      .replace(/\\n/g, "\n")
      .replace(/\\t/g, "  ");
  }
  normalized = normalized.replace(/\]\s*\n\s*\(/g, "](");
  normalized = repairMarkdownTables(normalized);
  normalized = convertLatexDelimiters(normalized);
  return normalized;
}

const markdownComponents = {
  a: ({ href, children, node, ...props }: any) => (
    <a
      className="font-medium text-primary underline underline-offset-2 hover:text-primary-strong"
      href={href}
      rel="noreferrer"
      target="_blank"
      {...props}
    >
      {children}
    </a>
  ),
  blockquote: ({ children, node, ...props }: any) => (
    <blockquote
      className="border-l-2 border-primary/60 pl-3 text-on-surface-variant"
      {...props}
    >
      {children}
    </blockquote>
  ),
  code: ({ children, node, ...props }: any) => {
    // If the parent is not <pre>, render as inline code
    const isParentPre = node?.parent?.tagName === "pre";
    if (!isParentPre) {
      return <InlineCodeRenderer {...props}>{children}</InlineCodeRenderer>;
    }
    return <code {...props}>{children}</code>;
  },
  h1: ({ children, node, ...props }: any) => (
    <h3 className="text-title-sm text-on-surface" {...props}>
      {children}
    </h3>
  ),
  h2: ({ children, node, ...props }: any) => (
    <h4 className="text-body-sm font-semibold text-on-surface" {...props}>
      {children}
    </h4>
  ),
  h3: ({ children, node, ...props }: any) => (
    <h5 className="text-body-sm font-semibold text-on-surface" {...props}>
      {children}
    </h5>
  ),
  h4: ({ children, node, ...props }: any) => (
    <h6 className="text-label-caps text-on-surface-muted" {...props}>
      {children}
    </h6>
  ),
  h5: ({ children, node, ...props }: any) => (
    <h6 className="text-label-caps text-on-surface-muted" {...props}>
      {children}
    </h6>
  ),
  h6: ({ children, node, ...props }: any) => (
    <h6 className="text-label-caps text-on-surface-muted" {...props}>
      {children}
    </h6>
  ),
  ol: ({ children, node, ...props }: any) => (
    <ol className="list-decimal space-y-1 pl-5" {...props}>
      {children}
    </ol>
  ),
  p: ({ children, node, ...props }: any) => <p {...props}>{children}</p>,
  pre: CodeBlockRenderer,
  span: ({ className, children, node, ...props }: any) => {
    if (
      props["data-latex-math"] === "display" ||
      (className && className.includes("katex-display"))
    ) {
      return (
        <div className="overflow-auto rounded-lg border border-theme-card-border bg-theme-card/60 px-3 py-2 text-on-surface [&_.katex-display]:my-0">
          <span className={className} {...props}>
            {children}
          </span>
        </div>
      );
    }
    return (
      <span className={className} {...props}>
        {children}
      </span>
    );
  },
  table: ({ children, node, ...props }: any) => (
    <div className="overflow-auto rounded-lg border border-theme-card-border bg-theme-card/70">
      <table
        className="min-w-full border-collapse text-left text-body-sm"
        {...props}
      >
        {children}
      </table>
    </div>
  ),
  tbody: ({ children, node, ...props }: any) => (
    <tbody className="divide-y divide-theme-card-border" {...props}>
      {children}
    </tbody>
  ),
  td: ({ children, node, ...props }: any) => (
    <td className="px-3 py-2 align-top text-on-surface" {...props}>
      {children}
    </td>
  ),
  th: ({ children, node, ...props }: any) => (
    <th
      className="border-b border-theme-card-border px-3 py-2 font-semibold"
      {...props}
    >
      {children}
    </th>
  ),
  thead: ({ children, node, ...props }: any) => (
    <thead
      className="bg-theme-control/80 text-label-caps text-on-surface-variant"
      {...props}
    >
      {children}
    </thead>
  ),
  ul: ({ children, node, ...props }: any) => (
    <ul className="list-disc space-y-1 pl-5" {...props}>
      {children}
    </ul>
  ),
};

const remarkPlugins = [remarkGfm, remarkMath];
const rehypePlugins: any[] = [
  [
    rehypeKatex,
    {
      displayMode: false,
      output: "htmlAndMathml",
      throwOnError: false,
      trust: false,
    },
  ],
  rehypeLatexMathDataAttr,
];

export function MarkdownContent({ value }: { value: string }) {
  const normalizedValue = useMemo(
    () => normalizeMarkdownSource(value),
    [value],
  );

  if (!normalizedValue.trim()) {
    return (
      <p className="text-body-sm text-on-surface-muted">
        {normalizedValue.trim() ? normalizedValue : ""}
      </p>
    );
  }

  return (
    <div className="space-y-3 text-body-sm leading-6 text-on-surface">
      <ReactMarkdown
        components={markdownComponents}
        rehypePlugins={rehypePlugins}
        remarkPlugins={remarkPlugins}
      >
        {normalizedValue}
      </ReactMarkdown>
    </div>
  );
}
