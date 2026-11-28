# F6：局部机器证明前置设计与首片选型方案

> **性质**：非规范前置设计与收口索引 · **状态**：14 片 F6 双轨证明全部完成并归档（`SPEC-0490`/`0492`/`0498`–`0500`/`0505`–`0510`/`0512`–`0514`） · **读取时机**：查阅 F6 证明范围、工具路线、证据边界与已归档切片账本时 · **唯一真源**：各归档 Spec 验收账本；本页维护总体选型与证据边界

基点：`main@f3b9cc5ad28ffe8db95ff16a0ebda8b70f182ce7`，Guide v0.47。
本设计严格遵守[主计划](../parser-formal-verification-plan.md#5-分阶段执行与退出条件)与[验证合同 §4](contracts/validation.md#4-文法证据与版本合同)：
**不引入外部证明器依赖，不修改生产 Rust 代码，不宣称任何算法已取得 G4 证明。**
在无精化证明连接模型与生产 Rust 代码前，纯模型定理只能记为模型级证明，有界模型检验只能记为有界验证，二者均不得冒称全输入生产 G4。

## 1. 候选稳定算法评估与首片选型

从当前已稳定、边界清晰的 Parser 恢复内部模块中筛选出 3 个候选算法：

| 候选 | 生产源码定位 | 核心规模 | 现有证据上限 | 形式化适配度与建议 |
|---|---|---|---|---|
| **候选 A（推荐首片）**：`angle_group_closers` 反向 8 状态位掩码 DP 与前向 `OrdinaryGroups` 等价性 | `crates/lang-frontend/src/parser/engine/recovery.rs`（`angle_group_closers`，L594–645）与 `ordinary_groups.rs`（`OrdinaryGroups::visit_group`，L165–207） | ~50 行纯整数/位掩码 DP + ~35 行栈状态机，零外部副作用 | `angle_group_tests.rs` 仅穷举长度 `n = 5`（$6^5 = 7{,}776$ 组，属于 `tested` 级有限见证） | **首选**：规约天然是无限长数学等价定理（`∀ n ≥ 0`），状态空间极紧凑（`active ∈ 0..8`），无词法/语法回调依赖，且最容易同时对接 Lean 4 模型归纳与 Kani 直接 Rust 检验 |
| **候选 B（次选第二片）**：`OrdinaryGroups` / `GroupClosers::confirmed_closer` 栈计数不变量与边界单调性 | `crates/lang-frontend/src/parser/engine/recovery/ordinary_groups.rs`（L1–344） | ~340 行，含 `RecoveryBoundaries`、4 项水位、`confirmed_closer` 与 `skip_before_closer` | `separator_tests.rs` 与 `parser_block_line_continuation/group_boundary.rs` 有限用例 | **次选**：适合作为候选 A 之后的第二片；依赖 `Stops` 位掩码与 `starts_block_statement` 词法前瞻，抽象边界略宽 |
| **候选 C（暂缓）**：`LexicalRecoveryIndex` 词法 owner 栈健全性与 `recover_declaration_tokens` 良基终止性 | `crates/lang-frontend/src/parser/engine/recovery.rs`（L53–380、L669–980） | >600 行，耦合 `strict_trials`、`lambda_headers`、`file_angle` 与多处回退 | 线性访问计数与对抗性矩阵 | **暂缓**：耦合面广、Rust 切片与可变状态复杂，不适合作为首个机器证明切片 |

## 2. 首片（候选 A）精确输入域、可信前提与源码对应

### 2.1 精确输入域

对 `angle_group_closers(lexed, terminals)` 剥离词法过滤层后，核心算法作用于有限分隔符序列：

- **分隔符种类**：`Kind ::= Paren (0) | Bracket (1) | Brace (2)`，对应 `group_delimiter(symbol)` 返回的 `kind ∈ {0, 1, 2}`。
- **开闭方向**：`Opening ::= true (opener) | false (closer)`。
- **核心输入序列**：`D : List (Nat × Kind × Bool)`，其中首分量 `raw` 满足严格单调递增（`∀ p < q < D.length, D[p].raw < D[q].raw`），对应源码中 `delimiters: Vec<(usize, usize, bool)>`。
- **祖先活动集合**：`active ∈ {0, …, 7}`，第 `k` 位为 `1` 表示当前存在至少一个未闭合的 `kind = k` 祖先分组（`shape = 1 << kind`）。

### 2.2 显式可信前提（TCB）

以下边界不在候选 A 首片定理内，必须作为显式前提登记：

1. `GroupOwners::outside_lexical_symbol` 已正确屏蔽字符串/插值内部符号并按序推进 `TerminalOwnerEvent`，产出纯顶层普通分隔符子序列。
2. `delimiters` 中的 `raw` 下标由 `lexed.lexemes().iter().enumerate()` 生成，严格递增且所有 `raw < lexed.lexemes().len()`。
3. 目标平台 `usize` 满足 `delimiters.len() + 1` 不发生整数溢出（在 64 位宿主上由输入长度受内存上限约束保证；Kani 轨道可在有界输入下直接验证无溢出）。

### 2.3 核心证明义务（P1–P4）

| 编号 | 命题名称 | 数学陈述（对任意长度 `n = D.length` 与任意 `D`） | 对应 Rust 源码位置 |
|---|---|---|---|
| **P1** | **边界表有界与无越界** | `∀ i ≤ n, ∀ active < 8, i ≤ boundaries[i][active] ≤ n`；且当 `closes(end, kind)` 为真时必有 `end < n`，从而 `delimiters[end]` 与 `matched[raw]` 永不越界 | `recovery.rs:610–642` |
| **P2** | **8 状态祖先转移主引理** | 对任意起点 `i ≤ n` 与任意祖先位掩码 `active < 8`（等于某个外层栈 `S` 的非零计数集合），`boundaries[i][active]` 精确等于：从 `i` 起以空局部栈在祖先掩码 `active` 下向前扫描（遇到 opener 压入局部栈；遇到 closer 时若局部栈中存在同类 opener 则先按栈规则弹出直至闭合该同类 opener），首次遇到「局部栈中不存在同类 opener，且为属于祖先的闭合符（`!opening ∧ (active & (1 << kind) ≠ 0)`）或孤立右花括号（`!opening ∧ kind == 2`）」时的下标（若始终未遇则为 `n`） | `recovery.rs:616–635` |
| **P3** | **反向 DP 与前向 `OrdinaryGroups` 无限长等价（主定理）** | 对任意 `i < n`，若 `D[i] = (raw_i, kind_i, true)`，则反向 DP 写入的 `matched[raw_i]` 等于从 `i` 起新建空 `OrdinaryGroups::default()` 逐个执行 `visit(raw, symbol)` 后首次返回 `Some(raw_i)` 时的闭合符 `raw`（若从未返回则为 `None`） | `recovery.rs:636–642` ↔ `ordinary_groups.rs:177–207` |
| **P4** | **`NonZeroUsize` 编码无损性** | 若 `D[i]` 为 opener 且存在匹配 closer `D[end]`，则必有 `i < end`；结合 `raw` 严格单调递增可得 `D[end].raw ≥ 1`，故 `NonZeroUsize::new(delimiters[end].0)` 永不返回 `None` | `recovery.rs:639` |

> **关键证明结构**：P2 对 `n - i` 做逆向强归纳（因为 `opening` 分支先跳到 `end = boundaries[i + 1][active | shape]`，再在 `closes(end, kind)` 时递归读取 `boundaries[end + 1][active]`，其中 `i < end + 1 ≤ n`，归纳度量严格递减）。由 P2 令 `active = 1 << kind_i` 即可直接推出 P3，将 `angle_group_tests.rs` 目前仅覆盖 `n = 5` 的有限穷举提升为对所有 `n ≥ 0` 成立的数学定理。

## 3. 工具选型对比与推荐双轨方案

本机当前未安装任何证明器（已实测 `lean`/`lake`/`coqc`/`kani`/`z3` 均不在 `PATH`）。正式启用前，各候选工具权衡如下：

| 工具 | 能证明的性质 | 生产 Rust 对应方式 | 成本与局限 | 证据等级 |
|---|---|---|---|---|
| **Lean 4** | P1–P4 的**无限长（`∀ n ≥ 0`）**机器检查定理 | 独立数学模型（`formal/parser/f6_angle_groups/`，零 Mathlib）+ 路径触发 CI 校验 | 无法自动验证生产 Rust 编译器语义，存在“模型 ↔ Rust 源码”的人工精化审查间隙 | `proved (model, unbounded)`（**非**生产 G4） |
| **Kani** | 直接对生产 Rust 函数验证无 panic、无越界、无溢出及 P3 等价性 | `#[cfg(kani)]` harness 直接调用生产 `angle_group_closers` 与 `OrdinaryGroups` | 受 CBMC 展开深度限制，仅能覆盖有界长度（如 `n ≤ 8` 或 `n ≤ 12`） | `validated (bounded Rust, n ≤ K)` |
| **Verus / Creusot** | 直接在 Rust 源码或其翻译层做无限长演绎证明 | 需按工具注解重写 Rust 子集或维护专用工具链镜像 | 对现有 `rustc 1.96.0` 与 workspace 依赖治理侵入过高，首片维护成本过大 | 暂不推荐用于首片 |

### 推荐策略：Lean 4 + Kani 双轨并行 + 路径触发的轻量真实 CI

不采用“原始源码文本哈希失效门禁”（注释、格式调整或变量重命名都会引发高频误报，且无法提供语义验证）；改为由 **路径触发的轻量真实 CI** 直接运行双轨校验：

1. **目录与依赖隔离**：
   - Lean 4 证明工程放在独立顶层目录 `formal/parser/f6_angle_groups/`，**仅依赖 Lean 4 核心库（零 Mathlib）**，不加入 `Cargo.toml` workspace，零 Cargo 依赖变更。
   - Kani harness 仅放在 `#[cfg(kani)]` 条件编译块内，不影响常规 `cargo build` / `cargo test` / `cargo clippy`。
2. **路径触发的轻量真实 CI（独立 workflow，先 advisory）**：
   - 新建独立 workflow（如 `.github/workflows/parser-formal-f6.yml`），仅在 `formal/parser/f6_angle_groups/**`、`crates/lang-frontend/src/parser/engine/recovery.rs` 或 `crates/lang-frontend/src/parser/engine/recovery/ordinary_groups.rs` 发生变化时触发，对无关 PR 耗时为 **0**。
   - **Lean 4 轨**：因零 Mathlib 依赖且仅 ~200–300 行纯归纳证明，缓存固定版本工具链后单次 `lake build` 仅需数秒，负责验证 `∀ n ≥ 0` 的无限长算法定理（P1–P4）。
   - **Kani 轨**：固定有界长度（如 `n ≤ 6` 或 `n ≤ 8`），直接对生产 Rust 函数做符号模型检验，**语义级捕获生产实现漂移**（注释与 `rustfmt` 不误报，逻辑改动即真实验证无 panic、无越界与等价性）。
   - 先以 **advisory** 运行，待双轨工具链缓存与运行时间稳定后，再按 F5 升级门槛评估是否升为 path-filtered required。

## 4. 已确认决策与下一步实施切片

本设计已与维护者确认以下 3 项方向，后续直接进入独立 F6 实施 Spec（按 `main` 最新库存分配编号）：

1. **首片范围**：已确认 **候选 A（`angle_group_closers` 8 状态 DP ↔ 前向 `OrdinaryGroups` 等价性）**。
2. **工具路线**：已确认 **Lean 4（零 Mathlib 无限长定理）+ Kani（有界生产 Rust 符号检验）双轨并行**。
3. **CI 策略**：不采用源码文本哈希门禁；采用 **路径触发的独立轻量 CI workflow** 直接运行双轨验证（先 advisory，对无关 PR 零开销）。

## 5. 实施收口账本（14 片全量归档）

截至 `main@a899841c`（PR #477 [F6 Lean 4 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703151) 与 [F6 Kani 作业](https://github.com/Halckon/Koven/actions/runs/45797758634/job/139832703749)，以及 PR #483、PR #486、PR #490 与 PR #491 连续扩展至 `main@f2ea2feb`），全部 **14 个高价值纯解析内核切片（共 95 条 Lean 4 机器检查模型定理 + 35 条 Kani 直接 Rust harness）** 已完成实现、接入 [.github/workflows/parser-formal-f6.yml](../../../.github/workflows/parser-formal-f6.yml) 并验收归档。其中 13 片共 87 条定理为 `proved (model, unbounded)`，Slice 6（`SPEC-0508` Stops 边界格）的 8 条定理为 `proved (model, complete finite lattice)`；Kani 轨道保持 `validated (bounded Rust)`，二者均不升级为全输入生产 G4：

| 切片 | 归档 Spec | 证明对象 | Lean 4 定理数（证据等级） | Kani Harness 数 |
|---|---|---|---|---|
| Slice 1 | [SPEC-0490](../../archive/specs/0490-f6-angle-group-machine-proof.md) | `angle_group_closers` 8 状态反向 DP ↔ `OrdinaryGroups` | 4 (`unbounded`) | 6 |
| Slice 2 | [SPEC-0492](../../archive/specs/0492-f6-ordinary-groups-machine-proof.md) | `OrdinaryGroups` 栈计数与 `confirmed_closer` 单调性 | 14 (`unbounded`) | 4 |
| Slice 3 | [SPEC-0498](../../archive/specs/0498-f6-group-owners-machine-proof.md) | `GroupOwners` 词法深度与 `terminal_owner_events` 栈平衡 | 5 (`unbounded`) | 1 |
| Slice 4 | [SPEC-0499](../../archive/specs/0499-f6-adjacent-poison-machine-proof.md) | `ElsePoisonTail` 缓存可靠与同行花括号保留 | 4 (`unbounded`) | 1 |
| Slice 5 | [SPEC-0500](../../archive/specs/0500-f6-pratt-precedence-machine-proof.md) | Pratt 运算符优先级、结合律与非结合链检出 | 8 (`unbounded`) | 2 |
| Slice 6 | [SPEC-0508](../../archive/specs/0508-f6-stops-lattice-machine-proof.md) | `Stops` / `TypeStops` / `DeclarationStops` 边界传播格 | 8 (`complete finite lattice`) | 2 |
| Slice 7 | [SPEC-0509](../../archive/specs/0509-f6-lambda-trial-machine-proof.md) | `LambdaHeaderIndex` 单遍流式 DFA 等价性与线性步数 | 5 (`unbounded`) | 2 |
| Slice 8 | [SPEC-0505](../../archive/specs/0505-f6-lexeme-stream-machine-proof.md) | `validate_lexemes` 词法流连续划分与严格单调性 | 5 (`unbounded`) | 2 |
| Slice 9 | [SPEC-0506](../../archive/specs/0506-f6-diagnostic-anchor-machine-proof.md) | `diagnostic_anchor_prefix` 前缀和单调与区间查询 | 5 (`unbounded`) | 2 |
| Slice 10 | [SPEC-0507](../../archive/specs/0507-f6-strict-call-trial-machine-proof.md) | `StrictCallTrialIndex` 索引、反向 DP 与递归预算 | 6 (`unbounded`) | 2 |
| Slice 11 | [SPEC-0510](../../archive/specs/0510-f6-lexical-owner-events-machine-proof.md) | `LexicalRecoveryIndex` 词法 owner 栈与 `verified_events` 构造 | 11 (`unbounded`) | 2 |
| Slice 12 | [SPEC-0512](../../archive/specs/0512-f6-when-boundary-machine-proof.md) | `WhenEntryBody` / `SelectorScanState` 换行新选择器与 `if`/`else` 平衡 | 7 (`unbounded`) | 3 |
| Slice 13 | [SPEC-0513](../../archive/specs/0513-f6-supertypes-boundary-machine-proof.md) | `supertypes` 委托边界、`deinit` 后缀决策与 Lambda 尾扫描 | 6 (`unbounded`) | 3 |
| Slice 14 | [SPEC-0514](../../archive/specs/0514-f6-block-boundary-machine-proof.md) | `block` 块语句分隔符、换行边界、显式 `block {` 与缺失块根扫描 | 7 (`unbounded`) | 3 |
