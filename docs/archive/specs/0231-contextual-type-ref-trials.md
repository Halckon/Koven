# SPEC-0231：上下文 TypeRef 与严格调用试探一致性

> **性质**：实施 Spec · **状态**：done · **读取时机**：修复上下文词的 TypeRef 解析时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `done` |
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

## 7. 最终交付与关闭验收（2026-10-02）

实现提交 `772b1f7f45d059c051bdaa748b767fedac49dc89` 最终经
[PR #7](https://github.com/Halckon/Koven/pull/7) head
`11051e200441a21cdf6dee6a6d153d2e9ffe26c6` 合并为
`e22e11b736aab1231209e3403bd0c931b9ddb940`，包含于复核基线
`34189046319a8b727285d471596647d5de56996e`。
该 head 的 [CI 36877486546](https://github.com/Halckon/Koven/actions/runs/36877486546)
8/8 jobs success，Ubuntu/macOS 的 workspace check、strict clippy、core、stage 与 Guide
步骤均实际成功。该版本 stage 明确选择 `parser_contextual_type_ref`、相关直接 suites 和
§5 所列八个 matrix；core 执行 frontend lib 和完整 codegen。它不是 frontend 全量门禁，
当时 stage 尚不选择 `parser_call_argument`，不能用绿灯抹去其原三项失败。

| 原验收项 | 直接证据与关闭判断 |
|---|---|
| §4.1 red 与 AST/Span | §5 原 1 passed / 6 failed 与最终 9 passed 保留；[contextual suite](../../../crates/lang-frontend/tests/parser_contextual_type_ref.rs)比较 casts/typed-calls 的嵌套函数 TypeRef、参数模式及真实 Span，证明原 InvalidLexemeStream 的修复 |
| §4.2 上下文与恢复 | 同 suite 的 `mode_words_at_type_boundaries_remain_ordinary_type_names`、`move_is_a_type_name_unless_followed_by_a_function_parameter_list` 核对普通名称；named-parameter 两项和 duplicate-mode 测试核对首模式、L0039/Span 及后续恢复；不把函数 TypeRef 的 `(` 规则扩到具名参数 |
| §4.2 strict trial 一致与回滚 | `malformed_contextual_type_trials_never_commit_partial_type_nodes` 断言失败 trial 无 TypeRef 泄漏；`nested_mode_type_trials_keep_cursor_and_structure_across_trivia` 核对 1/8/32 层节点数与消费终点；`contextual_type_heads_ignore_trivia_at_the_decision_gap` 对 cast/call 入口和换行/comment gap 同时验证 |
| §4.3 预算、下游与门禁 | §5 Parser lib 28 项含线性计数/资源预算，八个 matrix 12 项、直接 suites 157 项与 codegen 478 项均保持历史结果；最终双宿主对应选择补齐平台证据，没有放宽预算或声称所有 frontend targets 均执行 |
| §4.4 交付 | 最终 PR7 head、成功 CI 与 main merge 相互对应，原正式 Parser/strict trial Goal 已有关闭证据；文档状态、路径与 inventory 随生命周期收尾同步 |

§5 “borrow 兼容策略尚待确认”是实施时点快照。后续已启用 v0.40 并由
[SPEC-0242](0242-automatic-borrow-call-migration.md) 迁移调用参数语法和相应测试；本 Spec
没有越过授权替该决策作选择，原三项失败及未运行记录继续保留。本轮 docs-only 未重跑 Cargo。
调用 marker、inout/once callable、一般换行与类型检查不因本次关闭扩入原 Goal。
