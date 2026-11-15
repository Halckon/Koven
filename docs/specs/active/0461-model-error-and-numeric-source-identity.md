# SPEC-0461：SSA model 错误与 numeric proof 来源职责归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批模型错误与测试来源职责整理时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0461` |
| 所属 Phase | Phase 4，typed SSA及其测试支持 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续批量推进；沿fix/spec-0456与PR #433，不另开PR |
| 前置 Spec | [0460](0460-verifier-borrowed-call-and-place-tests.md)，冻结输入636c56d0，已整合main c3ceea14 |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 影响范围 | model.rs、while_flow_tests/numeric_proof_contract.rs及私有子模块与对应policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. Goal与范围

按两个完整职责拆分，而非截断算法或拼容量：

1. model.rs的完整ModelError enum、Display和Error impl移入私有model/error.rs；根直接
   pub(crate) re-export保留原model::ModelError身份及有效域。所有variant、错误字符串、
   typed ID/Span、唯一ProgramOwner/atomic保持不变，仅移除因此孤立的Error import。
2. numeric_proof_contract.rs中source_spans/source_origin完整两函数移入私有
   numeric_proof_contract/source_identity.rs。负责独立parse坐标与两种package布局，保留
   单一Spans/only、SourceMap的原角色及只比较start/end的明确合同；不改变SourceId准入。
   根私有use维持调用，子函数pub(super)仅恢复原私有可达域。

保留整个numeric CFG/固定点/owner-view/Loan结束/drop、dominance、精确次数和所有测试。
不迁移测试或改FQN，不增加pub/allow/wrapper或重复helper，不导入未合#427。
两根与新文件均不超1000，仅删除对应旧额度，保留其他policy逐JSON相同。

## 2. 节奏与验收

- [ ] R1：完整块机械保全、直接和间接consumer核对，身份/权限不扩大。
- [ ] R2：每片默认定向GREEN→移动→GREEN，本地可构建提交；不伪造RED。
- [ ] R3：与0462成组统一双配置组合、all-target Clippy、fmt/docs/尺寸/policy、独立完整提交区间审查。
- [ ] R4：Architecture/账本同步后一次push同PR，精确新头CI另验；未获合并授权不归档或合入main。

原始快照/日志留仓库外。既有真实失败必须记录并定位，不能删断言、ignore或扩大语言语义。
当前不宣称未执行的测试、CI-only校准、fault或sanitizer已通过，也不把34项合理保留当待清零目标。

## 3. 真实证据

待逐片记录；本地门禁不等于官方新头CI，也不重复累计原已覆盖测试。
