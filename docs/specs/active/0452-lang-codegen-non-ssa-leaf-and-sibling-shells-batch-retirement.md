# SPEC-0452: `lang-codegen` 非 `ssa` 子树叶子薄壳与同级测试小文件批量收敛（129 个）

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/`

| 字段 | 值 |
|---|---|
| 编号 | 0452 |
| 标题 | `lang-codegen` 非 `ssa` 子树叶子薄壳与同级测试小文件批量收敛（129 个） |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-11-12 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0451 |

---

## 1. 背景与动机

在 [SPEC-0451](0451-cross-parent-leaf-test-shells-batch-retirement.md) 完成 `ssa` 测试域跨父模块打包收敛后，`crates/lang-codegen/src/` 其余目录（`lib.rs` 顶层用例、`llvm/`、`llvm/map/`、`native/`、`native_tests.rs`、`native_map_provider_tests.rs`、`native_unit_source_test_support.rs` 等）仍分布大量 17–476 行的独立薄壳文件。本 Spec 通过直接父模块内联（68 个）与同级宿主折叠（61 个）一次性批量收敛这 129 个独立文件，同时严格保证所有非例外修改后文件物理行数 `< 1000`（历史基线 `native/unit_tests.rs` 从 1951 行进一步净减至 1925 行）。

---

## 2. 设计与收敛方案

### 2.1 收敛后父模块与同级宿主行高

| 模块路径 | 收敛后行高 |
|---|---:|
| `lib.rs` | 928 |
| `llvm.rs` | 939 |
| `llvm/container_tests.rs` | 937 |
| `llvm/debug_tests.rs` | 739 |
| `llvm/layout_tests.rs` | 661 |
| `llvm/map.rs` | 851 |
| `llvm/object_tests.rs` | 849 |
| `native/unit_boxed_enum_tests.rs` | 909 |
| `native/unit_constant_tests.rs` | 862 |
| `native/unit_container_size_tests.rs` | 318 |
| `native/unit_for_report_resource_tests.rs` | 674 |
| `native/unit_generic_container_tests.rs` | 753 |
| `native/unit_map_consume_tests.rs` | 851 |
| `native/unit_map_loop_owner_tests.rs` | 722 |
| `native/unit_map_operand_last_use_tests.rs` | 870 |
| `native/unit_owned_null_cleanup_tests.rs` | 968 |
| `native/unit_test_directory.rs` | 886 |
| `native/unit_tests.rs` | 1925 |
| `native_borrow_value_field_cases.rs` | 335 |
| `native_boxed_enum_tests.rs` | 862 |
| `native_map_provider_d10_w09_tests.rs` | 785 |
| `native_map_provider_tests.rs` | 838 |
| `native_map_tests.rs` | 749 |
| `native_nullable_when_tests.rs` | 913 |
| `native_nullable_when_tests/sibling_route.rs` | 487 |
| `native_numeric_literal_tests.rs` | 740 |
| `native_ordered_map_tests.rs` | 765 |
| `native_pending_construction_tests.rs` | 873 |
| `native_runtime_constructor_tests.rs` | 761 |
| `native_sanitizer_tests.rs` | 609 |
| `native_scalar_joint_tests.rs` | 817 |
| `native_sequential_for_tests.rs` | 921 |
| `native_string_length_tests.rs` | 750 |
| `native_tests.rs` | 1123 |
| `native_unit_source_test_support.rs` | 937 |
| `owner_own_call_tests.rs` | 848 |
| `owner_replace_break_downstream_tests.rs` | 977 |
| `owner_return_tests.rs` | 779 |
| `test_support.rs` | 475 |

### 2.2 退役薄壳清单（129 个）

- `llvm/aggregate.rs`
- `llvm/aggregate_tests.rs`
- `llvm/borrowed_generate_tests.rs`
- `llvm/emission_failure.rs`
- `llvm/entities.rs`
- `llvm/entry_tests.rs`
- `llvm/map/drop.rs`
- `llvm/map/hash.rs`
- `llvm/map/order.rs`
- `llvm/map/rehash.rs`
- `llvm/map/require.rs`
- `llvm/map/with_value.rs`
- `llvm/nullable_tests.rs`
- `llvm/ordered_map_test_support.rs`
- `llvm/scalar.rs`
- `llvm/string_tests.rs`
- `llvm/synthetic_zst_tests.rs`
- `llvm/tagged.rs`
- `llvm/utf8_tests.rs`
- `native/unit_analysis_test_support.rs`
- `native/unit_bitwise_tests.rs`
- `native/unit_borrow_string_tests.rs`
- `native/unit_constant_owner_tests.rs`
- `native/unit_container_append_tests.rs`
- `native/unit_container_clear_tests.rs`
- `native/unit_container_insert_at_tests.rs`
- `native/unit_container_remove_at_tests.rs`
- `native/unit_container_remove_first_tests.rs`
- `native/unit_container_remove_last_tests.rs`
- `native/unit_field_resource_replace_tests.rs`
- `native/unit_for_atomic_tests.rs`
- `native/unit_for_boundary_tests.rs`
- `native/unit_for_conditional_receiver_tests.rs`
- `native/unit_for_resource_tests.rs`
- `native/unit_for_tests.rs`
- `native/unit_generic_body_budget_tests.rs`
- `native/unit_generic_body_tests.rs`
- `native/unit_map_key_return_tests.rs`
- `native/unit_map_word_frequency_tests.rs`
- `native/unit_non_null_tests.rs`
- `native/unit_owned_map_borrow_tests.rs`
- `native/unit_owned_source_continue_tests.rs`
- `native/unit_owned_source_return_tests.rs`
- `native/unit_receiver_two_phase_tests.rs`
- `native/unit_resource_deinit_tests.rs`
- `native/unit_resource_exchange_tests.rs`
- `native/unit_root_primitive_tests.rs`
- `native/unit_runtime_constructor_tests.rs`
- `native/unit_runtime_global_tests.rs`
- `native/unit_string_clone_tests.rs`
- `native/unit_temporary_null_cleanup_tests.rs`
- `native_abort_output_tests.rs`
- `native_bitwise_tests.rs`
- `native_borrow_last_use_cases.rs`
- `native_borrow_place_cases.rs`
- `native_borrow_result_tests.rs`
- `native_borrow_storage_cases.rs`
- `native_constant_tests.rs`
- `native_field_replace_tests.rs`
- `native_field_resource_replace_tests.rs`
- `native_map_constant_operand_tests.rs`
- `native_map_for_scalar_tests.rs`
- `native_map_promotion_cases.rs`
- `native_map_provider_births_tests.rs`
- `native_map_provider_borrow_exits_tests.rs`
- `native_map_provider_conditional_tests.rs`
- `native_map_provider_control_flow_tests.rs`
- `native_map_provider_exits_tests.rs`
- `native_map_provider_formation_tests.rs`
- `native_map_provider_future_use_tests.rs`
- `native_map_provider_history_tests.rs`
- `native_map_provider_local_arguments_tests.rs`
- `native_map_provider_local_exits_tests.rs`
- `native_map_provider_local_future_tests.rs`
- `native_map_provider_local_reassignment_tests.rs`
- `native_map_provider_local_resource_future_tests.rs`
- `native_map_provider_local_resources_tests.rs`
- `native_map_provider_local_sources_tests.rs`
- `native_map_provider_nested_exits_tests.rs`
- `native_map_provider_nested_tests.rs`
- `native_map_provider_resource_continuation_tests.rs`
- `native_map_provider_resource_forms_tests.rs`
- `native_map_provider_resource_tests.rs`
- `native_map_provider_scope_future_tests.rs`
- `native_map_provider_scope_lambda_body_tests.rs`
- `native_map_provider_scope_lowering_tests.rs`
- `native_map_provider_scope_member_parameter_tests.rs`
- `native_map_provider_sequential_tests.rs`
- `native_map_provider_source_forms_tests.rs`
- `native_map_provider_stack_tests.rs`
- `native_map_provider_temporary_argument_cfg_tests.rs`
- `native_map_provider_temporary_arguments_tests.rs`
- `native_map_provider_temporary_delivery_tests.rs`
- `native_map_provider_temporary_exits_tests.rs`
- `native_map_provider_temporary_forms_tests.rs`
- `native_map_provider_temporary_tests.rs`
- `native_map_require_cases.rs`
- `native_map_with_cases.rs`
- `native_non_null_assertion_tests.rs`
- `native_nullable_rc_proof_tests.rs`
- `native_nullable_when_tests/products.rs`
- `native_ordered_map_tests/probe.rs`
- `native_ordered_map_tests/runner.rs`
- `native_owned_map_borrow_cases.rs`
- `native_rc_nullable_while_tests.rs`
- `native_resource_exchange_tests.rs`
- `native_root_primitive_tests.rs`
- `native_runtime_constructor_tests/evaluation.rs`
- `native_runtime_constructor_tests/storable_helpers.rs`
- `native_sanitizer_tests/map_provider.rs`
- `native_sequential_for_tests/atomic_tests.rs`
- `native_sequential_for_tests/owned_source_tests.rs`
- `native_sequential_for_tests/unit_storage_boundary_tests.rs`
- `native_sequential_for_tests/unit_storage_tests.rs`
- `native_string_clone_tests.rs`
- `native_tests/map_sibling_route_tests.rs`
- `owner_alias_return_tests.rs`
- `owner_boolean_comparison_tests.rs`
- `owner_group_condition_tests.rs`
- `owner_loan_receipt_sibling_tests.rs`
- `owner_negated_condition_tests.rs`
- `owner_pair_call_tests.rs`
- `owner_replace_break_ci_tests.rs`
- `owner_replace_return_tests.rs`
- `owner_short_circuit_leaf_not_tests.rs`
- `owner_short_circuit_tests.rs`
- `test_support/diagnostics.rs`
- `unit_container_origin_bridge_tests.rs`
- `unit_type_origin_bridge_tests.rs`

---

## 3. 验收标准

1. 上述 129 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `1811` 降至 `1682`。

## 4. 集成后恢复项对账（部分进展）

输入检查点 `baf0b364` 已续接 #421 冲突修复 `4e201f4b`。当时 §2.2 的 129 项中，
125 个路径不再存在，以下 4 个文件仍保留；路径不存在仅是物理清单检查，不证明内容完整迁移。
§2.1 行高及 §3 总文件数是原批次目标，不作为当前验收结果。

| 恢复项（相对 `crates/lang-codegen/src/`） | 本轮处理 |
|---|---|
| `owner_group_condition_tests.rs` | 将完整同名模块并回原 `native_unit_source_test_support.rs` |
| `owner_negated_condition_tests.rs` | 同上，保留 Not parity、真实 Return receipt 与 native 输出断言 |
| `owner_short_circuit_leaf_not_tests.rs` | 同上，继续使用原父模块导入的 short-circuit helper |
| `native_nullable_rc_proof_tests.rs` | 保留 447 行独立模块；内联父文件已为 955 行，不能直接合并且不应再次跨域挂载 |

三个 owner 模块各含 3 个测试；同名内联后层级、可见性、fixture、测试身份和配置边界不变。
去掉外层缩进后，三个正文分别与输入文件逐字节相同。父文件从 800 行变为 955 行，
未扩大可见性、提高尺寸上限、压行或删除断言。

| 验证 | 实际结果 |
|---|---|
| feature 配置 `cargo test -p lang-codegen --lib --features loan-observation-test-support -- owner_group_condition_tests owner_negated_condition_tests owner_short_circuit_leaf_not_tests` | 修改前后均 9 passed / 0 failed，退出码 0；完整测试名称一致，包含真实 native 执行 |
| 默认配置上述 9 项，加 #421 的 readonly wrapper、Legacy 对照及 nullable 分类用例 | 12 passed / 0 failed，退出码 0 |
| 默认及 feature 的 `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 均退出码 0 |
| fmt、Rust 尺寸门禁 | 均退出码 0；内联宿主 955 行 |
| 独立只读审查 | 核对三个旧文件、完整 diff、挂载、private helper 来源、fixture、cfg 与位置敏感风险，未发现本候选的具体问题 |

