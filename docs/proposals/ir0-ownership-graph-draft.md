# IR0：所有权图规格前置草案、归因统计模板与多阶段路线图脚本设计

> **性质**：非规范候选设计（阶段一 IR0 前置草案） · **状态**：未启用（不修改 Guide v0.47，不取代 [ADR-0032](../adr/accepted/0032-ownership-refinement-layer.md)，不批准 Spec，不授权实现） · **读取时机**：在细化层迁移期间积累 IR0 归因数据、准备阶段一 IR0 正式规格与新 ADR，或扩展 `scripts/gen_roadmap.py` 支持多阶段路线图文件时 · **唯一真源**：本页维护 IR0 的归因统计模板、所有权图最小语句集/place 文法/不变量草案，以及多阶段路线图生成脚本设计；架构决断见[演进总纲之八](evolution-08-compiler-ir-escape-simd-selfhost.md) §2（J27–J30），实施排期见[之十三](evolution-13-implementation-roadmap.md) §4.2、§6、§7 第 5 项

本文不修改或启用任何 Guide 规则，不取代现行 ADR-0032（按[之八](evolution-08-compiler-ir-escape-simd-selfhost.md) J30，所有权图工程实现须在细化层迁移与 SPEC-0392 收口、并经新 ADR 批准之后才可启动）。

## 1. IR0 归因统计模板与降级门槛

按[演进总纲之八](evolution-08-compiler-ir-escape-simd-selfhost.md) §2.5，IR0 的第一步是**用真实修复历史检验“引入 frontend 所有权图”的必要性**，而非默认一定要建图。细化层迁移期间可按以下固定格式持续登记所有权检查与降低相关的 `fix(...)` 提交：

### 1.1 归因分类（主因互斥）

| 类别代码 | 归因类别 | 判定标准 | 引入所有权图能否消除 |
|---|---|---|---|
| **C1** | 同一规则两处实现不一致 | frontend 所有权检查与 codegen 降低/校验对同一控制流或所有权语义各自推导并出现分歧 | **是**（语义只定义在图上一次，J28） |
| **C2** | 事实协议缺失、过宽或多余 | `MissingFact`、`InvalidModel`、或为跨层传递新构造事实而反复扩张协议字段 | **是**（细化层直接消费已检查所有权图，事实协议退役，J28） |
| **C3** | 新语法/控制流构造未覆盖 | 同一语义需在 AST 所有权遍历与 lowering 两处分别补分派分支（如 `UnsupportedNode`） | **是**（只需补一处“语法 → 图”去糖，J27） |
| **C4** | 局部算法或边界缺陷 | 单层内部逻辑错误（如错误码、Span、LLVM ABI 细节），与树/图载体及跨层协议无关 | **否**（独立缺陷） |

### 1.2 统计登记表模板与止损门槛

```text
样本区间：固定起止 SHA [base_sha..head_sha]，仅计入触及 lang-frontend/ownership_checking 或 lang-codegen/ssa 的 fix 提交
字段：提交 SHA | 主因(C1/C2/C3/C4) | 涉及语法家族 | 症状（MissingFact/InvalidModel/诊断偏差/codegen错） | 备注
```

**降级止损门槛（沿用之八 §2.5）**：统计窗口收口时，若 `C1 + C2 + C3` 合计占比低（无法证明跨层协议与按树分派是主要返工源），则 IR1–IR4 **降级为“仅参考检查器使用”**，不启动生产所有权检查器重写与新 ADR。

### 1.3 首轮真实主干采样实证（`92383a60..e428bc5d`，2026-11-28）

对 `origin/main` 最近 800 个提交（`92383a60..e428bc5d`，覆盖 PR #441 → PR #501 的 `SPEC-0341` / `E3` / `SPEC-0392` 细化与迁移窗口）筛选触及 `lang-frontend/src/ownership_checking` 或 `lang-codegen/src/ssa` 的真实 `fix(...)` 提交，共得 **56 条**样本（全部固化在 `scripts/tests/test_ir0_attribution_sampler.py` 的 `REAL_WINDOW_56_COMMITS`）：

