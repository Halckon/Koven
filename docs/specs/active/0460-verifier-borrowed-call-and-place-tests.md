# SPEC-0460：Verifier 借用调用与 place 测试职责归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批 verifier 测试根尺寸欠账时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0460` |
| 所属 Phase | Phase 4，typed SSA/LLVM测试组织 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续；逐片定向测试、本地提交，批次统一验证，继续同PR #433 |
| 前置 Spec | [0459](0459-ssa-domain-tests-and-size-debt.md)，输入888ec601 |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 影响范围 | verify_scalar_tests、verify_ownership_tests及私有领域子模块/两根尺寸policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. Goal与节奏

将两个历史超限verifier测试根按完整借用调用、place/field领域归位，根和新文件均不超1000，
退休旧额度；原SSA/LLVM断言、身份/Span、fixture/helper与用例数量保持不变。

沿用fix/spec-0456与PR #433，不另建PR。分别完整族定向GREEN→移动→GREEN后本地提交，
批次末统一双配置组合回归、Clippy/fmt/docs/尺寸/policy与独立审查，最后集中push。
本地提交不是交付，不借上一批CI宣称当前头通过，不中断上一批尚在运行的CI。

## 2. 两片范围

### 2.1 借用调用域

从1393行verify_scalar_tests根移入私有verify_scalar_tests/borrowed_calls.rs：
直接Borrow调用及instance receiver两项、function pointer Borrow invoke/签名拒绝两项、
直接调用拒绝Value/错误loan kind及失效Borrow两项，共六项完整测试。
保留MoveOnly owner、receiver在显式实参之前、pointer ABI、free次数、精确错误数/loan身份。

根留基础scalar和loan生命周期；不按容量混装。只在新作用域将一处super::model路径
改为同一crate::ssa::model全路径，其他正文逐字节相同。FQN仅增加borrowed_calls层。

### 2.2 Place/field域

从1659行verify_ownership_tests根移入私有verify_ownership_tests/places.rs：
root_place_take、owner消费使place失效两项，加四项heap/inline field replace/loan测试，
以及它们原有四个私有field构图helper，共六项测试和完整helper，不拆构图/断言。

根留唯一Harness、TestType、origin及通用构图/错误helper，仍由后代直接私有访问；
保留无借用直接根、whole owner、跨字段receiver依赖、exclusive/reborrow与field归属检查。
FQN仅增加places层，完整块正文不改。

## 3. 保全与非目标

不复制Harness，不增pub/allow/wrapper/re-export；不改生产checker、ABI、准入、Origin、
错误种类/位置、CFG、canonical身份或其他测试源。迁移前确认无外部固定旧完整FQN消费者，
父族prefix仍选全部用例。两根import及留根helper不动，不把祖先绑定误删为unused。

只退休两根baseline/exception对应登记，其他policy逐JSON核对相同；不放宽1000额度。
不动其余31项超限，不清除34项合理保留，不导入未合#427或扩展borrow/receiver语义。

## 4. 验收

- [x] R1：十二测试与四个field helper完整保全（仅一处等价model路径）；原root剩余imports/helper/test不变，FQN一对一无副本。
- [x] R2：两根/新文件均不超1000，只删除对应旧额度，47项policy tests及尺寸门禁通过。
- [x] R3：每片默认完整原族迁移前后实际通过，再分别本地提交；不减少测试数或改断言。
- [x] R4：批次末双配置组合回归、双all-target Clippy、fmt/docs/尺寸/空白、独立完整内容审查；有具体发现修后复核。
- [ ] R5：Architecture/真实账本同步，同PR集中push，新头官方CI单独验收后才归档。

## 5. 实施顺序

1. [x] 冻结完整块、私有helper和consumer，跑原两族基线。
2. [x] borrowed_calls移动、原族定向测试 → 本地提交。
3. [x] places/helper移动、原族定向测试 → 本地提交。
4. [ ] 一次组合门禁/独立审查与账本 → 集中push，精确候选CI另验。

不伪造RED；若既有用例真实失败，保留证据定位，不改语言合同或删断言。

## 6. 真实证据

输入合同提交74dc3943，Rust内容与888ec601相同；逐片默认定向后本地提交，组末统一双配置门禁，未逐片push。

