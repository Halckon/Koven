# SPEC-0431: `map_source_unit_contract`、`range`、`map_iteration/local/identity`、`nullable_when`、`nominal`、`loop_control/nullable_while`、`borrow_result` 及 `borrow_argument` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：done · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0431 |
| 标题 | `map_source_unit_contract`、`range`、`map_iteration/local/identity`、`nullable_when`、`nominal`、`loop_control/nullable_while`、`borrow_result` 及 `borrow_argument` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | done |
| 创建日期 | 2026-04-17 |
| 完成日期 | 2026-11-13 |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0430 |

---

## 1. 背景与动机

在 [SPEC-0430](../../archive/specs/0430-outer-borrowed-unit-lower-verify-operation-and-frontend-shells-retirement.md) 完成 `outer_borrowed_while`、`verify/closure_escape`、`verify_operation` 及 `lower_frontend` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留最后一批可并入小于 500 行宿主模块而不突破 1000 行上限的薄壳文件：

1. `ssa/lower_frontend_map_source_unit_contract.rs`（458 行）：仅向 `lower_frontend_map_loop_unit_identity.rs` 暴露 Map 迭代源契约，可收敛入同父宿主 `ssa/lower_frontend_map_loop_unit_contract.rs`。
2. `ssa/lower_frontend/range.rs`（599 行）：仅向 `lower_frontend` 暴露 N1a 区间扩展接收者、绑定与返回降低，可收敛入 `ssa/lowering_support.rs` 的测试专用内联模块。
3. `ssa/lower_frontend/map_iteration/local/identity.rs`（602 行）：仅向 `map_iteration/local` 暴露具名局部 Map 声明、生产者与顺序长度守卫识别，可收敛入 `ssa/constant_materialization.rs` 的测试专用内联模块。
4. `ssa/lower_frontend/nullable_when.rs`（613 行）：仅向 `lower_frontend` 暴露 owned 可空 `when` 主语求值、非空证明边与提取降低，可收敛入 `ssa/expression_core/checked.rs` 的测试专用内联模块。
5. `ssa/lower_frontend/nominal.rs`（619 行）：仅向 `lower_frontend` 暴露名义具体实例到 SSA 存储类型的映射，可收敛入 `ssa/carry.rs` 的测试专用内联模块。
6. `ssa/lower_frontend/loop_control/nullable_while.rs`（633 行）：仅向 `lower_frontend/loop_control` 暴露直接可空 `while` 条件证明与退出校验，可收敛入 `ssa/carry/columns.rs` 的测试专用内联模块。
7. `ssa/lower_frontend/borrow_result.rs`（642 行）：仅向 `lower_frontend` 暴露借用结果声明校验、局部绑定与返回降低，可收敛入 `ssa/carry/source_columns.rs` 的测试专用内联模块。
8. `ssa/lower_frontend/borrow_argument.rs`（655 行）：仅向 `lower_frontend` 暴露按值/借用实参求值与临时位置载体，可收敛入 `ssa/mod.rs` 的测试专用内联模块。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/lower_frontend_map_source_unit_contract.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend_map_loop_unit_contract.rs`。
2. 退役 `crates/lang-codegen/src/ssa/lower_frontend/range.rs`，将其内联至 `crates/lang-codegen/src/ssa/lowering_support.rs`。
3. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local/identity.rs`，将其内联至 `crates/lang-codegen/src/ssa/constant_materialization.rs`。
4. 退役 `crates/lang-codegen/src/ssa/lower_frontend/nullable_when.rs`，将其内联至 `crates/lang-codegen/src/ssa/expression_core/checked.rs`。
5. 退役 `crates/lang-codegen/src/ssa/lower_frontend/nominal.rs`，将其内联至 `crates/lang-codegen/src/ssa/carry.rs`。
6. 退役 `crates/lang-codegen/src/ssa/lower_frontend/loop_control/nullable_while.rs`，将其内联至 `crates/lang-codegen/src/ssa/carry/columns.rs`。
7. 退役 `crates/lang-codegen/src/ssa/lower_frontend/borrow_result.rs`，将其内联至 `crates/lang-codegen/src/ssa/carry/source_columns.rs`。
8. 退役 `crates/lang-codegen/src/ssa/lower_frontend/borrow_argument.rs`，将其内联至 `crates/lang-codegen/src/ssa/mod.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/lower_frontend_map_source_unit_contract.rs` | `crates/lang-codegen/src/ssa/lower_frontend_map_loop_unit_contract.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/range.rs` | `crates/lang-codegen/src/ssa/lowering_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local/identity.rs` | `crates/lang-codegen/src/ssa/constant_materialization.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_when.rs` | `crates/lang-codegen/src/ssa/expression_core/checked.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/nominal.rs` | `crates/lang-codegen/src/ssa/carry.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/loop_control/nullable_while.rs` | `crates/lang-codegen/src/ssa/carry/columns.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/borrow_result.rs` | `crates/lang-codegen/src/ssa/carry/source_columns.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/borrow_argument.rs` | `crates/lang-codegen/src/ssa/mod.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 部分目标已被后续 Spec 再次合并 | 7/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 ``cargo test -p lang-codegen --lib -- lowering_support constant_materialization carry nominal checked` = `84 passed / 0 failed`` |

宿主核对说明：

- `crates/lang-codegen/src/ssa/lower_frontend_map_loop_unit_contract.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/nullable_map_consume_lowering_tests/single.rs`（`507` 行）
