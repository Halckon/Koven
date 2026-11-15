# SPEC-0459：SSA tagged/provider 测试职责与尺寸欠账

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批 SSA 测试根职责归位时 · **唯一真源**：本批范围与证据，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0459` |
| 所属 Phase | Phase 4，typed SSA 测试组织 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，不改语义 |
| 批准依据 | 用户要求继续；逐片定向测试和本地提交，成组统一验证并叠同PR #433 |
| 前置 Spec | [0458](0458-ssa-verifier-responsibility-and-size-debt.md)，输入 `d7b066e7` |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 影响范围 | aggregate/container operation测试根、各自私有领域子模块及两根尺寸登记 |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. Goal

按实际测试领域归属，将两个超千行根降到1000以内并退休其历史额度，完整保留测试/fixture、
helper身份与断言，不改变生产代码，不以减少文件数量为目标。

沿用用户授权分支 `fix/spec-0456` / PR #433，不另建PR。两个切片分别跑定向测试并本地提交，
组末统一组合回归、双Clippy、fmt/docs/尺寸/policy及独立内容审查后集中push；不逐片全套门禁。

## 2. 两片范围

### 2.1 Aggregate tagged领域

原根 `ssa/aggregate_operation_tests.rs` 1036行，最后两项tagged用例一起移入私有子模块
`aggregate_operation_tests/tagged.rs`：

- `tagged_payload_construction_place_and_recursive_drop_form_one_linear_flow`
- `tagged_operations_reject_out_of_range_case_and_wrong_payload_type`

正例验证完整construct/place/drop线性链和render；负例验证错误payload及越界variant。
完整测试块逐字节移动，FQN仅增加tagged层，原8项族整体仍选择执行。

### 2.2 Container provider领域

原根 `ssa/container_operation_tests.rs` 1032行，将同一provider CFG两项完整移入私有
`container_operation_tests/provider.rs`：

- `provider_cfg_carries_owner_and_source_loan_and_ends_each_element_loan`
- `provider_element_loan_can_end_after_a_block_edge`

保留header/body/continue/exit参数、source/element loan、跨边结束/owner drop、稳定render
及精确operation次数断言；不把helper或源码拆成片段。FQN仅增加provider层，原10项族整体仍执行。

## 3. 保全与非目标

两个根各自保持唯一origin/add_types及构图/错误helper；私有子模块直接使用父级绑定，
不复制harness，不加pub/allow/wrapper/re-export。根其余测试与导入不变，不改生产/Span/错误/receipt。
四项旧完整名称在scripts/workflow中无固定消费者；原根前缀仍选中全部子模块用例。

只删除两根baseline/exception四项登记，其他policy值逐JSON核对不变。
不提高1000上限，不动其余33项超限，不移动其他合理保留模块，不导入未合#427。
不弱化断言、ignore、known-gap或native oracle，不把当前CI补成历史全路径审计。

## 4. 验收

- [ ] R1：四项完整属性/正文逐字节保全；私有helper/原根其余字节不变，四个新FQN各恰一次，旧名无副本。
- [ ] R2：两个根和新文件均不超1000；只退休四项旧额度，尺寸门禁与47项policy tests通过。
- [ ] R3：逐片默认完整8/10项族，迁移前基线及迁移后定向结果一致，再各自本地提交。
- [ ] R4：组末双配置组合回归、双all-target Clippy、fmt/docs/尺寸/空白及独立内容审查；实际结果与未测范围分开记。
- [ ] R5：同步事实与账本，集中推送同PR，最终精确CI另验后才归档，不借旧绿灯。

## 5. 顺序计划

1. [ ] 冻结两个测试块及helper/调用/选择路由，定向基线。
2. [ ] tagged完整归位、定向测试 → 本地提交。
3. [ ] provider完整归位、定向测试 → 本地提交。
4. [ ] 组合门禁及独立审查，更新快照/账本 → 集中push后验精确CI。

纯组织重构使用原GREEN→移动→原族GREEN，不伪造RED；若既有用例失败，先保留真实失败再定位。
无跨crate生产API变化，不重复workspace check；本地不执行frontend全量/完整LLVM慢组/CI-only faults。

## 6. 证据

实施后记录实际结果；原始日志和机械快照置仓库外。前置头d7b066e7的
[官方CI](https://github.com/Halckon/Koven/actions/runs/43664829997)已success，
但不能作为本批新头的验证。Spec仍in-progress、PR仍Draft，不提前归档或合入。
