import { useEffect, useId, useMemo, useState, type ReactNode } from "react";

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; items: string[] }
  | { type: "quote"; text: string }
  | { type: "code"; language: string | null; text: string }
  | { type: "table"; headers: string[]; rows: string[][] };

export function MarkdownContent({ value }: { value: string }) {
  const normalizedValue = useMemo(() => normalizeMarkdownSource(value), [value]);
  const blocks = useMemo(() => parseMarkdownBlocks(normalizedValue), [normalizedValue]);
  if (blocks.length === 0) {
    return <p className="text-body-sm text-on-surface-muted">{normalizedValue.trim() ? normalizedValue : ""}</p>;
  }

  return (
    <div className="space-y-3 text-body-sm leading-6 text-on-surface">
      {blocks.map((block, index) => {
        if (block.type === "heading") {
          return renderMarkdownHeading(block, index);
        }
        if (block.type === "list") {
          return (
            <ul className="list-disc space-y-1 pl-5" key={index}>
              {block.items.map((item, itemIndex) => (
                <li key={itemIndex}>{renderInlineMarkdown(item)}</li>
              ))}
            </ul>
          );
        }
        if (block.type === "quote") {
          return (
            <blockquote className="border-l-2 border-primary/60 pl-3 text-on-surface-variant" key={index}>
              {renderInlineMarkdown(block.text)}
            </blockquote>
          );
        }
        if (block.type === "code") {
          if (isMermaidLanguage(block.language)) {
            return <MermaidDiagram key={index} value={block.text} />;
          }
          return (
            <pre className="overflow-auto rounded-lg bg-theme-control p-3 text-code-sm text-on-surface" key={index}>
              <code>{block.text}</code>
            </pre>
          );
        }
        if (block.type === "table") {
          return (
            <div className="overflow-auto rounded-lg border border-theme-card-border bg-theme-card/70" key={index}>
              <table className="min-w-full border-collapse text-left text-body-sm">
                <thead className="bg-theme-control/80 text-label-caps text-on-surface-variant">
                  <tr>
                    {block.headers.map((header, headerIndex) => (
                      <th className="border-b border-theme-card-border px-3 py-2 font-semibold" key={headerIndex}>
                        {renderInlineMarkdown(header)}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="divide-y divide-theme-card-border">
                  {block.rows.map((row, rowIndex) => (
                    <tr key={rowIndex}>
                      {row.map((cell, cellIndex) => (
                        <td className="px-3 py-2 align-top text-on-surface" key={cellIndex}>
                          {renderInlineMarkdown(cell)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          );
        }
        return <p key={index}>{renderInlineMarkdown(block.text)}</p>;
      })}
    </div>
  );
}

function renderMarkdownHeading(block: Extract<MarkdownBlock, { type: "heading" }>, key: number) {
  const content = renderInlineMarkdown(block.text);
  if (block.level <= 1) {
    return (
      <h3 className="text-title-sm text-on-surface" key={key}>
        {content}
      </h3>
    );
  }
  if (block.level === 2) {
    return (
      <h4 className="text-body-sm font-semibold text-on-surface" key={key}>
        {content}
      </h4>
    );
  }
  if (block.level === 3) {
    return (
      <h5 className="text-body-sm font-semibold text-on-surface" key={key}>
        {content}
      </h5>
    );
  }
  return (
    <h6 className="text-label-caps text-on-surface-muted" key={key}>
      {content}
    </h6>
  );
}

function parseMarkdownBlocks(value: string): MarkdownBlock[] {
  const lines = value.replace(/\r\n/g, "\n").split("\n");
  const blocks: MarkdownBlock[] = [];
  let paragraph: string[] = [];
  let list: string[] = [];
  let codeLanguage: string | null = null;
  let codeLines: string[] = [];

  function flushParagraph() {
    if (paragraph.length > 0) {
      blocks.push({ type: "paragraph", text: paragraph.join(" ").trim() });
      paragraph = [];
    }
  }

  function flushList() {
    if (list.length > 0) {
      blocks.push({ type: "list", items: list });
      list = [];
    }
  }

  for (let lineIndex = 0; lineIndex < lines.length; lineIndex += 1) {
    const line = lines[lineIndex];
    const fence = line.match(/^```([^\s`]*)\s*$/);
    if (fence) {
      if (codeLanguage !== null) {
        blocks.push({ type: "code", language: codeLanguage, text: codeLines.join("\n") });
        codeLanguage = null;
        codeLines = [];
      } else {
        flushParagraph();
        flushList();
        codeLanguage = fence[1] ?? "";
      }
      continue;
    }

    if (codeLanguage !== null) {
      codeLines.push(line);
      continue;
    }

    const table = readMarkdownTable(lines, lineIndex);
    if (table) {
      flushParagraph();
      flushList();
      blocks.push(table.block);
      lineIndex = table.nextIndex - 1;
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      continue;
    }

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      blocks.push({ type: "heading", level: heading[1].length, text: heading[2].trim() });
      continue;
    }

    const listItem = line.match(/^\s*[-*]\s+(.+)$/);
    if (listItem) {
      flushParagraph();
      list.push(listItem[1].trim());
      continue;
    }

    const quote = line.match(/^>\s?(.+)$/);
    if (quote) {
      flushParagraph();
      flushList();
      blocks.push({ type: "quote", text: quote[1].trim() });
      continue;
    }

    paragraph.push(line.trim());
  }

  flushParagraph();
  flushList();
  if (codeLanguage !== null) {
    blocks.push({ type: "code", language: codeLanguage, text: codeLines.join("\n") });
  }

  return blocks.filter((block) => {
    if (block.type === "table") return block.headers.length > 0;
    return "text" in block ? block.text.trim() : block.items.length > 0;
  });
}

function renderInlineMarkdown(text: string) {
  const normalizedText = unescapeMarkdownPunctuation(text);
  const parts: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*|\[[^\]\n]+\]\(https?:\/\/[^)\s]+\))/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(normalizedText))) {
    if (match.index > lastIndex) {
      parts.push(normalizedText.slice(lastIndex, match.index));
    }
    const token = match[0];
    if (token.startsWith("`")) {
      parts.push(
        <code className="rounded bg-theme-control px-1 py-0.5 text-code-sm text-primary" key={`${match.index}-code`}>
          {token.slice(1, -1)}
        </code>,
      );
    } else if (token.startsWith("[")) {
      const link = token.match(/^\[([^\]\n]+)\]\((https?:\/\/[^)\s]+)\)$/);
      if (link) {
        parts.push(
          <a
            className="font-medium text-primary underline underline-offset-2 hover:text-primary-strong"
            href={link[2]}
            key={`${match.index}-link`}
            rel="noreferrer"
            target="_blank"
          >
            {link[1]}
          </a>,
        );
      } else {
        parts.push(token);
      }
    } else {
      parts.push(
        <strong className="font-semibold text-on-surface" key={`${match.index}-strong`}>
          {token.slice(2, -2)}
        </strong>,
      );
    }
    lastIndex = match.index + token.length;
  }
  if (lastIndex < normalizedText.length) {
    parts.push(normalizedText.slice(lastIndex));
  }
  return parts;
}

