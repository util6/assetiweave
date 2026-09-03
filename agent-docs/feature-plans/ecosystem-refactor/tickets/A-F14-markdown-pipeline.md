# A-F14：Markdown AST 生态替代手写解析器

> **Status: PLANNED**。使用 `superpowers:executing-plans`，一轮只做本卡。

**Goal:** 删除块/行内自研解析，保留 Conversation 的可信代码/图表/公式展示与性能边界。
**Depends:** A-F13。
**Contracts:** C-BASE、C-UI。
**Gates:** G-FE、G-BEHAVIOR。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/components/conversations/ConversationMarkdown.tsx`。
- Create: 同目录 `ConversationMarkdown.test.tsx`、`ConversationMarkdownRenderers.tsx`（迁出原 MermaidDiagram/可信代码块组件；若已有则扩展）。
- Read/Test: `ConversationDiff.tsx`、`conversationDiffLanguage.ts` 及现有可视区渲染调用方。
- Consumes/Produces: 保持 `MarkdownContent({value}:{value:string})` 生产入口；可信代码 fence renderer 根据 language 路由到原 Diff/Mermaid 或普通 code。不增加外部组件执行接口。

## 核心实现

```tsx
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import rehypeKatex from "rehype-katex";
// 在现有 MarkdownContent 的展示边界使用；components 接已有可信 renderer。
<ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]}>
  {value}
</ReactMarkdown>
```

`components` 的 code/pre renderer 区分 inline code 与 fenced block；使用 AST node/父 pre 结构和 language class，不用从文本重新解析 Markdown。继续使用 `isDiffLanguage` 与 Mermaid 安全配置。无 language 的 fence 仍渲染 pre/code，inline code 不包装成整块。标题主题层级、table overflow、链接打开策略和空文本由现有组件行为约束。

```tsx
/* @vitest-environment jsdom */
import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { MarkdownContent } from "./ConversationMarkdown";
it("GFM 表格与行内代码实际渲染", () => {
  const {container} = render(<MarkdownContent value={'| A | B |\n| --- | --- |\n| `x` | y |'} />);
  expect(screen.getByRole("table")).toBeTruthy();
  expect(container.querySelector("td code")?.textContent).toBe("x");
});
it("内容中的脚本不成为可执行 DOM", () => {
  const {container} = render(<MarkdownContent value={'<script>window.fixture = 1</script>'} />);
  expect(container.querySelector("script")).toBeNull();
});
```

## 步骤

- [ ] 建立旧行为 characterization：中英文段落、标题、表格、列表、引用、inline code、代码 fence、diff、Mermaid、数学和链接。正常行为可先 green；新增 native pipeline adoption 断言先 red。
- [ ] `pnpm add -E react-markdown@10.1.0 remark-gfm@4.0.1 remark-math@6.0.0 rehype-katex@7.0.1`；复用现有 KaTeX CSS 与 Mermaid/Diff 依赖。
- [ ] 提取原可信 renderer，接到 react-markdown components；默认不启用 rehype-raw，不创建新 MarkdownBlock/InlineMarkdownToken 解析体系。
- [ ] 旧 `normalizeMarkdownSource` 中换行规范化/已支持数学分隔符转换若库无同义输入，可保留一个有单测的薄文本适配；不得保留原 tokenizer 作为 fallback parser。公式用 remark-math/rehype-katex 生成，不保留第二套通用 LaTeX 分段器。
- [ ] Diff/Mermaid 保留原代码语言别名和错误降级；容器可视区/折叠策略不因新 AST 渲染提前执行所有大图表。对未展开的大内容证明没有启动 Mermaid 渲染。
- [ ] 删除 parseMarkdownBlocks、手写 inline tokenizer/renderInlineMarkdown 及旧 token 类型；清理只为解析器存在的测试，业务断言在新入口上保留。
- [ ] 运行以下命令与长会话手工验证，记录解析/渲染失败不影响导航。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/components/conversations/ConversationMarkdown.test.tsx
pnpm typecheck
pnpm test
pnpm lint
pnpm build
```

**完成：** 真实 Conversation 入口由 AST 库解析；主题/安全链接/图表/数学/长内容策略保留；旧通用解析器零生产引用。
**API:** [react-markdown](https://github.com/remarkjs/react-markdown)。
