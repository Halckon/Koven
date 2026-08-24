# SPEC-0038：具体闭包环境与间接调用后端

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-038` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 高阶函数](../guide/01-design-decisions.md#4-高阶函数与一等公民支持)、[v0.27 capture](../guide/01-design-decisions.md#27-简化-closure-capture-与跨线程转移v027) 与 [Phase 4 roadmap](../guide/06-roadmap.md#phase-4llvm-代码生成) |
| 批准依据 | 当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权 |
| 前置 Spec | SPEC-0032、SPEC-0034、SPEC-0035 `done` |
| 前置 ADR | [ADR-0006](../adr/0006-typed-ssa-block-parameters.md)、[ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md)、[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md)、[ADR-0009](../adr/0009-concrete-closure-internal-abi.md) `accepted` |
| 阻塞项 | 无；不同 concrete closure layout 的 existential 合流、字段存储和 public ABI 明确排除 |
| 影响范围 | `lang-codegen` callable/closure SSA type、operation、verifier、LLVM type/adapter/drop glue 与测试；Architecture |
| 语言语义变更 | 否；实现现行 guide 与 ADR-0009 已封闭的后端表示 |

## 2. Goal

完成后，`lang-codegen` 能用 target-independent typed SSA 表示无捕获 function pointer 与具有
有序 shared/owned capture 的 concrete closure，验证 formation、loan、调用和唯一析构，并按
ADR-0009 lower 为裸 LLVM function pointer 或 `{function pointer, inline environment}`，通过
真实 indirect call 执行且不引入隐式 heap allocation、类型擦除或第二种 closure 表示。

## 3. 范围与需求

### 3.1 callable 与 closure SSA

- 建立 IR-local callable signature，保留有序参数类型与零/一个返回类型；function-pointer
  identity 只含签名，concrete-closure identity 另含源码顺序的 capture mode/type，且至少有
  一个 capture。两类 callable value 均为 MoveOnly；无 capture 必须使用 function pointer。
- 支持取得已声明函数地址、按 concrete closure type + thunk + capture operands 原子形成完整
  closure，以及读取 callable 执行 indirect invoke。function address 必须与目标签名精确一致；
  closure thunk 的 entry 为 environment 后接用户参数，返回契约与 closure signature 一致。
- owned capture operand 使用普通 SSA value：Copyable 可读，MoveOnly 在 formation 时唯一消费。
  shared capture operand 使用现有 shared loan；exclusive loan、mode/type/arity 不匹配和把无捕获
  callable 伪装成 closure 必须由 verifier 拒绝。
- invoke 只读取 callable owner，不消费它，因此同一 closure 可重复同步调用；Value 用户实参
  继续沿用现有 copy/move effect。shared capture loan 在 closure 存活期间持续阻止来源的
  move/mutation/drop，closure owner 跨 edge 时依赖必须随新 owner identity 转移。
- closure drop 结束 shared capture loan，并按 capture 逆序唯一析构 owned MoveOnly slot；
  Copyable capture 与 function pointer 不生成 runtime drop glue。损坏的 loan 生命周期、提前
  `BorrowEnd`、drop 后 invoke、双 drop 和遗漏正常出口必须稳定拒绝。

### 3.2 LLVM 表示与调用

- function pointer lower 为一个 LLVM pointer，`FunctionAddress` 指向 ModuleLowerer 已声明的
  精确 target；不得分配 environment 或生成 wrapper allocation。
- concrete closure lower 为 `{ptr, environment}` first-class aggregate；environment 是 capture
  顺序的 concrete struct，owned slot 使用实际 LLVM value，shared slot 使用 storage pointer。
  layout 只来自当前 target `DataLayout`，不含 type tag、drop pointer、allocator header 或
  `void*` erased environment。
- closure thunk 接收 environment first-class aggregate 作为首参数，之后接收用户参数；invoke
  提取 code/environment 后使用 LLVM indirect call。function pointer invoke 不插入 environment。
  两者都必须通过 LLVM verifier，并锁定精确调用顺序和确定文本。
- shared capture 只保存已有 place/loan 的 pointer；LLVM adapter 不通过 pointer equality 推导
  alias 或 lifetime。owned MoveOnly capture 的 drop helper 从 environment 逆序 extract 并复用
  type-directed glue；整个形成、调用和 drop 路径不调用 `malloc/free`。

### 3.3 分阶段交接

- 本 Spec 先用手工构造且通过 verifier 的 SSA 覆盖所有 capture mode/type/CFG/runtime 不变量，
  不以尚未支持 function-type concrete provenance 的完整 frontend→SSA 接线替代后端验收。
- 后续 frontend 接线必须直接消费 SPEC-0032 的 `ClosureDescriptor`、capture/drop/loan facts，并
  为 higher-order 实例保存 concrete closure identity；不得只按 `TypeKind::Function` 或 lambda
  AST 形状猜测 layout。

## 4. 非目标

- 不实现不同 closure layout 的 existential coercion、运行时 tag/union、统一 heap box、引用
  计数、动态 dispatcher、closure layout 合流或 public FFI callable ABI。
- 不实现 bound member reference、普通 callable reference 的 overload 选择、instance receiver、
  thread/channel runtime、linker、`main`、object、DWARF 或完整源码 closure lowering。
- 不改变 v0.27 capture/escape/`Transferable` 诊断，不重新计算 frontend liveness，不增加用户
  自定义 destructor、异常展开或完整 NLL。
- 不借本 Spec 扩展多返回值、借用返回、一般 MoveOnly Borrow/Inout callable 参数 ABI；closure
  shared capture 使用已经存在的 place/loan SSA 能力。

## 5. 验收标准

- [ ] function-pointer/concrete-closure type identity、signature、capture mode/type、MoveOnly 能力、
      inline-cycle、跨 module ID 与确定 debug text 正反矩阵通过。
- [ ] function address、owned/shared formation、indirect invoke、drop 的 operation contract 正反
      矩阵通过；错误 target/env/signature/arity/mode/result 在 LLVM construction 前拒绝。
- [ ] owned MoveOnly capture 形成时消费且逆序 drop；Copyable capture 保持可用且无 glue；
      callable 可重复 invoke，drop/use-after-drop/双 drop/正常出口遗漏被拒绝。
- [ ] shared capture loan 随 closure owner 和 CFG edge 存续，阻止来源 move/mutation/drop；提前
      loan end 被拒绝，closure drop 后 loan 精确结束且来源恢复可用。
- [ ] AArch64 LLVM IR 锁定裸 function pointer 与 `{ptr, environment}` 两种且仅两种表示、真实
      indirect call、environment 首参数和 capture 顺序；无 type tag、drop pointer、`malloc`、
      `free`、动态 `alloca` 或 erased `void*` environment。
- [ ] closure drop 对 owned MoveOnly capture 逆序调用既有 glue，shared/Copyable slot 不析构
      来源；合法 module 同时通过自建 verifier 与 LLVM verifier并产生确定文本。
- [ ] `lang-codegen` 窄测及 workspace 标准基线通过；本 Spec 涉及的 production Rust 文件遵守
      1000 行软上限，Architecture、Spec 索引和 roadmap 只记录实际完成事实。

## 6. 技术方案与边界

- callable/closure type model 与 operation contract 使用独立 SSA 子模块；既有 model/render/
  ownership verifier 只增加必要分派，不把环境规则堆入接近软上限的文件。
- verifier 的 closure→shared-loan 依赖是 owner 状态的一部分；CFG edge transfer 必须重绑定到
  successor owner，不能仅依赖原始 `LoanId` 在集合中“仍然存在”。
- LLVM closure layout/invoke 放在独立 adapter 模块；drop declaration/definition继续由
  `RuntimeAbi` 集中，closure lowering 不直接声明 allocator 或 drop symbol。
- 具体表示完全服从 ADR-0009；若实现证明 existential 合流不可避免，应停止并以新 guide/ADR
  处理，不得在本 Spec 内静默加入 type erasure。

## 7. 实施计划

1. [x] 建立 callable/closure SSA type、function-address/formation/invoke operation、render 与局部
   verifier → 验证：identity、signature、owned capture、target 与 operation 正反矩阵。
2. [ ] 实现 owned/no-capture LLVM layout、indirect invoke 与 closure drop glue → 验证：裸 pointer、
   `{ptr, env}`、重复调用、逆序 drop、零 allocation 与 LLVM verifier 矩阵。
3. [ ] 把 shared capture loan 依赖并入 ownership/CFG verifier 和 LLVM pointer slot → 验证：提前
   end、冲突、edge transfer、drop release 与混合 capture 矩阵。
4. [ ] 运行 workspace 基线、同步 Architecture/roadmap/Spec 验收并审查 staged diff → 验证：实际
   退出状态、文件规模和文档一致性。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | callable/closure SSA type、operation、render 与 owned verifier | `feat(codegen): model concrete closure operations (SPEC-0038)` |
| 2 | function pointer、owned environment、indirect invoke 与 drop LLVM lowering | `feat(codegen): lower owned closure environments (SPEC-0038)` |
| 3 | shared capture loan/CFG 依赖、LLVM pointer slot 与 done 验收 | `feat(codegen): verify borrowed closure environments (SPEC-0038)` |

## 9. 未决问题

- 无。不同 concrete closure layout 的统一 storage 已由 ADR-0009 明确排除；遇到该边界必须
  返回 unsupported lowering，而不是改变本 Spec 表示。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | SPEC-0032/0034/0035 `done`；ADR-0006–0009 `accepted`；v0.27 capture 与 Phase 4 concrete closure 要求已生效 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo test -p lang-codegen --all-targets`（SSA slice） | 通过 | 77 项；新增 4 项 callable/closure identity、inline cycle、function address、owned formation、重复 invoke、drop、错误 thunk/shared formation 与 move-after-capture 矩阵 |
| `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21 cargo clippy -p lang-codegen --all-targets -- -D warnings`（SSA slice） | 通过 | 无 warning；closure type construction 与 ownership effect 已独立成模块，`model.rs` 987 行、`verify_ownership.rs` 922 行，后续 LLVM/shared-loan 职责不继续堆入两者 |
