# SPEC-0461：SSA model 错误与 numeric proof 来源职责归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批模型错误与测试来源职责整理时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
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
不迁移测试或改FQN，不扩大有效可见域、不新增allow/wrapper或重复helper，不导入未合#427。
两根与新文件均不超1000，仅删除对应旧额度，保留其他policy逐JSON相同。

## 2. 节奏与验收

- [x] R1：完整块机械保全、直接和间接consumer核对，身份/权限不扩大。
- [x] R2：每片默认定向GREEN→移动→GREEN，本地可构建提交；不伪造RED。
- [x] R3：与0462成组统一双配置组合、all-target Clippy、fmt/docs/尺寸/policy、独立完整提交区间审查。
- [x] R4：Architecture/账本同步后一次push同PR，精确新头CI另验；未获合并授权不归档或合入main。

原始快照/日志留仓库外。既有真实失败必须记录并定位，不能删断言、ignore或扩大语言语义。
当前不宣称未执行的测试、CI-only校准、fault或sanitizer已通过，也不把34项合理保留当待清零目标。

## 3. 真实证据

合同6edff77f，两片先默认定向、通过后本地提交，未逐片push：

| 切片 | 真实执行及保全 |
|---|---|
| ModelError | f413e38a；ssa::tests 4、type_tests 13、verify_tests 11，前后各 **28/0/0**，逐名相同；根1067→925，新error146（含最终格式整理） |
| numeric来源 | bdc72950；实际numeric_for消费者前后各 **1/0/0**，内部保留四源原始/Group与legacy/package-unit/memory-unit SSA/LLVM证明；根1005→944，新source_identity67 |
| 机械核对 | 完整enum/impl、两函数及剩余根正文除空白/明确scope路径外完全相同，atomic/ID/Spans/only不复制，全部测试/fixture/FQN字节不变；仅删本批model baseline+exception及numeric exception三项 |
| 独立审查 | 与0462完整6edff77f..48275563九文件2180行diff分页审查，检查路径属性、typed ID/错误身份、独立SourceMap只比坐标及间接consumer，未发现具体问题 |

两次真实编译装配失败均先停线定位并留日志：新增mod最初误接既有operation的path属性，
恢复属性绑定后28项通过；numeric模块既有装配使默认child路径错误，显式path修复后原
consumer通过。没有把后续E0614连锁错误当另一语义缺陷，没有更改断言/诊断或跳过测试。

组末与0462同树双配置各 **634/0/0**，逐名集合一致、所有filter命中；workspace all-target
check/Clippy、feature codegen all-target Clippy、fmt、47项policy tests通过。docs959与
尺寸通过：2156手写Rust、25历史超限，四根合计29→25，本Spec两根29→27；未提高额度。
Architecture初次204行触发入口上限，保留失败后将本批概述纳入对应段落，200行复验通过；
没有改Rust压行过门禁。

原始日志/快照/机械脚本及完整区间审查留仓库外。完整组合与0462同一次执行，不重复累计。
Specs保持in-progress；本地门禁不等于新头CI，不借父636c56d0的43675343635全绿替代本批。
未跑本地完整frontend/codegen慢组、真实generated-owner校准、CI-only faults或sanitizer全套。

## 最终交付验收（PR #433）

用户授权先合入再从main开展后续。最终代码头 `914797c2c02dd3787ded8ff1255b53d4d3bc9e09`
普通整合main `fdf5766b2b2707fe29c7ab7f7b8445b98ef329ad` 后，双配置组合各 **663/0/0**；
workspace all-target check/双Clippy、fmt、policy47、docs962、尺寸/空白通过，历史超限
**37→21**（本PR各批次累计，不将此总数归因单一Spec）。

[精确代码PR CI 43699903034](https://github.com/Halckon/Koven/actions/runs/43699903034)
completed/success：双宿主Check/Clippy/Tests、Preview producer与独立consumer、最终汇总
实际成功；Editor skipped不计执行。此前c2328e14的PR为43692407513，43692403094是push，
两者不能替代本集成头。独立交付复核完整实际两个diff（分发2861行、主干增量555行），
无具体问题；历史0456–0464独立审查保持各自范围，不补成原188路径全量正文审计。

本页旧pending/未完成文字是当时检查点，以上最终记录闭合本次合同勾选；原失败、观察期限
及本地未跑项保留，不把CI或归档倒写成所有本地检查已跑。归档候选CI和实际合并另行核验，
不代表E3终态、清空34合理保留或扩大语言准入。