| 统计口径 | 样本数 | C1（双处规则分歧） | C2（事实协议/门禁失配） | C3（AST/双路径重复分派） | C4（单层局部缺陷） | 图可消除占比（C1+C2+C3） | 止损判定（门槛 50%） |
|---|---:|---:|---:|---:|---:|---:|---|
| **逐条人工复核（按 `git show` 改动文件与主因）** | **56** | **7 (12.5%)** | **7 (12.5%)** | **22 (39.3%)** | **20 (35.7%)** | **64.3% (36/56)** | **`PROCEED_IR1_ADR`** |
| 路径感知升级版 `ir0_attribution_sampler.py` | 56 | 6 (10.7%) | 8 (14.3%) | 23 (41.1%) | 19 (33.9%) | **66.1% (37/56)** | **`PROCEED_IR1_ADR`** |
| 初版纯标题关键词启发式（已淘汰） | 56 | 14 (25.0%) | 0 (0.0%) | 0 (0.0%) | 42 (75.0%) | 25.0% (14/56) | 漏报（未识别路径特征） |

主要返工簇：
- **C3 最大返工源（39.3%，22/56）**：AST 括号组 `grouped` 穿透 5 条（`a951dc6c`、`5c140cbb`、`6964506c`、`c5ae9142`、`93d070d4`）；`refine/subset` 准入与 `unit_lower` 双路径分流 11 条（`2f162ddd`、`46b348c3`、`35c2f53f`、`6d203139`、`3f1e8974`、`6573c9f3`、`62ee41e5`、`96de2e58`、`7dc47f42`、`61bbb425`、`c26047e6`）；Elvis / nullable 控制流 / AST 递归栈帧 6 条（`182a0a00`、`90ebf74f`、`320b84df`、`59c269e1`、`823df340`、`51845889`）。
- **C1 + C2 贡献最大单次跨层修改体量（25.0%，14/56）**：C1 含 `1fa838ee`（251 文件）、`4cbaf35f`（145 文件）、`89e473a9`（68 文件）、`978a24f4`、`0078eb0c`、`dd3d2432`、`4b689170`；C2 含 `3bcd444a`（152 文件，`lowering_facts.rs` + `refine/` + `verify_ownership/`）、`92cb7fc4`（24 文件）、`71fe9558`、`36f088c7`、`d16d7613`、`b5262a0f`、`f00860fe`。

**边界声明**：本轮实证数据已跨过 IR0 §1.2 止损门槛（证明“引入所有权图”具备明确返工收益），但按[演进总纲之八](evolution-08-compiler-ir-escape-simd-selfhost.md) J30，**生产 IR1 工程实现与取代 ADR-0032 的新 ADR 仍须等待 v1 的 `E2/E3/E5` 收口并经正式评审批准后方可启动**。

## 2. 所有权图（Ownership Graph）最小规格草案（J27）

本节定义位于 `lang-frontend` 逻辑边界的唯一中间表示。**硬约束（回应 ADR-0032）**：所有权图**严禁包含 SSA 值类型、寄存器/调用 ABI、指针 niche 或字节布局信息**；它只表达类型检查之后的控制流、place、所有权移动/借用/析构与源码来源映射。

### 2.1 函数图结构与多态策略

- **基本单元**：每个具名函数、方法、lambda thunk 与 `deinit` 体对应一张 `OwnershipGraph`。
- **泛型多态（回应之八 §2.5 风险项）**：所有权图在泛型层面**保持多态**（一份图服务该泛型的全部实例化），节点上的能力谓词（`Copyable`、`Escapable`、`DropPolicy`、`Transferable`、`Shareable`）以**符号约束/占位谓词**保留，在具体单态化或[J46 接口产物](compilation-unit-dependency-rules.md)跨单元实例化时代入求值。这样既避免编译时间膨胀，也为阶段二 IR4 之后将 J46 泛型导出体从源码切至所有权图奠定基础。
- **输入资格与产物身份（对齐 J28 / J29）**：只有通过 Phase 2 完整类型检查、无 `Error`/`Deferred` 残留的函数体才允许构建 `OwnershipGraph`；带错误恢复的函数体显式标记为 `GraphStatus::Poisoned`，禁止进入 IR3 权威检查或 IR4 细化层消费。

