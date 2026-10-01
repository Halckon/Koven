# SPEC-0234：普通 block 的换行表达式边界

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：修改 block 顶层 Pratt 续行边界时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P1-234` |
| 所属 Phase | Phase 1 |
| 语言规范 | [block element 与续行](../../guide/06-blocks-control-flow.md) |
| 批准依据 | 用户批准继续演进计划；计划 §1.1 已明确换行默认分隔及续行例外 |
| 前置 Spec / ADR | 无新增依赖；基于 main 的既有 parser owner/Pratt 合同 |
| 阻塞项 | 无 |
| 影响范围 | 普通/control/nested block 的 expression、initializer 与相关 parser 测试 |
| 语言语义变更 | 否；实现已启用 v0.38 规则，清除词法/block/尾 lambda 的遗留文案冲突 |

## 1. Goal

普通 block 的完整左表达式遇到换行后的 `(`、`+`、`-` 时归还 block dispatch，
不误合并成 call 或二元运算；等待操作数或未闭合 group/call/index 时继续解析。

## 2. 范围与边界

- 在 block 顶层有效的 `foo` 换行 `(bar)` 为 Name + Group 两个 expression statement。
- `a` 换行 `+b` / `-b` 为 Name + Prefix 两项；同一行的既有 call/infix 不变。
- 局部 initializer 与 control/nested block 使用同一边界。
- 行尾未完成 operator、仅可中缀/后缀的 token、显式分号、typed-call 试探与 owner recovery 保持。
- 复用 LF/CRLF/comment carrier 和 Span 合同；不改 Lexer token 或全局 expression 规则。
- Guide01 的“分号不分隔 block”、Guide06 的“没有分隔符”恢复说明及 Guide07 尾 lambda 段的“分号 unsupported”是
  已批准 §1.1 留下的过时文本，本切片仅清除此直接冲突，不新增语言选择。

## 3. 非目标

同行相邻 element 缺分隔符的诊断、前导/重复分号选择、lambda 顶层 body 的尾表达式 grammar
分开实施。当前 lambda 与 ordinary block 共用部分软 stop，本切片必须保留 lambda owner
边界而不借修复悄悄扩范围；这不将旧行为提升成规范。独立 expression 与顶层 initializer
也不套用普通 block 顶层的新边界。

## 4. 实施与验收

1. [x] AST/Span 与 LF/CRLF/carrier 红测，更新与新规范冲突的旧 block-infix 矩阵期望。
2. [x] 最小 Pratt/postfix 边界实现，保留 delimiter/typed-call/lambda owner 行为。
3. [x] 普通套件、共享 parser matrix、下游检查和严格 lint。
4. [x] Architecture 与实际验证账本同步。
5. [ ] 发布、PR CI 与归档。

## 5. 验证账本

| 命令 / 验收 | 实际结果 | 限制 |
|---|---|---|
| 初次可执行行为红测：新 `parser_block_line_continuation` | 2 通过、8 失败 | AST element 被错误合并；实现后同套件通过 |
| `for_header_delimiters_suspend_inherited_block_line_breaks` 红测 | 0 通过、1 失败 | if 内 for header 错误产生 L0010/L0060/L0013；真实 opener 内清 soft stop 后通过 |
| 定向 parser 12 targets（下列列表） | 205 通过 | 含新 suite 11 与 entry line-break matrix 2；非 frontend 全量 |
| 8 个共享 matrix targets（下列列表） | 10 通过 | 递归/小栈/恢复/trivia/长注释预算未减弱 |
| `cargo test -p lang-frontend --lib parser::engine::tests --locked --offline` | 19 通过、158 filtered | 包含 block/lambda dispatch 与 postfix raw-visit 倍增预算 |
| `cargo test -p lang-frontend --test parser_call_argument --locked --offline` | 25 通过、原有 3 失败 | 无新增失败，不称全绿 |
| `cargo fmt --all -- --check` / `cargo check --workspace --all-targets --locked --offline` | 均通过 | 最终源码 |
| `cargo clippy -p lang-frontend -p lang-codegen --all-targets --locked --offline -- -D warnings` | 通过 | 严格 lint |
| `cargo test -p lang-codegen --lib --locked --offline` | 478 通过 | Linux LLVM/native 下游；原 fixture 的前导加号已迁至前一行末 |
| `python3 scripts/check_docs.py` / `python3 -m unittest discover -s scripts/tests -v` / `git diff --check` | 386 篇 / 21 项 / 通过 | 结构检查不等于语义证明 |
| 独立只读复核 | 修复 for-header 漏点后无阻塞 | AST/Span/owner/恢复及 native fixture 运算和断言保持 |
| macOS / PR CI | 未运行 | 尚未获发布批准 |

## 6. 提交计划

`fix(parser): honor block newline continuation boundaries (SPEC-0234)`。

### 测试目标与兼容边界

12 个 parser targets 为 `parser_block_line_continuation`、`parser_entry_line_break_boundary_matrix`、
`parser_block`、`parser_expression`、`parser_control_flow`、`parser_lambda`、
`parser_local_destructuring`、`parser_trailing_lambda`、`parser_declaration`、`parser_file`、
`parser_operator_matrix`、`parser_error_propagation`。
8 个 matrix targets 为 `parser_recursion_boundary_matrix`、`parser_stack_isolation_matrix`、
`parser_entry_trivia_invariance_matrix`、`parser_entry_prefix_truncation_matrix`、
`parser_entry_suffix_truncation_matrix`、`parser_entry_lexical_poison_insertion_matrix`、
`parser_long_block_comment_line_breaks`、`parser_long_line_comment_boundaries`。
两批均使用 `cargo test -p lang-frontend`，逐项重复 `--test`，并附
`--no-fail-fast --locked --offline`，串行执行。

call-argument 的既有失败保持为 `call_only_ampersand_is_not_a_prefix_operator_or_lexer_error`、
`empty_recovery_children_do_not_extend_parent_spans_across_trivia`、
`l0033_expected_value_preserves_committed_prefix_and_empty_error_value`。

首次下游 native 477 通过、1 失败：旧 fixture 将完整 initializer 后的 `+` 放在下一行，
在新规则下会开启新语句。仅把该 `+` 移至上一行末以保留五项求和，值为 200、退出状态、
stdout 与 stderr 的全部原断言保持；未修改 codegen 生产逻辑，也未降低验证强度。
