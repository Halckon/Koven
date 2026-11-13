# SPEC-0430: `outer_borrowed_while`、`verify/closure_escape`、`verify_operation` 及 `lower_frontend` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0430 |
| 标题 | `outer_borrowed_while`、`verify/closure_escape`、`verify_operation` 及 `lower_frontend` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0429 |

---

## 1. 背景与动机

在 [SPEC-0429](0429-map-iteration-closure-verify-map-source-and-unit-lower-shells-retirement.md) 完成 `map_iteration`、`closure`、`verify_ownership/borrow_result`、`map_source/formation` 及 `unit_lower` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批仅向父模块或同层模块暴露私有实现的薄壳文件：

1. `ssa/lower_frontend/enum_type_test.rs`（305 行）：仅向 `lower_frontend` 暴露枚举类型测试与无载荷 Copyable 枚举条件降低，可收敛入 `ssa/lower_frontend/control/carried.rs`。
2. `ssa/lower_frontend/outer_borrowed_while/source.rs`（515 行）：仅向 `outer_borrowed_while` 暴露外层借用 `for` 与内层可空 `while` 的源筛选与载体证明，可收敛入 `ssa/lower_frontend/container/access.rs`。
3. `ssa/lower_frontend/cleanup_guard_harness.rs`（516 行）：仅向测试守卫暴露私有清理守卫脚手架，可收敛入 `ssa/lower_frontend/container.rs`。
4. `ssa/verify/closure_escape.rs`（550 行）：仅向 `verify` 与 `verify_types` 暴露借用闭包逃逸校验与类型捕获计算，可收敛入 `ssa/verify_ownership/field_exchange.rs`。
5. `ssa/verify_operation/integer.rs`（557 行）：仅向 `verify_operation` 暴露整数转换、标量常量、字节数组、区间与 Map/借用返回局部类型契约，可收敛入 `ssa/verify_ownership/root_exchange.rs`。
6. `ssa/lower_frontend/pending_call.rs`（576 行）：仅向 `lower_frontend` 暴露待完成调用借用与可空字段溯源，可收敛入 `ssa/expression_core.rs` 的测试专用内联模块。
7. `ssa/verify_operation/range.rs`（582 行）：仅向 `verify_operation`、`verify` 与 `verify_ownership` 暴露 `Range`、可调用签名与 `NullableTake` 分支证明契约，可收敛入 `ssa/borrow_result_support.rs`。
8. `ssa/lower_frontend/map.rs`（599 行）：仅向 `lower_frontend` 暴露 Map 容器操作降低，可收敛入 `ssa/statement_core.rs` 的测试专用内联模块。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/lower_frontend/enum_type_test.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs`。
2. 退役 `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/source.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/container/access.rs`。
3. 退役 `crates/lang-codegen/src/ssa/lower_frontend/cleanup_guard_harness.rs`，将其内联至 `crates/lang-codegen/src/ssa/lower_frontend/container.rs`。
4. 退役 `crates/lang-codegen/src/ssa/verify/closure_escape.rs`，将其内联至 `crates/lang-codegen/src/ssa/verify_ownership/field_exchange.rs`。
5. 退役 `crates/lang-codegen/src/ssa/verify_operation/integer.rs`，将其内联至 `crates/lang-codegen/src/ssa/verify_ownership/root_exchange.rs`。
6. 退役 `crates/lang-codegen/src/ssa/lower_frontend/pending_call.rs`，将其内联至 `crates/lang-codegen/src/ssa/expression_core.rs`。
7. 退役 `crates/lang-codegen/src/ssa/verify_operation/range.rs`，将其内联至 `crates/lang-codegen/src/ssa/borrow_result_support.rs`。
8. 退役 `crates/lang-codegen/src/ssa/lower_frontend/map.rs`，将其内联至 `crates/lang-codegen/src/ssa/statement_core.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/lower_frontend/enum_type_test.rs` | `crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/source.rs` | `crates/lang-codegen/src/ssa/lower_frontend/container/access.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/cleanup_guard_harness.rs` | `crates/lang-codegen/src/ssa/lower_frontend/container.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify/closure_escape.rs` | `crates/lang-codegen/src/ssa/verify_ownership/field_exchange.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_operation/integer.rs` | `crates/lang-codegen/src/ssa/verify_ownership/root_exchange.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/pending_call.rs` | `crates/lang-codegen/src/ssa/expression_core.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_operation/range.rs` | `crates/lang-codegen/src/ssa/borrow_result_support.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/lower_frontend/map.rs` | `crates/lang-codegen/src/ssa/statement_core.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。
