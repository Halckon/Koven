# SPEC-0063: 解析 postfix 错误传播运算符 `?`

| 字段 | 值 |
|---|---|
| 状态 | approved |
| Goal ID | `KOV-P1-063` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.19](../guide/00-index.md)：[`Result<T, E>` 与 postfix `?`](../guide/01-design-decisions.md#19-resultt-e-错误值与-postfix-v019-正式启用) |
| 批准依据 | 用户于 2026-08-20 同意前述完整错误值契约并要求继续实施；当前持续 Goal 的站立授权 |
| 前置 Spec | SPEC-0016 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser / AST、诊断回归、fixture、Architecture |
| 语言语义变更 | 是；必须先由 guide v0.19 批准，不能由本 Spec 自行确定 |

## 1. Goal

完整文件、block、lambda 与独立表达式入口能够确定性解析已批准 guide 所定义的 postfix
错误传播 `?`，并与 nullable type、safe call、Elvis 和其他 postfix 形成无歧义 AST。

## 2. 背景

SPEC-0016 已实现最近 callable `return`，满足候选设计的 Parser 前置。SPEC-0017 已预留给
class-family，因此使用尚未占用的插入式编号 SPEC-0063，避免重编号既有路线图。Lexer 已把
`?`、`?.`、`?:` 作为三个最长匹配固定符号；本 Spec 不需要词法增量。

现行 v0.19 已明确：
`?` 与裸 `return` 使用同一个最近 callable 边界；位于 lambda 内时只退出该 lambda，不形成
外层具名函数的非局部 return。

## 3. 范围与需求

- 增加 `Expression::Propagate { value, question_span }`；合成 Span 从 operand 起点到真实
  `?` 终点，保留 source identity。
- 把单字符 `?` 纳入现有最高优先级 postfix 循环，与 call、index、member、safe member、
  callable reference 和 `!!` 按源码顺序左结合；允许连续 postfix。
- 所有 Phase 1 表达式入口只按语法建立 AST，不根据名称、返回类型或 `Result` 实参拒绝
  `?`；最近 callable、`Result<T, E>` 与错误类型一致性留给 Phase 2。
- 复用 Lexer 已有最长匹配：`result?`、`result?.member`、`result ?: fallback` 必须走三个不同
  路径，Parser 不拆分 `?.` 或 `?:`。
- 普通缺 operand、尾随输入和 owner-aware 恢复复用已发布诊断含义；若无需新的稳定错误类别，
  不为本功能分配新错误码。
- 单个 postfix 链保持单调 `O(n)`，不回扫完整 token 流，也不为每个 `?` 递归遍历既有 AST。

## 4. 非目标

- 不检查 operand 是否为 `Result<T, E>`、外层 callable 返回类型或错误类型是否相同。
- 不实现 `Result` / `Ok` / `Err` 声明、名称解析、脱糖、所有权转移、IR 或 codegen。
- 不实现标签或从 lambda 非局部返回外层函数。
- 不改变 nullable `type_ref`、safe call `?.`、Elvis `?:`、non-null assertion `!!` 或其优先级。
- 不提前实现 SPEC-0017 class-family。

## 5. 验收标准

- [ ] compile-pass 覆盖 `result?`、`load()?`、`load()?.member?`、call / index / member / `!!` 的
      左结合组合，以及完整文件、block、lambda 和 initializer 上下文。
- [ ] AST 精确断言 operand、`question_span`、合成 Span、source identity 和确定性节点顺序。
- [ ] `result?` / `result?.member` / `result ?: fallback` / nullable `Result<T, E>?` 分别形成预期
      token 与 AST，不互相误判。
- [ ] 连续 `result??` 按两个 postfix 节点解析；其类型合法性留给 Phase 2。
- [ ] compile-fail 覆盖缺左 operand、独立 `?`、owner 边界附近的错误区，断言既有错误码、
      精确 Span、后续 expression / block element 保留且不重复 Lexer 根因。
- [ ] N 与 2N 长 postfix 链证明显著访问线性增长，并在接近 recursion budget 的输入上不因
      AST 回看而栈溢出。
- [ ] 真实 pass / fail `.ko` fixture 被 harness 枚举执行。
- [ ] frontend 窄测、workspace fmt/check/Clippy/test/build 全部通过，无 ignored / skipped。
- [ ] Architecture、guide roadmap、Spec 索引与验证记录同步为实现后事实。
- [ ] 独立提交成功。

## 6. 技术方案与边界

在 Pratt parser 既有 postfix 循环中消费 `Symbol::Question`，以当前 lhs 创建一个新的索引式
expression 节点，再继续处理后续 postfix。该结构天然表达左结合，不需要新的 precedence
分支或递归入口。`Symbol::QuestionDot` 与 `Symbol::QuestionColon` 继续分别由 member postfix
和 Elvis 层处理；nullable `?` 只由 `type_ref` parser 消费。

Phase 1 不从父 AST 反查 callable：Parser 对任何表达式位置都保留 `Propagate`。Phase 2 在
顺序遍历 callable body 时绑定最近 callable，并执行 guide 批准后的 `Result`、错误类型与
lambda 边界检查。这避免 Parser 根据尚不存在的类型信息产生伪诊断。

## 7. 实施计划

1. [x] 启用 v0.19 并解除 Spec 阻塞，进入 `approved` / `in-progress` → 验证：guide 版本、
       changelog、roadmap 与 Spec 引用一致
2. [ ] 扩充 AST 与 postfix parser → 验证：专用 AST、Span、结合性和消歧窄测
3. [ ] 补齐错误恢复、复杂度与真实 fixture → 验证：compile-pass / fail、N→2N 和 harness
4. [ ] 同步 Architecture 与验收记录 → 验证：workspace 全量基线和文档一致性检查

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | v0.19 语言契约与治理同步 | `docs(guide): define postfix propagation in v0.19` |
| 2 | Parser、AST、测试、fixture、Architecture 与完成状态 | `feat(frontend): parse postfix propagation (SPEC-0063)` |

## 9. 未决问题

- 无；lambda 与具名函数的最近 callable 边界已由 v0.19 封闭。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| Markdown 相对链接与 `git diff --check` | 通过 | 两份变更文档的本地目标均存在；无空白错误 |
| Rust / fixture / workspace 基线 | 未执行 | 等待 SPEC-0063 实施 |
