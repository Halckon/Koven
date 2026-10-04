# SPEC-0256: 当前可执行教程与CLI完整输出合同

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施和核验本恢复切片时 · **唯一真源**：本页合同与验收

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-RECOVERY-0256` |
| 所属 Phase | Phase 6；工程治理 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户授权本机重建；仅本地 main 分阶段提交，最终集中验证 |

## Goal 与边界

当前可执行教程与CLI完整输出合同。基线为 PR36 main `201d415d126d86183275d86ff2bc45caae6586a4`。
不恢复原 SHA，不改语言语义，不发布远端，不运行成本实验。

教程Markdown fence为七个当前正例与两个诊断负例的唯一源码；manifest保存完整JSON预期，
planned例不编译。正例逐个真实build、核对artifact并执行；负例核对exit 2和无artifact。
CLI对`src/SRC`别名与nested logical root在文件系统操作之前给出稳定诊断。
editors与教程输入触发Rust矩阵，core包含lang-std。

## 验收账本

中间执行按授权跳过；本机集中执行`python3 scripts/check_tutorial.py`退出0：
七个真实build/artifact/run正例、两个完整JSON负例exit2且无artifact通过，一例planned不执行。
CLI六组82项全部通过，包含logical root别名及nested root；stage中的编辑器契约与std/core通过。
实际结果见[恢复账本](../../development/recovery-local-delivery.md)，未执行Linux/远端CI。


## 2026-10-04 本机恢复最终验收

本轮仅按用户授权在本地main重建和交付，禁止推送/PR/远端更改。
实际Mac验证见[本机恢复账本](../../development/recovery-local-delivery.md)：core、
ownership_iteration184、stage75个target/994项、Guide23、教程7正例+2完整JSON负例通过；
Return只读自查修复5+3定向通过，最终workspace all-targets严格Clippy通过。
既有LLDB权限ignore1保留；Linux/远端CI未运行，旧云端证据不替代本轮。
本地验收归档不等于原P0–P5全部退出；P2成本raw丢失、预算接受未授权，缺口不关闭。
unit-for仍准确拒绝，源码MoveOnly ZST、旧SharedFieldLoan swap误报及其他原非目标保持。
