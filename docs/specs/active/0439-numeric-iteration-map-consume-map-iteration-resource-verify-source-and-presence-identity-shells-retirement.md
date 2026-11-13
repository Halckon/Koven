# SPEC-0439: `numeric_iteration`、`map_consume`、`map_iteration` 根/资源/校验/源及 `rc_nullable_while_presence_identity` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0439 |
| 标题 | `numeric_iteration`、`map_consume`、`map_iteration` 根/资源/校验/源及 `rc_nullable_while_presence_identity` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0438 |

---

## 1. 背景与动机

在 [SPEC-0438](../../archive/specs/0438-outer-borrowed-while-handoff-exits-carrier-borrowed-cleanup-formed-temporary-and-presence-source-shells-retirement.md) 完成 `outer_borrowed_while`、`source_owner/handoff`、`map_iteration` 退出/成形/临时项及可空清理壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批测试专用与旧路径对比壳模块：

1. `ssa/lower_frontend/loop_control/numeric_iteration.rs`（781 行）：仅承载测试对比用数值 `for` 循环与提供者证明，可收敛入 `ssa/lowering_support_tests.rs`。
2. `ssa/lower_frontend/map_consume/nullable.rs`（794 行）：仅承载测试对比用可空 Map 消费降低，可收敛入 `ssa/rc_nullable_while_memory_document_tests.rs`。
3. `ssa/lower_frontend/map_iteration/resource.rs`（844 行）：仅承载测试对比用资源 Map 迭代证明，可收敛入 `ssa/lower_frontend_root_boundary_tests.rs`。
4. `ssa/lower_frontend/map_consume.rs`（863 行）：仅承载测试对比用 Map 消费、资源析构与解构投影，可收敛入 `ssa/binding_lifecycle_probe_tests.rs`。
5. `ssa/lower_frontend/map_iteration/conditional/verify.rs`（865 行）：仅承载测试对比用条件 Map 迭代验证，可收敛入 `ssa/root_primitive_lower_tests.rs`。
6. `ssa/lower_frontend/map_iteration/source.rs`（900 行）：仅承载测试对比用 Map 迭代源身份与拓扑证明，可收敛入 `ssa/multi_borrow_range_boundary_tests.rs`。
7. `ssa/lower_frontend/map_iteration.rs`（875 行）：仅承载测试对比用双分量 Map 迭代入口，可收敛入 `ssa/receiver_receipt_sibling_tests.rs`。
8. `ssa/rc_nullable_while_presence_identity.rs`（811 行）：仅承载可空 `Rc` `while` 存在性身份单测支撑，可收敛入 `ssa/named_literal_mapping_tests.rs`。

---

## 2. 设计与收敛方案

| 退役壳模块 | 行高 | 收敛宿主模块 | 收敛后宿主行高 |
|---|---:|---|---:|
| `ssa/lower_frontend/loop_control/numeric_iteration.rs` | 781 | `ssa/lowering_support_tests.rs` | 994 |
| `ssa/lower_frontend/map_consume/nullable.rs` | 794 | `ssa/rc_nullable_while_memory_document_tests.rs` | 913 |
| `ssa/lower_frontend/map_iteration/resource.rs` | 844 | `ssa/lower_frontend_root_boundary_tests.rs` | 941 |
| `ssa/lower_frontend/map_consume.rs` | 863 | `ssa/binding_lifecycle_probe_tests.rs` | 919 |
| `ssa/lower_frontend/map_iteration/conditional/verify.rs` | 865 | `ssa/root_primitive_lower_tests.rs` | 976 |
| `ssa/lower_frontend/map_iteration/source.rs` | 900 | `ssa/multi_borrow_range_boundary_tests.rs` | 995 |
| `ssa/lower_frontend/map_iteration.rs` | 875 | `ssa/receiver_receipt_sibling_tests.rs` | 967 |
| `ssa/rc_nullable_while_presence_identity.rs` | 811 | `ssa/named_literal_mapping_tests.rs` | 901 |

---

## 3. 验收标准

1. 上述 8 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `2141` 降至 `2133`。
