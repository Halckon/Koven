# SPEC-0035：聚合、class 分配与显式 drop/free 后端基元

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P4-035` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 Phase 4](../guide/06-roadmap.md#phase-4llvm-代码生成)、§5 值/引用表示、§25 条件 `Copyable` 与 §26 ASAP drop facts |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0034、SPEC-0029 `done`；SPEC-0033 typed SSA/verifier 前置链已完成 |
| 前置 ADR | [ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted`；其首个 target 前置 ADR-0007 已 `accepted` |
| 阻塞项 | 无；源码 nominal/enum/Box constructor typed facts 尚未发布，按下述边界迁移到候选 0183/0184，不阻塞本 Spec 的后端基元 |
| 影响范围 | `lang-codegen` aggregate/heap-owner SSA type 与 operation、verifier、LLVM type/layout/runtime adapter、测试；Architecture |
| 语言语义变更 | 否；实现现行 guide 与 ADR-0008 已封闭的后端表示，不新增构造器选择或推导规则 |

## 2. Goal

完成后，`lang-codegen` 的 target-independent typed SSA 能显式表示有限内联聚合、普通 class/
Box 独占 heap owner、整体构造/复制/消费式拆分、字段 place 与 drop；verifier 能证明类型、
Copyable/MoveOnly 和唯一消费不变量，LLVM adapter 能按目标 `DataLayout` 生成合法聚合类型、
系统 allocation、字段访问、递归 drop glue 与唯一 `free`，且不依赖源码名称猜测构造器。

## 3. 范围与需求

### 3.1 SSA 类型与布局无关契约

- 为源码有序字段的内联 aggregate 与间接 heap owner 建立 IR-local 类型。aggregate 的
  `Copyable` 必须与全部字段能力一致；heap owner 始终 MoveOnly，其字段只描述 pointee payload，
  不把 payload 复制成 owner 本身。
- 支持先声明 heap-owner，再把它定义为指向一个显式 aggregate payload 类型，使
  `class Node(var next: Node?)` 这类由 pointer 打断的递归可表示，并让 payload place 具有独立
  语义类型；内联 aggregate 仍必须有限，未定义类型、跨 module type ID、重复定义、非法
  inline cycle 和同 module 重名均 fail loud。
- SSA debug text 必须按 type/source/field 顺序稳定显示 aggregate 与 heap-owner 定义；类型
  identity 不依赖 hash、LLVM handle 或机器地址。

### 3.2 SSA operation 与 verifier

- 增加整体 aggregate construct、Copyable field projection、完整 consume/explode、heap allocate、
  heap payload place 与字段 place operation；operand/result 类型、字段下标与 owner/place 类别
  由 operation verifier 精确锁定。
- aggregate construct 对 MoveOnly 字段发生消费；Copyable projection 不消费源 aggregate；
  consume/explode 必须一次产生全部字段并消费整个 MoveOnly aggregate，不形成部分移动状态。
- heap allocate 消费一个完整 payload 并产生唯一 MoveOnly owner；heap payload/field place 只
  建立访问路径，不复制或移动 owner。既有 Borrow/Read/Mutate/loan verifier 继续作用于投影
  place。
- `Drop` 只接受仍 Available 的 MoveOnly aggregate/heap owner；Copyable、已 consume、已 drop、
  存在冲突 loan 或非 owning place 均被 verifier 拒绝。CFG edge/block parameter 仍保证每条正常
  路径恰好消费或 drop 一次。

### 3.3 LLVM layout、allocation 与 drop/free

- LLVM adapter 以同一 target machine 的 `DataLayout` 构造声明顺序 struct，计算 storage size、
  ABI alignment、field offset 与 stride；不能在 SSA/frontend 硬编码 AArch64 数值。
- aggregate 使用 first-class LLVM struct value；construct/project/explode 使用等价的
  insert/extract value。大 aggregate 的最终寄存器/间接 ABI 交给 target backend，不生成 Box、
  `malloc` 或隐藏 owner。
- heap owner 使用 opaque pointer 指向已定义 payload struct。allocation 只经集中 runtime-ABI
  adapter 声明 `malloc`/`abort`，请求 `max(storage_size, 1)`，null 进入 noreturn abort 路径；
  成功后完整写入 payload 才产生 owner。
- type-directed drop glue 对 MoveOnly aggregate 字段和 heap payload 字段确定性递归；heap owner
  payload 完成后恰好调用一次 `free`。Copyable aggregate 不产生 drop glue，move/explode 后的
  源 storage 不重复 drop，abort 路径不生成 unwind cleanup。
