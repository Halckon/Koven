# 所有权细化层

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 `ssa::refine`、子集预检或 inline nullable 表示时 · **唯一真源**：`lang-codegen` 代码与测试 · [ADR-0032](../adr/accepted/0032-ownership-refinement-layer.md) · [ADR-0033](../adr/accepted/0033-inline-nullable-representation.md) · [SPEC-0350](../specs/active/0350-ownership-refinement-nullable-pilot.md)

## 分流

`lower_owned_unit_with_entry` 对每个无 receiver、无类型替换、无借用/carrier 返回的函数实例先做
确定性子集预检（`ssa::refine::subset`），只读 typed/ownership facts 与 AST 构造种类，不试跑 lowering。
预检通过且函数签名或函数体触及任一 nullable 类型时，函数体由 `ssa::refine` lowering；失败即报错，
不回退。其余函数仍走 `unit_lower` 的按家族路径。精确子集以 SPEC-0350 §3 的已实现清单为准。

## 两段 lowering

- 细化（`refine/build`）：facts 映射为槽位 CFG。析构点、Borrow 实参 loan、Value 交付、null 比较的
  proof view 都是显式语句；view 只在 `if`/`while` 条件真边上建立，并在 owner 析构、移动或控制转移
  前结束。null 比较 operand 的析构点在两条出边各发一次。
- SSA 构造（`refine/emit`）：先把单前驱的直线后继并入其 `Goto` 前驱，再按槽位活跃性生成 block
  参数，非空目标末尾追加 view。abort 块继承同一分支另一条边的活跃集合，存活 owner 因此在 Abort
  边上有归属；产物经完整 verifier。

已接入：Boolean/整数/String 标量与 checked 算术、String 字面量/拼接/比较、`println`、顶层直接调用、
`if`/`while`/短路、inline nullable、字段只含标量/String 的非泛型 class 及其 pointer-like nullable、
class 构造、Copyable 字段读取、`!!`（Consume，null 边 Abort）。

## inline nullable 布局

`SsaTypeKind::InlineNullable { inner, niche }`：String 与顺序容器用 bytes/buffer 指针 niche（空串与
空容器使用非 null 的 `koven.zst.sentinel`），其余 inner 用 `{i1, T}`。复用 NullableWrap/Null/IsNull/
Take 与 `NullableBranch`；drop glue 只在 present 时释放 payload。旧 `unit_lower` 的存储能力仍只到
pointer-like nullable。

实现入口：`crates/lang-codegen/src/ssa/refine.rs` 及 `refine/` 子模块、`ssa/inline_nullable.rs`、
`llvm/runtime/inline_nullable.rs`；测试在 `ssa/refine_tests.rs` 与 `llvm/inline_nullable_tests.rs`。
