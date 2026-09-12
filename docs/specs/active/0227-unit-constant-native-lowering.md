# SPEC-0227：跨文件常量 SSA 与 native 交付

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：接入 unit 常量 native 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-227` |
| 所属 Phase | Phase 4 |
| 语言规范 | [v0.36 §36](../../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值) |
| 批准依据 | 用户持续授权分阶段实施；2026-09-13 前置完成后迁入 active |
| 前置 Spec | SPEC-0198/0199/0208/0209/0210/0226 `done` |
| 前置 ADR | ADR-0007/0008/0010/0018/0020 `accepted` |
| 阻塞项 | 无；SPEC-0226 已交付完整 owned capability 与物化/drop/短路事实 |
| 影响范围 | `lang-codegen` unit SSA planning/lowering/verifier/native；必要 CLI 编排与测试；Architecture |
| 语言语义变更 | 否 |

## 1. Goal

完成后，专用 unit native 路径消费同一次 const-enabled typed 与 owned 产物，将运行时常量
use 降为标量值或独立 String literal temporary，生成并运行具有精确结果与清理行为的本机产物。

## 2. 背景与范围

- 复用单文件常量 lowering、IR-local Char、String literal 构造及既有 unit planning/native 管线。
  不重新解析/求值 initializer，不从 AST 恢复缺失的物化或 drop 事实。
- 新入口验证完整 inputs/names/environment/typed/owned 身份链及 executable gate；旧基础
  入口保持原能力边界，不通过可伪造转换接受新事实。
- scalar use 使用 typed ConstValue 精确 width/signedness/Unicode scalar；String use 按
  Phase 3 物化与控制流位置生成独立 owner，执行已验证 drop/transfer。
  同时消费调用前缀 pending temporary：后续实参提前 return/break/continue 时结束借用并清理，
  Abort 不清理，已提交 Value 不重复清理。命名 MoveOnly Value operand 也可能以实参 expression
  为 pending owner；不得只缓存 AST category 为 Temporary 的表达式而漏掉该事实。
  String 插值还需消费内部输入的 `AfterExpression` 与提前退出清理，嵌套插值按各自位置
  释放；当前 unit String lowering 仅解码 plain literal，不能将 Phase 3 事实视为已支持插值。
- 短路必须消费专用 owned 的 source-qualified 执行计划：左侧正常完成后 RHS 为
  Always/Never/Conditional；不得通过 AST 猜测缺失计划。分支编号沿用 0=true、1=false，
  AND RHS 为 0、OR RHS 为 1；消费 skip/RHS 的 BranchExit 清理，并保留 RHS 退出后的 skip 后继。
  基础入口既有单边 move 的 MissingFact 拒绝不因新入口接入而被隐式解除。
- 常量声明及初始化依赖无运行时存储、global/init guard、namespace capture 或退出析构。
  不能为跨文件读取引入 singleton 或稳定常量地址。
- 使用既有 process entry（含 argv）、object 原子发布与链接/执行流程；必要 CLI 编排仅负责
  选择正确阶段入口，不承载语义或重新推导事实。

## 3. 非目标

不实现通用 CTFE、关联函数调用、runtime globals、跨 compilation-unit ABI、新 LLVM type
体系或 v0.37；不修改已完成 0198/0199 的验收含义。

## 4. 验收标准

- [ ] import 后选择及绝对路径覆盖顶层常量和五类关联 namespace；全部 11 种常量类型 native 输出正确。
- [ ] 跨文件 chain 与重复 use 保留精确值，短路/control 保持 typed/owned 执行位置。
- [ ] String 多次读取产生独立临时 owner；借用、转移、返回及 Abort 的清理与 literal 对照一致，
  不重复清理已转移 owner；使用既有 IR 或计数证据核实分配/释放，而非仅检查退出码。
- [ ] IR/verifier 不出现常量声明 storage/global/init 或 namespace capture；Char 保持既有 Char 契约。
- [ ] 失败 typed/owned、混合分析及缺失物化事实在写出前被拒绝；失败保留既有输出文件。
- [ ] 正逆 source inputs、重复构建产生确定性结果；argv entry 与普通 unit 非常量最近回归通过。
- [ ] 新旧入口 capability 编译契约、native 正反例和必要 CLI 编排验证通过，Architecture/验收同步。

## 5. 实施与提交

1. [ ] 显式消费新 owned capability 并接 unit planner → 验证：身份/缺事实拒绝与现有基础边界。
2. [ ] 接 scalar/String 物化与 cleanup lowering → 验证：IR/verifier 及 literal 对照。
3. [ ] 完成 native build/run、argv/确定性/原子输出矩阵 → 验证：上述逐项证据与归档。

每步形成可构建的独立切片，提交包含 `SPEC-0227`；不要把 SPEC-0226 的实现混入本 Spec 提交。

## 6. 验证记录

前置完成后先定位已有 unit lower/native test support；新增直接 constant suite，按影响选
最近消费者，不重复单文件已稳定验收。公开入口追加编译契约和
[分层验收](../../development/testing.md)要求的 workspace 编译检查；不运行 frontend 全量测试。
目标工具不可用时明确记录阻塞，不将 IR 或编译通过写成 native 运行通过。

| 验收项 | 结果 | 原因 |
|---|---|---|
| Rust / native 实施与验证 | 未执行 | 本次启用并定位消费边界，尚未实现 native |

合同复核与门禁：独立边界审查通过，已显式补入顶层常量验收；docs check（353 Markdown）、
文档检查器 21 项测试与 diff check 通过。该记录属于 draft 建立时的合同检查。2026-09-13 前置完成，本次迁入 active；Rust/native 验收尚未执行。
