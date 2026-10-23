# SPEC-0350: E3 所有权细化层与 nullable 家族试点

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3（通用所有权细化层）时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 为准，长期决定以 ADR-0032/0033 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0350` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 nullable flow 与 extraction](../../guide/09-nullability-errors.md#nullable-flow-与-extraction)、[Guide 15 nullable 实施边界](../../guide/15-conformance-and-staging.md) |
| 批准依据 | 2026-10-23 用户选择：先做 E3 nullable 试点；inline nullable 采用 tag + payload 并做 niche；试点只做 7 个 probe（不含 Elvis 的 p15）；细化层与 D10 方向一致，终态统一使用新层 |
| 基线 | `feature/spec-0349-builtin-result` 合并 main `1a200aaa` |
| 分支 | `feature/spec-0350-ownership-refinement` |
| 前置 ADR | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)（细化层）、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)（inline nullable 表示），2026-10-23 accepted |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `lang-codegen` 中建立 ADR-0032 的两段式 lowering：先细化为基于槽位的显式 CFG，再统一构造 SSA。
以 nullable 家族为首个迁移对象，让下列 7 个 conformance probe 在单文件与 project 两种模式下都能
build 并按期望运行；迁移不新增按“类型 × 控制流”拆分的 lowering 模块。

| probe | 2026-10-23 现状（`feature/spec-0349` 头，`lower_scalar_unit_with_entry` 直接 lowering，仓外临时测试） | 根因 |
|---|---|---|
| `p14_nullable` | MissingFact，`if (flag) Node(5) else null` | 合流处缺 T→T? 包装（`merge_unit_exits`） |
| `p14b_nullable_simple` | OK | 现由 `unit_lower` 通过，试点后改由新层承接，必须保持 OK |
| `p14c_nullable_int` | MissingFact，`val a: Int? = 5` | 无 inline nullable 表示 |
| `q05_nullable_param` | OK | 同 p14b |
| `v03_join_nullable` | UnsupportedNode，`Int?` 返回类型 | 无 inline nullable 表示 |
| `v13_while_nullable_int` | MissingFact，`var x: Int? = 5` | 无 inline nullable 表示 |
| `x02_string_nullable_return` | UnsupportedNode，`String?` 返回类型 | 无 inline nullable 表示 |

baseline `conformance/baseline-aef3f4f8.tsv` 中 p14b、q05 记为 InvalidSsa；当前分支头已通过，说明
其间的修复已生效，表中以实测为准。

## 2. 设计

### 2.1 细化（`ssa::refine`）

- 输入：`unit_plan` 选出的函数实例、validated typed/ownership facts、现有 callable ABI 与类型 intern。
- 输出 `RefinedBody`：槽位表（named binding、temporary、receiver、loan）和基本块列表，语句与
  terminator 见 ADR-0032。
- 每条 `UnitDropFact`、`UnitLoanFact`、`UnitValueDeliveryFact` 与 call 合同都映射到唯一 CFG 位置；
  映射表按 `UnitDropPoint` 变体逐一定义，未定义的变体在预检阶段把函数划出子集。
- nullable 只消费既有事实：
  - `UnitNullComparisonDescriptor` 决定条件中的 null 比较生成 `SwitchNull`，以及哪条边带 proof；
  - `UnitNonNullUseDescriptor` 决定哪些 use 从 proof 取值；
  - `UnitNonNullAssertionDescriptor` 与 ownership plan 决定 `!!` 的 take 与 Abort 边；
  - typed join 结果（Guide 06 的 T 与 Nothing? 合成 T?）决定分支值是否先 `NullableWrap`。

  细化层不读取 AST 拼写推断非空性。

### 2.2 SSA 构造（`ssa::refine::ssa_build`）

- 按 sealed-block 算法为槽位插入 block parameter；块入口所有权状态在全部前驱一致，否则 `InvalidModel`。
- `Move`/`Drop`/`LoanBegin`/`LoanEnd` 机械映射为现有 SSA 操作；nullable 使用 ADR-0017/0033 的 operation 与
  `NullableBranch`。
- 产物经现有 verifier；未通过不进入 LLVM。

### 2.3 inline nullable（ADR-0033）

- `SsaTypeKind::InlineNullable { inner, niche }`、类型 intern、LLVM 类型映射、drop glue 与 verifier 规则；
- 首批 niche：String（`bytes == null`）与顺序容器（`buffer == null`），其余显式 tag；
- 第一步单独提交：String 空串 bytes 从 null 改为非 null sentinel（字面量、concat、clone、argv 四处），
  更新锁定 null 字面量的 String LLVM 测试，并跑 String 相关 native 测试；
- 现有 `NullableHandle` 路径不变。

### 2.4 分流（迁移期）

- `lower_owned_unit_with_entry` 内对每个函数实例做确定性子集预检（§3），通过则走细化层，否则走现有
  `unit_lower` 函数体路径。预检只读 typed facts 与 AST 构造种类，不试跑 lowering。
- 细化层接手的函数失败即报错，不回退。
- 预检规则本身有单元测试锁定：同一输入的分流结果固定，子集外构造逐类给出负例。

## 3. 试点子集（预检白名单）

函数体中所有表达式、语句与类型都属于下列集合时，函数才进入细化层：

- 类型：Unit、Boolean、Char、整数、String、普通 class（HeapOwner）、`Box`，以及它们的 nullable；
- 表达式：字面量与 `null`、名称读取（含非空 use）、class 构造、class 字段读取、顶层函数的直接
  调用（Value/Borrow 参数）、`println`、整数算术（沿用 checked 运算）、
  `==`/`!=`（整数、Boolean、Char、String、与 `null` 比较）、`&&`/`||`、`!`、`if` 表达式、`!!`；
- 语句：`val`/`var` 局部声明、局部 `var` 赋值、表达式语句、`if` 语句、`while`、`return`。

子集外（例如 when、for、闭包、receiver 调用、Rc、容器、Map、enum、value class、泛型实例、
字段赋值、`break`/`continue`、`?:`、`?.`）一律由现有路径处理。子集只为试点设定，后续家族 Spec 逐步扩大。

## 4. 现有代码的处理

- 试点不删除 `nullable_proof`、`nullable_when`、`nullable_comparison`、`non_null_assertion`、
  `while_proof`：它们仍服务子集外函数（如 Box/Rc 的 nullable when、Borrow 参数上的 proof）。
  每个模块在其最后一个消费者迁入细化层的那个 Spec 中删除。
- 新代码按阶段组织（细化、SSA 构造、子集预检、inline nullable 类型与操作），不按类型 × 控制流组合拆分；
  每个文件遵守 1000 行软上限。
- 进入 `lowering_dependency_tests` 的方向检查：`refine` 不依赖 `unit_lower` 的函数体模块，只共享
  类型 intern、callable ABI 与 lowering support。
- 子集内函数从旧路径改走新层后，原有只锁定 IR 形状的测试按新输出更新并逐项说明；锁定行为的
  测试（native 输出、退出码、drop/分配计数、诊断）不得放宽。

## 5. 非目标

- Elvis（p15）、safe call、`as?`：Guide 15 仍写为延后，留给 N03 及后续 Guide 版本；
- Borrow/Inout/字段/元素 nullable place 的 proof（需另行的 nullable-place ADR）；
- D10 范围内的 `lower_frontend` 与旧 checker 退役（SPEC-0341）；
- LSP、新诊断码、frontend 新事实。若试点发现事实缺口，停下来另开 frontend Spec，不在 codegen 推断。

## 6. 验收标准

- [x] ADR-0032、ADR-0033 accepted（2026-10-23）。
- [ ] 7 个 probe 在单文件与 project 两种模式下 build 成功，stdout 与期望一致、退出 0。
- [ ] 154 个 probe 的 conformance 矩阵与本 Spec 基线相比，没有原本 OK 的 probe 变差，两模式差异为 0。
- [ ] inline nullable：`Int?`、`Boolean?`、`Char?`、`String?` 的 wrap/null/branch/take/drop 有 SSA verifier
  正反测试，以及 LLVM 布局测试（含 niche 的 size 等式）。
- [ ] `String?` 的 drop 计数：non-null 与 null 路径各验证分配与释放次数相等。
- [ ] 子集预检的正负例测试；同一输入在正逆 inputs 顺序下分流与 SSA 输出确定。
- [ ] 受影响目标定向测试通过：`lang-codegen` 中 nullable、unit_lower、verifier、LLVM nullable 相关测试，
  `lang-cli` native 与 project 测试中 nullable 相关项；fmt 与 `check_docs.py` 通过。
- [ ] 没有新增按类型 × 控制流组合拆分的 lowering 模块；Architecture 只记录实际已实现事实。

## 7. 风险

- 细化层首片要承接子集内的基础构造（调用、算术、控制流），工作量主要在这里，nullable 本身反而较小；
- SSA 构造需与 verifier 的 loan 规则逐项对齐，可能暴露现有事实的缺口；
- Guide 15 原写“首轮 native 实施只覆盖”pointer-like nullable，同时允许 inline nullable 经独立 ADR 与
  后继 Spec 推进。2026-10-23 按用户要求做了不改含义的表述澄清（native 按表示分批推进，inline nullable
  在 ADR 接受后由后继 Spec 接入），不提升 Guide 版本；Elvis 等的延后不变。

## 8. 用户决定与待确认

1. 已确认（2026-10-23）：迁移期按函数确定性预检分流（§2.4）——函数体全在已迁移子集内走新层，
   否则走旧路径，逐家族迁移；终态删除旧函数体路径与预检。
2. 已确认（2026-10-23）：已迁移家族的旧模块在最后一个消费者迁走时删除（§4），不保留兼容层。
3. 已确认（2026-10-23）：String 的 niche 用 bytes 指针，空串改为非 null sentinel；不采用 capacity 哨兵。
   容器 buffer 现已使用非 null sentinel，直接用指针 niche。