### 2.2 Place 文法

```ebnf
place      = local_root, { projection } ;
local_root = LocalId ;                     (* 参数、this、局部 val/var、显式物化临时槽 *)
projection = ".field(", FieldId, ")"       (* class / value class 字段投影 *)
           | ".variant(", EnumCaseId, ", ", FieldId, ")"  (* smart-cast 后枚举变体载荷 *)
           | ".index(", Operand, ")"       (* 容器元素 place，仅用于借用/读写，不隐式部分移出 *)
           | ".deref" ;                    (* 借用引用 / Box 解引用投影 *)
```

- 每个 `LocalId` 记录：声明类型（Phase 2 `TypeId`）、可变性（`Val` / `Var`）、模式（`Own` / `Borrow` / `Inout`）、词法作用域 `ScopeId`、以及是否属于用户源码绑定或去糖临时槽。
- **不引入显式生命周期参数**：`LoanId` 与 `Origin` 直接由图上数据流（[借用内核演算](loan-kernel-calculus.md) §2）推导，不进入类型语法。

### 2.3 语句集与控制流终结符

**基本块内语句（`GraphStmt`）**：

| 语句 | 语义 | 消费者 |
|---|---|---|
| `StorageLive(local)` / `StorageDead(local)` | 标记局部槽位的词法存活起止边界 | L0/L1 活性分析、IR2 析构展开 |
| `Init(place, rvalue)` / `Assign(place, rvalue)` | 首次初始化或覆盖赋值（覆盖前须先处理旧值析构） | 借用检查、IR2、IR4 |
| `Move(place)` / `Copy(place)` | 显式区分所有权转移与 `Copyable` 复制（由 C0 能力谓词决定） | 借用检查、IR4 |
| `LoanBegin(loan, kind, place, parent?)` | 建立 `Shared` / `Exclusive`（含 `Reserved` 两阶段预留状态）或子 loan | L0/L1 借用检查、IR4 |
| `LoanActivate(loan)` | **两阶段借用激活点**：置于所有实参求值完成、进入被调方之前，把 `Reserved` 接收者借用升级为 `Active` 并在该程序点重新校验仍存活的重叠借用（区分实参期已结束的临时读与持续到调用边界的共享借用，对齐 Guide 10） | L0/L1 借用检查、IR4 |
| `LoanEnd(loan)` | 结束指定 loan（IR1 影子版按词法/现行规则放置，L1 改为活性分析产出） | L0/L1、IR4 |
| `Drop(place, policy, condition)` | IR2 析构展开遍插入的显式析构动作：**策略与执行条件正交**——`policy ∈ { Immediate, LexicalRequired }`，`condition ∈ { Unconditional, Conditional(DropFlagId) }`（支持带 `deinit` 的值仅在单分支被移动时，汇合处同时满足 `LexicalRequired` 与 `Conditional(flag)`） | IR2/C1、IR4 |
| `SuspendPoint(id, live_set)` | **阶段五 KB8 预留槽位**：标识 `yield` / `suspend` 挂起边界，阶段一–四不产生 | IR5（S3/S4） |

**基本块终结符（`GraphTerminator`）**：

