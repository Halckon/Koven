# AGENTS.md — Koven Agent 工作入口

本文件适用于整个仓库，只保留每次任务都必须知道的规则。先用 §1 目录表定位职责，再按
读取路由打开对应 crate 的 `AGENTS.md`；领域细节按需披露，不做整仓预读。

## 1. 目录结构与读取路由

### 1.1 目录职责（两层）

| 路径 | 职责 |
|---|---|
| `crates/lang-frontend/` | Source/Span、Lexer、AST/Parser、名称/类型/所有权、格式化、诊断；不依赖 LLVM |
| `crates/lang-codegen/` | typed SSA、verifier、LLVM、目标文件与 native runtime 边界；不重推语言语义 |
| `crates/lang-cli/` | 流水线编排、项目发现、诊断渲染、链接与进程退出；不承载核心算法 |
| `crates/lang-lsp/` | LSP 协议、位置适配、复用 frontend 诊断；不复制检查器 |
| `crates/lang-std/` | `koven/**/*.ko` 标准库实现真源；Rust target 只承载 Cargo 边界 |
| `docs/guide/` | 现行语言语义与强制 Phase 规范；唯一真源，当前 v0.36 |
| `docs/architecture/` | 当前实现事实快照 |
| `docs/development/` | 开发、验证与交付规则 |
| `docs/specs/`、`docs/adr/` | 一次变更的合同；guide 留白处的长期决策记录 |
| `docs/proposals/`、`docs/archive/` | 未启用候选设计与只读历史；默认不读取 |
| `editors/` | tree-sitter / textmate 编辑器支持 |
| `scripts/` | `check_docs.py` 文档结构门禁及其测试 |

每个 crate 目录都有自己的 `AGENTS.md`（按任务读取路由 + 边界规则）；crate 依赖保持
单向无环，只有跨 crate API 才使用 `pub`。

### 1.2 权威边界

1. 用户当前要求决定本次授权范围。
2. 本文件及作用域更近的 `AGENTS.md` 规定工作方式。
3. [现行 Koven v0.36 规范](docs/guide/README.md)规定语言语义与强制 Phase 边界。
4. 已批准 Spec 规定一次交付；accepted ADR 记录 guide 留白处的长期架构决定。
5. 代码、测试和 [Architecture](docs/architecture/README.md)证明当前实现事实。

### 1.3 读取路由

| 任务 | 入口 |
|---|---|
| Lexer、Parser、类型或所有权 | [lang-frontend](crates/lang-frontend/AGENTS.md) |
| SSA、LLVM、native 或 runtime | [lang-codegen](crates/lang-codegen/AGENTS.md) |
| CLI / project | [lang-cli](crates/lang-cli/AGENTS.md) |
| LSP | [lang-lsp](crates/lang-lsp/AGENTS.md) |
| 标准库 | [lang-std](crates/lang-std/AGENTS.md) |
| 文档、Spec、ADR | [docs 治理](docs/AGENTS.md) |

## 2. 全局开发护栏

- 始终使用中文沟通；不清楚的语义必须指出并询问，不凭 Kotlin、Rust 或 LLVM 经验补齐。
- 先读公开接口、直接调用方、共享类型和现有测试，再修改；每行 diff 都应能追溯到当前需求。
- 优先最小、单一职责的实现，不添加未请求的抽象、配置、兼容层或未来功能。
- 生产 Rust 文件以 1000 物理行软上限；新增独立职责优先按领域提取，不机械切片。
- 保持确定性；稳定产物和诊断不得依赖无序集合迭代、机器路径、时区或随机状态。
- 非法用户源码必须返回结构化诊断，不得进入无说明的 `panic!`、`unwrap()` 或 `expect()`。
- 不用大范围 `clone()` 掩盖阶段所有权问题；`unsafe` 只允许存在于最小边界并附
  `// SAFETY: ...`。
- 库 crate 不直接打印日志；用户输出由 CLI/LSP 边界统一渲染。
- 保留用户的未提交改动，不顺手重构、格式化或清理无关代码。
- 避免新增依赖，除非已按 [依赖治理](docs/development/dependencies.md)完成适配、兼容、安全、许可和成本审计。

## 3. 架构硬边界

```text
源码 → Lexer → Parser / AST → 名称与类型检查 → 所有权检查
     → typed SSA → LLVM IR → 目标文件 → 本机可执行文件
```

- workspace 固定为 `lang-frontend`、`lang-codegen`、`lang-cli`、`lang-lsp`、`lang-std`；新增 member
  需要独立架构决策。
- AST、阶段产物与诊断保留可追溯 `Span`；阶段通过明确输入/产物通信，不使用隐式全局状态。
- LLVM 细节只存在于 `lang-codegen`；语言语义真源在 guide，不在任何实现层。

## 4. 变更与验证

1. 明确所属 Phase、假设、非目标和可验证成功标准。
2. 行为变化先有失败测试，再做最小实现；规范未定义或互相冲突时停止。
3. 按 [测试与分层验收](docs/development/testing.md)选择直接行为、共享契约和下游检查；不以无关全量测试代替定向证据。
4. 禁止并发运行争用同一 Cargo target 的本地门禁；不以 `cargo clean` 作为普通步骤。
5. 同步当前事实到 Architecture、验收到 Spec；未运行的检查必须明确报告。
6. 语言语义变化必须先形成并明确启用新 guide；新功能需要 Spec，长期架构变化需要 ADR。

纯文案、链接和历史归档不需要 Spec。完成的 Spec、被取代的记录和旧规范进入 archive；它们不再
参与默认开发路由。

## 5. 明确禁止

- 不提前实现 v2 动态分发/`Shareable`、v3 协程、v4+ 自举或未排期保留字语义。
- 不把普通 `class`、`value class`、`Box`、`Rc` 或 Koven 的简化借用规则替换为 Kotlin/Rust 的默认语义。
- 不复制关键字表、运算符表或完整语言规则到 AGENTS、Spec、ADR、Architecture。
- 不把候选 proposal、draft Spec 或代码现状反向提升为现行规范。
- 不宣称未执行、被跳过、超时或仍在运行的检查已经通过。

## 6. 交付格式

简要说明结果与 Phase、必要修改、验证命令及结果、未运行项和剩余不确定性。“完成”只用于请求
范围已实现且必需验证没有静默跳过的情况。
