# SPEC-0433: `refine` 五类借用凭证 `ci_tests`、`verify/closure_escape/types/graph_tests` 及 `verify_operation/map` 壳子模块退役

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0433 |
| 标题 | `refine` 五类借用凭证 `ci_tests`、`verify/closure_escape/types/graph_tests` 及 `verify_operation/map` 壳子模块退役 |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-04-17 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0432 |

---

## 1. 背景与动机

在 [SPEC-0432](0432-source-owner-local-storage-string-clone-checkpoint-carrier-facts-and-closure-tests-shells-retirement.md) 完成 `source_owner`、`local_storage`、`string_clone`、`checkpoint`、`carrier`、`closure/runtime_tests`、`nullable_numeric_cleanup/tests` 及 `orchestrate/registration_tests` 壳子模块收敛后，`crates/lang-codegen/src/ssa/` 下仍残留一批 `refine` 借用凭证、`verify/closure_escape` 及 `verify_operation/map` 的孤立单文件测试壳模块：

1. `ssa/refine/binding_receipt/emitted/ci_tests.rs`（163 行）：仅包含局部借用绑定凭证的 CI 篡改负控测试，可收敛入宿主 `ssa/refine/binding_receipt.rs`。
2. `ssa/refine/loan_receipt/emitted/ci_tests.rs`（181 行）：仅包含通用借用凭证的 CI 篡改负控测试，可收敛入宿主 `ssa/refine/loan_receipt.rs`。
3. `ssa/refine/named_receipt/emitted/ci_tests.rs`（181 行）：仅包含具名借用凭证的 CI 篡改负控测试，可收敛入宿主 `ssa/refine/named_receipt.rs`。
4. `ssa/refine/receiver_receipt/emitted/ci_tests.rs`（182 行）：仅包含接收者借用凭证的 CI 篡改负控测试，可收敛入 `ssa/refine/build/condition.rs`。
5. `ssa/refine/temporary_receipt/emitted/ci_tests.rs`（299 行）：仅包含临时借用凭证的 CI 篡改负控测试，可收敛入 `ssa/refine.rs`。
6. `ssa/verify/closure_escape/types/graph_tests.rs`（122 行）：仅包含借用闭包类型捕获图测试，可收敛入宿主 `ssa/verify_ownership/field_exchange.rs`。
7. `ssa/verify_operation/map/require_tests.rs`（118 行）：仅包含 `Map.requireValue` 局部操作契约测试，可收敛入 `ssa/verify_ownership/closure.rs`。
8. `ssa/verify_operation/map/with_tests.rs`（183 行）：仅包含 `Map.withValue` 局部操作契约测试，可收敛入 `ssa/model/operation.rs`。

---

## 2. 目标与范围

### 2.1 核心目标

1. 退役 `crates/lang-codegen/src/ssa/refine/binding_receipt/emitted/ci_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/binding_receipt.rs`。
2. 退役 `crates/lang-codegen/src/ssa/refine/loan_receipt/emitted/ci_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/loan_receipt.rs`。
3. 退役 `crates/lang-codegen/src/ssa/refine/named_receipt/emitted/ci_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/named_receipt.rs`。
4. 退役 `crates/lang-codegen/src/ssa/refine/receiver_receipt/emitted/ci_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine/build/condition.rs`。
5. 退役 `crates/lang-codegen/src/ssa/refine/temporary_receipt/emitted/ci_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/refine.rs`。
6. 退役 `crates/lang-codegen/src/ssa/verify/closure_escape/types/graph_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/verify_ownership/field_exchange.rs`。
7. 退役 `crates/lang-codegen/src/ssa/verify_operation/map/require_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/verify_ownership/closure.rs`。
8. 退役 `crates/lang-codegen/src/ssa/verify_operation/map/with_tests.rs`，将其内联至 `crates/lang-codegen/src/ssa/model/operation.rs`。

### 2.2 非目标

1. 不改变任何 SSA lowering、refinement、验证或渲染语义。
2. 不使任何修改后的非例外 Rust 源文件达到或超过 1000 行。

---

## 3. 设计与实现方案

### 3.1 模块合并映射

| 退役文件 | 合并目标文件 | 合并后行数上限 |
|---|---|---|
| `crates/lang-codegen/src/ssa/refine/binding_receipt/emitted/ci_tests.rs` | `crates/lang-codegen/src/ssa/refine/binding_receipt.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/loan_receipt/emitted/ci_tests.rs` | `crates/lang-codegen/src/ssa/refine/loan_receipt.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/named_receipt/emitted/ci_tests.rs` | `crates/lang-codegen/src/ssa/refine/named_receipt.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/receiver_receipt/emitted/ci_tests.rs` | `crates/lang-codegen/src/ssa/refine/build/condition.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/refine/temporary_receipt/emitted/ci_tests.rs` | `crates/lang-codegen/src/ssa/refine.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify/closure_escape/types/graph_tests.rs` | `crates/lang-codegen/src/ssa/verify_ownership/field_exchange.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_operation/map/require_tests.rs` | `crates/lang-codegen/src/ssa/verify_ownership/closure.rs` | < 1000 行 |
| `crates/lang-codegen/src/ssa/verify_operation/map/with_tests.rs` | `crates/lang-codegen/src/ssa/model/operation.rs` | < 1000 行 |

---

## 4. 验收标准

1. 上述 8 个壳子模块文件已从版本库物理删除。
2. `python3 scripts/check_rust_sizes.py --base main` 通过，修改文件均小于 1000 行，手写 Rust 文件数净减 8。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 通过。
4. `cargo test -p lang-codegen --lib` 编译与单测通过。