| 终结符 | 语义 | 说明 |
|---|---|---|
| `Goto(target)` | 无条件跳转 | 普通控制流 |
| `Branch(cond, then_bb, else_bb)` / `Switch(discriminant, arms, default?)` | 条件与枚举判别跳转 | 覆盖 `if`、`when`、`while`、`&&`/`||` |
| `Call(callee, args, dest, normal_bb, failure_bb?)` | 函数/方法/闭包调用；**预留 `failure_bb` 失败出口边** | **落实 [FD0](fd0-supervision-boundary-litmus.md) 预留 #3**：阶段一 `failure_bb` 置空（v1 abort 直接终止进程），阶段四 FD1 启用显式监督边界时激活为失败展开路径 |
| `CheckedOp(op, operands, dest, normal_bb, failure_bb?)` / `Fail(reason, failure_bb?)` | **可路由的本地程序失败**（越界检查、整数溢出、`!!`、`error()`） | 与 `FatalAbort` 严格分离：阶段一 `failure_bb = None`（进程直接 abort）；FD1 启用监督边界后将本地检查接入 `failure_bb = Some(cleanup_bb)`，沿当前函数展开至最近监督边界 |
| `Return` | 函数正常返回（含 `?` 提前返回，均走普通 `Return` 而非 `failure_bb`） | 与 Guide 09 一致 |
| `FatalAbort(reason)` | **不可路由的致命进程终止**（`deinit` 失败、失败跨越 FFI 帧、runtime 内部错误），永无后继块 | 任何版本均直接终止进程且不执行待运行析构（对齐 U0 FS6/FS7/FFI5） |

### 2.4 源码映射与诊断保真（回应之八 §2.5 风险项）

每条 `GraphStmt` 与 `GraphTerminator` 必须携带不可变元信息：

- `span: Span`：精确指向原 AST 表达式/声明的 UTF-8 字节半开区间；
- `desugar_origin: DesugarOrigin`（封闭枚举：`Direct`、`PostfixQuestion`、`ForIteration`、`TwoPhaseReceiver`、`ShortCircuit`、`CompoundAssign`、`ImplicitDrop`、`ClosureCapture`）。

诊断渲染器按 `desugar_origin` 选择面向用户源码语法的模板（例如 `PostfixQuestion` 报“`?` 传播路径上……”而非暴露内部临时槽名），避免图检查导致诊断质量回退。

### 2.5 分层校验与影子差分门槛（J28 / J29）

构图结构校验与 Phase 3 所有权诊断必须严格分层，避免把本应由 Phase 3 诊断的用户源码错误误判为内部图损坏：

1. **构图结构校验（IR1 构图后立即执行，只检图本身完好性）**：
   - CFG 可达块均以合法终结符结尾，无悬空 `BlockId`、`LocalId`、`LoanId` 或跨作用域非法引用；
   - 每条语句与终结符均携带合法 `Span` 与 `DesugarOrigin`；
   - `LoanBegin` / `LoanActivate` / `LoanEnd` 的 `LoanId` 引用闭合且 `parent` 引用合法；
   - **不在此层拒绝**“移动后再次使用”“未初始化读取”“借用冲突”等用户源码所有权错误——这些合法构图后的程序必须交给 Phase 3 图上检查器产出稳定错误码与主 `Span`。
2. **Phase 3 图上所有权分析与 IR2 展开后不变量**：
   - 图上检查器负责对 `Move`/`Copy`/`Init`/`LoanBegin`/`LoanActivate`/`LoanEnd` 执行流分析，在冲突点或二次使用点报告与生产检查器一致的诊断码与主 `Span`；
   - 对**已通过 Phase 3 检查**并完成 IR2 析构展开的合法图，额外校验：每条正常返回与可路由失败出口路径上，每个 `MoveOnly` 资源恰好被消费或析构一次（零漏析构、零双析构）。
3. **IR1–IR3 影子切换门槛**：
   - 在已声明支持的语法切片内，图参考检查器与生产检查器在全部测试语料（含正例与应拒绝反例）上比较接受/拒绝、诊断码与主 `Span`；
   - 任何差异必须归因到“生产实现缺陷 / 构图器缺陷 / 规格模糊 / 已批准语义放宽”四类之一；存在未解释差异时禁止切换权威（IR3）；
   - 必须包含手工故障注入测试（故意在已检查图中构造悬垂 loan、双析构或错调用目标），确认展开后不变量与跨阶段差分均能稳定拦截。

