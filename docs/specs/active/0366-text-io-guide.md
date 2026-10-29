# SPEC-0366: 文本与同步 IO 规范启用

> **性质**：规范迁移合同 · **状态**：in-progress · **读取时机**：核验 v0.47 启用边界时 · **唯一真源**：本次迁移与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | ECO-V1-M0-S5-0366 |
| 所属 Phase | 跨 Phase 规范前置；不交付生产实现 |
| 基线 | main 72b0414e1f4cc2d9f7dc251c96947bfc18bd6ce3 |
| 分支 | feature/spec-0366-text-io-guide |
| 批准依据 | 2026-10-22 整体合同与启用授权；2026-10-29 三项错误边界决定，见启用账本 |
| 语言语义变更 | 是；Guide v0.47 |

## Goal 与非目标

完整迁移已批准的文本／IO 合同，保全 v0.46，统一 current 入口与门禁；不得缩减失败、owner、资源规则。
不实现新公共函数、不添加占位标准源，不把规范启用当 M0/M1 或生产能力完成。

## 交付与验证

- 语义唯一真源为 [Guide13](../../guide/13-program-runtime-standard-library.md#文本字节与严格解析)；
  [启用账本](../../archive/migrations/v0.47-enablement.md)记录逐项落点、原文保全与实际验证。
- 核对全部批准合同及用户三个错误决定；独立审阅语义、一致性、错误路径和未启用范围。
- v0.46 十六页反向链接还原后逐字节一致；没有历史规则或测试结论改写。
- 文档治理测试与结构检查、生成图和 roadmap freshness、diff 检查必须通过。
- 新 API 没有生产实现，因此不声称 native 通过；已有 Guide 例子不添加依赖新 API 的假成功。
- PR 适用 CI 通过后归档，最终 head 门禁全部成功再合并。

## 当前验收

迁移已落地；独立语义复核发现并闭合标准 String 扩展的旧禁令与旧 toString 延后文案，无剩余阻断。
41 项 checker、1 项 DAG、4 项 roadmap 测试及 818 篇文档结构检查、diff 检查通过；16 页旧规范反向还原逐字节一致。
除删除 Guide13 未带上限的旧文件示例外，既有 Kotlin fences 完全不变；未运行新 API native 测试，因为未新增实现。
PR 适用 CI 与最终归档闭环待完成；实现 Spec 在规范交付后逐 Goal 建立。