function normalizeMarkdownSource(value: string) {
  let normalized = value.replace(/\r\n/g, "\n");
  if (shouldDecodeEscapedLineBreaks(normalized)) {
    normalized = normalized
      .replace(/\\r\\n/g, "\n")
      .replace(/\\n/g, "\n")
      .replace(/\\t/g, "  ");
  }
  return normalized.replace(/\]\s*\n\s*\(/g, "](");
}

function shouldDecodeEscapedLineBreaks(value: string) {
  const escapedLineBreakCount = value.match(/\\n/g)?.length ?? 0;
  return escapedLineBreakCount >= 2 || /\\n\s*(?:[#>|*-]|\|)/.test(value) || /\|\s*\\n\s*\|/.test(value);
}

function unescapeMarkdownPunctuation(text: string) {
  return text.replace(/\\([\\`*_{}\[\]()#+\-.!|>])/g, "$1");
}

function isMermaidLanguage(language: string | null) {
  return language?.trim().toLowerCase() === "mermaid";
}

function readMarkdownTable(
  lines: string[],
  startIndex: number,
): { block: Extract<MarkdownBlock, { type: "table" }>; nextIndex: number } | null {
  const headers = splitTableRow(lines[startIndex]);
  if (headers.length < 2) return null;

  const nextLine = lines[startIndex + 1];
  if (!nextLine) return null;

  const hasSeparator = isTableSeparatorRow(nextLine, headers.length);
  if (!hasSeparator && splitTableRow(nextLine).length < 2) return null;

  const rows: string[][] = [];
  let nextIndex = hasSeparator ? startIndex + 2 : startIndex + 1;
  while (nextIndex < lines.length && isMarkdownTableRow(lines[nextIndex])) {
    if (!isTableSeparatorRow(lines[nextIndex])) {
      rows.push(normalizeTableCells(splitTableRow(lines[nextIndex]), headers.length));
    }
    nextIndex += 1;
  }

  if (rows.length === 0 && !hasSeparator) return null;
  return { block: { type: "table", headers, rows }, nextIndex };
}

function isMarkdownTableRow(line: string) {
  return splitTableRow(line).length > 1;
}

function splitTableRow(line: string) {
  const trimmed = line.trim();
  if (!trimmed.includes("|")) return [];
  const start = trimmed.startsWith("|") ? 1 : 0;
  const end = trimmed.endsWith("|") && !isEscapedAt(trimmed, trimmed.length - 1)
    ? trimmed.length - 1
    : trimmed.length;
  const content = trimmed.slice(start, end);
  const cells: string[] = [];
  let cell = "";
  for (let index = 0; index < content.length; index += 1) {
    const char = content[index];
    if (char === "|" && !isEscapedAt(content, index)) {
      cells.push(cell.trim());
      cell = "";
    } else {
      cell += char;
    }
  }
  cells.push(cell.trim());
  return cells;
}

function isTableSeparatorRow(line: string, expectedCellCount?: number) {
  const cells = splitTableRow(line);
  if (expectedCellCount != null && cells.length !== expectedCellCount) return false;
  return cells.length > 1 && cells.every((cell) => /^:?-{3,}:?$/.test(cell));
}

function normalizeTableCells(cells: string[], cellCount: number) {
  if (cells.length === cellCount) return cells;
  return Array.from({ length: cellCount }, (_, index) => cells[index] ?? "");
}

function isEscapedAt(value: string, index: number) {
  let slashCount = 0;
  for (let cursor = index - 1; cursor >= 0 && value[cursor] === "\\"; cursor -= 1) {
    slashCount += 1;
  }
  return slashCount % 2 === 1;
}

function MermaidDiagram({ value }: { value: string }) {
  const reactId = useId();
  const diagramId = useMemo(
    () => `conversation-mermaid-${reactId.replace(/[^a-zA-Z0-9_-]/g, "")}`,
    [reactId],
  );
  const [svg, setSvg] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function renderDiagram() {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          fontFamily: "inherit",
          securityLevel: "strict",
          startOnLoad: false,
          theme: "base",
        });
        const rendered = await mermaid.render(diagramId, value);
        if (!cancelled) {
          setSvg(rendered.svg);
        }
      } catch {
        if (!cancelled) {
          setSvg(null);
        }
      }
    }

    void renderDiagram();
    return () => {
      cancelled = true;
    };
  }, [diagramId, value]);

  if (svg) {
    return (
      <div
        className="overflow-auto rounded-lg border border-theme-card-border bg-theme-card/75 p-3 [&_svg]:mx-auto [&_svg]:h-auto [&_svg]:max-w-full"
        data-mermaid-diagram="true"
        dangerouslySetInnerHTML={{ __html: svg }}
      />
    );
  }

  return (
    <pre
      className="overflow-auto rounded-lg border border-theme-card-border bg-theme-control p-3 text-code-sm text-on-surface"
      data-mermaid-diagram="true"
    >
      <code>{value}</code>
    </pre>
  );
}
