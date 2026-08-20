# SPEC-0013: 解析局部 `val` 解构

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-013` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.14 文档集](../guide/00-index.md)：[`05-grammar-calls-lambda.md` 第 9 节](../guide/05-grammar-calls-lambda.md)、[`06-roadmap.md` Phase 1](../guide/06-roadmap.md) |
| 批准依据 | 用户在当前持续 Goal 中授权继续分阶段实施 Specs；现行 v0.14 已启用 |
| 前置 Spec | SPEC-0012 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` block / lambda body Parser、statement AST、诊断、fixture、Architecture |
| 语言语义变更 | 否；实施现行 v0.14 已定义的 Phase 1 解构语法与恢复契约 |

## 1. Goal

完成后，`lang-frontend` 能在 block 和 lambda body 中把
`val (a, b) = expression` 解析为唯一 `Statement::LocalDestructuring`，保留有序
binding、真实 delimiter 与唯一 initializer，并以 `L0040`–`L0046` 对未支持
pattern、缺失 token 和非局部上下文做确定性 owner-aware 恢复。

## 2. 背景

SPEC-0009 已建立 block / statement 序列，SPEC-0010 已建立 lambda body，
SPEC-0012 已完成当前最后一项前置 callable 语法。现有局部声明只接受单名
`val` / `var`；本 Spec 只在两种 body dispatch 中增加已封闭的局部 `val` 解构，
为 Phase 2 的分量名称 / 数量检查与 Phase 3 的原子所有权转移保留源码结构。

## 3. 范围与需求

### 3.1 提交与 AST

- 仅在 block / lambda body 已识别 `val` 且下一显著 token 为 `(` 时提交解构；
  其他 `val name ...` 继续使用现有 `Statement::LocalVariable` 路径。
- 正常产生式精确为 `val (Identifier {, Identifier}) = expression`；至少一个
  binding，不接受 `_`、nested pattern、binding / pattern 类型标注或 trailing comma。
- 在 `Statement` 中唯一新增 `LocalDestructuring { val_span, left_paren_span,
  bindings: Vec<NameMarker>, right_paren_span: Option<Span>, equals_span: Option<Span>,
  initializer: ExpressionId }`；不新增 pattern table、Item 变体或 AST ID 类别。
- bindings 严格按源码顺序保留 `Present` / `Missing` / `Error`；initializer 只有
  一个 `ExpressionId`。重名、分量数量、拷贝 / 消费语义不属于 Parser。
- statement `Span` 从真实 `val` 起到 initializer 或最后实际消费 token 终；
  零宽 child 不跨 trivia 扩大父范围，缺 delimiter 不伪造 `Some(Span)`。

### 3.2 诊断与上下文

在集中 catalog 连续注册：

| 错误码 | 固定消息 | 核心边界 |
|---|---|---|
| `L0040` | `expected destructuring binding` | 缺 binding 时使用空边界；空项逗号覆盖并消费逗号 |
| `L0041` | `expected destructuring separator` | 相邻 Identifier 取后者起点空范围；普通非法区主范围只覆盖首 token |
| `L0042` | `unsupported destructuring form` | `_`、nested / typed pattern 或 body 中 `var (` / `const val (` |
| `L0043` | `unsupported destructuring context` | 独立声明入口的 `val (` / `var (` / `const val (` 覆盖真实 `(` |
| `L0044` | `unsupported destructuring trailing comma` | 覆盖并消费 `)` 前的真实逗号 |
| `L0045` | `expected destructuring initializer separator` | 缺 `=` 时保留 expression / boundary，非法区覆盖实际消费范围 |
| `L0046` | `expected destructuring initializer` | 已消费 `=` 后缺表达式；Lexer poison 根因不重复分类 |

- `var (` / `const val (` 在 block 和 lambda body 中形成单一 `Statement::Error`；
  独立声明入口的三种前缀形成单一 `Item::Error`。不提前实现 SPEC-0014 文件顶层同步。
- 缺 `)` 复用 `L0010 expected closing delimiter`；当前 token 为 `=` 时保留它并
  继续 initializer。缺 `=` 且当前可开始 expression 时只发 `L0045` 并继续解析。
- 绑定恢复的顶层 soft stop 为 `,` / `)` / `=`，initializer 恢复继续使用
  block / lambda 的 element boundary 与调用方 hard stop。nested delimiter、string /
  interpolation owner 内的同形 token 不能冒充边界。
- 同根因不产生 binding / closer / initializer 级联；每轮消费输入或停在明确
  boundary，单个 pattern 与 initializer 合计 `O(n)`。

### 3.3 测试与回归

- 新增专用 integration test，覆盖 block / lambda 正例、AST / `Span`、
  `L0040`–`L0046`、`L0010`、上下文拒绝、owner / terminal Lexer、multi-SourceMap、
  深嵌套和长序列单调性。
- 在现有 block / lambda fixture suite 中各接入代表性 pass / fail，不复制 runner。
- 复跑 SPEC-0007–0012 受影响回归、fixture harness 与 workspace 基线。

