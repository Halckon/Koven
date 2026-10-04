# SPEC-0259: 真实LLVM可恢复发射失败与TLS恢复

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施与验收本恢复切片时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-RECOVERY-0259` |
| 所属 Phase | Phase 4 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户明确授权按清单本机重建，最终集中验证 |

## Goal 与边界

真实LLVM可恢复发射失败与TLS恢复。不新增public API、依赖、ABI或语言能力。
0259仅test路径改变真实LLVM输出路径，发生在lower/verify之后，以普通文件之下的object.o触发ENOTDIR；
不以commit失败替代，不声称fatal或堆零泄漏。0260保持canonical顺序、first-match、identity及错误Span，
同一vector借给planner/lower；保留不同driver能力。

## 验收

新增真实测试源码已保存；中间执行按用户要求跳过，最终集中验证结果待记录。
