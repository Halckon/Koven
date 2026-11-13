# SPEC-0448: `map_provider_exits_pipeline_tests` 测试域叶子薄壳子模块批量退役（132 个）

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0448 |
| 标题 | `map_provider_exits_pipeline_tests` 测试域叶子薄壳子模块批量退役（132 个） |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0447 |

---

## 1. 背景与动机

在 [SPEC-0447](../../archive/specs/0447-unit-lower-receiver-sequential-for-nullable-unit-enum-and-entry-contract-tests-shells-retirement.md) 完成跨目录测试子壳收敛后，`crates/lang-codegen/src/ssa/map_provider_exits_pipeline_tests/` 域内仍存在 132 个可直接内联回直接父模块的叶子薄壳文件（各宿主合并后均严格低于 985 行）。本 Spec 一次性将该域可合并叶子全部收敛回各自父模块。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/map_provider_exits_pipeline_tests/ci_baseline.rs` | 64 | `ssa/map_provider_exits_pipeline_tests.rs` | 359 |
| `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg.rs` | 92 | `ssa/map_provider_exits_pipeline_tests.rs` | 452 |
| `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_runtime.rs` | 116 | `ssa/map_provider_exits_pipeline_tests.rs` | 569 |
| `ssa/map_provider_exits_pipeline_tests/query_test.rs` | 118 | `ssa/map_provider_exits_pipeline_tests.rs` | 688 |
| `ssa/map_provider_exits_pipeline_tests/query_inventory.rs` | 119 | `ssa/map_provider_exits_pipeline_tests.rs` | 808 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery_boundaries.rs` | 141 | `ssa/map_provider_exits_pipeline_tests.rs` | 950 |
| `ssa/map_provider_exits_pipeline_tests/births/sources.rs` | 218 | `ssa/map_provider_exits_pipeline_tests/births.rs` | 325 |
| `ssa/map_provider_exits_pipeline_tests/births/ast.rs` | 298 | `ssa/map_provider_exits_pipeline_tests/births.rs` | 624 |
| `ssa/map_provider_exits_pipeline_tests/births/facts/callable.rs` | 154 | `ssa/map_provider_exits_pipeline_tests/births/facts.rs` | 478 |
| `ssa/map_provider_exits_pipeline_tests/births/graph/control.rs` | 89 | `ssa/map_provider_exits_pipeline_tests/births/graph.rs` | 283 |
| `ssa/map_provider_exits_pipeline_tests/births/graph/intrinsic.rs` | 128 | `ssa/map_provider_exits_pipeline_tests/births/graph.rs` | 412 |
| `ssa/map_provider_exits_pipeline_tests/births/graph/callable.rs` | 244 | `ssa/map_provider_exits_pipeline_tests/births/graph.rs` | 657 |
| `ssa/map_provider_exits_pipeline_tests/births/graph/provider.rs` | 260 | `ssa/map_provider_exits_pipeline_tests/births/graph.rs` | 918 |
| `ssa/map_provider_exits_pipeline_tests/borrowed/identity.rs` | 145 | `ssa/map_provider_exits_pipeline_tests/borrowed.rs` | 297 |
| `ssa/map_provider_exits_pipeline_tests/conditional/query_test.rs` | 28 | `ssa/map_provider_exits_pipeline_tests/conditional.rs` | 226 |
| `ssa/map_provider_exits_pipeline_tests/conditional/query_conditions.rs` | 131 | `ssa/map_provider_exits_pipeline_tests/conditional.rs` | 358 |
| `ssa/map_provider_exits_pipeline_tests/conditional/query_inputs.rs` | 380 | `ssa/map_provider_exits_pipeline_tests/conditional.rs` | 739 |
| `ssa/map_provider_exits_pipeline_tests/future_use/facts.rs` | 567 | `ssa/map_provider_exits_pipeline_tests/future_use.rs` | 748 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/sources.rs` | 132 | `ssa/map_provider_exits_pipeline_tests/local_arguments.rs` | 256 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/ast.rs` | 302 | `ssa/map_provider_exits_pipeline_tests/local_arguments.rs` | 559 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/facts/route.rs` | 122 | `ssa/map_provider_exits_pipeline_tests/local_arguments/facts.rs` | 524 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/graph/cfg.rs` | 53 | `ssa/map_provider_exits_pipeline_tests/local_arguments/graph.rs` | 246 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/graph/arguments.rs` | 169 | `ssa/map_provider_exits_pipeline_tests/local_arguments/graph.rs` | 416 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/graph/receiver.rs` | 186 | `ssa/map_provider_exits_pipeline_tests/local_arguments/graph.rs` | 603 |
| `ssa/map_provider_exits_pipeline_tests/local_arguments/graph/provider.rs` | 270 | `ssa/map_provider_exits_pipeline_tests/local_arguments/graph.rs` | 874 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/owner_flow.rs` | 379 | `ssa/map_provider_exits_pipeline_tests/local_exits.rs` | 581 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation/representation.rs` | 312 | `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation.rs` | 590 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation/facts/calls.rs` | 258 | `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation/facts.rs` | 577 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation/graph/conditional.rs` | 91 | `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation/graph.rs` | 444 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation/graph/uses.rs` | 504 | `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries/formation/graph.rs` | 949 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/facts/ast.rs` | 388 | `ssa/map_provider_exits_pipeline_tests/local_exits/facts.rs` | 751 |
| `ssa/map_provider_exits_pipeline_tests/local_future/sources.rs` | 135 | `ssa/map_provider_exits_pipeline_tests/local_future.rs` | 266 |
| `ssa/map_provider_exits_pipeline_tests/local_future/facts/loans.rs` | 155 | `ssa/map_provider_exits_pipeline_tests/local_future/facts.rs` | 479 |
| `ssa/map_provider_exits_pipeline_tests/local_future/facts/ast.rs` | 330 | `ssa/map_provider_exits_pipeline_tests/local_future/facts.rs` | 810 |
| `ssa/map_provider_exits_pipeline_tests/local_future/graph/provider.rs` | 395 | `ssa/map_provider_exits_pipeline_tests/local_future/graph.rs` | 949 |
| `ssa/map_provider_exits_pipeline_tests/local_future/graph/continuation/condition.rs` | 215 | `ssa/map_provider_exits_pipeline_tests/local_future/graph/continuation.rs` | 468 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/sources.rs` | 155 | `ssa/map_provider_exits_pipeline_tests/local_reassignment.rs` | 309 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/boundaries.rs` | 189 | `ssa/map_provider_exits_pipeline_tests/local_reassignment.rs` | 499 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/completed_epoch/facts.rs` | 107 | `ssa/map_provider_exits_pipeline_tests/local_reassignment/completed_epoch.rs` | 269 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/completed_epoch/ast.rs` | 126 | `ssa/map_provider_exits_pipeline_tests/local_reassignment/completed_epoch.rs` | 396 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/completed_epoch/graph.rs` | 550 | `ssa/map_provider_exits_pipeline_tests/local_reassignment/completed_epoch.rs` | 947 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/facts/ast.rs` | 96 | `ssa/map_provider_exits_pipeline_tests/local_reassignment/facts.rs` | 345 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/graph/storage.rs` | 187 | `ssa/map_provider_exits_pipeline_tests/local_reassignment/graph.rs` | 639 |
| `ssa/map_provider_exits_pipeline_tests/local_resource_future/sources.rs` | 172 | `ssa/map_provider_exits_pipeline_tests/local_resource_future.rs` | 288 |
| `ssa/map_provider_exits_pipeline_tests/local_resource_future/facts/borrows.rs` | 126 | `ssa/map_provider_exits_pipeline_tests/local_resource_future/facts.rs` | 438 |
| `ssa/map_provider_exits_pipeline_tests/local_resource_future/graph/anchors.rs` | 310 | `ssa/map_provider_exits_pipeline_tests/local_resource_future/graph.rs` | 676 |
| `ssa/map_provider_exits_pipeline_tests/local_resources/boundaries.rs` | 124 | `ssa/map_provider_exits_pipeline_tests/local_resources.rs` | 366 |
| `ssa/map_provider_exits_pipeline_tests/local_resources/facts/owner.rs` | 14 | `ssa/map_provider_exits_pipeline_tests/local_resources/facts.rs` | 332 |
| `ssa/map_provider_exits_pipeline_tests/local_resources/facts/nominal.rs` | 128 | `ssa/map_provider_exits_pipeline_tests/local_resources/facts.rs` | 461 |
| `ssa/map_provider_exits_pipeline_tests/local_resources/graph/storage.rs` | 88 | `ssa/map_provider_exits_pipeline_tests/local_resources/graph.rs` | 709 |
| `ssa/map_provider_exits_pipeline_tests/local_sources/unit.rs` | 547 | `ssa/map_provider_exits_pipeline_tests/local_sources.rs` | 745 |
| `ssa/map_provider_exits_pipeline_tests/nested/query_cleanup.rs` | 101 | `ssa/map_provider_exits_pipeline_tests/nested.rs` | 243 |
| `ssa/map_provider_exits_pipeline_tests/nested/query_test.rs` | 121 | `ssa/map_provider_exits_pipeline_tests/nested.rs` | 365 |
| `ssa/map_provider_exits_pipeline_tests/nested/query_tail.rs` | 164 | `ssa/map_provider_exits_pipeline_tests/nested.rs` | 530 |
| `ssa/map_provider_exits_pipeline_tests/nested/query_graph.rs` | 222 | `ssa/map_provider_exits_pipeline_tests/nested.rs` | 753 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/guards.rs` | 111 | `ssa/map_provider_exits_pipeline_tests/nested_exits.rs` | 335 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_test.rs` | 122 | `ssa/map_provider_exits_pipeline_tests/nested_exits.rs` | 458 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_tail.rs` | 178 | `ssa/map_provider_exits_pipeline_tests/nested_exits.rs` | 637 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_graph.rs` | 262 | `ssa/map_provider_exits_pipeline_tests/nested_exits.rs` | 900 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_guards.rs` | 103 | `ssa/map_provider_exits_pipeline_tests/nested_exits/query_inputs.rs` | 384 |
| `ssa/map_provider_exits_pipeline_tests/resource/continued.rs` | 86 | `ssa/map_provider_exits_pipeline_tests/resource.rs` | 180 |
| `ssa/map_provider_exits_pipeline_tests/resource/nullable.rs` | 216 | `ssa/map_provider_exits_pipeline_tests/resource.rs` | 397 |
| `ssa/map_provider_exits_pipeline_tests/resource/continuation/borrowed.rs` | 350 | `ssa/map_provider_exits_pipeline_tests/resource/continuation.rs` | 495 |
| `ssa/map_provider_exits_pipeline_tests/resource/terminated/returning.rs` | 195 | `ssa/map_provider_exits_pipeline_tests/resource/terminated.rs` | 380 |
| `ssa/map_provider_exits_pipeline_tests/resource/terminated/fe.rs` | 439 | `ssa/map_provider_exits_pipeline_tests/resource/terminated.rs` | 820 |
| `ssa/map_provider_exits_pipeline_tests/resource_forms/representation.rs` | 205 | `ssa/map_provider_exits_pipeline_tests/resource_forms.rs` | 433 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/ast.rs` | 382 | `ssa/map_provider_exits_pipeline_tests/scope_future_observer.rs` | 520 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts/deep.rs` | 115 | `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts.rs` | 140 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts/provider.rs` | 124 | `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts.rs` | 265 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts/unit.rs` | 340 | `ssa/map_provider_exits_pipeline_tests/scope_future_observer/facts.rs` | 606 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/graph/provider/transport.rs` | 247 | `ssa/map_provider_exits_pipeline_tests/scope_future_observer/graph/provider.rs` | 516 |
| `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/sources.rs` | 29 | `ssa/map_provider_exits_pipeline_tests/scope_lambda_body.rs` | 118 |
| `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/ast.rs` | 207 | `ssa/map_provider_exits_pipeline_tests/scope_lambda_body.rs` | 326 |
| `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/facts/unit.rs` | 325 | `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/facts.rs` | 348 |
| `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/graph/factory.rs` | 116 | `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/graph.rs` | 516 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/sources.rs` | 58 | `ssa/map_provider_exits_pipeline_tests/scope_lowering.rs` | 139 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/ast.rs` | 292 | `ssa/map_provider_exits_pipeline_tests/scope_lowering.rs` | 432 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/facts/callers.rs` | 107 | `ssa/map_provider_exits_pipeline_tests/scope_lowering/facts.rs` | 376 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/facts/body.rs` | 193 | `ssa/map_provider_exits_pipeline_tests/scope_lowering/facts.rs` | 570 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/graph/callers.rs` | 151 | `ssa/map_provider_exits_pipeline_tests/scope_lowering/graph.rs` | 405 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/graph/provider.rs` | 189 | `ssa/map_provider_exits_pipeline_tests/scope_lowering/graph.rs` | 595 |
| `ssa/map_provider_exits_pipeline_tests/scope_member_parameter/ast.rs` | 240 | `ssa/map_provider_exits_pipeline_tests/scope_member_parameter.rs` | 362 |
| `ssa/map_provider_exits_pipeline_tests/scope_member_parameter/facts.rs` | 452 | `ssa/map_provider_exits_pipeline_tests/scope_member_parameter.rs` | 815 |
| `ssa/map_provider_exits_pipeline_tests/scope_member_parameter/graph/provider.rs` | 175 | `ssa/map_provider_exits_pipeline_tests/scope_member_parameter/graph.rs` | 499 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer/facts/callers.rs` | 125 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer/facts.rs` | 188 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer/facts/unit.rs` | 276 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer/facts.rs` | 465 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer/graph/callers.rs` | 171 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer/graph.rs` | 571 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer/ast.rs` | 246 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer.rs` | 368 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer/facts/provider.rs` | 130 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer/facts.rs` | 493 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer/graph/provider.rs` | 237 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/conditional_owner_observer/graph.rs` | 523 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/sources.rs` | 30 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage.rs` | 119 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/ast.rs` | 229 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage.rs` | 349 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/facts/unit.rs` | 323 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/facts.rs` | 358 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/graph/factory.rs` | 89 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/graph.rs` | 272 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/graph/reader.rs` | 190 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/graph.rs` | 463 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/graph/provider.rs` | 194 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/int_storage/graph.rs` | 658 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/ast.rs` | 403 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer.rs` | 553 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/facts/deep.rs` | 128 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/facts.rs` | 150 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/facts/provider_contract.rs` | 150 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/facts.rs` | 301 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/facts/unit.rs` | 334 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/facts.rs` | 636 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/provider_graph/conditional.rs` | 124 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/provider_graph.rs` | 463 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/provider_graph/provider.rs` | 319 | `ssa/map_provider_exits_pipeline_tests/scope_value_births/owner_update_observer/provider_graph.rs` | 783 |
| `ssa/map_provider_exits_pipeline_tests/sequential/identity/graph.rs` | 406 | `ssa/map_provider_exits_pipeline_tests/sequential/identity.rs` | 589 |
| `ssa/map_provider_exits_pipeline_tests/source_forms/facts.rs` | 104 | `ssa/map_provider_exits_pipeline_tests/source_forms.rs` | 347 |
| `ssa/map_provider_exits_pipeline_tests/source_forms/graph.rs` | 453 | `ssa/map_provider_exits_pipeline_tests/source_forms.rs` | 801 |
| `ssa/map_provider_exits_pipeline_tests/stack/runtime_owned_resource.rs` | 102 | `ssa/map_provider_exits_pipeline_tests/stack.rs` | 311 |
| `ssa/map_provider_exits_pipeline_tests/stack/runtime.rs` | 273 | `ssa/map_provider_exits_pipeline_tests/stack.rs` | 585 |
| `ssa/map_provider_exits_pipeline_tests/stack/runtime_variants.rs` | 352 | `ssa/map_provider_exits_pipeline_tests/stack.rs` | 938 |
| `ssa/map_provider_exits_pipeline_tests/stack/facts/representation.rs` | 153 | `ssa/map_provider_exits_pipeline_tests/stack/facts.rs` | 432 |
| `ssa/map_provider_exits_pipeline_tests/stack/facts/source_drops.rs` | 184 | `ssa/map_provider_exits_pipeline_tests/stack/facts.rs` | 617 |
| `ssa/map_provider_exits_pipeline_tests/temporary/query_cleanup.rs` | 88 | `ssa/map_provider_exits_pipeline_tests/temporary.rs` | 227 |
| `ssa/map_provider_exits_pipeline_tests/temporary/query_test.rs` | 121 | `ssa/map_provider_exits_pipeline_tests/temporary.rs` | 349 |
| `ssa/map_provider_exits_pipeline_tests/temporary/query_body.rs` | 239 | `ssa/map_provider_exits_pipeline_tests/temporary.rs` | 589 |
| `ssa/map_provider_exits_pipeline_tests/temporary/query_inputs.rs` | 268 | `ssa/map_provider_exits_pipeline_tests/temporary.rs` | 858 |
| `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_query/wrapper.rs` | 255 | `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_query.rs` | 586 |
| `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_query/ast.rs` | 283 | `ssa/map_provider_exits_pipeline_tests/temporary_argument_cfg_query.rs` | 870 |
| `ssa/map_provider_exits_pipeline_tests/temporary_arguments/ast.rs` | 241 | `ssa/map_provider_exits_pipeline_tests/temporary_arguments.rs` | 380 |
| `ssa/map_provider_exits_pipeline_tests/temporary_arguments/facts.rs` | 250 | `ssa/map_provider_exits_pipeline_tests/temporary_arguments.rs` | 631 |
| `ssa/map_provider_exits_pipeline_tests/temporary_arguments/graph/factory.rs` | 227 | `ssa/map_provider_exits_pipeline_tests/temporary_arguments/graph.rs` | 555 |
| `ssa/map_provider_exits_pipeline_tests/temporary_arguments/graph/provider.rs` | 424 | `ssa/map_provider_exits_pipeline_tests/temporary_arguments/graph.rs` | 980 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/ast.rs` | 320 | `ssa/map_provider_exits_pipeline_tests/temporary_delivery.rs` | 512 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/facts.rs` | 332 | `ssa/map_provider_exits_pipeline_tests/temporary_delivery.rs` | 845 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph/arguments.rs` | 181 | `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph.rs` | 329 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph/control.rs` | 219 | `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph.rs` | 549 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph/provider.rs` | 284 | `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph.rs` | 834 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/guards.rs` | 125 | `ssa/map_provider_exits_pipeline_tests/temporary_exits.rs` | 349 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_test.rs` | 126 | `ssa/map_provider_exits_pipeline_tests/temporary_exits.rs` | 476 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/unit.rs` | 301 | `ssa/map_provider_exits_pipeline_tests/temporary_exits.rs` | 778 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_inputs_unit.rs` | 598 | `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_inputs.rs` | 966 |
| `ssa/map_provider_exits_pipeline_tests/temporary_forms/facts.rs` | 249 | `ssa/map_provider_exits_pipeline_tests/temporary_forms.rs` | 451 |
| `ssa/map_provider_exits_pipeline_tests/temporary_forms/ast.rs` | 289 | `ssa/map_provider_exits_pipeline_tests/temporary_forms.rs` | 741 |
| `ssa/map_provider_exits_pipeline_tests/temporary_forms/graph/representation/factory.rs` | 424 | `ssa/map_provider_exits_pipeline_tests/temporary_forms/graph/representation.rs` | 655 |

---

## 3. 验收标准

1. 上述 132 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2069` 降至 `1937`。