- adapter 返回前运行 LLVM verifier；相同 SSA 重复生成完全相同的 LLVM IR 文本，不包含机器
  绝对路径或随机次序。

### 3.4 分阶段交接

- 本 Spec 允许用手工构造且通过 verifier 的 SSA 做直接后端验收，因为 frontend 当前没有
  nominal/enum/Box constructor descriptor。测试必须明确证明输入经过自建 verifier，不能绕过
  SSA 直接拼 LLVM。
- 候选 SPEC-0183 负责发布构造器 target、实例类型、Value 参数映射与字段/case 顺序 typed
  facts；候选 SPEC-0184 在其完成后负责源码/frontend facts→本 Spec SSA operation 的 lowering
  和端到端回归。两者未完成不得被表述为“源码 aggregate codegen 已完成”。

## 4. 非目标

- 不选择普通/泛型 class/value-class constructor、enum case constructor 或 intrinsic
  `Box<T>(value)`，不定义其显式/推导类型实参规则，也不修改 frontend 诊断；这些属于候选
  0183。
- 不 lower instance member method、接口委托 receiver 或隐式 receiver ownership；它们继续
  等待 0180/0181。字段 place 基元不据此猜测 method receiver mode。
- 不实现 enum tagged payload storage、nullable niche、String、container buffer、closure
  environment、object/link/run、DWARF、public FFI 或优化；enum 与 nullable 的完整 runtime
  lowering在对应 typed-fact/后端 Spec 中承接。
- 不生成 object、不调用 linker，也不把 LLVM 文本测试声称为可运行程序；object/link/run 属于
  SPEC-0039。
- 不新增 runtime crate、Rust allocator shim、异常展开、自定义 allocator、引用计数、RTTI、
  vtable 或用户 destructor。

## 5. 验收标准

- [x] aggregate/heap-owner type declaration/definition 正反矩阵锁定字段顺序、能力、递归 handle、
      inline cycle、跨 module ID、重复定义与确定 debug text。
- [x] construct/copy projection/consume-explode/heap allocate/payload+field place/drop operation 的
      operand/result/field-index 正反矩阵通过，非法部分移动和 Copyable drop 被 verifier 拒绝。
- [x] CFG ownership matrix证明 MoveOnly aggregate 与 heap owner 在每条正常路径恰好 consume 或
      drop 一次，loan 与 field/payload drop 冲突仍由同一 verifier 拒绝。
- [x] AArch64 LLVM IR 锁定声明顺序 struct、first-class aggregate call/return、insert/extract、
      target-derived size/alignment，以及大型 aggregate 没有 `malloc`/implicit Box。
- [x] class/Box 形状的 heap owner IR 锁定 `malloc(max(size,1))`、null→abort、payload 初始化、
      递归 field drop 和唯一 `free`；Copyable payload copy 不调用 glue/free。
- [x] 人工损坏 SSA 在 LLVM construction 前被拒绝；合法 module 通过 LLVM verifier且重复文本相同。
- [x] `lang-codegen` 窄测与 workspace 标准基线通过；全部生产 Rust 文件遵守 1000 行软上限，
      Architecture、Spec 索引与 roadmap 只记录实际完成事实。

## 6. 技术方案与边界

- `ssa::model` 只保存 target-independent named type identity、字段 type ID、ownership 与 operation；
  target size/alignment/pointer width 不进入 SSA。
- type declaration/definition、operation contract、linear ownership、debug render 和 LLVM adapter
  分属现有模块职责；文件接近软上限时按这五个变化原因拆分，不建立无领域含义的 utils。
- LLVM adapter 将原来的 scalar-only value map 扩展为受类型约束的 `BasicValueEnum` 映射；整数
  helper 继续只接收 integer，不能通过无检查 downcast 迁就 aggregate。
- runtime symbol 声明、allocation failure block 与 recursive drop glue集中在独立
  runtime/layout adapter；普通 operation lowering 不直接拼系统符号。
- 当前没有用户 destructor，聚合内部使用 ADR-0008 的确定性逆字段遍历只是后端约定；本 Spec
  不新增语言级可观察顺序。

## 7. 实施计划

1. [x] 建立 named aggregate/heap-owner SSA type declaration/definition、render 与 verifier →
   验证：类型图、能力、递归/循环和确定性矩阵。
2. [x] 增加 aggregate/heap/place/allocation operation 与 linear ownership contract →
   验证：operation + CFG ownership 正反矩阵。
