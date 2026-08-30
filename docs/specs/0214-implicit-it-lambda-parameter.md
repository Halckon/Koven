# SPEC-0214：隐式 `it` lambda 参数

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P123-214` |
| 所属 Phase | Phase 1/2/3 纵向切片 |
| 语言规范 | 现行 v0.33 [`05-grammar-calls-lambda.md`](../guide/05-grammar-calls-lambda.md) |
| 批准依据 | 2026-08-30 用户明确要求在 v0.32 完成后启用 v0.33 并继续分阶段实施 |
| 前置 Spec | SPEC-0010、0018、0019、0032、0067、0173、0197、0198、0213 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` lambda AST/name/type/ownership 与 compilation-unit facts；`lang-codegen` 复用 synthetic 参数 anchor；测试、Architecture/Roadmap |
| 语言语义变更 | 是；实施现行 v0.33 lambda 参数增量 |

## 1. Goal

完成后，所有无显式 header 的 lambda 在获得唯一一参数 expected function type 时，都建立类型
和 Borrow/Value/Inout mode 来自该参数的隐式 `it` binding；尾 lambda、圆括号实参和普通
lambda 使用相同 name/type/ownership facts。

## 2. 背景

SPEC-0213 只把尾 lambda 规范化为普通 `CallArgument`。若 `f { it }` 与 `f({ it })` 行为不同，
就会把调用表面语法泄漏进 lambda 名称和类型语义。现有 name resolver 只为显式参数 Span 建立
`LambdaParameter`，type/ownership checker 也从该列表取得 mode 与 binding，因此隐式 `it`
不是 Parser 糖，必须作为跨 AST、名称、类型和所有权的独立纵向切片实施。

## 3. 范围与需求

- 为 `arrow_span == None` 的 lambda 保存真实 `{` anchor，并在 lambda-local scope 建立 contextual
  `it` candidate；显式 header（含 `{ -> ... }`）不建立 candidate。
- expected function type 恰有一个参数时激活 candidate，发布 source-qualified symbol、类型和
  ParameterMode；body 未读取 `it` 时仍保持一参数 callable shape。
- 无 expected 且引用 `it` 时发 L0083；expected 零参数但引用 `it`，或 expected 多参数但省略
  header 时发 L0084，并保持 recovery typed product、不泄漏候选试算事实。
- headerless lambda 中 candidate 优先于外层同名 binding；显式 `{ -> it }` 可按普通作用域读取
  外层 binding。nested lambda/local shadowing 按既有顺序作用域规则工作。
- single-file 与 compilation-unit name/type facts 使用稳定 source-qualified identity；overload
  candidate trial、文件输入置换和失败恢复不泄漏 symbol type/mode/call/capture facts。
- ownership 将激活的 `it` 初始化为普通 lambda parameter，应用现有 Value/Borrow/Inout 使用、
  move 与 loan 规则，并明确不把它登记为 closure capture。

## 4. 非目标

- 不把 `it` 变成 Lexer 关键字或全局保留字，不改变显式参数、具名函数参数或普通局部名称。
- 不增加隐式多参数名、tuple 参数、receiver lambda、label return、implicit `this` 或参数类型推导。
- 不改变 callable ABI、closure environment layout、SSA/LLVM 或 runtime；后端复用现有参数契约。
- 不实现 SPEC-0213 之外的调用语法，也不实施 v0.33 project build。

## 5. 验收标准

- [x] compile-pass 覆盖 `f { it }`、`f({ it })`、typed local lambda、unused unary `it`、
  Borrow/Value/Inout、nested lambda、local shadowing 与显式 `{ -> outerIt }`。
- [x] name/type facts 对三种 lambda 位置发布等价的 lambda-local symbol、参数类型/mode；synthetic
  symbol 以真实 `{` anchor 定位，不伪造 Identifier Span，重复分析和文件置换结果一致。
- [x] compile-fail 精确覆盖无 expected 的 L0083、零/多参数 expected 的 L0084、overload
  ambiguous/no-match；断言 primary/label Span、recovery gate 和候选事实零泄漏。
- [x] ownership 正反例覆盖 Borrow 读取、Value MoveOnly 移动后使用、Inout 修改能力与 capture
  排除；显式/隐式参数在同一契约下产生等价结果。
