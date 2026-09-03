# A-F11：React Hook Form接管来源表单

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** 来源导入/编辑的values/errors/reset交给成熟表单库，复用Zod。
**Architecture:** RHF处理输入生命周期，Zod校验，SourceInput构造仍为领域转换函数。
**Tech Stack:** `../02-dependencies.md` 锁定的React Hook Form/@hookform/resolvers，保留现有Zod 4。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F10。
**Contracts:** C-BASE、C-FRONTEND、C-UI。
**Read:** 入口、对应契约节、playbook。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/components/sources/SourceImportDialog.tsx`、`SourceEditDialog.tsx`、`frontend/src/utils/sourceImport.ts`、`frontend/src/schemas/source.ts`（只复用现有schema，不改DTO）。
- Create: `frontend/src/components/sources/sourceFormSchema.ts`、`sourceFormSchema.test.ts`、`SourceImportDialog.test.tsx`。
- Test: 既有SourceEditDialog、sourceImport、schemas/source测试。
- Consumes: 既有 `SourceImportFormValues`、`buildImportSourceInput(values):SourceInput`、`Source`、`sourceInputSchema`；PathPickerInput/Switch仍用现有组件。
- Produces（本卡创建）: `sourceFormSchema: z.ZodType<SourceImportFormValues>`（实际用z.object推导）；输出保留priority字符串，DTO转换函数负责数值化。没有通用“表单配置平台”。

## Red 与关键实现

先跑既有默认glob、priority、来源备份按钮测试green。新增到 `sourceFormSchema.test.ts`：

```ts
import { expect, it } from "vitest";
import { sourceFormSchema } from "./sourceFormSchema";
import type { SourceImportFormValues } from "../../utils/sourceImport";
const valid: SourceImportFormValues = {
  rootPath: "/tmp/skills", name: "", priority: "10", enabled: true,
  includeGlobsText: "", excludeGlobsText: "",
};
it("空路径和小数priority属于字段错误", () => {
  const result = sourceFormSchema.safeParse({ ...valid, rootPath: " ", priority: "1.5" });
  expect(result.success).toBe(false);
  if (!result.success) expect(result.error.issues.map((i) => i.path[0]).sort()).toEqual(["priority", "rootPath"]);
});
```

Zod字段约束：rootPath trim/min(1)；priority沿用 `Number.isInteger(Number(value))`（当前空字符串等价0，业务行为不在本卡暗改）；其余输入字段类型同现有模型。表单采用：

```ts
const form = useForm<z.input<typeof sourceFormSchema>, unknown, z.output<typeof sourceFormSchema>>({
  resolver: zodResolver(sourceFormSchema), defaultValues,
});
```

`defaultValues`由当前组件已有initial-values转换产生；本卡保留并抽为具名纯函数，不用schema默认值覆盖编辑对象。切换source/open时调用 `form.reset`。受控Switch用 `Controller`；目录选择结果用 `setValue("rootPath", value, {shouldDirty:true,shouldValidate:true})`。提交通过 `handleSubmit`，保存错误用root错误/原通知，目录选择pending仍是独立局部状态。

## 步骤

- [ ] **Baseline**：跑现有两组转换/schema测试及SourceEditDialog按钮测试。
- [ ] **Red**：新增schema测试与SourceImportDialog真实提交测试（空路径不调用onSubmit、选目录清除错误）；增加两个文件引用RHF且删除fieldErrors state的guard。
- [ ] **Migrate**：安装库；一次改SourceImport再改SourceEdit，分别跑测试；导入默认glob与编辑时空glob的不同语义保持。
- [ ] **Clean**：删除 `SourceImportFormErrors/validateSourceImportForm/hasSourceImportFormErrors` 及重复field reset/updateValue；保留 `buildImportSourceInput/deriveSourceName` 等领域转换。
- [ ] **Verify**：执行下列命令，真实UI提交调用仍是原onSubmit契约。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/components/sources/sourceFormSchema.test.ts frontend/src/components/sources/SourceImportDialog.test.tsx frontend/src/components/sources/SourceEditDialog.test.tsx frontend/src/utils/sourceImport.test.ts frontend/src/schemas/source.test.ts
pnpm typecheck
pnpm lint
```

## 验收与停止

原字段与备份动作完整、失效字段定位正确、切source不泄漏草稿。若RHF导致对话框焦点/PathPicker ref冲突，合并ref保留DialogFrame焦点契约；不更换基础Dialog。后端校验仍权威。

**API 来源:** [RHF useForm](https://react-hook-form.com/docs/useform)、[Zod resolver](https://github.com/react-hook-form/resolvers#zod)。
