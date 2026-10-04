# SPEC-0258: 五成员四条内部直接声明依赖门禁

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施和核验本恢复切片时 · **唯一真源**：本页合同与验收

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-RECOVERY-0258` |
| 所属 Phase | Phase 6；工程治理 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户授权本机重建；仅本地 main 分阶段提交，最终集中验证 |

## Goal 与边界

五成员四条内部直接声明依赖门禁。基线为 PR36 main `201d415d126d86183275d86ff2bc45caae6586a4`。
不恢复原 SHA，不改语言语义，不发布远端，不运行成本实验。

从`cargo metadata --locked --offline --no-deps`核对五成员与四条内部直接声明边。
normal/dev/build、target条件、optional和rename均计入；重复、反向、非成员及非法path明确拒绝。
CI独立job无条件运行，required汇总保留该job。此门禁不检查第三方transitive、
patch/config覆盖或lock新鲜度，不把声明关系称为完整依赖图。

## 验收账本

中间执行按授权跳过；本机metadata实际五成员四条内部直接声明边通过。
Python恢复接线/metadata合同六项通过，normal/dev/build、target、optional/rename及
duplicate/reverse/nonmember/invalid path均实测；workflow无条件job及required needs断言通过。
完整Python 108项通过，workflow helper触发补充后相关六项再次通过。
配置验收不等于远端CI实际运行；未验证第三方transitive、patch/config或lock新鲜度。


## 2026-10-04 本机恢复最终验收

本轮仅按用户授权在本地main重建和交付，禁止推送/PR/远端更改。
实际Mac验证见[本机恢复账本](../../development/recovery-local-delivery.md)：core、
ownership_iteration184、stage75个target/994项、Guide23、教程7正例+2完整JSON负例通过；
Return只读自查修复5+3定向通过，最终workspace all-targets严格Clippy通过。
既有LLDB权限ignore1保留；Linux/远端CI未运行，旧云端证据不替代本轮。
本地验收归档不等于原P0–P5全部退出；P2成本raw丢失、预算接受未授权，缺口不关闭。
unit-for仍准确拒绝，源码MoveOnly ZST、旧SharedFieldLoan swap误报及其他原非目标保持。
