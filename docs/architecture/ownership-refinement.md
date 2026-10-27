# 所有权细化层

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 `ssa::refine`、子集预检或 inline nullable 表示时 · **唯一真源**：`lang-codegen` 代码与测试 · [ADR-0032](../adr/accepted/0032-ownership-refinement-layer.md) · [ADR-0033](../adr/accepted/0033-inline-nullable-representation.md) · [SPEC-0350](../archive/specs/0350-ownership-refinement-nullable-pilot.md)

## 分流

`lower_owned_unit_with_entry` 对每个无类型替换、无借用/carrier 返回的函数实例先做
确定性子集预检（`ssa::refine::subset`），只读 typed/ownership facts 与 AST 构造种类，不试跑 lowering。
receiver 只接受 Copyable enum（Value/Borrow）与普通 class（Borrow/Inout）。预检通过且函数签名或函数体
触及任一 nullable 类型、或使用已迁入的家族（`when`、enum `is`、字段替换、`Inout` 参数或 receiver、
`&` 实参）时，函数体由
`ssa::refine` lowering；失败即报错，不回退。其余函数仍走 `unit_lower` 的按家族路径；带 `Inout` 参数的函数若未进入细化层，以 UnsupportedNode
拒绝（unit planning 已为 `Inout` 参数分配 exclusive loan 实体）。精确子集以
SPEC-0350 §3 的已实现清单与 SPEC-0356 为准。

## 两段 lowering

- 细化（`refine/build`）：facts 映射为槽位 CFG。析构点、Borrow 实参 loan、Value 交付、null 比较的
  proof view 都是显式语句；view 只在 `if`/`while` 条件真边上建立，并在 owner 析构、移动或控制转移
  前结束。null 比较 operand 的析构点在两条出边各发一次。
- SSA 构造（`refine/emit`）：先把单前驱的直线后继并入其 `Goto` 前驱，再按槽位活跃性生成 block
  参数，非空目标末尾追加 view。abort 块继承同一分支另一条边的活跃集合，存活 owner 因此在 Abort
  边上有归属；产物经完整 verifier。

已接入：Boolean/整数/String 标量与 checked 算术、String 字面量/拼接/比较、`println`、顶层直接调用、
`if`/`while`/短路、`when`（无 subject 时 Boolean 条件按序测试；标量 subject 按值比较或测整数 Range 成员；enum subject
比较判别值，值位置无 `else` 时落空边为不可达 Abort；命中边消费 `WhenAlternativeMatch`；String subject 的无插值 literal/Group 首核
消费 0357 的 `WhenComparisonCompleted` 与 `WhenAlternativeNoMatch`，比较先于候选清理，
直接 Borrow String 参数通过 caller loan 比较，未扩展 borrow-result binding/field/index/复杂候选；
新增真实源/计数测试的官方 CI 结果见 [0360](../specs/active/0360-string-when-refinement.md)）、
Copyable 非泛型且 payload 全为标量的 enum（case 构造、`is`/`!is`、无 payload case 等值、经
`TaggedPayloadPlace` 读取窄化名、`this` 或隐式 `this` 的 payload）及 enum 方法的 receiver、`loop` 与 `break`/`continue`（跳转前结束本轮循环打开的 view）、inline nullable、字段只含标量/String 的非泛型 class 及其 pointer-like nullable、
class 构造、Copyable 字段读取（经窄化 view、局部 owner 的临时 shared loan、Borrow/Inout 参数或当前
receiver 的 loan）、`!!`（Consume，null 边 Abort）；赋值与 `Inout`（SPEC-0358）：标量 `Inout` 参数经
`LoanMutate` 整体替换，`&local` 借出 root place 并在调用后读回（MoveOnly 经 `RootPlaceTake`），`&p` 继续
传递 `Inout` 参数的 loan；`owner.f = v` 先求值 RHS，再经 exclusive loan `HeapFieldReplace`，并由替换
消费 `BeforeReplacement` 析构；receiver 调用中 Borrow receiver 在实参前借出，`Inout` receiver 的
exclusive loan 在调用点激活。

## inline nullable 布局

`SsaTypeKind::InlineNullable { inner, niche }`：String 与顺序容器用 bytes/buffer 指针 niche（空串与
空容器使用非 null 的 `koven.zst.sentinel`），其余 inner 用 `{i1, T}`。复用 NullableWrap/Null/IsNull/
Take 与 `NullableBranch`；drop glue 只在 present 时释放 payload。旧 `unit_lower` 的存储能力仍只到
pointer-like nullable。

实现入口：`crates/lang-codegen/src/ssa/refine.rs` 及 `refine/` 子模块、`ssa/inline_nullable.rs`、
`llvm/runtime/inline_nullable.rs`；测试在 `ssa/refine_tests.rs` 与 `llvm/inline_nullable_tests.rs`。
