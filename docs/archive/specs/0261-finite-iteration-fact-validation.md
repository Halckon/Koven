# SPEC-0261: 有限只读 iteration fact validator

> **性质**：有界变更合同 · **状态**：done · **读取时机**：实施和验收 for 事实校验时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-261` |
| 所属 Phase | Phase 3 readonly facts → Phase 4 validation |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 用户完整恢复清单第13项；本机重建、最终集中验证 |

## Goal 与边界

以真实typed provider/element descriptor核验published ownership源求值、Shared binding、
退出condition及element/provider/source清理顺序。拒绝temporary root替换绕过source求值及漏Return descriptor。
先保持source/analysis identity→diagnostic→deferred→constant门禁，再校验for schema；native入口先校验分析。
不提供public mutation、unsafe、feature后门或dev反向依赖，不扩成通用resolver或动态provider引擎。

## 验收账本

本机frontend完整库190项通过，其中三项私有真实产物mutation测试分别验证只读identity、
temporary-root替换拒绝以及漏Return/reordered cleanup拒绝。
`single_file_analysis_compile_contracts`六项全部通过（168.48秒），包含新增外部只读validator、
字段mutation拒绝与错误reason字段E0616封闭性；无失败、忽略或过滤。
真实native调用方与优先级合同在修后完整codegen库787 passed / 1既有LLDB权限ignored中通过。
中间执行按用户要求跳过；其余最终本机门禁见[恢复账本](../../development/recovery-local-delivery.md)。
Mac不能替代Linux CI，未执行远端门禁。

最终只读自查发现Return范围扫描跨lambda误拒绝，与Guide最近callable边界不符。
真实产物红测为4 passed / 1 failed / 187 filtered；使用既有只读名称作用域排除嵌套callable，
同选择5 passed / 0 failed / 187 filtered。保留循环内lambda正例及lambda内循环漏Return负例，
未增加public mutation或扩大native lambda能力；下游source CFG三项通过（785 filtered），最终workspace all-targets严格Clippy通过（24分23秒）。


## 2026-10-04 本机恢复最终验收

本轮仅按用户授权在本地main重建和交付，禁止推送/PR/远端更改。
实际Mac验证见[本机恢复账本](../../development/recovery-local-delivery.md)：core、
ownership_iteration184、stage75个target/994项、Guide23、教程7正例+2完整JSON负例通过；
Return只读自查修复5+3定向通过，最终workspace all-targets严格Clippy通过。
既有LLDB权限ignore1保留；Linux/远端CI未运行，旧云端证据不替代本轮。
本地验收归档不等于原P0–P5全部退出；P2成本raw丢失、预算接受未授权，缺口不关闭。
unit-for仍准确拒绝，源码MoveOnly ZST、旧SharedFieldLoan swap误报及其他原非目标保持。
