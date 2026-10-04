# SPEC-0257: 有界组合门禁去重与失败传播

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施和核验本恢复切片时 · **唯一真源**：本页合同与验收

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-RECOVERY-0257` |
| 所属 Phase | Phase 6；工程治理 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户授权本机重建；仅本地 main 分阶段提交，最终集中验证 |

## Goal 与边界

有界组合门禁去重与失败传播。基线为 PR36 main `201d415d126d86183275d86ff2bc45caae6586a4`。
不恢复原 SHA，不改语言语义，不发布远端，不运行成本实验。

组合按core（含std）→ownership_iteration→stage→剩余guide_litmus→tutorial推进，
77个唯一frontend integration target、10次Cargo调用；独立入口保留，无skip配置。
本机集中验证还修复外部编译合同的增量产物选择：共享test helper读取当前integration
executable对应的Cargo 1.96 fingerprint，只选择实际直接依赖的rlib。旧构建variant保留，
不按时间戳猜测、不执行clean；未知或歧义身份明确失败，不声称Cargo公共格式兼容层。

## 验收账本

中间执行按用户授权跳过，以下为本机集中验收；历史云端结果不可替代。

本机artifact选择修复实测：codegen外部合同2项，frontend五组外部合同31项全部通过；
stage三组75个target/994项全部通过，Guide23与教程7+2通过。六项Python接线/metadata合同通过，
包括77唯一target、10次Cargo调用、各阶段失败37传播，以及无条件required job断言。
失败后按原序续跑，不将重试次数或分段通过写作一次完整脚本退出0；最终记录见
[恢复账本](../../development/recovery-local-delivery.md)。


## 2026-10-04 本机恢复最终验收

本轮仅按用户授权在本地main重建和交付，禁止推送/PR/远端更改。
实际Mac验证见[本机恢复账本](../../development/recovery-local-delivery.md)：core、
ownership_iteration184、stage75个target/994项、Guide23、教程7正例+2完整JSON负例通过；
Return只读自查修复5+3定向通过，最终workspace all-targets严格Clippy通过。
既有LLDB权限ignore1保留；Linux/远端CI未运行，旧云端证据不替代本轮。
本地验收归档不等于原P0–P5全部退出；P2成本raw丢失、预算接受未授权，缺口不关闭。
unit-for仍准确拒绝，源码MoveOnly ZST、旧SharedFieldLoan swap误报及其他原非目标保持。
