# SPEC-08:接口覆盖矩阵与命令元数据统一(P3)

- 状态:Draft v3(v1 审计 #9 与补充问题修订,§1 基线为更正后事实;v2 复审无针对本篇的条目,版本号随全套同步)
- 前置:SPEC-01(错误码进契约后再统一元数据更顺,但非硬依赖)
- 进程模型假设(SPEC-00 §3a):本篇只涉及契约与文档生成,与进程角色无关。
- 交付物:生成式覆盖矩阵、canonical↔Tauri 映射层、完整性守卫

---

## 1. 背景与原则(基线已更正)

基线事实(main@190bb0e,经直接读取 `cli/internal/schema/contract.json` 核实):

- 166 个 Tauri command;249 个 Engine 契约条目。
- **`canonical_method` 字段全部填充**:多数条目 canonical 与 method 相同,其余为 alias→canonical 指向,构成"一个 canonical 能力对多个 Engine 方法名"的结构;Go 侧协议读取该字段,CLI e2e 存在对 canonical 值的断言。**该字段是活跃契约的一部分,MUST NOT 删除或废弃。**(此前版本声称"字段全为 None、建议废弃"系测量错误,已更正。)
- Engine 契约条目已含 `method/canonical_method/description/risk/confirmation_required/exposure/supports_dry_run/params_schema/cli/since/deprecated`——**Engine registry 是既有的、生成式的元数据事实源**。

原则(MUST):
1. 覆盖矩阵是**生成物**,不允许手写维护;生成脚本进 CI,与 `pnpm cli:contract` 同级。
2. 文档(README/specs)MUST NOT 写死命令计数;引用矩阵链接。
3. Tauri 保持薄的类型化 wrapper;**不要求**用 registry 宏生成 Tauri 函数。
4. **不制造第三份元数据**(修订,审计补充问题):`risk`/`confirmation_required`/`exposure` 的唯一事实源保持为 Engine registry(经 contract.json 消费);任何新结构 MUST NOT 重新声明这些字段。

## 2. 映射层:SurfaceMapping(取代原 CommandMeta 设计)

采纳审计给出的方案 2:矩阵直接消费现有 Engine registry 元数据,新增的只是 canonical 能力与 Tauri 命令的对应关系。

```rust
/// 唯一新增的静态表:canonical 能力 ↔ Tauri 命令 的映射。
/// MUST NOT 携带 risk/confirmation 等已由契约承载的字段。
pub(crate) struct SurfaceMapping {
    pub canonical_method: &'static str,     // 契约 canonical_method;能力聚合键
    pub tauri_command: Option<&'static str>,// 无对应 Tauri 面时为 None
    pub note: Option<&'static str>,         // 仅单侧存在时的差异说明
}
```

覆盖矩阵生成 `scripts/gen-surface-matrix.(sh|rs)`:

1. 输入:`contract.json`(按 `canonical_method` 聚合,alias 列入同行;`risk/confirmation_required/cli` 直接取自契约)+ `grep '#\[tauri::command\]'` 清单 + `SurfaceMapping` 静态表。
2. 输出:`docs/generated/surface-matrix.md`,表列 = canonical 能力 × {Engine method(s)(含 alias)、CLI verb、Tauri command、risk、confirmation}。
3. **完整性守卫**(仿 DSH completeness guard):任一 Tauri command 未被任何 `SurfaceMapping` 引用,或任一契约 canonical 未出现在矩阵 → 脚本非零退出并列出遗漏。首次运行允许显式豁免清单 `scripts/surface-matrix-exemptions.txt`,基线只减不增(守卫校验)。
4. Tauri 侧若需展示风险/确认语义(确认弹窗、危险样式),读取由契约生成的共享常量,逐步替换实现里的散落定义;每改一批跑冒烟。**不在 Rust 里第二处手写 risk。**

## 3. canonical_method 的角色(修订,审计 #9)

- 保留并继续由 Engine registry 生成;矩阵以它为聚合键,alias 关系显性呈现。
- 若发现某条目 canonical 指向不存在的方法、或 alias 环,守卫报错——这把"契约自洽"也纳入 CI。
- `CommandMeta`/方案 A(废弃字段)/方案 B(另行填充)从本 SPEC 移除,不再是待裁决项。

## 4. 后续 typed domain event 扩面(承接 SPEC-04 §7)

新的跨域消费需求出现时,流程固定:先在 SPEC-07 消费者矩阵登记 → 按 SPEC-04 §7 清单评审新变体 → 实现。MUST NOT 因 UI 通知类需求新增 domain event(走 progress/transport)。

## 5. 验收

- `pnpm gen:surface-matrix`(新脚本)在 CI 绿;删除一条 `SurfaceMapping`(临时提交)能红;构造一个 alias 指向缺失 canonical 的假契约样本,守卫能红(单测形式)。
- `docs/generated/surface-matrix.md` 提交并在 `docs/repository-structure.md` 登记为生成物(禁手改)。
- 豁免清单基线数记录进守卫,只减不增。
- 契约零破坏:`pnpm cli:contract` 与 `pnpm cli:test:e2e` 全绿(含既有 canonical 断言,如 `source.remove`)。
- 抽查验收:任选 5 个"重要工作流"(同步、批量挂载、备份、市场安装、翻译),矩阵显示三 surface 覆盖状态与实际一致。
