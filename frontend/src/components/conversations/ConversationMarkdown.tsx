import katex from "katex";
import { useEffect, useId, useMemo, useState } from "react";

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; items: string[] }
  | { type: "quote"; text: string }
  | { type: "code"; language: string | null; text: string }
  | { type: "math"; display: boolean; text: string }
  | { type: "table"; headers: string[]; rows: string[][] };

type InlineMarkdownToken =
  | { type: "text"; value: string }
  | { type: "code"; value: string }
  | { type: "strong"; value: string }
  | { type: "link"; label: string; href: string }
  | { type: "math"; value: string };

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
        if (block.type === "math") {
          return <LatexMath display={block.display} key={index} value={block.text} />;
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

    const mathBlock = readMarkdownMathBlock(lines, lineIndex);
    if (mathBlock) {
      flushParagraph();
      flushList();
      blocks.push(mathBlock.block);
      lineIndex = mathBlock.nextIndex - 1;
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
  return tokenizeInlineMarkdown(text).map((token, index) => {
    if (token.type === "text") {
      return unescapeMarkdownPunctuation(token.value);
    }
    if (token.type === "code") {
      return (
        <code className="rounded bg-theme-control px-1 py-0.5 text-code-sm text-primary" key={`${index}-code`}>
          {token.value}
        </code>
      );
    }
    if (token.type === "link") {
      return (
        <a
          className="font-medium text-primary underline underline-offset-2 hover:text-primary-strong"
          href={token.href}
          key={`${index}-link`}
          rel="noreferrer"
          target="_blank"
        >
          {unescapeMarkdownPunctuation(token.label)}
        </a>
      );
    }
    if (token.type === "math") {
      return <LatexMath display={false} key={`${index}-math`} value={token.value} />;
    }
    return (
      <strong className="font-semibold text-on-surface" key={`${index}-strong`}>
        {unescapeMarkdownPunctuation(token.value)}
      </strong>
    );
  });
}

function tokenizeInlineMarkdown(text: string) {
  const tokens: InlineMarkdownToken[] = [];
  let index = 0;

  function pushText(value: string) {
    if (!value) return;
    const previous = tokens[tokens.length - 1];
    if (previous?.type === "text") {
      previous.value += value;
      return;
    }
    tokens.push({ type: "text", value });
  }

  while (index < text.length) {
    if (text[index] === "`") {
      const endIndex = text.indexOf("`", index + 1);
      if (endIndex > index + 1) {
        tokens.push({ type: "code", value: text.slice(index + 1, endIndex) });
        index = endIndex + 1;
        continue;
      }
    }

    if (text.startsWith("**", index)) {
      const endIndex = text.indexOf("**", index + 2);
      if (endIndex > index + 2) {
        tokens.push({ type: "strong", value: text.slice(index + 2, endIndex) });
        index = endIndex + 2;
        continue;
      }
    }

    const link = readInlineMarkdownLink(text, index);
    if (link) {
      tokens.push(link.token);
      index = link.nextIndex;
      continue;
    }

    const parenMath = readParenInlineMath(text, index);
    if (parenMath) {
      tokens.push(parenMath.token);
      index = parenMath.nextIndex;
      continue;
    }

    const dollarMath = readDollarInlineMath(text, index);
    if (dollarMath) {
      tokens.push(dollarMath.token);
      index = dollarMath.nextIndex;
      continue;
    }

    pushText(text[index]);
    index += 1;
  }

  return tokens;
}

function readInlineMarkdownLink(
  text: string,
  startIndex: number,
): { token: Extract<InlineMarkdownToken, { type: "link" }>; nextIndex: number } | null {
  if (text[startIndex] !== "[") return null;

  const labelEndIndex = text.indexOf("]", startIndex + 1);
  if (labelEndIndex <= startIndex + 1 || text[labelEndIndex + 1] !== "(") return null;

  const hrefEndIndex = text.indexOf(")", labelEndIndex + 2);
  if (hrefEndIndex === -1) return null;

  const href = text.slice(labelEndIndex + 2, hrefEndIndex);
  if (!/^https?:\/\/[^)\s]+$/.test(href)) return null;

  return {
    nextIndex: hrefEndIndex + 1,
    token: {
      href,
      label: text.slice(startIndex + 1, labelEndIndex),
      type: "link",
    },
  };
}

