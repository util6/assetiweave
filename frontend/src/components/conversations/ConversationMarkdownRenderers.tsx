import React, { useEffect, useId, useMemo, useRef, useState } from "react";
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

function cleanupMermaidDomElements(prefixId: string) {
  if (typeof document === "undefined") return;
  document.getElementById(prefixId)?.remove();
  document.getElementById(`d${prefixId}`)?.remove();

  // Self-heal and clean up any dangling error elements or temp containers
  const danglingElements = document.querySelectorAll(
    `body > div[id^="d${prefixId}"], body > div[id^="${prefixId}"], body > svg[id^="${prefixId}"], body > [id^="dconversation-mermaid-"]`,
  );
  danglingElements.forEach((el) => el.remove());
}

export function MermaidDiagram({ value }: { value: string }) {
  const reactId = useId();
  const idPrefix = useMemo(
    () => `conversation-mermaid-${reactId.replace(/[^a-zA-Z0-9_-]/g, "")}`,
    [reactId],
  );
  const renderSeqRef = useRef(0);
  const [svg, setSvg] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const currentSeq = ++renderSeqRef.current;
    const currentId = `${idPrefix}-${currentSeq}`;

    async function renderDiagram() {
      if (!value.trim()) {
        if (!cancelled) setSvg(null);
        return;
      }

      let sandboxContainer: HTMLDivElement | null = null;
      if (typeof document !== "undefined") {
        sandboxContainer = document.createElement("div");
        sandboxContainer.setAttribute("aria-hidden", "true");
        sandboxContainer.style.position = "absolute";
        sandboxContainer.style.left = "-9999px";
        sandboxContainer.style.top = "-9999px";
        sandboxContainer.style.width = "1px";
        sandboxContainer.style.height = "1px";
        sandboxContainer.style.overflow = "hidden";
        sandboxContainer.style.opacity = "0";
        sandboxContainer.style.pointerEvents = "none";
        document.body.appendChild(sandboxContainer);
      }

      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          fontFamily: "inherit",
          securityLevel: "strict",
          startOnLoad: false,
          theme: "base",
          suppressErrorRendering: true,
        });

        if (typeof mermaid.setParseErrorHandler === "function") {
          mermaid.setParseErrorHandler(() => {});
        }

        const rendered = await mermaid.render(
          currentId,
          value,
          sandboxContainer ?? undefined,
        );
        if (!cancelled) {
          setSvg(rendered.svg);
        }
      } catch {
        if (!cancelled) {
          setSvg(null);
        }
      } finally {
        if (sandboxContainer) {
          sandboxContainer.remove();
        }
        cleanupMermaidDomElements(currentId);
        cleanupMermaidDomElements(idPrefix);
      }
    }

    void renderDiagram();
    return () => {
      cancelled = true;
      cleanupMermaidDomElements(currentId);
      cleanupMermaidDomElements(idPrefix);
    };
  }, [idPrefix, value]);

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
