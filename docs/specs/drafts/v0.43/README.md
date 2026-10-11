# v0.43 首片实施合同准备

> **性质**：候选变更索引 · **状态**：draft · **读取时机**：准备已获范围授权的 N1a 与立即消费首片时 · **唯一真源**：本目录各 Spec；现行语义仍见 [Guide](../../../guide/README.md)

2026-10-08 12:15 UTC，用户 `Sentinel_f249680deca4819196e97322bdc24963` 明确同意开始
r3 小修后的首片实施。授权覆盖本地合同准备、实现与验证，不包含提交、push、PR 或合并。
以上为2026-10-08准备时的授权记录。0289 的已批准 N1a 合同随 v0.43/ADR-0030
启用，现已按 PR72 实现 head 双宿主证据有界归档；最终交付见其 §22。本目录的
0292 仍是未启用 consume 合同，不随 N1a 自动启用。

- [0289 N1a 范围 carrier](../../../archive/specs/0289-n1a-range-carrier.md)：新内联描述符、根来源与单文件/unit 端到端交付。
- [0292 立即消费路径](0292-immediate-consuming-sequence.md)：chain temporary owner 与立即 owned 产出。

```text
现行 M2B 来源合同 / 已验收 0288
        + 最小 std 扩展声明 / 可复用范围构造原语
        -> 0289：N1a single/unit 类型、所有权、SSA verifier、native
        -> 0292：consume 接管与正常退出清理、立即消费算法
复制能力独立前置 -> 显式物化；通用 Clone 尚未批准
N1b/filter 借用视图、Option、多来源与惰性链不进入本图的首片
```

0289 的声明、来源、SSA/native 与计数证据分阶段保留，最终有界验收见其 §22；
consume 不由这些证据自动启用，0292 仍须独立满足规范和实施前置。
