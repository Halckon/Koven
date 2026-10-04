# SPEC-0260: unit planner/lower共享借用source查询

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施与验收本恢复切片时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-RECOVERY-0260` |
| 所属 Phase | Phase 4 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户明确授权按清单本机重建，最终集中验证 |

## Goal 与边界

unit planner/lower共享借用source查询。不新增public API、依赖、ABI或语言能力。
保持canonical顺序、first-match、identity及missing错误的None Span；
同一`Vec<&ParsedFile>`借给planner/lower，所有调用消费真实ParsedSourceUnit。
不复制或重排解析产物，保留不同driver能力及原diagnostic优先级。

## 验收

`unit_source_query_tests::canonical_lookup_preserves_borrowed_identity_first_match_and_missing_error`
在本机修后完整codegen库内通过；真实planner/lower/native调用方合同同时通过。
完整库为787 passed / 0 failed / 1既有LLDB权限ignored / 0 filtered。
本机集中门禁其余项由[恢复账本](../../development/recovery-local-delivery.md)记录；Linux/远端CI未运行。


## 2026-10-04 本机恢复最终验收

本轮仅按用户授权在本地main重建和交付，禁止推送/PR/远端更改。
实际Mac验证见[本机恢复账本](../../development/recovery-local-delivery.md)：core、
ownership_iteration184、stage75个target/994项、Guide23、教程7正例+2完整JSON负例通过；
Return只读自查修复5+3定向通过，最终workspace all-targets严格Clippy通过。
既有LLDB权限ignore1保留；Linux/远端CI未运行，旧云端证据不替代本轮。
本地验收归档不等于原P0–P5全部退出；P2成本raw丢失、预算接受未授权，缺口不关闭。
unit-for仍准确拒绝，源码MoveOnly ZST、旧SharedFieldLoan swap误报及其他原非目标保持。