| 验证 | 实际结果 |
|---|---|
| borrowed_calls | 默认原scalar族迁移前后各 **12/0/0**，本地提交5040a797；根1393→658，新文件740；六测试只有一处等价model全路径调整 |
| places | 默认原ownership族迁移前后各 **16/0/0**，本地提交a749635d；根1659→796，新文件868；六测试、四field helper及HeapFieldAction enum完整搬迁，既有too_many_arguments allow随原helper保留，不是新增 |
| 机械保全 | 两根原imports、通用Harness/TestType/origin/留根helper/test字节不变；新private mod继承同一祖先绑定，无复制构图器或扩权；policy仅删scalar baseline1392/exception1393及ownership baseline1659三项（ownership原无exception），其他JSON值完全相同 |
| 最终组合 | 延续前批122项（已含原12/16族），默认/feature各 **122/0/0**、exit0，未重复累计；所有filter命中，十二个新FQN各恰一次、旧完整名无副本，先前native仍实际运行 |
| 编译/格式 | 双all-target Clippy带-D warnings及fmt退出0；没有Rust生产/跨crate API变化，不另跑workspace check |
| policy/尺寸 | 47项policy tests通过；重新fetch origin/main后尺寸门禁退出0，2151手写Rust、29历史超限、无生成物登记，不扩大额度 |
| 独立审查 | 分页核74dc3943..a749635d完整3302行五文件patch，核SSA/LLVM receiver/pointer/free、精确错误数及loan身份、take与field全矩阵/构图helper、祖先权限/宏、FQN路由及policy；未发现具体问题，未用空工作区diff代替范围 |

Architecture同步实际归属；最终docs957与空白通过，之后仅收齐账本不改Rust。原始日志/快照留仓库外；未跑frontend/
完整codegen/完整LLVM慢组、本地CI-only faults或sanitizer全套，不把旧头或其他配置当当前通过。

组末fetch时远端main已推进c3ceea14（含#432及#434），本批冻结输入未导入这些后继变化。
尺寸门禁使用实际merge-base，不把它当成已验证最新main合并树；新候选CI及必要主干整合另验。
一次查询前批CI遇GitHub API EOF，不能据此判前批失败或成功；先前888ec601观察为in-progress。

## 7. 同PR主干整合

用户要求继续推进后，从clean的9ff7e1bb普通merge最新main
`c3ceea143a3a94ecc36f91cfb394170e71893889`，不rebase/force、不另开PR。
无冲突、无手工Rust或CI算法调整。主干已批准的#426资源分类说明/#432不同根nullable
writeback修复/#434 CI优化原样进入，不把它们作为本Spec新增语言授权。

机械整合核对：13个主干Rust/CI文件与main逐字节相同；17个本分支Rust/policy文件与
9ff7e1bb逐字节相同。两侧Architecture/roadmap自动合并后按真源重生成，无生成视图差异。

| 整合验证 | 实际结果 |
|---|---|
| 组合codegen | 原122+implicit_null五项，默认/feature各 **127/0/0**，所有过滤器实际命中；包括主干新增inner writeback后outer Borrow/return的native消费者 |
| 资源分类 | frontend `resource_deinit_type_facts`完整 **11/0/0** |
| 资源作用域native | CLI `native_cli`中的resource_scope_drop两项真实编译/链接/run，**2/0/0**，stdout精确为marker后deinit、stderr空 |
| CI-only边界 | `unit_single_source_analysis`仅编译/list，列出6项；ignored generated-owner校准入口未执行，不能记作6 passed或补成fault覆盖 |
| Rust | workspace all-target check与Clippy、feature codegen all-target Clippy、fmt退出0，同target顺序执行 |
| CI优化脚本 | pr_ci_reuse 35、check_ci_results 45、checker_evidence 4项unit tests均通过；其中拒绝/blocked JSON为测试fixture输出，不代表真实CI状态 |
| 门禁 | policy tests47通过、docs957、尺寸退出0：2152手写Rust、29历史超限；没有新增/放宽额度 |

这次整合验证归因合并后的实际工作树，不与整合前122项重复累计为覆盖；原始日志与
整合diff/机械核对置仓库外。独立只读复核已确认两侧代码/文档保全、proof交汇面与真实组合
选择，未发现具体问题；最终docs与暂存/工作区空白通过。真实generated-owner校准、CI-only
fault、完整双宿主与Preview仍待精确合并候选的官方CI，不以mock unit tests或list替代。
当前不归档或合入main。
