# v0.38 演进计划实施账本

> **性质**：变更进度索引 · **状态**：current · **读取时机**：继续演进计划或核对交付范围时 · **唯一真源**：对应 Guide、Spec 与代码测试

本页对照 2026-10-01 用户提供的 13 项演进计划记录实际边界，不替代语言规范，也不以
已出现类型名、AST 或示例作为端到端完成证明。审计基线为 PR #5 合并后的 `d3e64a4`；
第一个补全切片为 [SPEC-0229](active/0229-extended-numeric-literal-values.md)。

## 十三项实况

| 项 | 状态 | 已有证据与未闭合边界 |
|---|---|---|
| 1. 块换行与分号 | 部分 | `parser/engine/block.rs` 已处理分号，`engine/core.rs` 判断换行边界；跨行 call/prefix 和同一行完整分隔合同尚待覆盖，词法页仍有旧禁止条款 |
| 2. 上下文关键字 | 部分 | Lexer 已产出 Identifier；Parser 已识别 value class、move lambda、loop 与参数模式。move TypeRef、嵌套函数类型模式及旧调用 borrow 标记仍需闭合 |
| 3. 数值与具名位运算 | 部分 | SPEC-0229 数值定型/const/索引 identity/SSA/native 本机闭环，待 PR CI；Scanner、位运算 AST/precedence/type checking 已有。位运算 const/SSA/native 与 inv 尚未闭合，移位 count 边界未定 |
| 4. Box enum | 前端已有 | TypeRef、构造与递归布局前端测试已有；既有 Box native 测试装箱 value class，不能证明递归 enum native 能力 |
| 5. replace/swap | 类型层已有 | 标准环境绑定 intrinsic，单文件与 unit 发布参数/返回契约；缺专用 ownership/SSA/native 原子置换与返回旧 owner 证据 |
| 6. 两阶段 receiver 借用 | 未实现 | receiver 在实参前直接建立 active exclusive loan；需 Reserved/Activate 与 callee 存活 Borrow loan 冲突检查 |
| 7. deinit 双轨析构 | 语法/类型层已有 | deinit AST、body 检查与 has_deinit 标志已有；drop planner/codegen 不消费标志，无资源 lexical lifetime 或执行 deinit 的 native 证据 |
| 8. 静态 Str | 未实现 | BuiltinType 无 Str；当前静态文字仍形成 MoveOnly String owner。Guide 新旧类型规则冲突，转换/混合操作合同待定 |
| 9. 二等借用与 Escapable | 未实现 | 当前模型仅有 owned 值与调用期 loan，能力只有 Copyable/Transferable；Ref/InoutRef/Span/StringView、来源和逃逸规则需新规范 |
| 10. inout/once closure | 未实现新增部分 | 现有 ABI 是函数指针加具体 inline 环境；没有 mutable/once callable mode。计划的栈借用/堆逃逸分层需要 Guide 与取代 ADR-0009 的决策 |
| 11. 受控 unsafe 与 RawPtr | 未实现 | Guide 有总体方向；Parser 无 unsafe/extern 产生式，类型环境无 RawPtr，标准库尚无源码容器实现。权限来源与 C ABI 类型矩阵未封闭 |
| 12. 双层文档解耦 | 部分 | Guide/Architecture/Spec 已分目录；Guide 仍包含 AST payload、恢复算法、错误码与阶段内部合同 |
| 13. 十二 Litmus | 示例清单已有 | Guide15 列出十二段示例，没有独立 fixture/自动验收入口；部分例子仍使用未实现或冲突语义，不能宣称全量验收 |

### 主要代码与测试入口

- 语法：`crates/lang-frontend/src/lexer/scanner.rs`、`src/parser/engine/`；
  `tests/lexer.rs`、`tests/parser_block.rs`、`tests/parser_expression.rs`、`tests/parser_call_argument.rs`
- 数值：`src/type_checking/checker/literal.rs`、`src/type_checking/compilation_unit/bodies/checker/literals.rs`；
  `tests/numeric_literals.rs`、`tests/type_constants.rs`、`tests/multifile_constant_facts.rs`
- Box：`tests/type_copyability.rs` 的 `intrinsic_box_accepts_concrete_value_and_enum_classes_and_breaks_layout_cycles`
- 原子置换/deinit：`src/type_checking/checker/ownership_primitives.rs`、`src/type_checking/checker/nominal.rs`；
  `tests/type_checking.rs` 的 `intrinsic_replace_*`、`intrinsic_swap_*`、`class_with_*deinit*`
- receiver/drop：`src/ownership_checking/compilation_unit/dataflow/receiver.rs`、`drop_planner.rs`；
  后端 `crates/lang-codegen/src/ssa/unit_lower/call.rs`、`native/` 和 `native_tests.rs`

本节未带 crate 前缀的 `src/`、`tests/` 路径均相对于 `crates/lang-frontend/`。

## 已知独立基线失败

2026-10-01 在任何 SPEC-0229 生产修改前实际运行两个套件：

- `parser_call_argument`：25 passed / 3 failed。失败为
  `call_only_ampersand_is_not_a_prefix_operator_or_lexer_error`、
  `l0033_expected_value_preserves_committed_prefix_and_empty_error_value`、
  `empty_recovery_children_do_not_extend_parent_spans_across_trivia`；均关联软词转换后的旧断言。
- `multifile_type_checking`：99 passed / 5 failed。失败为
  `deferred_explicit_constructor_type_arguments_publish_no_construction_fact`、
  `cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join`、
  `unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary`、
  `top_level_initializers_publish_stable_cross_file_symbol_and_expression_types`、
  `companion_constant_initializers_publish_stable_ordinary_typed_facts`。

不得通过降低断言、放宽门禁或把定向通过称作 frontend 全量通过来隐藏上述差异。

## 规范待协调项

用户计划明确的新方向与旧正文冲突包括块内分号、调用 borrow 标记、Str 字面量及 unsafe。
对计划未确定的边界（移位 count、deinit body 权限和字段析构顺序、Str/String 转换）先取得
决定，再启用完整一致的 Guide；当前代码实现不反向成为语言规则。

后续各独立 Spec 在新分支内保留红测、实现、验证与提交证据。实现状态、远端 CI 和合并状态
分开记录，不因父 PR 已合并而假定新增切片也已验证或合并。