`0cf1d025` 检查点的原清单物理删除进度为 **128/129**，当时剩余 nullable Rc 文件待办。
125 项既有迁移的完整性、原总文件数目标、完整回归与双宿主 CI 仍须验收；本 Spec 保持
`in-progress`，不以本轮三个模块的有界通过替代整批验收。

### 4.2 同步 PR421 与新 owner/borrow 主干后的职责文件恢复

以 `4855b620` 为集成输入，同步 PR421 `9526753c`（含 main `3b7a9a7a`）。唯一
modify/delete 冲突是 `native_borrow_result_tests.rs`：此前的 18 个测试已内联到
`native_boxed_enum_tests.rs::borrow_result_tests`；新主干在原职责文件追加 12 个真实
native loop 用例，源文件从 271 行增为 568 行。继续内联会让当时 868 行父文件增长到
约 1165 行，违反尺寸护栏。因此恢复 canonical 文件，正文与 PR421 输入逐字节相同；
删除旧内联副本，把 `native_tests::borrow_result_tests` 从 re-export 改回直接模块声明。
没有复制测试、删掉新增矩阵或塞到无关宿主。

原有 helper 接口仍在 `native_tests::borrow_result_tests`；实际 libtest 路径回到 main
的 `native_tests::borrow_result_tests::*`。全清单发现 3269 项 feature 测试，末尾名称的
多重集与新 PR421 完全一致，但清单不证明执行通过。独立只读审查核对 18 旧 + 12 新测试、
两个内部 helper、fixture 入口、`super` 作用域及精确 FQN 引用，未发现本轮具体问题。

