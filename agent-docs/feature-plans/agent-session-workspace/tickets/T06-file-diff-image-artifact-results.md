# T06：补齐文件、Diff、图片与 Artifact 展示

## Outcome

Tool Step 中的文件位置、结构化 Diff、图片和已有 Artifact 结果以受控 renderer 展示；未知或不受支持内容有可读、有界降级。

## Blocked by

T03。

## Scope

- Requirements：R-CHAT-005、R-NFR-005/008
- Contracts：C-083–C-103
- Seams：S-BE-03、S-BE-04、S-FE-03、S-FE-07；AionUi FileChanges/AcpTool references
- Gates：G-05、G-07、G-10、G-11

## Preflight

1. 确认 T03 已 verified，并读取 content block discriminated union 与 fallback 合同。
2. 定位现有 Diff、Image、Artifact viewer/registry 和路径缩写 helper；禁止复制同类 renderer。
3. 在固定 AionUi 提交中读取 `03-codebase-seams.md` 指定的 FileChanges/AcpTool 文件。
4. 固定 diff/location/image/artifact/unknown/truncated fixtures 并记录 targeted test baseline。

## Red tests

1. ACP content 中 Diff/path/line 映射 typed blocks。
2. Diff 缺一侧文本时仍合法；compact/truncated 不伪造精确 line counts。
3. image path/mime/alt 通过合法 viewer 打开。
4. artifact 只使用 renderer registry/现有 capability。
5. unknown provider content 显示 type + bounded text，不崩溃。
6. 路径显示遵守 home 缩写和既有 path policy。

## Implementation steps

1. 对照 AionUi ACP Tool、MessageFileChanges 与现有 AssetIWeave renderer registry。
2. 完成 diff/location/image/artifact/unknown block mapping。
3. 复用现有 Diff/Image/Artifact viewer；需要新通用 primitive 时放 Foundation/Common。
4. 对 compact/truncated Diff 显示状态，不计算不可靠行数。
5. 对 path 做规范化、home 缩写和复制行为。
6. 在 Step detail 中按固定顺序组合，不增加 Provider 自定义 UI。
7. 加入错误边界与 fallback text。

## Acceptance criteria

- [ ] Diff/location/image/artifact 可端到端展示或明确降级。
- [ ] Provider 不能注入 React/HTML/script。
- [ ] 路径/locator 遵守现有公开 policy。
- [ ] truncated/partial 不被误标完整。
- [ ] lane 宽度不被内容撑开。
- [ ] 未引入 AionUi runtime 依赖。

## Verification

- Provider mapping fixtures；
- renderer component tests；
- malicious/unknown content tests；
- Diff/image browser/Tauri 手工验收；
- G-07/G-10/G-11。

## Non-goals

- 复制 AionUi Explorer/SCM/Office/Browser 子系统；
- 新的 Artifact 持久化模型；
- Team/Memory domain changes。

## Handoff

本卡与 T04/T05 可并行。记录每种 content 的 supported/degraded 状态供 T12 验收。
