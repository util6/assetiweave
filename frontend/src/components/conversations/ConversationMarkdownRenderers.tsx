import React, { useEffect, useId, useMemo, useState } from "react";
import { ConversationDiff } from "./ConversationDiff";
import { isDiffLanguage } from "./conversationDiffLanguage";

export function isMermaidLanguage(
  language: string | null | undefined,
): boolean {
  return language?.trim().toLowerCase() === "mermaid";
}

export function extractCodeInfo(node: any): {
  codeText: string;
  language: string | null;
  isBlock: boolean;
} {
  if (!node) {
    return { codeText: "", isBlock: false, language: null };
  }

  // If this is a <pre> element containing a <code> child
  if (node.tagName === "pre") {
    const codeChild = node.children?.find(
      (c: any) => c.type === "element" && c.tagName === "code",
    );
    if (codeChild) {
      const className = Array.isArray(codeChild.properties?.className)
        ? codeChild.properties.className.join(" ")
        : String(codeChild.properties?.className || "");
      const match = /language-([^\s]+)/.exec(className);
      const language = match ? match[1] : null;
      const codeText = extractTextContent(codeChild);
      return { codeText, isBlock: true, language };
    }
    return {
      codeText: extractTextContent(node),
      isBlock: true,
      language: null,
    };
  }

  // If this is directly a <code> element
  const className = Array.isArray(node.properties?.className)
    ? node.properties.className.join(" ")
    : String(node.properties?.className || "");
  const match = /language-([^\s]+)/.exec(className);
  return {
    codeText: extractTextContent(node),
    isBlock: false,
    language: match ? match[1] : null,
  };
}

export function extractTextContent(node: any): string {
  if (!node) return "";
  if (node.type === "text") return node.value ?? "";
  if (Array.isArray(node.children)) {
    return node.children.map(extractTextContent).join("");
  }
  return "";
}

export function MermaidDiagram({ value }: { value: string }) {
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

export function CodeBlockRenderer({
  node,
  children,
  ...props
}: {
  node?: any;
  children?: React.ReactNode;
  [key: string]: any;
}) {
  const info = extractCodeInfo(node);
  const rawText = (info.codeText || "").replace(/\n$/, "");

  if (isDiffLanguage(info.language)) {
    return <ConversationDiff value={rawText} />;
  }

  if (isMermaidLanguage(info.language)) {
    return <MermaidDiagram value={rawText} />;
  }

  return (
    <pre
      className="overflow-auto rounded-lg bg-theme-control p-3 text-code-sm text-on-surface"
      {...props}
    >
      <code>{children}</code>
    </pre>
  );
}

export function InlineCodeRenderer({
  children,
  ...props
}: {
  children?: React.ReactNode;
  [key: string]: any;
}) {
  return (
    <code
      className="rounded bg-theme-control px-1 py-0.5 text-code-sm text-primary"
      {...props}
    >
      {children}
    </code>
  );
}

export function rehypeLatexMathDataAttr() {
  return (tree: any) => {
    function visit(node: any, parent: any) {
      if (node.type === "element" && node.properties?.className) {
        const classNames = Array.isArray(node.properties.className)
          ? node.properties.className
          : String(node.properties.className).split(" ");
        if (classNames.includes("katex-display")) {
          node.properties["data-latex-math"] = "display";
        } else if (classNames.includes("katex")) {
          const isParentDisplay =
            parent?.type === "element" &&
            (Array.isArray(parent.properties?.className)
              ? parent.properties.className.includes("katex-display")
              : String(parent.properties?.className).includes("katex-display"));
          if (!isParentDisplay) {
            node.properties["data-latex-math"] = "inline";
          }
        }
      }
      if (node.children) {
        for (const child of node.children) {
          visit(child, node);
        }
      }
    }
    visit(tree, null);
  };
}
