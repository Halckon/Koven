# SPEC-0484：F3e 声明、类族与完整文件独立参考解析差分（F3e→F4a）

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F3e 文件头、声明与类族独立参考解析差分时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@d6976cf0` |
| 分支 | `feature/spec-0484-f3e-file-difftest` |

本 Spec 承接 [Parser 分阶段形式化验证计划](../../proposals/parser-formal-verification-plan.md) 的 **F3e→F4a 扩展切片**与
[F3e 声明与完整文件切片基线](../../proposals/parser-formal-verification/declaration-file-grammar-slice.md)。
接续已归档的 [SPEC-0483](../../archive/specs/0483-f3d-reference-difftest.md)（`parse_block` / F3d），
本片为 `parse_file` 与 `parse_declaration` 的 `D-FILE`、`D-VAL`、`D-FUN`、`D-CLASS`、`D-MEMBER` 建立独立参考解析器差分测试，
不修改生产 Parser/Lexer 或 Guide，不新增 Cargo 依赖。双宿主 Targeted Tests 显式运行本 Spec 的 `parser_f3e_file_diff`。

## 交付合同

1. **独立参考解析器与全树 `Span` 校验投影**：[parser_f3e_file_diff.rs](../../../crates/lang-frontend/tests/parser_f3e_file_diff.rs)
   与 [reference_parser.rs](../../../crates/lang-frontend/tests/f3e_file_diff/reference_parser.rs)
   消费仅含物理换行事件（`NL`）的词法流，独立解析文件头、顶层声明、类族、委托、`deinit`、`companion` 与成员修饰符；
   生产 `parse_file` 与 `parse_declaration` 递归断言所有 `Item`、修饰符、构造器字段、父类型、委托、枚举变体、参数与 `TypeRef`
   的 `Span` 包含关系后投影至同一规范化 S-expression。
2. **F3e 全部 12 项正例与 14 项反例见证**：覆盖空文件、仅换行、`package`/`import as`/`.*` 与尾分号、`val`/`var`/`const val`、
   `fun` 三种 body、五类 classifier、`by` 委托、`deinit`、`companion`、`public override inout fun`，以及通配符别名、
   声明后 `package`/`import`、同行无分隔、`fun f() = 1`、空 `value class`、修饰符乱序、`public deinit`、`interface` 私有方法、
   `object` 非 `const val` 成员、`enum` 分号后无成员、前导 `;` 与双 `;;` 等反例。
3. **独立 `parse_declaration` 入口、故障负对照与修饰符组合矩阵**：验证 `parse_declaration` 拒绝尾随分号或第二条声明；
   故障负对照验证比较器可检出包名、修饰符、委托、`deinit` 与接受/拒绝漂移；组合矩阵覆盖 64 组可见性 × `override` × receiver mode × 函数体组合。

## 本地运行

```bash
cargo test --locked -p lang-frontend --test parser_f3e_file_diff
```

## 检查点

- [x] F3e 文件与声明独立参考解析器、全树 `Span` 校验、26 项正反例与 64 组修饰符组合实现
- [x] 本地门禁（fmt / clippy / test / check_docs / check_rust_sizes）实跑验收
- [ ] PR CI 验收与归档
