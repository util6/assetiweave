# B01：宿主与能力协议的证据化决策

> **Status: BLOCKED_BY_B00**。本卡是研究/原型任务，不是未定技术的生产实现卡。

**Goal:** 对 P-RUNTIME 七项形成唯一、可复现、有净收益的建议，并取得明确采纳记录。
**Depends:** B00。
**Read:** P-BUSINESS、P-RUNTIME、P-DECISION；当前 ADR0012、ADR0013；当前 ExtensionKernel 和 Conversation package/外部协议源码。
**Files:** 生产代码只读。一次性实验放隔离临时目录，不写持久 new/v2 实现树；实验结果写 Issue #23。正式决定需要新 ADR 时按治理规则记录。

## 调查次序

- [ ] **先读已有契约**：追踪一个内置 Agent 安装、一个独立 Conversation Adapter 安装、一次注册/调用/取消、一个 TargetProvider JSON refresh 的真实路径。区分当前能力、目标缺口和历史文档愿景。
- [ ] **比较三类方案**：现有外部进程/声明式协议 + Rust 业务接口；成熟 Wasm 插件宿主；成熟 JS 插件容器。候选必须有官方文档、稳定版本、宿主语言/平台/许可证/维护状态证据。JS API 不因 DeepSeek 使用就自动适配 Rust。
- [ ] **使用官方来源**：读 [DeepSeek Harness architecture](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/architecture.md) 与 [Cordis](https://github.com/cordiverse/cordis) 的当前实现/维护声明。借鉴业务可替换性，不照搬其运行时。对实际候选查看官方发布、API 和兼容矩阵，记录查证日期与 exact version。
- [ ] **至少验证最佳候选与现有接缝基线**：使用候选官方最小 example，在隔离目录加入一个扫描能力测试实现；宿主构建一次后更换独立实现，证明不重编译；运行响应/错误/取消/进程或实例释放用例。记录完整 build/run 命令、版本、输入、输出和退出状态，保留实验 patch 引用以便复现，不仅记录截图。
- [ ] **测量代价**：分别记录空闲/一次调用内存、冷启动、打包增量、支持平台构建情况、运行时分发要求。未测平台写未测；不以估计数字决定选型。
- [ ] **填完七项结论**：按 P-RUNTIME 顺序写实际 Manifest/能力/schema 样例、错误/取消/租约规则和库边界；给出 W1–W5 每个包准备删除的现有机制。明确宿主仍负责的业务权限与数据 authority。
- [ ] **提交决策**：Issue 评论使用以下完整字段顺序：`结论 → 候选对比 → 官方依据 → 可复现实验 → 平台与代价 → 七项协议/生命周期结论 → 删除收益 → 未选方案代价 → 请求采纳的唯一方案`。

## 审查与结束

- 无净收益时建议复用现有进程/声明式能力并精简共享治理，不强行引入额外运行时。
- 存在未验证 ABI/API、无法复现实验或关键平台空缺时，不宣称可编码；记录需补的具体实验。
- 维护者明确采纳并写入 Issue 后才标 VERIFIED；在此之前状态 WAITING_FOR_DECISION，B02 不开始生产卡冻结。

本卡不承诺运行中无损热替换，也不实现新的部署产品；这些不能作为候选演示悄悄进入范围。
