# SPEC-0485：F3c 18 级表达式运算符层级独立参考解析差分（F3c→F4a）

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F3c 18 级表达式运算符层级差分时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@d6976cf0` |
| 分支 | `feature/spec-0485-f3c-expression-difftest` |

本 Spec 承接 [Parser 分阶段形式化验证计划](../../proposals/parser-formal-verification-plan.md) 的 **F3c→F4a 扩展切片**与
[F3c 18 级表达式运算符层级基线](../../proposals/parser-formal-verification/expression-grammar-slice.md)。
本片针对独立表达式入口 `parse_expression` 建立不共享生产 Parser 内部逻辑的 18 级参考解析器，
覆盖全部优先级、左右结合性、四组不结合运算符、调用实参上下文谓词与 Span 邻接禁符拒绝，
不修改生产 Parser/Lexer 或 Guide，不新增 Cargo 依赖。双宿主 Targeted Tests 显式运行本 Spec 的 `parser_f3c_expression_diff`。

## 交付合同

1. **独立 18 级参考解析器与全树 `Span`/操作符位置校验**：[parser_f3c_expression_diff.rs](../../../crates/lang-frontend/tests/parser_f3c_expression_diff.rs)
   与 [reference_parser.rs](../../../crates/lang-frontend/tests/f3c_expr_diff/reference_parser.rs)
   独立实现 `E-ATOM`、`E-TYPE-SIMPLE` 与 `E-P01`..`E-P18`；生产 `parse_expression` 递归断言全部节点、`TypeRef`、
   调用实参（含命名参数与 `&`）以及每个运算符 `operator_span` 位于左右操作数之间后投影至同一规范化 S-expression。
2. **F3c 全部 16 项正例、11 项反例与邻接禁符见证**：覆盖 7 种后缀、前缀、类型转换、6 个具名位运算、`..`/`..<`、
   `to`、`?:` 右结合、`in`/`!in`/`is`/`!is`、比较与相等不结合、`&&`/`||`、赋值右结合，以及无空白邻接禁符
   （`++`/`--`/`<<`/`>>`/`...` 拒绝）与空白分隔前缀（`+ +a`/`- -a`/`a + +b`/`a - -b` 接受）。
3. **故障负对照、28 组相邻优先级对与 40 组不结合全对矩阵**：故障负对照验证比较器可检出优先级互换、结合性退化与接受/拒绝漂移；
   相邻优先级对覆盖 P04–P18 全部 14 对双向组合（28 组）；不结合全对矩阵穷举四组内全部有序二元组合（4 + 16 + 16 + 4 = 40 组）。

## 本地运行

```bash
cargo test --locked -p lang-frontend --test parser_f3c_expression_diff
```

## 检查点

- [x] F3c 18 级独立参考解析器、全树 `Span` 与操作符位置校验、正反例与相邻优先级/不结合穷举矩阵实现
- [x] 本地门禁（fmt / clippy / test / check_docs / check_rust_sizes）实跑验收
- [ ] PR CI 验收与归档