## 4. 非目标

- 不实现 `var` / `const val` 解构、`_`、nested pattern、类型标注、trailing comma、
  解构赋值、参数 / for / catch pattern 或用户自定义 pattern 抽象。
- 不实现 Phase 2 的重名、分量完整性、类型 / `componentN()` 检查，或 Phase 3
  的拷贝、移动、完整消费和析构。
- 不实现 SPEC-0014 完整文件 / 跨声明恢复，也不提前实现 control-flow、
  class-family、module / import 或新的 AST table / 依赖。

## 5. 验收标准

- [x] block 与 lambda body compile-pass 覆盖单 / 多 binding、复杂 RHS、前后 element、
      nested body 与 trivia；唯一 initializer 且源码顺序不变。
- [x] AST 精确为唯一 `LocalDestructuring` 变体；真实 token / `Option<Span>`、
      binding marker、statement 与 initializer `Span` 满足同源 UTF-8 半开范围和零宽规则。
- [x] `L0040`–`L0046` 每类至少有一个最小 compile-fail，断言固定消息、主 `Span`、
      AST marker / Error 区、element 数量与恢复顺序。
- [x] 空项、缺 separator / `)` / `=` / initializer、trailing comma、`_`、nested / typed
      pattern 的分支优先级不产生同根因级联；缺 `)` 的 `L0010` 保留 opener label。
- [x] body 中 `var (` / `const val (` 与独立声明入口的三种解构前缀分别
      产生 `L0042` / `L0043`，不构造错误的 LocalDestructuring。
- [x] owner recovery 覆盖 nested delimiter、string / interpolation、`L0004`–`L0006`、异形
      outer hard closer 与恢复后合法 element；诊断顺序确定，无 panic。
- [x] multi-SourceMap、1024 递归预算和长正确 / 错误 pattern N→2N 证明同源、
      受控资源边界与单调 `O(n)`。
- [x] 代表性 block / lambda pass / fail fixture 真实执行；非零、sidecar 与 orphan 守卫继续有效。
- [x] 生产 catalog 精确新增 `L0040`–`L0046`，`L0001`–`L0039` 编号、消息、
      severity 与顺序无回归。
- [x] 受影响窄测试与 workspace fmt / check / Clippy / test / CLI build 全部通过；
      依赖树、manifest 与 lockfile 证明未新增依赖。
- [x] Architecture、guide / roadmap 状态、Specs 索引、本 Spec 任务与验证记录只陈述
      实际完成事实。

## 6. 技术方案与边界

在现有 `lang-frontend` Parser 内增加一个 body 局部 dispatch 与一个内嵌 statement
payload。binding 列表复用 `NameMarker`；initializer 复用现有 expression Parser 与
block / lambda Stops。恢复复用已有的 delimiter / lexical-owner 扫描不变量，只增加
destructuring 层的明确 soft stop；不改 workspace、crate 依赖或长期架构边界。

## 7. 实施计划

1. [x] 增加 AST 变体、`L0040`–`L0046` 与 block / lambda / declaration 提交分流
   → 验证：最小上下文正反例与 catalog 测试
2. [x] 实现 binding list、initializer、`Span` 与 owner-aware 恢复
   → 验证：专用 integration test 的分支矩阵、owner、同源与线性证据
3. [x] 接入 block / lambda fixture 并复跑 SPEC-0007–0012 窄回归
   → 验证：现有 fixture guard 和受影响 test targets
4. [x] 同步 Spec 验收记录、Architecture、guide 与索引
   → 验证：workspace 全基线、链接和 staged diff 一致

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | LocalDestructuring AST / Parser、L0040–L0046、测试 / fixture、Architecture 与完成记录 | `feat(frontend): parse local val destructuring (SPEC-0013)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 前置门禁 | 满足 | 现行 v0.14 已启用；SPEC-0012 已于提交 `6387759` 完成；当前持续 Goal 的站立授权有效 |
| `cargo test -p lang-frontend --test parser_local_destructuring --locked --offline` | 通过 | 14 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_block --locked --offline` | 通过 | 20 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_lambda --locked --offline` | 通过 | 16 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_declaration --locked --offline` | 通过 | 22 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_expression --locked --offline` | 通过 | 52 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test diagnostic_model --locked --offline` | 通过 | 9 passed；生产 catalog 精确为 L0001–L0046 |
| `cargo test -p lang-frontend --test fixtures --locked --offline` | 通过 | 22 passed；block / lambda 各新增 1 pass + 1 fail 并真实执行 |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target 检查通过 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 合计 252 passed；0 failed / ignored / measured / filtered |
| `cargo build -p lang-cli --locked --offline` | 通过 | `lang-cli` dev build 成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend` 自身；manifest / lockfile 无差异 |
| Markdown 链接与 `git diff --check` | 通过 | 纳入治理范围的 44 份 Markdown 共 225 个本地目标零断链；无 whitespace error |
| Architecture 同步 | 通过 | 已记录 LocalDestructuring、L0040–L0046、fixture 及 Phase 2 / 3 边界 |
