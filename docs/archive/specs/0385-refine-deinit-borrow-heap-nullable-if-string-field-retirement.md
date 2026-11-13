# SPEC-0385：`deinit` 字段借用、独立堆对象可空 `if` 与字符串字段接收者旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `deinit` 字段借用（`unit_lower/deinit_borrow.rs`）、字符串字段扩展接收者（`unit_lower/receiver/string_field.rs`）与独立堆对象可空 `if` 非空证明入口（`unit_lower/control/nullable_if/heap.rs` 及 `control/nullable_if.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0385` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在最新 `main` 与 SPEC-0384 基线上继续推进 E3 下一个切片 |
| 基线 | `46585e00`（SPEC-0384） |
| 分支 | `feature/spec-0385-refine-rc-nullable-when-map-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0380](0380-refine-deinit-nested-fields-and-box-payloads.md)、[SPEC-0383](../../archive/specs/0383-refine-final-legacy-retirement.md)、[SPEC-0384](../../archive/specs/0384-refine-legacy-module-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 [SPEC-0380](0380-refine-deinit-nested-fields-and-box-payloads.md)、[SPEC-0383](../../archive/specs/0383-refine-final-legacy-retirement.md) 与 [SPEC-0384](../../archive/specs/0384-refine-legacy-module-retirement.md) 已将 `deinit` 析构体、多层 `this` 字段借用/读取、可信字符串扩展方法接收者及堆对象可空 `if` 非空证明视图全量路由至 `ssa::refine` 的基础上，彻底退役 `unit_lower` 中已无任何生产或测试调用方的冗余 legacy 模块与分支（合计清理约 755 行旧路径代码）：

1. **删除 `unit_lower/deinit_borrow.rs`（262 行）**：移除仅服务于 legacy `deinit` / `this` 多跳字段路径的 `is_this_field_path`、`lower_this_field_borrow` 与 `read_this_field_path`，同步清理 `unit_lower.rs`、`aggregate.rs`、`borrow.rs` 与 `string_clone.rs` 中的挂载与分支。
2. **删除 `unit_lower/receiver/string_field.rs`（62 行）**：移除 `lower_string_field_receiver`（可信字符串扩展方法在 `ssa::refine` 中已默认激活 `migrated` 并支持 `r.field.ext()`），同步清理 `receiver.rs`。
3. **退役 `unit_lower/control/nullable_if/heap.rs` 与 `control/nullable_if.rs` 中的独立堆对象 `if` 证明入口（约 430 行）**：移除 `prepare_heap_nullable_if_entry`、`heap_if_direct_borrow_call`、`heap_if_call_qualifies` 与 `standalone_if_needs_root_borrow`，仅保留 `NamedNullableIfRoot`、`heap_if_source_context`、`validate_named_nullable_if_condition` 与 `heap_if_root_operand` 供 `heap/field.rs`、`string.rs` 及 `while` 复测证明共享。

## 2. 边界与不变量

- `unit_lower/field_borrow.rs` 仍保留供未带 `migrated` 触发项的普通只读借用参数字段读取与 `value class` 字段借用边界测试使用。
- `unit_lower/control/nullable_if/rc.rs` 与 `unit_lower/nullable_when*` 仍保留供 `rc_nullable_while_bounded_tests` 及 `native_nullable_when_tests` 的精确阶段边界测试使用。
- 所有合法源码经默认入口路由至 `ssa::refine` lowering 并通过 SSA verifier 与 LLVM IR 生成；通过 `with_per_family_lowering` 强制进入 `unit_lower` 时，已退役路径确定性返回 `UnsupportedNode` / `MissingFact`。

## 3. 验收标准

- [x] 删除 `crates/lang-codegen/src/ssa/unit_lower/deinit_borrow.rs` 与 `crates/lang-codegen/src/ssa/unit_lower/receiver/string_field.rs`，精简 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/heap.rs` 与 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if.rs`（净减约 755 行 legacy 代码）。
- [x] 新增 `ssa::refine_subset_tests::deinit_borrow_heap_nullable_if_and_string_field_receiver_route_to_refine` 与 `ssa::refine_tests::map_condition_tests::retired_deinit_borrow_and_heap_nullable_if_boundary_tests` 回归测试并全部通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 2 个退役项已从版本库移除 | 通过 | 逐个确认：2/2 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过 | 2/2 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- route_to_refine retired_` = `19 passed / 0 failed`（含本批四份 Spec 的具名回归测试） |

核对说明：

- 确认删除：`unit_lower/deinit_borrow.rs`、`unit_lower/receiver/string_field.rs`
- 具名测试通过：`ssa::refine_subset_tests::deinit_borrow_heap_nullable_if_and_string_field_receiver_route_to_refine`
- `scripts/gen_roadmap.py` 可执行且运行成功
- 验收标准中的复选框已全部勾选，本账本复核其可验证部分；历史已 squash，无法把改动单独归因到本 Spec 的提交。