function readParenInlineMath(
  text: string,
  startIndex: number,
): { token: Extract<InlineMarkdownToken, { type: "math" }>; nextIndex: number } | null {
  if (!text.startsWith("\\(", startIndex)) return null;

  const endIndex = text.indexOf("\\)", startIndex + 2);
  if (endIndex === -1) return null;

  const value = text.slice(startIndex + 2, endIndex).trim();
  if (!value) return null;

  return {
    nextIndex: endIndex + 2,
    token: { type: "math", value },
  };
}

function readDollarInlineMath(
  text: string,
  startIndex: number,
): { token: Extract<InlineMarkdownToken, { type: "math" }>; nextIndex: number } | null {
  if (text[startIndex] !== "$" || text[startIndex + 1] === "$" || isEscapedAt(text, startIndex)) {
    return null;
  }
  if (!text[startIndex + 1] || /\s/.test(text[startIndex + 1])) {
    return null;
  }

  for (let endIndex = startIndex + 1; endIndex < text.length; endIndex += 1) {
    if (text[endIndex] !== "$" || text[endIndex + 1] === "$" || isEscapedAt(text, endIndex)) {
      continue;
    }
    if (/\s/.test(text[endIndex - 1] ?? "")) {
      continue;
    }

    const value = text.slice(startIndex + 1, endIndex).trim();
    if (!value) return null;

    return {
      nextIndex: endIndex + 1,
      token: { type: "math", value },
    };
  }

  return null;
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

function readMarkdownMathBlock(
  lines: string[],
  startIndex: number,
): { block: Extract<MarkdownBlock, { type: "math" }>; nextIndex: number } | null {
  return readDelimitedMathBlock(lines, startIndex, "$$", "$$")
    ?? readDelimitedMathBlock(lines, startIndex, "\\[", "\\]");
}

function readDelimitedMathBlock(
  lines: string[],
  startIndex: number,
  openDelimiter: string,
  closeDelimiter: string,
): { block: Extract<MarkdownBlock, { type: "math" }>; nextIndex: number } | null {
  const firstLine = lines[startIndex].trim();
  if (!firstLine.startsWith(openDelimiter)) return null;

  const firstContent = firstLine.slice(openDelimiter.length);
  const sameLineCloseIndex = firstContent.indexOf(closeDelimiter);
  if (sameLineCloseIndex !== -1) {
    if (firstContent.slice(sameLineCloseIndex + closeDelimiter.length).trim()) return null;
    const text = firstContent.slice(0, sameLineCloseIndex).trim();
    if (!text) return null;
    return { block: { display: true, text, type: "math" }, nextIndex: startIndex + 1 };
  }

  const mathLines = firstContent.trim() ? [firstContent] : [];
  for (let lineIndex = startIndex + 1; lineIndex < lines.length; lineIndex += 1) {
    const line = lines[lineIndex];
    const closeIndex = line.indexOf(closeDelimiter);
    if (closeIndex === -1) {
      mathLines.push(line);
      continue;
    }

    if (line.slice(closeIndex + closeDelimiter.length).trim()) return null;
    const closingLineContent = line.slice(0, closeIndex);
    if (closingLineContent.trim()) {
      mathLines.push(closingLineContent);
    }

    const text = mathLines.join("\n").trim();
    if (!text) return null;
    return { block: { display: true, text, type: "math" }, nextIndex: lineIndex + 1 };
  }

  return null;
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

function LatexMath({ value, display }: { value: string; display: boolean }) {
  const html = useMemo(() => renderLatexMath(value, display), [display, value]);
  if (html) {
    if (display) {
      return (
        <div
          className="overflow-auto rounded-lg border border-theme-card-border bg-theme-card/60 px-3 py-2 text-on-surface [&_.katex-display]:my-0"
          data-latex-math="display"
          dangerouslySetInnerHTML={{ __html: html }}
        />
      );
    }

    return (
      <span
        className="align-baseline text-on-surface"
        data-latex-math="inline"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }

  if (display) {
    return (
      <pre
        className="overflow-auto rounded-lg border border-theme-card-border bg-theme-control p-3 text-code-sm text-on-surface"
        data-latex-math="display"
      >
        <code>{`$$\n${value}\n$$`}</code>
      </pre>
    );
  }

  return (
    <code
      className="rounded bg-theme-control px-1 py-0.5 text-code-sm text-primary"
      data-latex-math="inline"
    >
      {`\\(${value}\\)`}
    </code>
  );
}

function renderLatexMath(value: string, display: boolean) {
  try {
    return katex.renderToString(value, {
      displayMode: display,
      output: "htmlAndMathml",
      throwOnError: false,
      trust: false,
    });
  } catch {
    return null;
  }
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
