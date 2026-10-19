# 中立 lowering 支撑

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 lowering 错误归属、String decoder 或依赖边界时 · **唯一真源**：`lang-codegen::ssa::lowering_support` 与对应测试

私有 `ssa::lowering_support` 承接原 `LoweringError/Kind`、`Some(span)` 错误构造与
`string_literal::decode_plain`。single/unit adapters与unit planner保留原本地调用名，
`ssa`根及single adapter错误类型路径继续re-export；decoder算法逐字移动，driver和validation顺序不变。
这两个driver原本已经共用decoder，本片只修正职责归属，不新增String、常量求值或receiver能力。

[有界配对合同](../archive/specs/0255-neutral-lowering-support.md)使用逻辑source key、Span、
StringOwner结构和definition→use关系，不比较裸TypeId。单文件Borrow String receiver调用仍
UnsupportedNode，unit保留原支持；const与basic能力不混用。实际源码词法依赖护栏禁止unit回依赖
single adapter与IR core反向读取support；它不是完整Rust依赖图，也不代表P4全部收敛。

## 常量物化操作共享核心

[0341 首片](../specs/active/0341-shared-lowering-core.md)在 SSA 同层私有
`constant_materialization::operation` 共享 Boolean/Integer/Char/StringTemporary 四路操作映射。
single 与 unit 仍按各自原顺序查询 descriptor/materialization、验证身份、取得 SSA type，
随后将共享映射错误附回原 Span 并经原 append 路径发布；Group origin、owner/drop 和入口能力不变。
它依赖已验证值类型及 SSA model，不属于禁止 model 依赖的 `lowering_support`。
实际生产依赖扫描覆盖新模块与子树，并保留 IR/support 不能反向依赖它的边界。
这只是 D10 第一片；两入口其余 driver、CFG 与 cleanup 的收敛仍由 active Spec 分阶段推进。

## 整数表达式共享流程

