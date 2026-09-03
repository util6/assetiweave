# A-F12：React Hook Form接管分组表单

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** 分组创建/编辑的字段生命周期统一到RHF，保留手动成员与规则成员业务。
**Architecture:** Zod表单schema与现有AssetGroupInput schema组合；选择/预览是领域逻辑，不硬塞通用表单框架。
**Tech Stack:** A-F11已安装RHF/resolver/Zod，版本见依赖清单。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F11。
**Contracts:** C-BASE、C-FRONTEND、C-UI。
**Read:** 入口、对应契约节、playbook。

## 文件与接口

- Modify: `frontend/src/components/groups/SkillGroupCreateDialog.tsx`、`SkillGroupEditDialog.tsx`、`SkillGroupFormPrimitives.tsx`、`frontend/src/schemas/group.ts`（导出现有icon schema供复用）。
- Create: `frontend/src/components/groups/groupFormSchema.ts`、`groupFormSchema.test.ts`、`SkillGroupCreateDialog.test.tsx`。
- Test: 既有 `SkillGroupEditDialog.test.tsx`、`SkillGroupExclusiveMountDialog.test.tsx`、`schemas/group.test.ts`。
- Consumes: `AssetGroupInput`、`AssetGroup`、`AssetGroupDetail`、`AssetGroupIconSvg`、`assetGroupInputSchema`、原 `isHexColor`；现有create/edit onSubmit签名不改。
- Produces（本卡创建）:

```ts
export interface GroupFormValues {
  name: string; description: string; color: string; displayIcon: string;
  iconSvg: AssetGroupIconSvg | null; enabled: boolean;
}
export const groupFormSchema: z.ZodType<GroupFormValues>;
export function groupValuesToInput(values: GroupFormValues): AssetGroupInput;
```

schema实际保留推导。手动成员IDs作为当前选择集合独立保留；规则成员通过detail.members派生，禁止把规则成员自动持久化为manual。SVG编辑器内部未提交JSON文本是局部草稿，提交时用导出的既有icon schema parse，不再两份手写JSON形状检查。

## Red 与关键实现

先保留现有“备份当前草稿成员”测试green。新增下列测试，不需要Asset fixture：

```ts
import { expect, it } from "vitest";
import { groupFormSchema, groupValuesToInput } from "./groupFormSchema";
it("空名称拒绝，保存时只规范化表单字段", () => {
  const values = { name: " Review ", description: " ", color: "#10b981", displayIcon: " ", iconSvg: null, enabled: true };
  expect(groupFormSchema.safeParse({ ...values, name: " " }).success).toBe(false);
  expect(groupValuesToInput(groupFormSchema.parse(values))).toMatchObject({
    name: "Review", description: null, display_icon: null, enabled: true,
  });
});
```

`useForm`＋`zodResolver`接管六个字段；颜色草稿可用controlled field，失效值展示校验错误而非悄悄写库。Create保留随机初始颜色、nextSortOrder和空rules；Edit在保存时 `{...detail.group,...groupValuesToInput(values)}`，保留原id/asset_kind/rules/timestamps，不用表单schema重建完整Group。

## 步骤

- [ ] **Baseline**：运行分组现有behavior/schema测试并保存结果。
- [ ] **Red**：加入form schema/真实创建提交测试；扩展Edit测试证明rule-only成员不混入onSubmit第二参数；加入RHF接管guard。
- [ ] **Migrate**：先Create后Edit；字段reset按detail/open边界，成员搜索与备份按钮保持独立可用。
- [ ] **Clean**：删除六字段重复state/reset、formError与重复SVG结构解析；保留领域成员集合/随机颜色算法/备份判定。
- [ ] **Verify**：以下命令通过，group exclusive mount不受影响。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/components/groups/groupFormSchema.test.ts frontend/src/components/groups/SkillGroupCreateDialog.test.tsx frontend/src/components/groups/SkillGroupEditDialog.test.tsx frontend/src/components/groups/SkillGroupExclusiveMountDialog.test.tsx frontend/src/schemas/group.test.ts
pnpm typecheck
pnpm lint
```

## 验收与停止

两真实表单使用同一库与schema；manual/rule成员语义不变。GlobalSettingsDialog多数为即时生效偏好，继续A-F06 mutation，不为了统一而变整页提交表单。若发现第三类重复复杂表单，记录到交接但本卡不无限扩张。

**API 来源:** [RHF Controller](https://react-hook-form.com/docs/usecontroller/controller)、[Zod resolver](https://github.com/react-hook-form/resolvers#zod)。
