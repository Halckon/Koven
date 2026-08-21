# SPEC-0065: 按职责拆分 Parser 模块

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-065` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.20](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户明确要求按更新后的根 `AGENTS.md` 拆分 Parser |
| 前置 Spec | SPEC-0017、SPEC-0062、SPEC-0063、SPEC-0064 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser 内部模块、测试归属、Architecture |
| 语言语义变更 | 否；纯行为保持重构 |

## 1. Goal

在不改变 Parser 公共 API、AST、诊断或恢复行为的前提下，将聚合实现按稳定职责拆成可独立
维护的模块，并使所有手写 Parser 生产文件不超过 1000 个物理行。

## 2. 背景

当前 `parser/engine.rs` 同时承担入口编排、词法恢复索引、文件与声明、class-family、block、
表达式、postfix、TypeRef、上下文验证和测试职责，已经无法通过局部修改清晰判断变化边界。
现有 frontend 测试覆盖四个 Parser 入口、typed AST、精确诊断全序、owner-aware 恢复与线性
复杂度，可作为拆分前 characterization baseline。

## 3. 范围与需求

- `parser/mod.rs` 收敛为公共门面和 worker 入口；解析产物、语法模型与内部错误分别归档，
  并通过 re-export 保持现有公共路径。
- Parser engine 保留共享状态与入口编排，按词法恢复、文件、声明、class-family、解构、
  block、表达式、postfix、运算符、TypeRef、共享 cursor / AST / 诊断操作拆分子模块。
- 子模块只使用 engine 内部最小可见性共享 `Parser` 状态和必要方法；不复制 parser 状态、
  binding power、stop set、恢复索引或诊断规则。
- engine 内部单元测试移入专用测试模块，保持测试名称、断言与执行数量。
- 所有手写 `crates/lang-frontend/src/parser/**/*.rs` 生产文件不超过 1000 个物理行。

## 4. 非目标

- 不改变现行 guide、Lexer、AST shape、公开类型名、函数签名或模块外可见路径。
- 不新增语法、诊断码、依赖、日志、抽象 trait、缓存或 Phase 2 名称 / 类型行为。
- 不拆分 lang-frontend 的集成测试文件；它们不属于本次生产 Parser 模块职责。
- 不顺带重构 Lexer、Diagnostic、AST arena 或 fixture harness。

## 5. 验收标准

- [x] `parse_expression`、`parse_declaration`、`parse_block`、`parse_file` 及所有现有
      `lang_frontend::parser::*` 类型路径保持源兼容。
- [x] frontend 测试数量、测试名称和结果保持一致；AST、诊断内容 / 顺序、恢复与复杂度测试
      全部通过。
- [x] Parser 子模块按领域命名，不使用 `include!`、`utils.rs` / `common.rs`，不复制共享状态
      或扩大到 crate 公共可见性。
- [x] 所有手写 Parser 生产文件不超过 1000 个物理行，测试模块按独立职责归档。
- [x] workspace fmt、check、Clippy、test 和 CLI build 全部通过。
- [x] Architecture 已更新为实现后的 Parser 模块与数据流事实；Markdown 链接和 diff 检查通过。

## 6. 技术方案与边界

- `parser/mod.rs` 只保留模块声明、公共 re-export、四个 scoped-worker 入口和递归预算；解析
  产物、语法 payload、内部错误分别放入 `output`、`syntax`、`error`。
- `parser/engine.rs` 保留入口编排、共享 `Parser` 状态、stop / recovery owner 等跨领域不变量；
  各领域子模块通过 engine 内部可见的同类型 `impl Parser` 复用唯一 cursor、AST 与诊断流。
- 入口仍只构造一次词法恢复索引、strict-call trial 与 lambda-header index；正式 cursor 保持
  单调，所有诊断继续在原聚合边界使用 `ordered_diagnostics` 排序。
- 本次不引入新测试语义；拆分前的 309 个 frontend 测试作为 characterization baseline，
  拆分后以同一命令和 workspace 基线证明行为保持。

## 7. 实施计划

1. [x] 拆分公共门面、产物、语法模型与内部错误 → 验证：frontend check 和公共集成测试编译。
2. [x] 提取词法恢复及 engine 领域子模块 → 验证：每批提取后运行 frontend lib 与相关窄测试。
3. [x] 核对文件规模、可见性和重复定义 → 验证：行数审计、Clippy 与源码检索。
4. [x] 同步 Spec 验收记录与 Architecture → 验证：文档、模块树和实现一致。
5. [x] 执行 workspace 基线并创建独立提交 → 验证：提交只包含 SPEC-0065 范围。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 模块拆分、Architecture 与完成状态 | `refactor(frontend): decompose parser modules (SPEC-0065)` |

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --all-targets --locked --offline`（拆分前） | 通过 | 309 passed；0 failed / ignored / measured / filtered out |
| `cargo test -p lang-frontend --all-targets --locked --offline --quiet`（拆分后） | 通过 | 309 passed；测试 target、数量与结果保持一致 |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 316 passed；frontend 309、CLI 6、lang-std 1；0 failed / ignored / measured / filtered out |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 五个 workspace member 全部成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo build -p lang-cli --locked --offline` | 通过 | CLI dev target 构建成功 |
| Parser 文件规模与边界审计 | 通过 | 最大生产文件 963 行；无 `include!`、`utils.rs` / `common.rs` 或新增 `pub(crate)` engine 方法 |
| 公共声明名、Markdown 相对链接；`git diff --check` | 通过 | 拆分前后公共类型 / 函数名一致；本地链接均存在；无空白错误 |
