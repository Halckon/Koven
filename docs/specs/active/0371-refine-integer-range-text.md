# SPEC-0371：整数内建、整数区间、`String.clone()` 与文本原语迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的标量/区间/文本内建操作细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0371` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 表达式与运算符](../../guide/04-expressions-operators.md)、[程序、运行时与标准库](../../guide/13-program-runtime-standard-library.md) |
| 批准依据 | 用户确认优先推进 E3 所有权细化层功能 |
| 基线 | `feature/spec-0370-refine-lambda-bodies` `47208fc1` |
| 分支 | `feature/spec-0371-refine-integer-range-text` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0370](0370-refine-lambda-bodies.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将整数内建操作（`inv()`、八整数受检转换 `toByte()`..`toULong()`）、整数前缀 `+`/`-` 与位运算（`and`/`or`/`xor`/`shl`/`shr`/`ushr`）、`IntRange`/`LongRange` 值操作（`..`、`..<`、`in`、`!in`、`==`、`!=`）、`String.clone()` 以及可信标准文本原语（`CopyBytes`、`CopySlice`、`Utf8ErrorOffset`、`ByteLength`、`ByteAt`）迁入 `ssa::refine`，使已迁入家族（nullable、`when`、`Inout`、`Rc`、`value class`、闭包与 lambda 体）内的上述构造统一由所有权细化层 lowering。

## 2. 实施内容

1. **细化层 IR 与 SSA 发射（`refine/body.rs`、`refine/emit.rs`）**：
   - 新增 `ROp::IntegerNot`、`ROp::IntegerBitwise`、`ROp::CheckedIntegerConvert`、`ROp::IntegerRangeConstruct`、`ROp::IntegerRangeContains`、`ROp::IntegerRangeEqual`、`ROp::StringClone`、`ROp::StringCopyBytes`、`ROp::StringCopySlice`、`ROp::Utf8ErrorOffset`、`ROp::StringByteLength`、`ROp::StringByteAt` 及 `RAbort::CheckedConversion`。
   - 在 `emit.rs` 中将上述槽位操作与受检转换 Abort 映射为对应 SSA `Operation` 与 `AbortReason::CheckedConversion`。
2. **子集预检与槽位 CFG 构建（`refine/subset*`、`refine/build*`）**：
   - 将 `IntRange` 与 `LongRange` 纳入 `storage_type`；抽离 `subset/when.rs` 与 `subset/scalar_text.rs`、`build/scalar_text.rs`，保持单文件规模低于 1000 行上限。
   - 支持 `typed.integer_operation`（`Invert` 与 `CheckedConvert`）、整数前缀 `+`/`-`（含最小负整数字面量直降与受检 `0 - x`）、位运算二元操作、`typed.integer_range_operation`、`typed.string_operation`（`StringOperationKind::Clone`，含 Borrow 参数/捕获、非空窄化视图与局部/临时 `String` 借用）及 `descriptor.text_primitive()`。

## 3. 验收标准

- [x] `refine_tests::integer_range_and_string_clone_lower_in_refine` 验证混合受检整数转换、位运算、前缀负号、`IntRange` 构造与成员/相等判断、`String.clone()` 及可空返回的函数经 `ssa::refine` lowering 并通过原生运行验证。
- [x] `unit_lower_closure_tests`（10/10）与 `refine_closure_tests`（3/3）全部通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。
