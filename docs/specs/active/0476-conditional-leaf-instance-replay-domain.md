# SPEC-0476：Conditional leaf实例回放器归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批回放器整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
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

- [ ] R1：完整回放器/原场景机械保全，只有限域与模块/导入变化，测试FQN不变。
- [ ] R2：原测试精确命中1项（内部三个tail）基线/迁移后通过，本地提交。
- [ ] R3：原iteration模块测试组合、完整frontend、Clippy/fmt/policy/docs/尺寸/空白、generated-owner Python真实锚点回归与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

本组只迁一套完整回放器；file_parent与sibling_shared_loans不按容量机械拆，后续先独立
评估实际边界。原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，不累计重复。

## 3. 真实证据

待记录。