| 验证 | 实际结果 |
|---|---|
| feature all-targets check | 退出码 0 |
| 原 borrow native 全模块、三组 owner 与 local reassignment 合选 | 300 秒超时，45 项中观察到 42 项 passed，无观察到失败；缺少汇总，不计整组通过 |
| 收窄至新增 12 项 borrow native、原三组 owner 9 项、local reassignment 6 项 | 默认与 feature 均 27 passed / 0 failed / 0 ignored，退出码均 0 |
| 默认与 feature 的 all-targets Clippy（`-D warnings`） | 均退出码 0 |
| fmt、文档、Rust 尺寸门禁 | 均退出码 0；恢复文件 568 行、原 boxed 宿主降为 590 行，不提高上限 |

当前物理进度回调为 **127/129**，保留 `native_nullable_rc_proof_tests.rs` 与
`native_borrow_result_tests.rs` 两个职责文件。0451 的 21 项及 0453 的 7 项保留决定不变，
三个 Spec 合计保留 30 个原目标路径；该数量不是语义验收。本 Spec 继续 `in-progress`，
完整回归、既有迁移审计与双宿主 CI 尚未完成，不归档。

### 4.3 修复 sanitizer 静态门禁的退役路径消费者

集成分支 `2828b8d7` 的 [push CI 43542804418](https://github.com/Halckon/Koven/actions/runs/43542804418)
在 scripts 525 项测试中因一项 `FileNotFoundError` 失败：静态 wiring 测试仍读取已退役的
`native_sanitizer_tests/map_provider.rs`。实际 exporter 保留在同职责父模块
`native_sanitizer_tests.rs::map_provider`，完整 libtest FQN 未变化。

先以原静态测试复现失败，再改为确认实际内联模块声明并只截取该模块正文；原真实 IR、
双模式、12 项 manifest、精确消费标签及输入身份先于逐项执行的全部断言保留。没有恢复
空壳、扩大为全文件断言、跳过测试或修改生产 sanitizer 脚本。独立只读审查核对实际模块
边界、Rust 挂载、精确 exporter 调用和 consumer 合同，未发现本轮具体问题。

定向 Python 测试 23 项为 `OK (skipped=1)`；按 CI 使用的 GNU checksum 工具执行完整
`python3 -m unittest discover -s scripts/tests -v`，525 项为 `OK (skipped=1)`，退出码 0。
唯一跳过项为真实 Linux `/proc` 进程组资源记账，本机 macOS 未执行，不记作通过。
首次完整本机运行另因 BSD `sha256sum` 不支持 `--strict` 而失败；使用已安装 GNU
coreutils 后重验通过，未修改无关工具链测试。`check_docs.py`、diff whitespace 均通过。
本轮仅修正 Python 路径消费者，不重跑 Cargo；新 head 的远端 CI 仍须独立确认。