`ssa::expression_core::lower_integer` 统一 descriptor/类型/receiver 身份校验、八整数准入、
receiver 一次求值、三态传播、结果 SSA type 查询时点及 inv/checked conversion 提交选择。
两入口以原生 local/qualified ID 的窄接口桥接事实和提交，共享原 `LoweredValue` 三态；
原 dispatch、外层 temporary/drop 时点及结果类型错误差异保持。single 的缺映射仍为
UnsupportedNode，unit 仍为 MissingFact。整数小片当时保留两份 `checked.rs` 的 CFG；后继共同构造见下节。依赖扫描覆盖新核心，禁止反向依赖 driver/planner/LLVM。
真实来源反序、basic/const 配对、一次求值、Borrow 实体链及控制退出 native 已按
[0341 整数小片](../specs/active/0341-shared-lowering-core.md#11-整数小片实际验收2026-10-19)验收；
这不关闭整体表达式、语句、driver 与 CFG/cleanup 收敛责任。

## checked scalar CFG 核心

`ssa::expression_core::checked` 统一checked算术及转换的Boolean事实查询、双Value结果append、
有序角色读取、同型failure/success块与边、真边Abort、success角色恢复和原scalar交付。
两入口删除对应完整CFG算法及arithmetic操作构造，只通过四窄端口读取事实、借出IR cursor、
读取与恢复自身角色。append后才收集carry；错误Span与原首错、Value/access列顺序保持。
Abort不新增cleanup。核心依赖SSA model，位于中立support之外，依赖扫描覆盖整个子树。
[0341 checked验收](../specs/active/0341-shared-lowering-core.md#17-checked-scalar-cfg-小片实际验收2026-10-19)
记录普通SSA/LLVM与正常native结果。该checked小片当时未统一carry槽关联、alias索引与参数重绑；
temporary角色后继见下节，其它角色和CFG责任继续开放。
原single linear/Map与unit pending/receiver/loan恢复helper继续保留，不把差异隐藏为永久策略。

## temporary alias carry 核心

`ssa::carry` 统一temporary key有序收集、首个同source物理列复用、miss时才查询完整EntityType并追加，
以及该列全部key向当前Value的写回。single与unit删除四段重复循环，三存储端口仅投影source、
借用key字段及构造其它角色为空的槽；local/source-qualified key不转换。seed不去重，不能按旧source
全局合并参数，命中列不提前查询type。原single逐列temporary/capture/proof交错、unit独立恢复阶段，
以及长度/kind/clear/首错时点保留。[0341 temporary验收](../specs/active/0341-shared-lowering-core.md#19-temporary-alias-carry-小片实际验收2026-10-19)
记录真实Borrow Group配对与普通回归；capture loan、named/pending等其它角色和多入边合流仍待统一。
核心依赖model、中立error与Span，不反向读取checked/driver/planner/LLVM；IR/support禁止依赖carry。

## 普通 statement 顺序核心

`ssa::statement_core` 统一 inner 成功后非 Diverged 才发 AfterStatement cleanup 的 wrapper，
以及顺序执行、遇错误或 Diverged 即停、全正常返回 Unit 的循环。两入口的窄适配调用原
inner 与原 emit_drops；unit 在适配内保留现场 source-qualified statement 身份。
空容器由 wrapper 正常清理，单个 statement 的 Value 原样返回；single 的 Block/LambdaBody
与 unit 另含 ControlBody 的能力差异保持。普通statement小片当时未纳入 Map 专用 inner、
control-tail/lambda 尾值、原 cleanup 执行器与 CFG 状态运输。依赖门禁止 expression 反向读取 statement。
真实来源成对顺序、非空 AfterStatement、提前 return 和既有普通 SSA/native 已按
[0341 statement 小片](../specs/active/0341-shared-lowering-core.md#13-普通-statement-小片实际验收2026-10-19)验收；
共同 CFG、owner/loan、调用与 driver 层仍待收敛。

同一核心的 `lower_tail_sequence` 还统一已支持control-tail的prefix顺序、末项三态保留和
非Diverged body cleanup。空tail直接Unit；适配仍决定single ControlBody与unit额外LambdaBody
能力及fallback，原branch结果交付/BranchExit不动。single独立lambda的空体清理和结果合同
仍保留，不能与tail规则混合；[0341 tail验收](../specs/active/0341-shared-lowering-core.md#15-control-tail-小片实际验收2026-10-19)记录固定普通来源与保留边界。

## nullable 最后读取与 cleanup

两入口 scalar nullable comparison 先按 typed null descriptor 和已登记的 aggregate/pointer
表示读取 presence，再消费原 operand 的最后使用 drop。String 与 MoveOnly inline 的单文件
Boolean 判空、unit 最后 if 判空已有普通源码 SSA/native 证据；借用 aggregate 继续经过
既有 Copyable 门。单文件 owning if 的 NullableBranch、资源词法和后续消费事实保持。

两 frontend 对 nullable temporary 与 null literal 的比较，在实际 operand 完成后发布
AfterBinaryOperands 结果事实；Group 归一叶和 origin，借用结果与未完成值不产生 owning drop。
两入口以精确结果事实和已登记的 aggregate/NullableHandle 表示读取 presence，再清理临时
owner；Resource temporary 在 if body 前析构，具名 Resource 的词法清理不变。

## 现存入口差异

普通unit字段赋值与current receiver字段赋值不同：`unit_lower/assignment.rs`先处理后者，
一般aggregate projection仍以目标Span拒绝；M1A通过字段replace更新，不证明普通
`report.text = ...`已可执行。这里仅静态核对当前分流，未新增或执行源码回归。

普通`Resource?`工厂即使没有Map或判空，single仍在函数返回类型登记时被resource nominal
验证拒绝。现存`owned_null_cleanup_tests::single_nullable_factory_planning_boundary_precedes_any_comparison`
固定完整函数Span的UnsupportedNode，并有unit同源verifier正例；该边界早于body，
与0296的具名/临时判空清理无关。本次只读核对测试与分流，没有重跑。
