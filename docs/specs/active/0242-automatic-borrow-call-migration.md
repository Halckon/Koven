# SPEC-0242：调用点自动借用迁移

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：落实 v0.40 调用点自动借用与普通 borrow 名称时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P1-242` |
| 所属 Phase | Phase 1；Phase 2/3 与 native/CLI 回归 |
| 语言规范 | [调用实参](../../guide/07-calls-lambdas-closures.md#typed-call-argument)、[v0.40 迁移](../../guide/15-conformance-and-staging.md#v040-迁移与未完成边界) |
| 批准依据 | 2026-10-01 用户已启用 v0.40，并授权 SPEC-0241/0242 联合发布 |
| 前置 Spec / ADR | SPEC-0235 已启用 Guide v0.40；不新增 ADR |
| 阻塞项 | 本地扩大验证已完成；联合 PR CI 尚未完成，既有失败不得隐藏 |
| 影响范围 | 调用实参 parser、fixture、编辑器 grammar、定向门禁与实现快照 |
| 语言语义变更 | 否；落实已启用 v0.40，不改变声明 Borrow 或调用 `&` |

## 1. Goal 与非目标

- 调用点只把 `&` 构造为 `Inout` marker；`borrow`、`own`、`inout` 在 expression 中按普通 Identifier 解析。
- `borrow(x)` 与 `borrow (x)` 不因空白、comment、换行而获得不同实参身份；具名实参、member、typed call、尾 lambda 均沿普通路径处理。
- 旧 `f(borrow x)` / `f(name = borrow x)` 按既有 L0034 恢复：在后一个名称起点插入空 primary Span，保留两个值及后续实参的顺序、身份和精确 Span。
- 无 marker 的 Value/Borrow 交付继续消费现有 typed 契约和 ownership facts；不新增借用规则、兼容语法、AST table、IR 或降低资源预算。
- 声明参数、函数类型、receiver 的 Borrow marker 与显式 `&` 的 L0033/L0037/L0038 恢复保持；不扩大到整数移位、deinit、return 或其它语言迁移。

## 2. 实施与 fixture 迁移

`postfix::parse_argument_mode_marker` 只识别首个和重复的 `&`，不再向前判断 `borrow` 的后继表达式。`CallArgument` API 注释明确 parser 只生成 Inout。

迁移逐项区分语义目的：

- 合法调用 fixture 删除旧调用侧 borrow；保留原类型、参数映射、loan、move、drop、native/SSA 断言。
- Box/Pair/listOf 的非法模式用 `&` 替代旧 borrow，继续断言原 L0122 和不发布部分成功 facts，而不是把原负例改成正例。
- L0033 缺值改用 `&`；L0037 逆序与 L0038 重复改用真实 `&`，仍核对精确诊断和 Span。
- 普通 `own`/`inout` 缺 separator 核对 L0034；TypeRef 中独立 `borrow` 按上下文词合同保留 Qualified 类型与真实 Span，不伪造空 Error type。
- parser 共享合法 corpus 同步移除旧前缀；其 mutation/trivia/owner/资源套件须随合法输入一起重跑，不能仅运行新增三个测试。
- tree-sitter 以固定 CLI 0.26.12 重新生成；生成文件与编译器切片分别提交。编辑器仅提供近似语法支持，不等同于编译器接受合同。

## 3. 验证账本

本次从 main `e22e11b` 重新实施；丢失的原本地对象未被恢复，不声称字节级复原。旧日志只用于确定回归选择，以下仅把本次实际执行列为通过。

| 验收项 / 命令 | 实际结果 | 限制 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_call_argument --no-fail-fast`（实施前） | exit 101；25 passed / 6 failed / 0 ignored | 原三个旧断言失败，加普通 borrow call / removed prefix / name-member-lambda 三个新红测 |
| `cargo test -p lang-frontend --test parser_call_argument`（实施后） | exit 0；31 passed / 0 failed / 0 ignored | 真实 parser、AST、诊断、跨 source Span 与确定性 helper |
| 14个直接 parser/type/ownership/editor suites（含return） | 456 passed / 5 failed | 5项multifile_type_checking既有失败，名称见下；不代表frontend全量 |
| parser内部资源 / 共享 corpus matrix | 19 passed / 27 targets 29 passed | 保留线性预算；移除旧调用侧borrow后更新精确字符/token枚举计数，无新增skip |
| frontend lib / codegen+doctests / CLI / LSP | 180 / 517+4 / 66 / 26 passed | 0 failed / 0 ignored；Linux native实际link/run |
| workspace all-targets check、严格 clippy、fmt | 通过 | `--locked`、`-D warnings`，共享target串行 |
| 最终 stage / Guide gates | 55 targets 681 passed / 132 passed | Guide为126 frontend和6 codegen，codegen另有511 filtered |
| tree-sitter CLI 0.26.12：两个新增 corpus / 完整 corpus / 重复生成 | 2 passed；完整 5 passed / 5 failed；三个生成物 SHA256 一致 | 原五项失败不更新预期、不隐藏；TextMate 保持原样，80 项原测试通过 |
| 文档结构、检查器测试、`git diff --check` | 445 Markdown / 45 tests / diff检查通过 | inventory、索引与共享门禁已同步 |
| 远端 CI、macOS、frontend 全量 | 未运行 | 当前本地 Linux；不以定向通过替代全量或 CI |

