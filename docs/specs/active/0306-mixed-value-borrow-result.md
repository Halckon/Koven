# SPEC-0306: 混合 Value 参数的唯一来源借用返回

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收混合 Value/Borrow 参数的普通借用返回时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0306` |
| 所属 Phase | Phase 4 |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定)、[Guide05](../../guide/05-declarations-callables.md) |
| 批准依据 | 2026-10-13 用户要求先收尾现行里程碑；在 M2B 已启用合同内继续实现，不转新集合 API |
| 前置 Spec | [SPEC-0305](../../archive/specs/0305-multi-parameter-borrow-result.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0029](../../adr/accepted/0029-ordinary-borrow-result-continuation.md) |
| 关联 ADR | 无新结果 ABI 或语言决定 |
| 阻塞项 | 本地实施/验证未完成；外部发布另等具体授权 |
| 影响范围 | lang-codegen single/unit lowering、verifier、LLVM call adapter、正常 source/native 测试 |
| 语言语义变更 | 否 |

## 1. Goal 与输入

唯一来源仍是 Borrow 参数，其余形参可为 Borrow 或显式 own 所映射的 Value；在已有直接
具名调用、表达式体、受限 wrapper 与直线 borrow val 范围内，正确运输 Copyable 与既有
MoveOnly/Resource 参数。不是完整 M2B 完成，不引入新的参数或结果表示。

基于 actual main `bfdb00bd455d6847a7f14cf7391a198664ee6f1e`、tree
`795ac4346dce3c098a0acfd0ebcf198001b11341` 建立 `feature/spec-0306`。
两个 parent 为 `f7a4d15cecfa71b2fb8a4e9d4648d56e53702c05` 与
`b7a269d2e5589260cc7bc26fb3cfc28ca3fa5808`；完整 API commit JSON 恢复后逐对象核 SHA。
0305 已由 PR83 实际合并；本片不把尚在独立监视的 main CI 写成通过。

## 2. 有界合同

- source 必须由 typed Parameter(i) 和 ownership origin 证明为唯一 Borrow 参数；真实 source
  storage 可为已支持字段，保留已有泛型与 Group。其余参数为已支持的 Borrow/Value。
- 按源码顺序仅求值每个 operand 一次，立即应用 typed 参数契约，按 parameter_index 装配
  ABI。Value 按现行 Copyability copy/move，不伪造 loan，也不强制 Copyable 新 SSA ID/glue。
- BorrowCall verifier 验证 Value 活性与 MoveOnly 唯一消费、全部 Loan 活性；结果 alias/parent
  仅延续真正 source。共享源码产生的 Program 必须通过原有 normal-exit 检查，不放宽 verifier。
- CallReturn 只结束非来源本次新建 loan，清理其 Borrow temporary；复用外部 loan 不提前结束。
  Value temporary 转交后 caller 不重复清理；source/result 继续到已验证终止点。
- callee 使用既有 FunctionEntry、CallReturn、ControlTransfer drop facts，保持纯内存 ASAP
  与 Resource 逆声明顺序；先得到返回 loan，再按事实清理非来源 owner，最后 BorrowReturn。
  ControlTransfer 使用原外层 expression-body ID，不用 ungroup 后的内层 ID代替。
- wrapper 可转交 Value owner 或对它作辅助 shared reborrow；后者在嵌套调用后终止，前者只由
  新 owner 负责清理。返回 source 链始终存活；source/result 终止先于其 owner drop。
- single 与真正跨文件 unit 一致，缺失事实结构化拒绝；LLVM 实参复用既有 Value/Loan 映射，
  结果继续返回真实 storage pointer，不复制/retain borrowed payload。

## 3. 非目标

Inout 参数/来源、receiver、block 返回、caller CFG/loop 运输、间接/callable 参数、同步嵌套
borrow-result 实参、短 range/新 carrier ABI、条件借用、多来源、完整 NLL 或 std 新 API。
保持当前 structured rejection 与真实 Span。需要新语义或长期 ABI 决定时暂停报告。

本地禁止故障注入/校准、sanitizer、坏 IR/allocator/LLVM 重写和 aggregate 驱动；只运行审阅过的
正常 source→frontend→真实 SSA/verifier→原版 LLVM/object/link/run。语言定义的非法源码与
Abort 不属于故障注入；坏 IR verifier 负例仅静态编写交 CI，不能在云上执行。

## 4. 实施与验收账本

先移植仓库外15组已复现 Value 源码红测，扩唯一来源、参数顺序、owner/loan/drop 正反例；
再最小接线两入口、verifier 与 LLVM，按同一选择转绿，最后验证直接消费者和既有0305回归。

| 验收项 | 命令/选择 | 实际结果 |
|---|---|---|
| actual main / Spec 冻结 | 精确 commit/tree/双parent、库存/docs/diff | 精确对象已恢复，0306未占用；文档门禁待运行 |
| 正常 source 红测 | mixed_value_borrow 定向矩阵，single/unit | 待移植并在本基线运行 |
| SSA/Loan/Value/drop | 同一 source 矩阵与原版 LLVM verifier | 未实现、未运行 |
| native stdout/deinit | 同一正常 source 两入口 object/link/run | 未实现、未运行 |
| 错误来源/冲突/保留边界 | 普通负源码与既有0305 source矩阵 | 待运行 |
| 共享消费者 | 普通 Value 参数与原借用返回/字段/Group定向 | 待按实际影响选择 |
| 静态门禁 | fmt、codegen严格clippy、docs/尺寸/diff | 待运行 |
| 独立审查 | 最终精确输入 | 待安排 |
| 双宿主CI/归档/合并 | 精确远端head/merge/main | 未发布、未运行 |

准备阶段在0305冻结树已完成33组双入口正常探针，其中15组Value均前端接受而在真实lowering
拒绝；6次native对照和4项正常source SSA/LLVM测试成功。它们仅是准备证据，不能冒充本片
实现或本基线新测试通过。原始源码、stdout/stderr与恢复材料保留在仓库外。

## 5. 交付边界

本地分步提交并保留恢复包；Spec 保持 in-progress，独立审查后另请求具体发布授权。
不因本片成功关闭 Inout、同步 nested-use 等其他 M2B 缺口；未运行项按实际账本保留。