3. [x] 扩展 LLVM type/value/layout adapter并生成 first-class aggregate →
   验证：target DataLayout、call/return、insert/extract 与 verifier matrix。
4. [x] 接入系统 malloc/abort/free 与递归 drop glue →
   验证：class/Box/ZST payload、OOM branch、唯一 free 与无 unwind 文本矩阵。
5. [x] 运行 workspace 基线、同步事实并审查 staged diff →
   验证：实际退出状态、文件规模、Architecture/roadmap/Spec 一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | aggregate/heap-owner SSA type model、render 与 verifier | `feat(codegen): model aggregate ownership types (SPEC-0035)` |
| 2 | aggregate/place/allocation operations 与 ownership verifier | `feat(codegen): verify aggregate ownership operations (SPEC-0035)` |
| 3 | LLVM aggregate type/value/DataLayout lowering | `feat(codegen): lower aggregate values to LLVM (SPEC-0035)` |
| 4 | system allocation、recursive drop/free、Architecture 与 done 验收 | `feat(codegen): generate heap owner drop glue (SPEC-0035)` |

## 9. 未决问题

- 无；源码构造器 typed facts 是已登记的后续依赖，不属于本 Spec 的实现阻塞。若实施发现
  LLVM identified struct 无法在现有 Inkwell 版本表达本 Spec 所需递归 handle，必须先以最小
  smoke 证明并回到 ADR-0008，不得用隐式 Box 或 byte blob 绕过。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0034/0029 `done`、ADR-0008 `accepted`；发现 frontend constructor descriptor 缺口并迁移至候选 0183/0184 |
| `cargo fmt --all -- --check` | 通过 | aggregate/heap-owner type slice 格式无漂移 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets` | 通过 | 47 项；新增 5 项 named aggregate、递归 heap handle、跨 module、重复/未定义/inline cycle、能力与确定 debug text 矩阵 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo clippy -p lang-codegen --all-targets -- -D warnings` | 通过 | 无 warning；生产 `model.rs` 940 行、`verify.rs` 872 行，仍低于 1000 行软上限，后续 operation/layout 职责不继续堆入这两个文件 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets`（operation slice） | 通过 | 52 项；新增 5 项 operation contract、线性消费、nested loan、显式 CFG edge transfer 与确定 debug text 矩阵 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo clippy -p lang-codegen --all-targets -- -D warnings`（operation slice） | 通过 | 无 warning；named type builder/verifier 已拆至独立职责，生产文件均低于 1000 行软上限 |
| 2026-08-25 workspace 标准基线（operation slice） | 通过 | fmt、workspace check、workspace clippy `-D warnings`、workspace all-target test 与 `lang-cli` build 全部退出 0 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets`（LLVM aggregate slice） | 通过 | 56 项；新增 target DataLayout、first-class aggregate call/return/PHI/insert/extract、大 aggregate 无隐式 allocation、pending drop fail-loud 与 MoveOnly call consumption 矩阵 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo clippy -p lang-codegen --all-targets -- -D warnings`（LLVM aggregate slice） | 通过 | 无 warning；LLVM type/layout map 与 value lowering 维持独立职责 |
| 2026-08-25 workspace 标准基线（LLVM aggregate slice） | 通过 | fmt、workspace check、workspace clippy `-D warnings`、workspace all-target test 与 `lang-cli` build 全部退出 0 |
| `cargo check --workspace --all-targets` | 通过 | workspace 全 target 检查通过 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 通过 | workspace 无 warning |
| `cargo test --workspace --all-targets` | 通过 | 全部被调用的 workspace test target 退出状态为 0 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo build -p lang-cli` | 通过 | CLI dev 构建完成 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets`（runtime/drop slice） | 通过 | 59 项；新增系统 allocation/ZST/OOM/payload place、逆字段递归与自引用 drop glue、唯一 free、allocation split 后 PHI predecessor 矩阵 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo clippy -p lang-codegen --all-targets -- -D warnings`（runtime/drop slice） | 通过 | 无 warning；生产 `adapter.rs` 989 行、`runtime.rs` 301 行，其余本次触及生产文件更小，均低于 1000 行软上限 |
| 2026-08-25 workspace 标准基线（SPEC-0035 final） | 通过 | fmt、workspace check、workspace clippy `-D warnings`、workspace all-target test 与 `lang-cli` build 全部退出 0；最终 staged diff/check 通过 |