直接目标至少包括 `parser_expression`、`parser_contextual_type_ref`、`parser_call_argument`、
`parser_diagnostic_witness_matrix`、`type_callable`、`type_checking`、`multifile_type_checking`、
`ownership_checking`、`ownership_containers`、`multifile_ownership_checking`，并追加 shared corpus 的所有直接消费者。

已知 multifile type 基线失败为 `companion_constant_initializers_publish_stable_ordinary_typed_facts`、
`deferred_explicit_constructor_type_arguments_publish_no_construction_fact`、
`cross_file_when_diagnostics_cover_shape_order_coverage_and_branch_join`、
`unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary` 与
`top_level_initializers_publish_stable_cross_file_symbol_and_expression_types`。必须以本次结果复核，不能把历史失败或历史通过直接算作本次验收。

### 编辑器精确边界

完整 corpus 的既有失败仍是 File header and declarations、Calls and lambda、Own remains
declaration-only、Control flow、Reserved words are not identifiers。新 Ordinary borrow calls
与 Removed borrow argument prefix 均通过。`f(borrow x)` 正确形成 ERROR；另一个既有
lexer 近似会把 `f(borrow input)` 的 `input` 拆成 `in` 与 `put` 并误接受，移除 marker
前后均存在。本切片不改 scanner，也不由两个 corpus 通过宣称编辑器和编译器全面一致。
编译器针对真实 `input` 的 L0034、空 Span 与完整 AST 恢复已独立红测到绿测。

## 4. 交付状态

1. [x] 新增三个红测后再修改 parser；直接目标 31 项通过。
2. [x] 已迁移与本切片有关的合法与非法 fixture，未降低类型和所有权断言。
3. [x] 完成本次共享路径、下游、编辑器、工具与文档门禁并更新账本；既有失败如实保留。
4. [ ] 联合 Draft PR 公开剩余失败与未运行项；CI 全绿且无未决项后才能归档和合并。

## 联合分支检查点

基于 main `e22e11b736aab1231209e3403bd0c931b9ddb940` 的
`fix/spec-0241-0242-recovery` 已同时整合两项修复。本轮 14 个直接 suite 实际
456 passed / 5 failed；唯一失败目标 `multifile_type_checking` 为 99 passed / 5 failed，
五个失败名称与已知基线相同，未修改这些失败断言。其余 13 个目标全部通过：fixtures、
单/unit ownership、ownership_containers、parser_call_argument/contextual_type_ref/expression/
return_control/diagnostic_witness、type_callable/type_checking、tree_sitter_grammar/textmate_grammar。
其中 return-control 新增同行 return 链与深层 return-if 的确定性递归上限回归，11 项通过；
调用参数31项通过。文档结构445篇、脚本测试45项和 diff 检查通过。

两个切片各自经过独立只读合同复核，未发现阻断；复核不冒称运行过未执行的 Cargo。
最终本地 workspace check/严格 clippy、fmt、完整 core、共享 corpus 矩阵与 stage/Guide
门禁已通过，实际计数见上表。共享合法输入移除调用侧 `borrow ` 后，26个matrix文件
的精确字符/token枚举计数同步调整；首轮旧计数失败后，最终27个matrix目标全部通过，
没有删除测试、弱化恢复断言或放宽线性预算。双平台PR CI仍待发布后验证，保持in-progress，
不代表main已交付或已可合并。

## 最新 main 整合状态

初始PR #9 head `9e54af2` 已通过双平台完整CI，随后main合入SPEC-0240。
本PR正在合入 `8f3e460` 并复验位运算、return-control与自动借用的组合；0240新增夹具
没有旧调用侧`borrow x`，其inv独占冲突与typed身份断言原样保留。
初始head通过不替代合并后的验收；最新实际结果统一见
[SPEC-0241整合账本](0241-return-control-operands.md#最新-main-整合验收)。
