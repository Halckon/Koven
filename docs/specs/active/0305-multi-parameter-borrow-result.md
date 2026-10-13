# SPEC-0305: 多 Borrow 参数的唯一来源直接返回

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收多参数普通借用返回时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0305` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [Guide10](../../guide/10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定)、[Guide05](../../guide/05-declarations-callables.md) |
| 批准依据 | 2026-10-13 用户要求先收尾当前里程碑；在既有 M2B 合同内继续补实现，不转向新集合 API |
| 前置 Spec | [SPEC-0288](../../archive/specs/0288-map-native-execution.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0029](../../adr/accepted/0029-ordinary-borrow-result-continuation.md) |
| 关联 ADR | 无新 ABI 或长期架构决定 |
| 阻塞项 | 实现、定向本地验证与远端交付待完成 |
| 影响范围 | lang-codegen single/unit lowering、verifier、正常源码/native测试及实现快照 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

全部形参 Borrow 的直接具名函数可从任意唯一参数交付普通 shared 结果，在现有表达式体、
直线 borrow val 范围内保持真实 storage、来源延续和正确清理。不是完整 M2B 完成。

从 actual main `f7a4d15cecfa71b2fb8a4e9d4648d56e53702c05`、tree
`c24fa330381b4de7e3b5113c41c6dd6d3bef2f9a` 建立 `feature/spec-0305`。
用户另行批准的 Guide 旧尾句与两份 README 事实纠错作为独立首提交，只有三路径7+/6-，
不放宽语言/实现边界。0304只改变文档库存流程，本片通过 inventory.md 登记。

## 2. 有界合同

- 来源必须来自 typed `Parameter(i)` 与实际 ownership origin；全部参数 Borrow，无receiver。
- 按源码顺序求值每个实参，再按 argument→parameter mapping 排列 ABI operands；命名顺序
  不能改变来源。返回payload类型可以与来源根类型不同，保留已支持字段、泛型与透明Group。
- 保留每实参新建loan的精确身份；CallReturn只结束非来源的本次loan及temporary清理。
  source/result及其新祖先继续到前端指定结束点，先结果/子loan、再来源、最后owner drop。
- 同owner的多个shared实参不按root合并；复用既有borrow loan时不得结束外部loan。
- SSA签名保存真实来源下标；verifier同时验证全部Loan operand活性、参数/结果类型、唯一
  source→result alias和父子依赖。LLVM复用既有storage-pointer ABI，不复制/retain payload。
- single与真正跨文件unit一致。unit loans_ending_at仅按call枚举全部loan，不代表来源已结束；
  必须与精确BorrowSourceLoan的call/argument身份合用，不能整体结束或整体延长。

## 3. 非目标

receiver/Inout来源、混合Value参数、block返回、caller CFG/loop运输、间接调用、条件借用、
多来源、一等可存储借用、新集合API、完整NLL或新storage能力。保持结构化拒绝及真实Span。
禁止本地故障注入/校准、sanitizer、malformed IR/allocator改写、故障驱动和聚合驱动。
验收使用正常源码、真实SSA/verifier、原版LLVM/object/link/run；不运行计数IR重写helper。

## 4. 实施

1. 普通source红测覆盖来源首/中/末、命名反排、同类型不同root、同owner重复借用、wrapper、
   String/Resource字段/Copyable/MoveOnly storage。失败必须发生于真实lowering。
2. 两入口签名预检与orchestrate运输真实i；调用消费mapping/source/end/drop事实。
   unit call helper保留每实参新建loan归属；borrow result只延续来源链。
3. verifier核全部operand，保持别名与返回链限制；源码正常产物断言不修改IR。
4. 同选择转绿后运行直接消费者与已支持单参数回归，更新Architecture和本账本。
5. 本地小提交、仓库外恢复包；外部发布另等授权。真实CI/归档/合并按实际证据闭环。

## 5. 验收账本

| 验收项 | 命令/选择 | 实际结果 |
|---|---|---|
| 精确main与独立纠错 | raw commit SHA、tree、双父核验；docs checker/diff | main精确恢复；三路径纠错无冲突；655 Markdown通过 |
| 正常源码红测 | multi_parameter_borrow | 待执行 |
| 来源/参数/终止SSA证明 | single/unit正常Program断言及原版LLVM验证 | 待执行 |
| native真实输出与deinit | single/unit普通object/link/run | 待执行 |
| 原单参数与拒绝边界 | 对应borrow定向正常测试 | 待执行 |
| Rust与文档静态检查 | fmt、lang-codegen严格clippy、docs/尺寸policy/diff | 待执行 |
| 双宿主CI、归档与合并 | 精确PR head/merge/main | 未发布、未运行 |

基线准备在相同Rust生产树上实际运行：boundary 2项通过（证明尚未支持）、frontend相关
三个suite共22项通过、8组候选×single/unit在borrow marker拒绝、4个已有单参数native
对照成功；原始输入与日志在仓库外。它们不代表本片已实现或已通过新源码测试。

## 6. 剩余边界

只有本片明确的多Borrow参数形状可关闭；其余M2B入口逐项留后继，不以文档归档宣称完成。
没有现行Guide歧义；若实际实现需要新语义或ABI决定，暂停报告，不从语言经验补规则。
