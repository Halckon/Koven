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

- [ ] R1：十二测试与四个field helper完整保全（仅一处等价model路径）；原root剩余imports/helper/test不变，FQN一对一无副本。
- [ ] R2：两根/新文件均不超1000，只删除对应旧额度，47项policy tests及尺寸门禁通过。
- [ ] R3：每片默认完整原族迁移前后实际通过，再分别本地提交；不减少测试数或改断言。
- [ ] R4：批次末双配置组合回归、双all-target Clippy、fmt/docs/尺寸/空白、独立完整内容审查；有具体发现修后复核。
- [ ] R5：Architecture/真实账本同步，同PR集中push，新头官方CI单独验收后才归档。

## 5. 实施顺序

1. [ ] 冻结完整块、私有helper和consumer，跑原两族基线。
2. [ ] borrowed_calls移动、原族定向测试 → 本地提交。
3. [ ] places/helper移动、原族定向测试 → 本地提交。
4. [ ] 一次组合门禁/独立审查与账本 → 集中push，精确候选CI另验。

不伪造RED；若既有用例真实失败，保留证据定位，不改语言合同或删断言。

## 6. 真实证据

当前尚未执行本批验证；原始日志/快照留仓库外。无跨crate生产API变化，不重复workspace
check；不跑frontend/完整codegen/完整LLVM慢组、本地CI-only faults或sanitizer全套。
