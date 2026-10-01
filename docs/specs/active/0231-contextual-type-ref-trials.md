# SPEC-0231：上下文 TypeRef 与严格调用试探一致性

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：修复上下文词的 TypeRef 解析时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P1-231` |
| 所属 Phase | Phase 1 |
| 语言规范 | [上下文关键词](../../guide/01-lexical.md#上下文关键字与软关键字)、[TypeRef EBNF](../../guide/03-types-generics.md#typeref-与函数类型语法) |
| 批准依据 | 2026-10-01 用户要求继续演进计划；已启用批次 1 的上下文词仅在专用产生式头部生效 |
| 前置 Spec / ADR | 无新增前置 |
| 阻塞项 | 无；不涉及调用位置 borrow 标记的待定兼容策略 |
| 影响范围 | Parser TypeRef、具名/函数类型参数模式分派、strict typed-call trial |
| 语言语义变更 | 否；落实已启用 Identifier 通则和递归函数 TypeRef 产生式 |

## 1. Goal

正式 Parser 与只读 typed-call trial 对相同 TypeRef 作相同的完整识别；合法的嵌套函数参数
模式不产生 InvalidLexemeStream，普通上下文词名称不被无条件吞掉。

## 2. 范围

- `move` 仅在后跟 `(` 的函数类型头部识别为前缀，其他 TypeRef 位置使用普通 Identifier。
- 具名参数模式仍要求后继可作为参数名；函数类型参数模式允许后继 Identifier 或 `(`。
- 首模式与重复模式使用同一上下文条件，保留 L0039 的首模式、Span 和局部恢复合同。
- strict trial 同步普通上下文词类型名及嵌套函数类型模式，失败不得提交 TypeRef 或移动 cursor。
- 保持一次线性预索引与既有递归/复杂度预算，不扩大通用回溯或 AST 形状。
- 不改变调用实参 borrow / &、inout/once callable 模式、一般换行分隔或类型检查语义。

## 3. 规范一致性

Guide03 的旧句“move 只可作为函数类型前缀”限定的是关键字角色，不应推翻 Guide01 和用户
计划已启用的普通 Identifier 通则。本次仅将该句写清为“作为上下文关键字时”，不新增
关键字或保留字，也不引入依赖空格的分派。

## 4. 实施与验收

1. [x] AST/Span、普通类型名、nested mode、failed trial 回滚与内部错误红测。
2. [x] 正式 Parser 与 strict trial 一致，具名参数恢复不被函数类型规则扩大。
3. [x] 直接 suites、受影响 matrix/资源预算、fmt/严格 clippy/workspace check 与文档门禁通过；独立旧失败保持显式记录。
4. [ ] 发布授权后的 CI 验收和 Spec 归档。

## 5. 验证账本

| 验收项 / 命令 | 实际结果 | 限制 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_contextual_type_ref`（实施前） | 1 passed / 6 failed | nested mode typed-call 确实返回 InvalidLexemeStream；其余为 AST/诊断不一致 |
| 新 contextual suite（最终代码） | 9 passed / 0 failed / 0 ignored | 含审查追加的具名参数恢复及判别间隙 trivia 测试 |
| 直接 suites（下列集合，不含独立旧失败） | 157 passed / 0 failed / 0 ignored | 新 suite 最终单独重跑；其他输入/生产代码未变的结果复用 |
| `cargo test -p lang-frontend --test parser_call_argument`（与其他 suites 同次 `--no-fail-fast`） | 25 passed / 3 failed / 0 ignored | 与实施前相同的旧调用/soft-word 断言，未修改或忽略 |
| `cargo test -p lang-frontend --lib parser::` | 28 passed / 149 filtered / 0 ignored | 含 strict trial 线性计数与原有资源预算，无预算放宽 |
| 8 个 matrix suite（下列集合） | 12 passed / 0 ignored | 跨入口、token/trivia/poison、截断与精确递归边界 |
| `cargo test -p lang-codegen --lib` | 478 passed / 0 failed / 0 ignored | Parser 共享路径的下游 native/SSA 回归 |
| `cargo check --workspace --all-targets` | 通过 | 全部下游编译消费者 |
| `cargo clippy -p lang-frontend --all-targets -- -D warnings` | 通过 | 最终测试新增后重跑 |
| `cargo fmt --all -- --check` | 通过 | 最终格式化后重跑 |
| `python3 scripts/check_docs.py` | 386 Markdown / 0 errors | 最终账本核验 |
| `python3 -m unittest discover -s scripts/tests -v` | 21 passed | inventory 修改对应检查器测试 |
| `git diff --check` | 通过 | 最终账本核验 |
| frontend 全量 / macOS / 远端 CI | 未运行 | 按仓库规则仅定向 frontend；当前只有 Linux，发布授权待定 |

直接套件：`parser_contextual_type_ref`、`parser_expression`、`parser_declaration`、
`parser_class_family`、`parser_lambda`、`parser_file`；分别为 9、57、23、23、18、27。
Parser expression 中两个旧硬关键字假设测试按已启用规则迁移：普通 move 类型后的额外 token
仍报 L0013；真正 `move (` 前缀缺返回类型时仍保留 Function AST、真实前缀 Span 和 Error return。
没有删除负例、降低断言或把普通标识符重新设成硬关键字。

matrix 目标：`parser_prefix_truncation_matrix`、`parser_suffix_truncation_matrix`、
`parser_token_omission_matrix`、`parser_trivia_invariance_matrix`、
`parser_lexical_poison_insertion_matrix`、`parser_entry_adversarial`、
`parser_recursion_boundary_matrix`、`parser_stack_isolation_matrix`。

保留的三个旧失败为 `call_only_ampersand_is_not_a_prefix_operator_or_lexer_error`、
`l0033_expected_value_preserves_committed_prefix_and_empty_error_value`、
`empty_recovery_children_do_not_extend_parent_spans_across_trivia`；调用位置 borrow 兼容策略
尚待确认，本 Spec 不借 TypeRef 修复擅自决定它，也不宣称 frontend 全量通过。

## 6. 提交计划

`fix(parser): align contextual type reference trials (SPEC-0231)`，提交包含完整实际验证记录。
