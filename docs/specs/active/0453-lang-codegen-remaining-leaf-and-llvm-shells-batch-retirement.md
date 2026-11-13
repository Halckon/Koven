# SPEC-0453: `lang-codegen` 剩余同目录 `include_str!` 薄壳、`llvm` 子树同级模块与跨父测试小文件批量收敛（37 个）

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/`

| 字段 | 值 |
|---|---|
| 编号 | 0453 |
| 标题 | `lang-codegen` 剩余同目录 `include_str!` 薄壳、`llvm` 子树同级模块与跨父测试小文件批量收敛（37 个） |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-11-12 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0451, SPEC-0452 |

---

## 1. 背景与动机

在 [SPEC-0452](0452-lang-codegen-non-ssa-leaf-and-sibling-shells-batch-retirement.md) 完成 `lang-codegen` 非 `ssa` 子树首轮批量收敛后，仍残留三类可安全收敛的薄壳文件：
1. 同目录下含相对路径 `include_str!` 的直接父子薄壳（3 个）；
2. `llvm.rs`、`llvm/adapter.rs`、`llvm/map.rs`、`llvm/runtime.rs` 及顶层 `*_cases.rs` 下的同级叶子模块（26 个）；
3. 最佳适配（Best-Fit）跨父测试宿主可容纳的 `ssa` / `llvm` / `native` 纯叶子测试模块（8 个）。

本 Spec 一次性批量收敛上述 37 个独立文件，使 `crates/lang-codegen/src` 下所有 `< 500` 行的可退场测试薄壳基本清零，手写 Rust 文件总数从 `1682` 进一步降至 `1645`。

---

## 2. 设计与收敛方案

### 2.1 收敛后父模块与宿主行高

| 模块路径 | 收敛后行高 |
|---|---:|
| `lib.rs` | 928 |
| `llvm.rs` | 939 |
| `llvm/adapter.rs` | 1764 |
| `llvm/adapter/call.rs` | 685 |
| `llvm/adapter/module_lowering.rs` | 900 |
| `llvm/adapter/range.rs` | 681 |
| `llvm/closure.rs` | 680 |
| `llvm/container_tests.rs` | 937 |
| `llvm/layout.rs` | 918 |
| `llvm/map.rs` | 851 |
| `llvm/map/mutation.rs` | 852 |
| `llvm/runtime.rs` | 1011 |
| `llvm/runtime/abort.rs` | 673 |
| `llvm/string.rs` | 719 |
| `native.rs` | 505 |
| `native/unit_const_handoff_contract_tests.rs` | 852 |
| `native/unit_constant_tests.rs` | 862 |
| `native/unit_container_size_tests.rs` | 567 |
| `native/unit_map_loop_owner_tests.rs` | 920 |
| `native/unit_map_return_tests.rs` | 637 |
| `native/unit_numeric_literal_tests.rs` | 640 |
| `native/unit_tests.rs` | 1924 |
| `native_borrow_value_field_cases.rs` | 712 |
| `native_map_provider_tests.rs` | 961 |
| `native_ordered_map_tests.rs` | 765 |
| `native_ordered_map_tests/fixtures.rs` | 954 |
| `native_sequential_for_tests.rs` | 920 |
| `native_sequential_for_tests/cleanup_tests.rs` | 842 |
| `native_tests.rs` | 1123 |
| `native_unit_source_test_support.rs` | 936 |
| `ssa/closure_operation_tests/escape_tests.rs` | 780 |
| `ssa/lower_frontend_owned_source_return_native_tests.rs` | 978 |
| `ssa/lower_frontend_owned_source_return_tests.rs` | 972 |
| `ssa/lower_frontend_tests.rs` | 979 |
| `ssa/map_provider_exits_pipeline_tests.rs` | 950 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer.rs` | 972 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts.rs` | 978 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery.rs` | 845 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph.rs` | 978 |
| `ssa/mod.rs` | 969 |
| `ssa/nullable_map_consume_lowering_tests/resource.rs` | 784 |
| `ssa/sequential_for_lowering_tests.rs` | 799 |
| `ssa/sequential_for_lowering_tests/cleanup_tests.rs` | 977 |
| `test_support.rs` | 746 |

### 2.2 退役薄壳清单（37 个）

- `bitwise_test_support.rs`
- `llvm/adapter/abort.rs`
- `llvm/adapter/borrowed_generate.rs`
- `llvm/adapter/callable.rs`
- `llvm/adapter/constant.rs`
- `llvm/adapter/integer.rs`
- `llvm/adapter/integer_conversion.rs`
- `llvm/adapter/integer_range.rs`
- `llvm/adapter/map_iteration.rs`
- `llvm/adapter/nullable.rs`
- `llvm/adapter/root_exchange.rs`
- `llvm/adapter/storage.rs`
- `llvm/adapter/text.rs`
- `llvm/adapter/text_copy.rs`
- `llvm/closure_tests.rs`
- `llvm/container_tests/provider_cfg.rs`
- `llvm/debug.rs`
- `llvm/entry.rs`
- `llvm/map/iteration.rs`
- `llvm/runtime/inline_nullable.rs`
- `llvm/runtime/map_result.rs`
- `llvm/runtime/narrow_tests.rs`
- `llvm/runtime/string.rs`
- `llvm/type_map.rs`
- `llvm/utf8.rs`
- `native/unit_companion_boundary_tests.rs`
- `native/unit_const_handoff_test_support.rs`
- `native/unit_map_break_tests.rs`
- `native/unit_map_continue_tests.rs`
- `native_local_object_constant_qualifier_tests.rs`
- `native_map_provider_scope_value_births_tests.rs`
- `native_multi_borrow_cases.rs`
- `native_nested_borrow_cases.rs`
- `native_sequential_for_tests/conditional_owned_source_tests.rs`
- `ssa/closure_operation_tests/escape_tests/projected_content_tests.rs`
- `ssa/field_replace_lower_tests.rs`
- `ssa/nullable_map_consume_lowering_tests/resource/runtime.rs`

---

## 3. 验收标准

1. 上述 37 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `1682` 降至 `1645`。

## 4. main 集成修复检查点（未完成验收）

本节仅记录冲突修复，不改变原验收标准，不宣称 SPEC-0451–0453 或 E3 完成。
输入为 main `fbfb5a3a`、原始批次 tip `7451e9a3`、修复版前序批次 `1a30b51d`；
集成后的代码检查点为 `5f0e88d2`。旧的全选一侧冲突解决混合了不同模块布局；
本轮恢复有语义归属的 helper/test 子模块、移除跨域 packed 副本，保留原断言与 fixture。
其中 §2.2 的若干文件已恢复，原删除数量和 §2.1 行高不是当前验收结果，仍须重新对账。

| 检查 | 实际结果与边界 |
|---|---|
| 默认及 `loan-observation-test-support` 的 `cargo check -p lang-codegen --all-targets` | 均通过；后续两配置 Clippy 亦通过 |
| `cargo clippy -p lang-codegen --all-targets [--features loan-observation-test-support] -- -D warnings` | 两配置均退出码 0 |
| `cargo fmt --all -- --check`、`git diff --check` | 均退出码 0；格式化仅覆盖本批次已有失败的 codegen 文件 |
| `cargo test -p lang-codegen --lib --features loan-observation-test-support -- --list` | 3119 项；相对前序 3118 项，末尾名称多重集无缺项，仅新增 #407 nullable 分类用例；名称核对不能代替行为证明 |
| 全 feature lib 回归 | 1800 秒超时，部分 438 项通过、1 项失败，无最终退出码，不计通过 |
| feature SSA/LLVM 较广回归（`-- ssa:: llvm::`） | 1200 秒超时，部分 2016 项通过、46 项失败，无最终退出码，不计通过 |
| 上述 46 项失败集合原名重跑 | 修复 BorrowReturn 唯一来源 verifier 与 #407 分类 guard 后，46 passed / 0 failed，退出码 0 |
| 原子性子进程精确回归 | `native::unit_tests::boxed_enum_tests::for_atomic::actual_for_rejections_are_atomic_before_reservation_and_llvm`：1 passed；修正迁移后的 `--exact` 路径，保留确实运行 1 项的断言 |
| 受影响测试域定向回归 | nullable operation、closure operation、lowering entry、multi-borrow range、map stage gate、scalar ABI、BorrowReturn verifier、LLVM narrow/provider CFG、grouped/negated/leaf-not owner：feature 配置 120 passed / 0 failed；默认配置追加 #407 与原子性子进程测试，122 passed / 0 failed；均退出码 0 |
| 独立只读审查 | 核对 17 组 packed 正文恢复、挂载、fixture 与 #407 保留；未发现本轮尚未解决的具体问题，不替代全量与 CI 验收 |

剩余：完整回归及双宿主 CI、原退役数量与宿主职责重新对账。不得仅凭编译通过归档。
