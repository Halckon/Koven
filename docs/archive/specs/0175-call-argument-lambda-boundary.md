# SPEC-0175：修复调用实参 lambda 的 block 边界误判

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25 lambda / call argument 语法](../guides/v0.34-pre-restructure/05-grammar-calls-lambda.md) |
| 前置 Spec | SPEC-0010、SPEC-0012 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` call argument 边界、Parser block/callable 回归、Architecture |
| 语言语义变更 | 否；修复已合法语法的 Parser 实现漂移 |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |

## 2. Goal

让 block 内调用的未分组 lambda 实参（如 `apply({ item -> consume(item) })`）被解析为实参
表达式，而不是因 `{` 同时可开始 block element，提前把调用判为空实参并产生级联诊断。

## 3. 范围与需求

- call argument 的逗号、右括号与 EOF 仍优先作为边界。
- outer block stop 只有在当前 token 不能开始表达式时才终止实参；合法 `{` lambda 必须进入
  expression parser。
- 未分组空 body 与含 nested call 的 lambda 均合法，调用后的下一个 block element 保持可见。
- 不改变 lambda header、AST 形态、错误码、恢复 owner 或 grouped expression 语义。

## 4. 非目标

- 不实施 SPEC-0174 的 overload lambda 候选选择，也不改变 Phase 2 类型规则。
- 不接受 trailing lambda、匿名内部类或其他未定义语法。
- 不重构 Parser boundary 模型，不新增依赖或诊断码。

## 5. 验收标准

- [x] block Parser 无诊断接纳未分组 lambda 实参，保存一个 lambda、外层与 nested 两个 call，
      并保留后续 local。
- [x] callable typed test 与 Phase 2 fixture 不再依赖额外 group，Borrow/Inout expected contract
      继续通过。
- [x] 既有 call argument、block owner 与恢复测试不回归。
- [x] 一次受影响窄测和一次 workspace 标准基线通过；Architecture、Spec 索引与验证记录同步。

## 6. 技术方案

只收窄 `call_argument_boundary`：outer stop 与合法 expression starter 重叠时，由 expression
starter 获胜；逗号、右括号和 EOF 的调用 owner 边界保持不变。公开 block 测试锁定失败前
会产生 L0010/L0033/L0013/L0029 级联的最小形态。

## 7. 实施计划

1. [x] 建立失败回归并定位边界优先级 → 验证：未分组 typed callable 用例修复前失败。
2. [x] 最小修正 call argument boundary → 验证：同一用例通过。
3. [x] 补 Parser/fixture 回归并执行简化验收 → 验证：一次窄测与一次 workspace 基线。
4. [x] 同步文档、审查独立 staged diff 并提交 → 验证：仅包含本 Spec 范围，提交信息包含
       `SPEC-0175`。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | boundary 修复、回归、Architecture 与完成记录 | `fix(frontend): parse lambda call arguments (SPEC-0175)` |

## 9. 未决问题

- 无；SPEC-0174 的 guide 冲突是独立门禁，不影响本 Parser 修复。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| frontend 受影响窄测批次 | 通过 | `parser_block`、`parser_call_argument`、`type_callable`、`type_checking` 共 88 passed；0 failed / ignored / filtered out |
| workspace Cargo 基线 | 通过 | fmt、check、Clippy `-D warnings`、all-target tests 与 `lang-cli` build 均退出 0 |
| 首次 workspace 预检 | 已修正 | `cargo fmt --check` 发现新增测试的机械换行差异；运行 `cargo fmt --all` 后完整基线通过 |
| `git diff --check` | 通过 | 无 whitespace error |
