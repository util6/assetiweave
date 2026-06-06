import { useMemo, type ReactNode } from "react";

type MarkdownBlock =
  | { type: "heading"; level: number; text: string }
  | { type: "paragraph"; text: string }
  | { type: "list"; items: string[] }
  | { type: "quote"; text: string }
  | { type: "code"; language: string | null; text: string };

export function MarkdownContent({ value }: { value: string }) {
  const blocks = useMemo(() => parseMarkdownBlocks(value), [value]);
  if (blocks.length === 0) {
    return <p className="text-body-sm text-on-surface-muted">{value.trim() ? value : ""}</p>;
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
          return (
            <pre className="overflow-auto rounded-lg bg-theme-control p-3 text-code-sm text-on-surface" key={index}>
              <code>{block.text}</code>
            </pre>
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

  for (const line of lines) {
    const fence = line.match(/^```(\w+)?\s*$/);
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

  return blocks.filter((block) => ("text" in block ? block.text.trim() : block.items.length > 0));
}

function renderInlineMarkdown(text: string) {
  const parts: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*)/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = pattern.exec(text))) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    const token = match[0];
    if (token.startsWith("`")) {
      parts.push(
        <code className="rounded bg-theme-control px-1 py-0.5 text-code-sm text-primary" key={`${match.index}-code`}>
          {token.slice(1, -1)}
        </code>,
      );
    } else {
      parts.push(
        <strong className="font-semibold text-on-surface" key={`${match.index}-strong`}>
          {token.slice(2, -2)}
        </strong>,
      );
    }
    lastIndex = match.index + token.length;
  }
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }
  return parts;
}
