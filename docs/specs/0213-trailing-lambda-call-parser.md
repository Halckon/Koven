# SPEC-0213：尾 lambda 调用 Parser

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P1-213` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 v0.32；候选 v0.33 [`05-grammar-calls-lambda.md`](../guide/05-grammar-calls-lambda.md) |
| 批准依据 | 2026-08-28 用户明确要求审计并把尾 lambda 支持加入 roadmap；guide 启用门禁仍保留 |
| 前置 Spec | SPEC-0010、0012、0014、0175 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 用户尚未明确启用 v0.33 取代 v0.32 |
| 影响范围 | `lang-frontend` Parser/AST 组合、Parser tests、Architecture/Roadmap |
| 语言语义变更 | 是；v0.33 候选语法增量 |

## 1. Goal

完成后，Koven Parser 把 callee 与 `{` 之间没有换行的 `callee { ... }`、
`callee(args) { ... }` 与 `callee<T> { ... }` 解析为最后一个普通 `CallArgument` 为 lambda 的
单一 `Expression::Call`，
后续名称、类型、所有权与 codegen 阶段无需识别新的 AST 节点。

## 2. 背景

现行 v0.32 guide、SPEC-0010/0012/0175 和 `parser_lambda` 回归都明确拒绝 trailing lambda，要求
写 `f({ ... })`。因此这不是现有 Parser bug，也不能回开已完成 Spec。用户要求补入该能力后，
应以独立 Phase 1 增量先改变候选 guide，再复用既有 lambda、postfix call 与 `CallArgument`，
而不是混入正在实施的 SPEC-0197 多文件类型检查。

## 3. 范围与需求

- 在 callee 与 `{` 之间没有 LF/CRLF/含换行 comment 时，识别无圆括号、已有圆括号、显式
  完整类型实参、member 与 chained callee 后的一个尾 lambda。
- 无圆括号时建立一个 call；已有 `Expression::Call` 时追加到该 call 的 `arguments`，保持现有
  callee/type-argument identity，并把 call span 扩到 lambda closer。
- 生成 name/mode 均为空的普通 `CallArgument`；lambda expression/span/body 完全复用
  SPEC-0010，argument 映射、overload-lambda trial 与 capture/loan 继续复用后续阶段。
- LF、CRLF 或含换行 comment 后不得吸收 nested block；这不增加通用 block statement
  separator，block 内 `;` 继续沿用现行 unsupported-element 规则。
- 保持 postfix/type-argument 无副作用试探、lexical-owner recovery、UTF-8 Span、确定性 AST ID
  与线性复杂度不变量。

## 4. 非目标

- 不增加隐式 `it`、label return、receiver lambda、多个尾 lambda、参数 trailing comma、默认
  参数、`vararg` 或新的调用点 mode 拼写。
- 不改变 `CallArgument` 公共结构，不增加 TailLambda/Invoke 等 AST variant。
- 不修改名称、类型、所有权、SSA/LLVM、runtime 或 ABI 语义，也不实施 v0.33 的 project build。

## 5. 验收标准

- [ ] compile-pass/Parser 正例覆盖 `f {}`、`f(a) {}`、`f<T> {}`、member/chained callee、
  `move {}`、嵌套 lambda/call，以及跨 LF/CRLF gap 不吸收后续 nested block。
- [ ] AST 断言每例只有预期的单一 call 节点；尾 lambda 是最后一个无 name/mode `CallArgument`，
  argument/value/call span 与 AST ID 在重复解析中完全一致。
- [ ] compile-fail 覆盖同一 call 的第二个尾 lambda、尾部 name/mode 伪前缀、缺 lambda closer、
  typed-call 失败试探及 lexical poison；断言既有稳定诊断码与 UTF-8 byte Span，不产生级联。
- [ ] `f\n{}`、`f\r\n{}` 和含换行 comment 的对照仍解析为 expression statement + nested block；
  同行空格与无换行 comment 不改变尾 lambda AST。
- [ ] 既有 `parser_lambda`、`parser_call_argument`、`parser_expression`、`parser_block`、
  line-break/trivia/output-invariant suites 与 frontend/workspace 基线通过。
- [ ] Architecture 更新为实现后的事实，guide/roadmap/Spec 状态与实际启用门禁一致。

## 6. 技术方案与边界

只扩展 `lang-frontend` 的 postfix/type-argument call parser：当 callee 末端到下一个 `{` 的
trivia gap 不含换行时，复用现有 lambda primary parser，并通过现有 CallArgument constructor
建立或扩展 call。每轮只允许消费一个尾 lambda；已有 block element soft-stop 继续处理跨换行
的 nested block，分号仍按现行 block 错误处理。不把 callee 名称或类型环境带入语法判定。
该局部语法选择不需要 ADR。

## 7. 实施计划

1. [ ] 扩展 postfix/call parser 与 span/owner recovery → 验证：新增定向 Parser 正反测试。
2. [ ] 扩充 line-break、trivia、typed/member/chained 与 mutation 回归 → 验证：受影响 Parser suites。
3. [ ] 同步 Architecture、Spec 验收记录与状态 → 验证：workspace test、Clippy、文档一致性。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser、测试、Architecture 与完成记录 | `feat(frontend): parse trailing lambda calls (SPEC-0213)` |

## 9. 未决问题

- 无；v0.33 启用是状态门禁，不是语义未决问题。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_lambda lambda_accepts_existing_postfix_chain_but_not_trailing_lambda_call_sugar --locked --offline -- --exact` | 通过 | 现行 v0.32 负例证明 `f {}` 仍产生 L0013，尚未实现候选语法 |
