# SPEC-0064: 解析窄化接口委托

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-064` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.20](../guide/00-index.md)：[接口委托的独立实施边界](../guide/04-grammar-declarations-blocks.md#133-接口委托的独立实施边界) |
| 批准依据 | 用户明确启用 v0.20；当前持续 Goal 的站立授权 |
| 前置 Spec | SPEC-0017 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser / AST、诊断、fixture、Architecture |
| 语言语义变更 | 否；实现已批准的 v0.20 契约 |

## 1. Goal

普通 `class` 的 supertype entry 能确定性解析 `Interface by field`，保存真实 `by` 与目标名称，
并在缺失目标或扩张到其他 class-family / 任意表达式时局部恢复，不破坏后一 supertype、body
或顶层声明。

## 2. 范围与需求

- `SupertypeEntry` 增加可选 `DelegationClause`，保存完整 Span、真实 `by` 与三态目标名称。
- `by` 继续由 Lexer 产出 Identifier，只在 supertype 后按精确源码拼写提交。
- 只允许 ordinary `class` 使用该语法；value/interface/enum/object 上的同形语法使用 L0077。
- 缺目标使用新诊断 L0078 `expected delegation target`；`by make()`、`by a.b` 等扩张形式继续
  使用 L0077，并恢复到当前 supertype list owner。
- Phase 1 不判断目标是否同一主构造器的 `val` 字段，也不判断字段类型是否实现接口；这些
  名称与类型约束由 SPEC-0020 检查。

## 3. 非目标

- 不实现自动转发、override 冲突、receiver / Result 契约或所有权效果。
- 不支持属性委托、任意 delegate expression、运行时代理、裸 interface 值或 `dyn`。
- 不改变 class-family 之外的 Identifier、表达式或 TypeRef 语法。

## 4. 验收标准

- [x] ordinary class 单项/多项委托及委托与普通 supertype 混合时，AST 顺序、Span 和 source
      identity 精确。
- [x] `by` 在非委托上下文仍是普通 Identifier；Lexer token 类别和依赖图不变。
- [x] L0078 覆盖逗号、body、下一声明和 EOF 边界的缺失目标；零宽目标不跨 trivia 扩张父 Span。
- [x] 非 ordinary class、任意表达式、重复 `by` 和属性委托定向拒绝，恢复保留后一 entry/body。
- [x] N→2N 委托列表提供单调线性证据；真实 pass/fail fixture 由 harness 执行。
- [x] workspace fmt/check/Clippy/test/build、文档链接和 diff 检查全部通过。
- [x] Architecture、guide、教程、Spec 索引与验证记录同步；独立提交成功。

## 5. 实施计划

1. [x] 扩充 AST、L0078 与 supertype parser。
2. [x] 增加专用正反例、恢复、复杂度和 fixture 证据。
3. [x] 同步文档，执行 workspace 基线并独立提交。

## 6. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser、AST、诊断、测试、fixture、Architecture 与完成状态 | `feat(frontend): parse interface delegation (SPEC-0064)` |

## 7. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_interface_delegation --locked --offline` | 通过 | 5 passed；AST、L0078、拒绝边界与 Phase 2 延后 |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 316 passed；frontend 309、CLI 6、lang-std 1；0 failed / ignored / measured / filtered out |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 五个 workspace member 全部成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo build -p lang-cli --locked --offline` | 通过 | CLI dev target 构建成功 |
| Markdown 相对链接检查；`git diff --check` | 通过 | 本地目标均存在；无空白错误 |
