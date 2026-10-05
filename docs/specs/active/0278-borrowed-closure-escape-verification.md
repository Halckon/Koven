# SPEC-0278: borrowed closure 的 owned escape 交付校验

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实现或验收 borrowed closure SSA 逃逸防线时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0278` |
| 所属 Phase | Phase 4 typed SSA verifier |
| 语言规范 | 已启用 [Guide07](../../guide/07-calls-lambdas-closures.md)、[Guide10](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户持续实施里程碑、满足前置并行和据实际调整草稿的站立授权；本片只落实已有逃逸规则 |
| 前置 Spec | SPEC-0277 已 done，PR55 final/main 15/15 与实际产物闭环 |
| 前置 ADR | [ADR-0009](../../adr/accepted/0009-concrete-closure-internal-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) accepted |
| 关联 ADR | 无新增长期表示或 ABI 决定 |
| 阻塞项 | 无语义阻塞；三个 fixture 已真实红测，实现及完整验收待 |
| 影响范围 | lang-codegen SSA verifier、直接 malformed SSA 与既有消费者测试、Architecture |
| 语言语义变更 | 否 |

## 1. Goal 与基线

在 typed SSA 的既有 owned escape 消费点拒绝带 shared-capture 环境的 closure，
包括通过 owned capture 或实际存储类型包装的值；保留同步 Borrow、局部使用/Drop 与 CFG 运输。
不让当前合法签名的 `Borrow Int` factory 返回指向 entry loan 的 closure 并进入 LLVM。

工作树 `feature/spec-0278` 从实际 main `54b36675481c15da8a1f31d9d3185e4b49c83096` 创建；
前置交付见[0277账本](../../development/evidence/p2-linux-0277-delivery.json)。
代码审阅显示 ClosureLoans 当前仅记录本函数直接 Shared capture 与 CFG 运输，
Return 在消费 closure 后隐式释放 entry loan；该推断已由三个合法 fixture 的实际接受性确认和正式拒绝测试失败证明，见§6；生产修复未实施。

## 2. 范围与可复用接口

复用 `Module::concrete_closure`、IR-local SsaTypeId、既有 OperationContract/VerifyLocation/Origin。
已有 ClosureLoans 负责局部活跃 loan，不替换其功能；consume_value 的 Drop、Consume、CFG 等
合法路径保持原合同，不在通用 consume_value 中一律禁止 closure。

- escaping 消费点：Return，DirectCall 的 Value receiver/参数，CallableInvoke 的 Value 参数；
  AggregateConstruct/TaggedConstruct/HeapAllocate/SharedAllocate 的存储交付；
  HeapFieldReplace/InlineFieldReplace/HeapFieldExchange、ContainerConstruct/ContainerReplace；
  Mutate 写入 projected FieldPlace/ContainerElementPlace 及其 CFG 别名。RootPlace 局部写不一律拒绝。
- 从实际存储类型分类：直接 ConcreteClosure 的 Shared capture 为种子；递归传播到 Owned
  capture、aggregate/tagged 字段、heap/shared owner payload、nullable inner、container element。
  FunctionPointer 参数/返回签名、SharedReference target 不是 owned 存储，不沿它们误判。
  entry Value、CFG 参数和从存储读取的类型也须识别，不能只查本函数 ClosureConstruct。
  类型只证明 may-contain；空容器、NullableNull、选择无 closure 的 tagged variant 及其已知无捕获
  包装/CFG 移动须保留 known-no-capture 值证明。未知 entry/调用结果没有这种证明时才使用类型兜底，
  不能将 may-contain 直接当作每个当前值实际持有 capture。该值分类不跟踪 LoanId 生命周期。
  未知 may-contain 值因缺少无捕获证明而在 raw SSA owned delivery 被保守拒绝；不新增 Source
  L0137 规则，也不证明未知值实际持有 capture。后续 source lowering 须保留合法值所需证明。
  RootPlace Mutate、RootReplace/Swap 必须更新值证明，不能沿用写入前的空/null 快照。
- 类型分类有确定性缓存和有限图遍历，支持共享 DAG 与合法 owner 递归，避免按每个消费点
  重复递归或用 Rust 调用栈展开深链；不新增依赖、frontend facts 或资源预算。
- 外层 closure 只在真正 escaping 消费点递归检查；局部 Owned capture formation 不一律拒绝。
  既有 OperationContract reason 精确为 `borrowed closure cannot escape through owned value delivery`，
  location/origin 指向真实消费指令或 Return terminator。

## 3. 非目标与完整 M3A 终点

本片不是完整借用依赖传播修复：局部嵌套 closure 的 capture loan 激活/释放问题仍需独立封闭。
不修改 source lowering、ContainerGenerate ABI、1024 实例预算、Guide、LLVM 或 public API。
不运行 P2 重采样、预算接受或 M4b 真实故障校准，不触发 daybreak/cybercheck。

M3A 后继仍须实现 Array/List runtime-length constructor 的两条源码流水线到 SSA/LLVM/native，
覆盖 function pointer/shared/owned environment、负数与 initializer 求值顺序、ASAP、generic/helper
concrete identity 和 resource 清理。共享 SSA 修复不替代该完整终点，不关闭整个 M3A。

## 4. 验收标准

| ID | 必需证据 |
|---|---|
| E1 | 修改前真实 fail-first：Borrow Int entry 被 shared closure 捕获后 Return；固定错误 reason、Return location/Origin；不能仅以类型不合法或其它 ownership 错误作为成功 |
| E2 | Value-call（含 entry closure、receiver、indirect）、aggregate/container/heap/shared/tagged 存储和字段/container replacement 的绕过路径；Owned capture 外层 Return；projected-place Mutate 与 CFG 别名；各消费点具体 location/Origin |
| E3 | 正控：局部 shared closure Invoke→Drop、CFG 运输、同步 Borrow；仅 Owned capture 的返回、无 capture function pointer；FunctionPointer 签名提及 borrowed closure 不被误当环境；空容器、NullableNull、无 closure tagged variant 及其包装/CFG 正控；RootPlace 局部 Mutate |
| E4 | 类型分类深链、共享 DAG/合法递归图确定终止，不沿函数签名或 SharedReference target；实际存储 wrappers 不漏 |
| E5 | 原 closure/ownership/container/field replacement/root exchange/borrow 消费者；受影响严格 Clippy/fmt，文档/checker/inventory/尺寸门禁和独立全审 |
| E6 | PR 精确 head 双宿主实际执行新测试及直接消费者，归档后最终 head CI、merge/actual main 门禁与原始证据闭环 |

完整验收仍未完成，不提前勾选；实际已执行阶段见§6。负例必须先确认原 fixture 除新增 escape 限制外可以通过完整 verifier；
保留确认阶段证据，再断言修改前的预期拒绝测试失败。正反例不共享新实现生成的 oracle。

## 5. 技术边界与实施顺序

1. 冻结直接 Return 与 entry/包装绕过测试，根串行运行 Cargo 保存真实失败；检查实际命中数。
2. 在结构/类型/dominance 校验成功后独立执行 escaping 消费检查；复用原 ownership verifier。
   新职责使用独立模块，不扩大已超限 verify_ownership.rs。
3. 同选择由红到绿，补各 escaping 消费者/类型图正反例，再执行关联契约与本机 native 正控。
4. 独立全审及实际 gate，同步 Architecture 和此表；通过精确 PR head CI 后归档再合并。

每提交一个逻辑边界：合同与接线、代码/测试/实现验收、归档分别提交；用户已授权持续分支交付。

## 6. 验证记录

| 验收项 | 实际结果 |
|---|---|
| E1 原行为确认 | `cargo test --locked --offline -j 2 -p lang-codegen --lib borrowed_closure_escape_old_verifier_accepts_all_three_fixtures`：1 passed/0 failed/0 ignored/874 filtered，一 test 逐项确认三 fixture 原 verifier 接受；exit0 |
| E1 与 E2 entry 绕过红测 | `cargo test --locked --offline -j 2 -p lang-codegen --lib borrowed_closure_escape_rejects_`：0 passed/3 failed/0 ignored/872 filtered，exit101；三次 expect_err 都收到 Ok，不是类型/所有权错误 |
| 原始证据 | [红测账本](../../development/evidence/closure-escape-0278/red.json)保全源码指纹、原日志/临时 fixture；初次命令无效 libtest 参数失败独立保留，不作为行为红测 |
| 文档合同与 inventory | `check_docs.py` 524页通过；checker 37测试通过，diff通过；新增值证明要求再同步后须复核 |
| E1 green、E2完整/E3–E6 | 未执行；尚无生产 guard，无完成或归档声明 |
| 独立合同准备审阅 | 发现静态类型可能误拒绝空/null/inactive variant，及遗漏 projected Mutate；已纳入值证明、对应正反例，实际运行与复审待 |
| 前置 PR55 | final CI37265259330 与 main CI37266255625 15/15；实际产物独立核验，原成本材料冻结 |

## 7. 未决问题

无新语言语义问题。若真实 fixture 已被既有防线拒绝，应记录实际错误并重新定位最小缺口，
不得修改负例 oracle 或创建无必要的重复 guard。局部嵌套 capture 依赖后继必须先有失败测试。
