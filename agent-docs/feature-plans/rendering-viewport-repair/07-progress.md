# 渲染视口修复进度

## 元数据

| 字段 | 值 |
|---|---|
| 状态 | Ready for Luna |
| 规格日期 | 2026-09-07 |
| Issue | [#25](https://github.com/util6/assetiweave/issues/25) |
| 执行入口 | `00-execution-router.md` |
| 最终验收 | `04-verification-matrix.md` |

## 工作包状态

| 工作包 | 状态 | 证据/提交 |
|---|---|---|
| RVP-00 基线保护与复现 | 未开始 |  |
| RVP-01 Conversation 高度链 | 未开始 |  |
| RVP-02 同步虚拟行壳 | 未开始 |  |
| Gate A | 未开始 |  |
| RVP-03 Skill 绘制减负 | 未开始 |  |
| Gate B | 未开始 |  |
| RVP-04 Skill 虚拟化 | 条件执行 |  |
| RVP-05 全量验收与收口 | 未开始 |  |

## 已知调查基线

| 指标 | 基线 |
|---|---:|
| Conversation 原 viewport height | 36,343px |
| Conversation 原 mounted Turn | 80 |
| 高度链实验 viewport height | 469px |
| 高度链实验 mounted Turn | 4 |
| 异步范围提交无覆盖帧 | 15/48 |
| 同步范围提交无覆盖帧 | 0/48；重复 0/48 |
| 130 Asset 列表高度 | 约 16,597px |
| 130 Asset blur element | 131 |
| 130 Asset mounted rows | 130 |

以上是调查输入，Luna 必须在当前 HEAD 上重新采样，不得直接当作完成证据。

## 执行记录

### RVP-00

- HEAD：
- 工作区保护记录：
- 复现命令：
- 实测结果：

### RVP-01

- 高度来源：
- 展开分支：
- 收起分支：
- viewport/mounted 结果：

### RVP-02

- 第一次 48 帧结果：
- 第二次 48 帧结果：
- fast phase real commits：
- Console 警告：

### RVP-03

- scroll owner：
- blur element：
- 130 条 trace：
- 原生画面：

### Gate B 决策

- 结论：待定
- 证据：

### RVP-04

- 状态：条件执行
- 阈值与理由：
- 130/1,000 mounted rows：
- 行为回归：

### RVP-05

- 自动化门禁：
- Tauri/WebKit：
- ADR 收口：
- 剩余风险：
