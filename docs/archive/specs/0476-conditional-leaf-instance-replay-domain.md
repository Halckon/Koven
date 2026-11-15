# SPEC-0476：Conditional leaf实例回放器归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批回放器整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0476` |
| 所属 Phase | Phase 3 ownership测试回放 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0475](0475-fixture-discovery-execution-and-contracts.md)，输入130acff7 |
| 影响范围 | conditional_leaf完整测试回放器及私有子模块/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 唯一有界完整职责

从assert_conditional_leaf_replay中迁出完整replay_presence、EnvironmentInstance、ReplayState
及其全部impl、read_captured到conditional_leaf/replay.rs。这是独立的实例/selector/phi执行
模型，不按容量切场景。mentions谓词、fixture与全部静态/动态断言留原函数，不改测试FQN。

私有mod replay仅在本测试域；ReplayState/read_captured与两结构字段、四个被场景调用的
方法pub(super)只供父测试域使用；create/capture/snapshot/replay_presence维持私有。
函数内用私有导入原名；保留原实例计数、owner转移、capture path查找、selector old-state
读取后统一提交、phi clear/write与所有内部断言。无生产API/算法变更，无wrapper/额外clone/
allow/ignore/新依赖；不复用其他两套独立oracle，不改fixtures/source/span/cleanup身份。
根和新文件小于1000，仅删conditional_leaf exception，其他policy等值，不提高额度。

## 2. 验收

- [x] R1：完整回放器/原场景机械保全，只有限域与模块/导入变化，测试FQN不变。
- [x] R2：原测试精确命中1项（内部三个tail）基线/迁移后通过，本地提交。
- [x] R3：原iteration模块测试组合、完整frontend、Clippy/fmt/policy/docs/尺寸/空白、generated-owner Python真实锚点回归与独立精确diff审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

本组只迁一套完整回放器；file_parent与sibling_shared_loans不按容量机械拆，后续先独立
评估实际边界。原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，不累计重复。

## 3. 真实证据

合同f761a19b，实现282efad2，一套完整局部回放器按真实职责迁移，无容量拼接。
conditional_leaf根 **1155→838**，新replay模块 **322**；mentions、fixture和场景全部
断言原位，精确原测试FQN前后各 **1 passed/0 failed/0 ignored**，内部三个tail仍执行。
iteration::tests组合 **40/0/0**，与完整frontend不重复累计覆盖。

初次迁移工具按字段名替换过宽，误伤参数/模式/构造位置产生编译语法错误；随后从
原完整块重建，仅在两个明确struct体内加字段可见性，编译通过。保留first-compile-failed.log，
不当行为RED，不改断言绕过失败；最终补丁不含错误替换。完整回放器和剩余场景经
rustfmt机械核对：只有限定pub(super)、私有模块与函数内导入/缩进变化，所有表达式/
分支/临时快照/内部断言保持。policy只删该exception，其他JSON等值。

组末完整frontend **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail
doctest），ignored不计执行；workspace all-target cargo check0（非下游测试运行）、
frontend all-target Clippy -D warnings、generated-owner Python80（含真实源码锚点）、
fmt、policy47、docs973、尺寸/空白0。2195手写Rust，历史超限 **3→2**，本PR21→2。

独立fresh review完整f761a19b..282efad2真实diff与前像/父模块，No issues found，
确认单一实例模型职责边界、无隐式外围capture、测试域pub(super)不扩生产API；旧choices/
old_owners读取后统一提交、snapshot/phi clear/write/owner转移、capture path读取及全部
scene oracle保持。审查只读不Cargo，日志不算独立重跑；父另验组末门禁。
Architecture同步，原始日志/快照/完整diff/review留仓库外。

Rust门禁后未再改，未重跑下游codegen/CLI/LSP测试全套、真实生成资源变异校准、CI-only
fault或sanitizer；R4集中push和精确新头CI另验，Spec active、PR Draft，不归档合入。
剩余file_parent1143与sibling_shared_loans1145先保留；不按数量为零强行拆散连续场景证明。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
