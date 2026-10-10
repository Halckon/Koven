# v0.43 首片实施合同准备

> **性质**：候选变更索引 · **状态**：draft · **读取时机**：准备已获范围授权的 N1a 与立即消费首片时 · **唯一真源**：本目录各 Spec；现行语义仍见 [Guide](../../../guide/README.md)

2026-10-08 12:15 UTC，用户 `Sentinel_f249680deca4819196e97322bdc24963` 明确同意开始
r3 小修后的首片实施。授权覆盖本地合同准备、实现与验证，不包含提交、push、PR 或合并。
0289 的已批准最小 N1a 合同现已随 v0.43/ADR-0030 启用并迁入 active；本目录的
0290 仍是未启用 consume 合同，不随 N1a 自动启用。

- [0289 N1a 范围 carrier](../../active/0289-n1a-range-carrier.md)：新内联描述符、根来源与单文件/unit 端到端交付。
- [0290 立即消费路径](0290-immediate-consuming-sequence.md)：chain temporary owner 与立即 owned 产出。

```text
现行 M2B 来源合同 / 已保留 0288 WIP
        + 最小 std 扩展声明 / 可复用范围构造原语
        -> 0289：N1a single/unit 类型、所有权、SSA verifier、native
        -> 0290：consume 接管与正常退出清理、立即消费算法
复制能力独立前置 -> 显式物化；通用 Clone 尚未批准
N1b/filter 借用视图、Option、多来源与惰性链不进入本图的首片
```

声明前置与 carrier 类型已有真实红转绿证据，见 active 0289。计数边界已批准并记入
其 §6；std 来源授权与实际构造/交付继续受实施能力门约束。现有 `feature/spec-0288` 的 WIP 不覆盖、不归档；后继正式
分支交付遵循仓库规则，本次未改变 HEAD 或 branch。