**离线原型验证（2026-11-28）**：
- **归因统计与止损门槛（P0-2）**：`scripts/loan_kernel/ir0_attribution_sampler.py` 与 `scripts/tests/test_ir0_attribution_sampler.py`（4 组单测），结合路径特征（`lowering_facts.rs`、`refine/subset/`、`provider/borrow_result.rs` 等）与标题关键词互斥归入 C1–C4，过滤仅测试侧修改误判，并在真实主干 56 条样本（`92383a60..e428bc5d`）上复核通过。
- **IR0 两层校验、故障注入与影子差分门槛**：`scripts/loan_kernel/ir0_graph.py` 与 `scripts/tests/test_ir0_graph.py`（6 组单测），覆盖 `Poisoned` / 悬空引用 / 非法 `Span` / `FatalAbort`、用户二次移动分层、`LexicalRequired` + `Conditional(DropFlagId)` 正交组合、`Call.failure_bb` vs `FatalAbort`、`LoanActivate` 两阶段激活冲突与 `evaluate_shadow_diff`。
- **IR1 构图 + IR2 展开 + 真实全语料影子差分（P0-1）**：`scripts/loan_kernel/ir1_ir2_lowering.py` 与 `scripts/tests/test_ir1_ir2_lowering.py`（3 组单测），将 `checker.py` 全部 `cases/*.lk` 整值切片函数（150+ 函数，`early_drop=False/True`）构图为标准 `OwnershipGraph`，与 `DP.Verifier` 跑真实 `evaluate_shadow_diff` 达零未归因差异（`ir3_switch_allowed=True`），并对真实构图产物执行清空 `failure_bb` 与去析构标志的故障注入回归。

## 3. 多阶段路线图生成脚本扩展设计（之十三 §6）

按[演进总纲之十三](evolution-13-implementation-roadmap.md) §6（2026-11-11 用户决定：v1 之后每个阶段各用一份路线图文件，在该阶段入口条件满足时建立），现行 `scripts/gen_roadmap.py` 只读取单一 `docs/roadmap/v1.json`。未来立 Spec 扩展该脚本时固定以下约束：

1. **文件组织**：
   - 保持 `docs/roadmap/v1.json` 及其生成的 `by-stage.md` / `by-area.md` / `by-phase.md` 原样不变；
   - 后续阶段启用时新增 `docs/roadmap/stage1.json`、`stage2.json`……各自生成 `stage1-by-stage.md` 等视图，并由 `scripts/check_docs.py` 统一校验“无过期生成视图”。
2. **条目编号防冲突（之十三 §6 硬约束）**：
   - v1 已占用裸编号 `L1–L21`、`S1–S7`、`X02–X12`、`V1–V6`；
   - 阶段一及之后的路线图 JSON 中，所有步骤条目 ID **强制带路线前缀**（例如 `RL-L0`、`RL-L3`、`RC-C0`、`RS-S2a`、`RU-U1`、`RX-X0a`、`RIR-IR1`、`RF-F3`），由 `gen_roadmap.py` 的 schema 校验直接拒绝与 v1 撞名的裸编号。
3. **跨阶段入口显式引用**：每份后续阶段 JSON 的 `meta.entry_gates` 字段显式声明前置阶段 ID 与必过门槛（如 `stage1.json` 声明 `v1.0` + `P1/E2/E3/E5`），防止在入口未满足前提前激活条目。

**多阶段路线图离线校验原型（2026-11-28）**：上述防撞名、路线前缀、入口门槛与依赖无环约束已在非规范原型 `scripts/loan_kernel/ir0_roadmap_schema.py` 与 `scripts/tests/test_ir0_roadmap_schema.py`（3 组单测，只读对接真实 `docs/roadmap/v1.json`）中固化可复跑。