- [x] 现有无参 lambda `{ 1 }` 推导、显式 `{ -> ... }`、lambda/callable/name/type/ownership 定向
  suites 与 frontend 编译基线通过；按用户明确授权不运行约一小时的全量 frontend 测试。
- [x] Architecture 更新为实现事实，guide/roadmap/Spec 状态与实现状态一致。

## 6. 技术方案与边界

Parser 只补充真实 left-brace anchor 或等价可追溯身份，不把 `it` 解析成特殊 expression。
name resolver 在 headerless lambda scope 建立带 synthetic-origin 标记的 `LambdaParameter` candidate，
普通 `Expression::Name` 仍通过既有 reference target 指向它。type checker依据 expected arity 激活
并发布类型/mode，或产生 L0083/L0084 recovery；overload trial 必须快照这一完整状态。
ownership 只消费 validated typed symbol/mode，不重新猜 expected contract。该设计保持既有阶段
方向与 callable ABI，不需要 ADR。

## 7. 实施计划

1. [x] 建立 brace anchor、synthetic symbol 与单/多文件名称 identity → 验证：name/AST 定向矩阵。
2. [x] 接通 single/unit expected contract、诊断与 overload trial → 验证：type/callable/unit suites。
3. [x] 接通 ownership 参数状态与 capture 排除 → 验证：ownership/closure 正反矩阵。
4. [x] 同步 Architecture、Spec 验收记录与状态 → 验证：frontend compile、Clippy、文档一致性。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | AST/name/type/ownership、测试、Architecture 与完成记录 | `feat(frontend): bind implicit it parameters (SPEC-0214)` |

## 9. 未决问题

- 无；外层同名 binding 的消歧已由现行 guide 固定为显式 `{ -> it }`。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-28 current-state audit | 通过 | 现有 AST/name/type/ownership 都只消费显式 parameter Span，确认必须独立于 SPEC-0213 纵向实施 |
| `cargo test -p lang-frontend --test type_callable --locked --offline` | 通过 | 19 个 callable 用例；新增 trailing/group/typed local、三种 mode、unused unary、局部遮蔽、L0083/L0084 与 overload trial 提交/回滚 |
| `cargo test -p lang-frontend --test multifile_type_checking cross_file_lambdas_publish_expected_contract_and_callable_boundaries --locked --offline -- --exact` | 通过 | source-qualified implicit symbol/type/mode 与输入置换一致 |
| `cargo test -p lang-frontend --test ownership_closures implicit_it_is_a_value_parameter_and_never_a_capture --locked --offline -- --exact` | 通过 | Value MoveOnly use-after-move；implicit symbol capture 数为零 |
| `cargo test -p lang-frontend --test ownership_closures implicit_borrow_and_inout_match_explicit_parameter_ownership --locked --offline -- --exact` | 通过 | Borrow/Inout 显式/隐式参数 ownership 与 capture 结果一致 |
| `cargo test -p lang-frontend --test multifile_ownership_checking lambda_value_parameters_publish_entry_read_and_transfer_drop_facts --locked --offline -- --exact` | 通过 | 未使用 MoveOnly synthetic Value 参数在 `LambdaEntry` 精确析构 |
| `cargo check -p lang-frontend --tests --locked --offline` | 通过 | 所有 frontend 测试目标编译；不执行全量测试 |
| `cargo check -p lang-codegen --lib --locked --offline` | 通过 | unit closure lowering 以真实 `{` anchor 对齐 unary callable 参数，不改变 ABI |
| `cargo test -p lang-codegen lowers_move_only_value_lambda_parameters_from_exact_drop_facts --locked --offline` | 通过 | 1 个真实执行用例；headerless unary thunk 参数绑定、drop 与输入置换 |
| `cargo clippy -p lang-frontend --lib --test type_callable --test multifile_ownership_checking --locked --offline -- -D warnings`；`cargo clippy -p lang-codegen --lib --locked --offline -- -D warnings` | 通过 | 受影响 frontend/codegen 静态检查 |
| 当前任务验收授权 | 采用受影响 suites + 全测试目标编译 + 定向 Clippy | 用户明确要求避免约一小时的 `lang-frontend` 全量测试；本记录不宣称全量测试通过 |
