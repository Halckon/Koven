# SPEC-0248：列表式 Unit 容器的零大小存储

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或验收 Unit 列表式容器到 native 的首片时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-248` |
| 所属 Phase | Phase 4 |
| 语言规范 | [现行 v0.40](../../guide/README.md)、[Copyable 与布局](../../guide/11-copyability-layout-construction.md)、[容器与借用迭代](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-02 本片实施授权；独立 Spec、最小生产修复与分层验收 |
| 基线 / 分支 | main `57a54bf99807389adb5cc865a3104416627ccfb7` / `fix/spec-0248` |
| 前置 Spec | SPEC-0035、0036、0212 `done` |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md) `accepted` |
| 阻塞项 | 无新的语义或架构决定；实现与验收尚未完成 |
| 影响范围 | 单文件 frontend→SSA 列表式容器 lowering、LLVM container element storage/layout、领域测试与事实文档 |
| 语言语义变更 | 否；不提升 Guide 版本，不新建 ADR |

## 1. Goal 与边界

单文件 `arrayOf<Unit>`、`listOf<Unit>`、`mutableListOf<Unit>` 的列表式构造，在保留元素
表达式一次、源码顺序求值和 Unit callable `void` ABI 的前提下，完成真实 `for`、verified
SSA、LLVM object/link/run；零字节元素仍按逻辑索引迭代并执行 bounds guard。

本片是容器存储能力修复，独立于 [SPEC-0182](0182-sequential-for-lowering.md) 的验收补测。
0182 保持 active，原合同、历史验收及其“不改变 container layout/drop glue”边界不改写；
本片只向其提供明确有界的 Copyable Unit / temporary-source 正常迭代证据。

## 2. 现行依据与原始断点

Guide 已规定 Unit 为 Copyable、structurally storable，并要求 ZST 保留逻辑 length、bounds、
place identity 与适用的 drop 次数。ADR-0008 已规定 target DataLayout、固定 header 和非空、
对齐、module-private readonly sentinel；共享物理地址不能合并逻辑元素身份。

基线的非空列表式构造遇到 `LoweredValue::Unit` 缺少存储 operand；空构造虽没有该 operand，
LLVM 仍缺 Unit element layout/type。必须分别保留 frontend→SSA 非空正例和低层 verified
SSA 空容器正例的原始红证据，不能让一个断点遮住另一个，或把合法输入失败改为负向验收。

## 3. 范围与实现约束

- 仅在列表式容器 operand 边界，把已求值的 Unit 结果物化为
  `Operation::Constant(ScalarConstant::Unit)`；SSA 类型仍为内建 Unit。
- `self.lower(argument.value)` 只调用一次；核对 resolved element type、expression type 与
  SSA type 的内建 Unit identity。不按名称拼写猜测，不将 missing facts / Unknown / 错误类型
  伪装成 Unit；Diverged 原样传播，之后的 operand 和 ContainerConstruct 不生成。
- 保留 `LoweredValue::Unit` 作为已执行的无结果表达式；普通 Unit producer 的 SSA returns
  仍为 `[]`，LLVM 保持 `define void` / `call void` / `ret void`。容器 factory 仍返回既有 header。
- Unit storage 独立保存，仅供 container element 查询；普通 type map 查询、aggregate 字段、
  shared-control 和 Value(Unit) 参数/return ABI 不获得新的支持或隐式 fallback。
- 以 LLVM 空 literal struct `{}` 的实际 target DataLayout 得到 size/alignment，再经既有 checked
  layout plan 得到 stride。Unit size 为 0、container stride 为 0；alignment 取 target 值，不硬编码。
- 三种容器 header shape/size 保持既有合同。空与非空 Unit buffer 都用非空、只读、对齐 sentinel；
  不生成 Unit element GEP/store，不为元素 buffer allocate/free。可有 LLVM 合法的零字节
  `load {}`，不将“所有路径绝无 load”写成要求。
- 保留 signed Int 的负数及 `index >= length` 检查；成功 Read 必须受 guard 支配。
  logical index / owner / place / loan 身份不得由 sentinel pointer 相等替代。
- 不修改通用 `require_value`、`return_values`、call argument ordering、函数签名或 SSA
  first-class 判定；Unit 是 Copyable，不引入元素临时 owner/drop 义务。

## 4. 非目标与合法源能力缺口

- `own Unit` / Value(Unit) 参数、由 Value(Unit) 返回的表达式体、一般 Unit call operand、
  Borrow Unit 跨函数读取与 runtime-length initializer 的 void→element bridge。
- Unit-field value class 的 constructor materialization、projection/解构；nullable Unit、
  一般 aggregate/shared-control Unit storage、泛型 ABI、FFI 或全部 ZST 支持。
- CompilationUnit 的 `for`、Inout/field source、公开容器 API、growth、循环优化及新 runtime ABI。
- nominal/resource wrapper 构造、MoveOnly ZST 逻辑 drop/逆序析构的完整验收；Copyable Unit
  不替代这些证据，普通空 class 的 pointer owner 也不等价于 MoveOnly ZST。

上述 Guide 合法程序若仍遇 Unsupported/MissingFact，登记为后继正例能力缺口，不固化成
“应被拒绝”的语言负例。本片 bounds native 采用低层 verified SSA 的
`ContainerElementPlace → Read(Unit) → marker`，不使用尚不支持的 BorrowUnitCall 冒充覆盖。

## 5. 验收标准

### 5.1 原始红绿与三容器 × 0/1/3

- [ ] 未修基线分别运行非空 frontend→SSA 与空容器 LLVM 两项成功方向正例，保存真实错误；
  最小修复后原测试、不改 expected 转绿；frontend diagnostics 为空且执行 SSA verifier。
- [ ] Array/List/MutableList 各执行 empty/single/multi 的真实 object/link/run 九格，独立文件名为
  `unit_<array|list|mutable_list>_<empty|single|multi>.ko`；成功退出、stderr 为空。
- [ ] source factory 打印 `source`；三个 Unit producer 依源码顺序打印 first/second/third；
  named binding 在 body 中复制为 Unit、递增 Int count、打印 body；仅 count 等于固定 N 时
  打印 count-ok，末尾打印 done。每个 provider 使用下列手写 bytes，不由输入或生产 builder 推导。

| cardinality | 列表式实参 | 固定 stdout（`\n` 为 LF） |
|---|---|---|
| empty | 空实参，显式 `<Unit>` | `source\ncount-ok\ndone\n` |
| single | `first()` | `source\nfirst\nbody\ncount-ok\ndone\n` |
| multi | `first(), second(), third()` | `source\nfirst\nsecond\nthird\nbody\nbody\nbody\ncount-ok\ndone\n` |

### 5.2 独立 SSA 与 CFG oracle

- [ ] 同九格按 named FunctionId、instruction/entity 身份验证 producer call 后紧随 Unit Constant，
  operand 数固定为 0/1/3 且每项类型为 Unit；空容器仍为 concrete Unit element type。
- [ ] source DirectCall 与 ContainerLength 各一次且处于 preheader；cursor=0、length、source loan
  正确运输到 header，`cursor < length` 真边为 body 唯一入口，element place 使用当前 cursor。
- [ ] named binding 为本轮 element loan 的 Copyable Read，不 Consume/Drop 元素；正常回边
  结束 element loan、cursor+1 并保留同一 length/source loan。exhaustion 结束 source loan 后
  恰 Drop temporary owner 一次；按实际 block parameters/edge arguments 校验，非静态总次数。
- [ ] 另补 `count += 1` 和 `if` CFG 之后读取 Unit binding 的正例，验证重绑定后的 element loan
  身份；独立 discard 正例无强制 Unit Read 但仍迭代 N 次，不替代 named-binding 九格。
- [ ] 独立 Diverged 正例保留前一个 Unit producer 的一次副作用，后续 operand/construct 不出现；
  不扩大为异常展开或部分构造 cleanup。

### 5.3 LLVM、native bounds 与 ABI

- [ ] Unit layout 与实际 `{}` DataLayout 完全相符，size=0、stride=0；三种 header shape/size
  不变，sentinel 非空/private/constant 且 alignment 覆盖 Unit，无 element GEP/store 或 buffer
  allocator/free call。仅声明 malloc/free 不视为实际调用；合法 `load {}` 不视为失败。
- [ ] 低层 verified SSA Array<Unit> 经 object/link/run：索引 0/2 的成功 Read 后 marker 一次；
  -1、length、empty 的 0 在 marker 前 abort。检查 signed bounds guard 支配成功 Read；
  此项证明共享 element path，不宣称源码 BorrowUnitCall 或所有 provider 索引矩阵已覆盖。
- [ ] Unit producer/entry 的 `[]`/void ABI 与 factory header return 不变；普通 Unit type 查询、
  aggregate/shared-control、Value(Unit) 参数/return 保持原支持边界，既有 Unit root/CFG 回归通过。
- [ ] 两次 fresh frontend 分析链的一格完整 SSA/LLVM bytes 相同；不重印同一 Program，不扩张为
  跨 target、object/DWARF 或 CompilationUnit 确定性。

### 5.4 负向边界与交付

- [ ] Unit Constant 标成 Int、Unit operand 交给 Array<Int> 均由 SSA OperationContract 拒绝；
  Opaque、循环或超限 layout 继续 fail loud，不因 Unit storage fallback 获得空表示。
- [ ] 真正非法 source 保留既有诊断：Unit 带值 return 为 L0087，循环内 replacement source 为
  L0135；能力误配在 object 写出前拒绝，已有输出文件 bytes 保持不变。
- [ ] 定向、共享契约、完整 codegen 与双宿主 CI 按 §7 留证；新增测试不得 ignored/filtered 而
  计作通过。Architecture 仅在实现及实测结果确认后更新，0182 只追加部分证据。

## 6. 实施与提交计划

1. [ ] 两个独立原始红例 → 容器 operand / element storage 的最小修复 → 原正例转绿。
2. [ ] 九格与 SSA 身份、CFG 后 Unit Read、discard/Diverged、layout/bounds/ABI 及负向边界闭合。
3. [ ] 完成下述门禁，补真实验收及 Architecture；独立 PR、最终 head 双宿主 CI 与 Spec 生命周期同步。

建议单一有界提交：`fix(codegen): store Unit container elements (SPEC-0248)`；若分提交，每项
仍限本 Goal。不提高尺寸 baseline、不增长无关 legacy 入口，不改写 accepted ADR 历史。

## 7. 待测账本

以下均为计划，不是已执行证据；记录每次实际命中数、failed/ignored/filtered、退出码、宿主、
工具链、源码 head 和未运行原因。Cargo 使用同一共享 target 串行运行，不 clean；无性能结论。

| 验收项 / 计划目标 | 当前状态 |
|---|---|
| 原始非空 frontend→SSA / 空容器 LLVM 红绿 | 待执行、待记录实际错误和转绿结果 |
| 新 `unit_storage`：native 九格、SSA 九格、CFG/discard/Diverged、LLVM/bounds/ABI | 待执行；逐名核对实际测试和 native 子用例 |
| `lang-codegen --lib sequential_for` | 待执行；基线 35 项不能抵作本片新增证据 |
| `llvm::layout_tests`、`llvm::container_tests`、`ssa::container_operation_tests` | 待执行 |
| 既有 Unit root primitive / native root primitive 与能力落盘边界 | 待定位并执行 exact/领域过滤器 |
| `cargo test --locked --offline -p lang-codegen`（含 doctests） | 待执行；共享 LLVM storage 变更须完整 codegen 回归 |
| codegen all-targets check / 严格 clippy / fmt | 待执行 |
| docs、policy、尺寸、diff | 待执行；尺寸以 exact main 基线比较 |
| 最终 head 的 Ubuntu / macOS CI | 待发布及核验；逐名核对新增测试，skip/ignored 不计通过 |
| frontend/workspace 全量、性能、cold/warm 编译、RSS | 未运行；不以无关全量替代直接合同证据，未测性能不作改善声明 |

未决语义问题：无。本片关闭以实际证据为准，当前保持 `in-progress`。
