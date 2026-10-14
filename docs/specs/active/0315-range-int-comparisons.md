# SPEC-0315: 范围绑定函数内剩余 Int 比较

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：实施范围绑定与 Int 比较组合时 · **唯一真源**：本页合同与验收；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0315` |
| 所属 Phase | Phase 4 typed SSA 能力门与 native 验收 |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[表达式](../../guide/04-expressions-operators.md) |
| 批准依据 | 用户持续主线实施及小阶段提交授权；合并仍需具体批准 |
| 前置 Spec | [SPEC-0298](../../archive/specs/0298-range-binding-local-control-flow.md) |
| 基线 | 官方 fetch 确认 actual main `934b65908a7e8a42b3b7d969130aa2c95b191703`；独立 `feature/spec-0315`，不复制未合入0314 |
| 影响范围 | `borrow_result_support::binding` 的 typed 比较白名单及普通源码测试 |
| 语言语义变更 | 否 |

## 1. 冻结合同

仅为已发布 NewRangeDescriptor 绑定的完整函数开放 Int `!= <= > >=`，与既有 `== <`
保持同一合同：两个 operand 均由 typed facts 证明为 Int，表达式结果单独证明为 Boolean。
比较不新增 CFG，不根据语法放行全部 Binary，不重建 loan/来源/最后使用事实。
现有 statement-if 的双正常 arm、同 root 词法 scope 及显式运输原样复用。

不扩短路 `&& ||`、非 Int 比较、String concat、普通 borrow/metadata alias 全函数门、
不同 root、隐藏退出、分支新 binding、when/loop/value-if。Guide、frontend、ABI、verifier、
Resource 合同、LLVM 和 M13 文档陈旧问题均不在本片。

## 2. 验收账本

| 验收项 | 目标/过滤器 | 实际结果 |
|---|---|---|
| 原拒绝复现 | codegen lib 精确 `ssa::range_lowering_tests::local_control::boundary::range_local_control_keeps_unselected_binary_gates` | main源码实际1/1通过；8表达式×3入口，共24次精确UnsupportedNode/Span；新四比较12次拒绝；前后源码hash一致 |
| 新四比较红测 | codegen lib `ssa::range_lowering_tests::local_control::comparisons::` | 实际72/72红；均frontend成功后在比较精确Span以UnsupportedNode拒绝；四op×三位置×literal/constant与single/unit正逆序，0 ignored；生产未改、源码冻结一致 |
| 最后read后、scope结束前 | `last_read_`（静审仅24项） | tests-only修正夹具24/24精确红；metadata read loan、descriptor及source均由已有last-use事实在比较前结束；首轮误期望descriptor持续到块尾导致24 oracle失败，已按真实事实修正，修正后24/24 native通过 |
| 新四比较真实执行 | 同上 | 72/72转绿；每项负极值/负数/相等/正数/正极值五次输出，SSA核opcode/Int/Boolean/Span与RangeEnd→BorrowEnd→consume |
| statement-if 与同root父子/兄弟 | `comparisons::selection::` | tests-only实际72/72红，四op×String/MoveOnly/Resource×六fixture入口；均frontend通过，在比较精确Span UnsupportedNode；72/72转绿；两arm/合流及析构oracle通过 |
| 扩展边界基线 | `comparisons::boundary::` | 3/3通过；17种普通源码×六fixture入口共102次精确拒绝；非Int六种、短路/concat三种、两alias×四比较 |
| 旧能力与拒绝 | `ssa::range_lowering_tests::local_control::` 中静审过的普通源码目标 | 最终223/223通过（新增171、旧52含==/<18），0 ignored；短路/非Int与其他能力门保持 |
| 共享调用方 | `ssa::range_lowering_tests::scalar_counts::`；`ssa::borrow_result_boundary_tests::` | 6/6与4/4通过；scalar旧负表在独立main基线1/1复现，四比较initializer改为三数值×三入口native正例，其余旧拒绝保持 |
| frontend事实回归 | `ownership_borrow_last_use`、`ownership_range_construction`两独立integration target | 8/8与14/14通过，0 filtered/ignored；只读验证现有last-use及词法if边界，frontend零改 |
| 静态门禁 | `cargo fmt --all -- --check`、`cargo clippy --locked -p lang-codegen --all-targets -- -D warnings`、docs/diff、尺寸base934b6590 | 全部通过；docs667页，38项历史尺寸债无增长，生产变更仅四比较枚举 |
| 远端交付 | Draft PR与双宿主CI、归档、具体合并授权 | 未运行；阶段commit/push不表示完成 |

single 的 literal 与 const fixture 使用既有同一 constant-aware 分析/lowering入口；unit basic
与 validated constant unit 使用各自入口，不把basic成功冒充constant证据。每项通过真实
frontend、SSA verifier、原版LLVM和native；不造坏IR、不插桩或运行聚合discover。
日志、源码冻结与恢复材料留仓外。Rust1.96.0/LLVM21.1.8；baseline/current独立绝对target，
Cargo串行，不clean。Linux本地不替代macOS CI；未运行项不会标绿。

边界helper首编译E0277（Program不实现Debug）已改用显式取err，保留原失败日志；不计通过。
last-read首探索夹具误保留第二次read且输出模板不符，已修正并独立重跑，探索不作验收。

最小生产首跑199通过/24失败：三位置72、if选择72、旧52、新边界3全部通过；24个last-read
oracle误把词法块内等同仍live，实际RangeEnd与source BorrowEnd已在比较前。修正为
AfterEnds身份/顺序断言，fixture字节与生产不变，不把此调整宣称为新cleanup修复。

实现提交`12637fec`的代码冻结后，233项codegen及22项frontend实际通过，共255个唯一
测试名（新增172项codegen），无ignored；strict clippy/fmt均通过，测试前后与clippy后
源码SHA相同。选定模块已静审为普通源码链；未运行workspace/frontend全量、坏IR、
故障注入、sanitizer、计数插桩、mock或全量discover。本地Linux结果不替代后续双宿主CI。

## PR94 首轮 CI 失败与 CLI 漏选修正

[PR94 run38504210599/a1](https://github.com/Halckon/Koven/actions/runs/38504210599)
在真实 head `3cf8cd49248e11353758c7f00ffaf4d40a99ea53` / base `934b6590` 上，双宿主
Targeted均失败于 `range_project_cli::scalar_counts::public_std_range_scalar_counts_keep_other_binary_capability_gates`：
`n != 2` 已合法 build exit0、stdout/stderr空，旧 `!success` 断言仍要求拒绝。旧表另含
`n <= 3`、`n > 2`、`n >= 3`，均需随能力迁移。233项选定codegen在双宿主逐名通过；
22项frontend在该组合执行顺序中尚未到达，preview全部skipped，不能沿用本地结果称CI绿。

首轮本地只选codegen及frontend，遗漏已知下游 `range_project_cli`，是本片验收选择缺口；
不是新比较生产失效。修复从官方fetch的真实remote head新建 `fix/spec-0315`，不伪造父链。
独立baseline target原样重跑精确CLI项，实际0通过/1失败、34 filtered，复现相同exit0及
断言；前后源码hash相同。原失败日志与远端两宿主日志保留在仓外，不重写为通过。

只删除负表中这四个Int比较，其他五种短路/String concat/非Int表达式、UnsupportedNode
诊断及无产物原子断言原样保留。四个独立新测试经真实project loader→frontend→SSA/
verifier→native object→系统linker→执行，覆盖literal/basic与常量/validated constant unit、
绑定前/存活期间/结束后三位置，每次负极值/负数/阈值2/3/4/正极值六输入的显式真值输出。
既有 `==/<` 正例不改。全仓回查其他range/borrow能力门负例未发现第二处过时四比较表。

单文件CLI不加载/授权range std是独立既有边界；本片不改CLI生产、不伪造该入口支持。
project CLI覆盖公共stdlib加载，single codegen测试覆盖其已授权真实分析链，两者分账。
修复后必须选完整静审的 `cargo test --locked -p lang-cli --test range_project_cli`，
该完整目标已实际39/39通过，0 filtered/ignored（原35+新4），新四测试内部24次真实build/run
及144个输入输出对照；源码前后hash相同。CLI严格clippy、fmt/docs/diff/尺寸也通过；38项历史尺寸债无增长。


### 真实新 main 整合与联合复验

用户批准PR93后，官方fetch确认actual main `cedbe73ef3f30c9cf8ac5f0f067e77bb101c4b45`，
tree `3c61aee01286b803b6a8fdbc6f45a83e8bd6bab6`。保留CLI修复`b571b485`，正常merge为
`df3975e73479bde3bfed1c5433f2166149595900` / tree `35a1c8a4d6eb2662d2083a7d2d5ee950d6b33ac6`；
真实有序双父为`b571b485`、`cedbe73e`。仅索引/DAG有冲突，保留0314归档与0315 active并
重新生成图；生产相对新main仍只增加四个比较枚举，没有复制未合入实现或伪造父链。

| 新输入验收 | 精确选择 | 实际结果 |
|---|---|---|
| 0314 direct及共享Continue | codegen lib `owned_source_return_` / `owned_source_continue_` | 117/117、32/32 |
| Map source/return共享 | codegen lib `ssa::lower_frontend_tests::map_result_tests::` / `native::unit_tests::map_loop_owner_tests::` | 94/94、52/52，均原版普通源码链 |
| 0315及普通borrow边界 | codegen lib `ssa::range_lowering_tests::local_control::`、`ssa::range_lowering_tests::scalar_counts::`、`ssa::borrow_result_boundary_tests::` | 223/223、6/6、4/4 |
| CLI公共范围入口 | 完整 `range_project_cli` integration target | 39/39，0 filtered/ignored；旧负表和新增四op均逐名通过 |
| frontend既有事实 | `ownership_borrow_last_use` / `ownership_range_construction` | 8/8、14/14，0 filtered/ignored；是新输入实际执行，非沿用首轮CI |
| 静态门禁 | `cargo clippy --locked -p lang-codegen -p lang-cli --all-targets -- -D warnings`、fmt、docs/diff、尺寸base `cedbe73e` | 全通过；docs668页，38项历史尺寸债保留 |

合计589次执行，逐完整测试名去重562项；重复27项为0314结构3项与unit native24项，
不重复报功。所有选择0 ignored。前后及clippy后源码SHA均为
`7fadd767ad7e3976304d8685496e44dfb0f0b308fdb9cd97b2439ab353797d83`；精确命令、逐名清单、
日志摘要与恢复bundle留仓外。新base/head改变CI输入，必须重新取得双宿主物理CI证据，
不能复用首轮失败run或此前本地绿；尚未归档/合并0315。main CI由独立闭环跟踪。
