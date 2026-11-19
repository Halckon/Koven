# F3d：泛型调用、Lambda 与完整 TypeRef 切片（冲突登记与外部谓词合同）

> **性质**：非规范试验账本 · **状态**：高风险构造原始冲突与外部谓词已实测登记，待 F1 Guide 分支正式启用 D1–D3 后锁定最终 golden · **读取时机**：实施 F1/D1–D3 Guide 同步或扩展 F4a 独立参考文法至泛型与 lambda 前 · **唯一真源**：本页维护 F3d 规则 ID、原始 LALR(1) 冲突清单与四类必选外部谓词

基点 `main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，现行规范 Guide v0.47，拟启用方向见
[F1 Guide 修订草案](guide-amendment-draft.md)（D1–D3 已获维护者同意，须在独立 `feature/spec-<id>` 分支同步 Guide 与生产实现）。
本轮不修改 Guide、生产 Parser、依赖或 CI；离线冲突实测使用 macOS 自带 GNU Bison 2.3，临时目录已清理。

## 1. 规则 ID 映射与依赖边界

| 规则 ID | 规范与草案来源 | 覆盖构造 | 所需外部谓词 / 消解规则 |
|---|---|---|---|
| `G-TYPEREF` | [03 TypeRef 与函数类型语法](../../guide/03-types-generics.md#typeref-与函数类型语法) | 限定名、嵌套泛型 `Outer<Inner<T>>`、可空 `T?`、函数类型 `[move] ([mode] T, ...) -> R` 与括号可空函数类型 `((T) -> R)?` | 词法层保持单个 `>`（不合并 `>>`）；软词 `move`/`own`/`borrow`/`inout` 按前瞻上下文区分；`((...) -> R)?` 与参数为函数类型的 `((...) -> R) -> R` 需超 1-token 前瞻区分 |
| `G-TYPED-CALL` | [03](../../guide/03-types-generics.md#typeref-与函数类型语法) + [04 Postfix](../../guide/04-expressions-operators.md#postfix调用索引与函数引用) + [D1 草案](guide-amendment-draft.md#d1尾-lambda-的附着点) | `f<T>(...)`、`f<T> { ... }`、`f<T>\n{ ... }`、`f()\n{ ... }` 与失败回退 `<` 比较 | 严格、无副作用 `<...>` trial；后继为 `(` 或按 D1 规则为 `{`（已开始调用后缀后允许跨行，但不得越过 `;` / caller boundary）才提交，否则从 `<` 零状态回退 |
| `G-LAMBDA` | [07 Lambda literal](../../guide/07-calls-lambdas-closures.md#lambda-literal) + [D2/D3 草案](guide-amendment-draft.md#d2return-的局部目标) | `move` 前缀、可选参数头 `a, b ->` / `->`、body 内无标签本地 `return`、嵌套普通 block vs 嵌套 lambda | D3 表达式 primary 位置 `move` 仅当下一非 trivia 为 `{` 时提交；严格 header trial 至 `->`；body element 位置裸 `{ ... }` 优先为嵌套 block（表达式位置 `({ ... })` 为嵌套 lambda）；D2 `return` 操作数不得跨 `NL` |

## 2. 原始 CFG 冲突实测清单（GNU Bison 2.3，不通过删构造隐藏冲突）

| 子切片 | 原始 CFG 冲突实测 | 根因与对应外部谓词 |
|---|---|---|
| `typeref_no_paren_nullable`（不含括号可空函数类型） | **0 conflicts**（退出 0） | 嵌套 `Outer<Inner<T>>` 与 `move (own T) -> R` 在单个 `>` + 软词前瞻下无冲突 |
| `typeref_raw`（含 `((T) -> R)?`） | **2 shift/reduce** | `'('` 后见 `'('` 或 `KW_MOVE` 时，1-token 前瞻无法区分「括号可空函数类型」与「首个参数为函数类型的函数类型」 |
| `typed_call_raw`（仅 `ID` 类型实参 + `<` 比较） | **0 conflicts**（仅限最简模型） | 纳入完整表达式后 `a < b > (c)` 在纯 CFG 上与比较链冲突，须依赖无副作用 `<...>` trial（`parser/trial.rs`） |
| `lambda_raw`（header + D2 `return` + 嵌套 block + `NL`） | **8 shift/reduce, 1 reduce/reduce** | 集中暴露三类真实边界：① `{ ID` 处 header vs 首表达式；② `{` 处嵌套 block vs 嵌套 lambda；③ `return` 后 `NL` 处裸 `return` vs 带操作数 `return` |
| D1 跨行尾 lambda（见 [F5 演练](syntax-gate-checklist.md#3-真实候选演练d1已开始调用后缀后的跨行尾-lambda)） | **1 shift/reduce**（前瞻 `NL`） | `f()\n{}` 存在「跨行尾 lambda」与「调用 + 换行分隔 + 嵌套 block」双推导，由 D1 条文显式规定已开始调用后缀后优先附着 |

## 3. 四类必选外部谓词合同（F3d 不可由纯 LALR(1) 替代）

1. **无副作用试探（Zero-Side-Effect Trial）**：`typed_call_suffix` 的 `<...>` 与 `lambda_header` 的 `id, ... ->`
   失败时必须零状态回退，不得保留已分配 AST/TypeRef、诊断或消费状态。
2. **上下文软词前瞻（Contextual Soft Keywords）**：表达式位置 `move` 仅在下一非 trivia 为 `{` 时作为 lambda 前缀（D3）；
   TypeRef 位置 `move` 仅在下一非 trivia 为 `(` 时作为函数类型前缀，`own`/`borrow`/`inout` 仅在参数类型起点前作为模式前缀。
3. **换行阻断与附着优先级（NL Predicates）**：`return` 操作数遇 LF/CRLF/含换行块注释即截断为裸 `return`（D2）；
   已开始的 `(...)` 或 `<...>` 调用后缀遇 `NL+` 后接 `{` 时优先附着为尾 lambda，但遇 `;` 或 caller boundary 立即阻断（D1）；
   未开始调用后缀的 `f\n{}` 不附着。
4. **块/Lambda 位置分派（Statement vs Expression Context）**：在 block/lambda body element 起点出现的裸 `{ ... }`
   按嵌套普通 block 解析，不作为独立 lambda；需作为 lambda 值时通过表达式位置（如 `({ ... })`、赋值或实参）区分。

## 4. 证据边界与后续实施前置条件

- **证据等级**：本页仅登记 F3d 高风险构造的原始 LALR(1) 冲突位置与必选外部谓词合同，**不宣称全 F3d 零冲突或已完成生产差分**。
- **未运行项**：本轮未修改 Guide、生产 Parser 或 CI；因沙箱无 `cargo` 与主仓库 `.git` 权限，未执行 `cargo test` 或创建新的实施分支。
- **正式实施顺序（按依赖图）**：
  1. 在主仓库基于最新 `main` 新建 `feature/spec-<id>` 分支，分配全局 Spec 编号（当前本工作树基线 `docs/specs/inventory.md` 最新归档至 `0476`，开分支前须以最新 `main` 的 `inventory.md` 为准），先同步启用 **F1（D1–D3 Guide 修订）** 与对应 Parser 红绿测试。
  2. 在独立 `feature/spec-<id>` 分支持久化 **F4a（离线 Lark Earley 普通块差分）**，补齐双宿主与固定 Python 版本复现验收。
  3. 再基于已启用的 D1–D3 将 F3d 正反例接入 F4a 扩展语料，并按 F4b/F5 推进 Tree-sitter 实跑与提案模板正式入库。
