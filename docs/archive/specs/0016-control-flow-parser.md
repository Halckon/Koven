# SPEC-0016: 解析 control-flow、jump 与 super

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-016` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.18](../guides/v0.34-pre-restructure/00-index.md)：[control-flow、jump 与 super](../guides/v0.34-pre-restructure/04-grammar-declarations-blocks.md#12-spec-0016-control-flowjump-与-super) |
| 批准依据 | 用户于 2026-08-20 明确同意并要求实施 v0.18；当前持续 Goal 的站立授权 |
| 前置 Spec | SPEC-0009 `done`；组合入口依赖 SPEC-0014 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser / AST、诊断、fixture、Architecture |
| 语言语义变更 | 否；实现已经批准并启用的 v0.18 契约 |

## 1. Goal

完整文件、block、lambda 与独立表达式入口能够确定性解析 v0.18 的 `if` / `when`、
loop-family、jump 和 `super`，保留位置敏感 `else` 与 owner-aware 恢复证据。

## 2. 范围与需求

- 增加 `If`、`When`、`Return`、`Break`、`Continue`、`SuperMember` expression AST。
- 增加 control body、when entry / condition、for binding 结构，以及 `While` / `For` / `Loop`
  statement AST。
- 缺 `else` 的 `if` 只在完整 statement element 合法；其他 value context 发 L0057。
- 支持 subjectful / subjectless `when`、逗号条件、换行 / 分号 entry 分隔。
- 支持 loop body、for 单名称 / 解构 binding 与 `_`，以及最近 callable jump 的 Phase 1 结构。
- 实现 `super<Interface>.member` 并复用既有 postfix 链。
- 分配并测试 L0055–L0065，保持单调 owner-aware 恢复和确定诊断顺序。

## 3. 非目标

- 不做条件 Boolean 检查、分支类型、`Nothing`、return / break / continue target 检查。
- 不做 `when` 穷尽性、smart cast、iterator 名称绑定或接口默认方法检查。
- 不实现标签、非局部 lambda return、`do while`、postfix `?` 或 class-family。
- 不改变普通 block 的 `Unit` 规则、现有表达式优先级或已发布 L0001–L0054 含义。

## 4. 验收标准

- [x] statement / value context 的有无 `else` 正反例及精确 L0057 / Span。
- [x] `if` / `else if`、两种 `when`、条件族、entry 分隔和 control block AST 正例。
- [x] `while` / `for` / `loop`、单名称 / 解构 / `_` binding 与 jump AST 正例。
- [x] lambda 内裸 `return` 保存为最近 callable 结构；标签与 Kotlin 非局部拼写不获接受。
- [x] `super<Interface>.member` 可继续 call / member；缺组件产生 L0063 / L0064 / L0011。
- [x] L0055–L0065 反例恢复保留下一个 branch / entry / block element 且不重复 Lexer 根因。
- [x] 完整文件 pass / fail fixture 实际被 harness 枚举执行。
- [x] 窄测及 workspace fmt/check/Clippy/test/build 全部通过，无 ignored / skipped / filtered。
- [x] Architecture、guide roadmap、Spec 索引与验证记录同步为实现后事实。
- [x] 独立提交成功。

## 5. 技术方案与边界

control body 使用内嵌枚举连接 expression 或专用 `Statement::ControlBody`，从而保存尾表达式
语义而不改变普通 `Statement::Block`。Pratt primary 解析 expression-family 控制流；block / lambda
dispatcher 解析 loop statement，并只在最终根恰为缺 `else` 的 `If` 时授予 statement context。
when entry 与 owner baseline 恢复单调消费，不从每个 entry 回扫全输入。

## 6. 实施计划

1. [x] 扩充 AST、诊断目录与 primary / dispatcher → 验证：专用 Parser / diagnostic 窄测
2. [x] 实现 when / loop / super 恢复和上下文诊断 → 验证：正反例、Span 与恢复矩阵
3. [x] 增加 fixture、同步 Architecture 与验收记录 → 验证：fixture 和 workspace 基线

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser、AST、诊断、测试、fixture、Architecture 与完成状态 | `feat(frontend): parse control flow (SPEC-0016)` |

## 8. 未决问题

- 无；类型与目标解析边界已明确延后。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_control_flow --test diagnostic_model --test fixtures --locked --offline` | 通过 | control-flow 7、diagnostic 9、fixture 23；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --all-targets --locked --offline` | 通过 | 281 passed；0 failed / ignored / measured / filtered |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target 可检查 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo test --workspace --all-targets --locked --offline -q` | 通过 | 288 passed；0 failed / ignored / measured / filtered |
| `cargo build -p lang-cli --locked --offline -q` | 通过 | CLI 构建成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | `lang-frontend` 无外部依赖 |
| Markdown 相对链接检查；`git diff --check` | 通过 | 本 Spec 涉及的现行文档链接均存在；无空白错误 |
