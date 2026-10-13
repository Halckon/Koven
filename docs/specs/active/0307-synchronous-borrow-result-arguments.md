# SPEC-0307: 普通借用结果的同步 Borrow 实参消费

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收普通借用结果的同步实参时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P3-0307` |
| 所属 Phase | Phase 3 来源/使用/终止事实；Phase 4 SSA/native |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md#普通借用结果与显式局部绑定) |
| 批准依据 | 2026-10-13 用户要求先完成现行M2B；0306合并后批准按已审准备范围继续同步Borrow实参片 |
| 前置 Spec | [SPEC-0306](../../archive/specs/0306-mixed-value-borrow-result.md) |
| 前置 ADR | [ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0029](../../adr/accepted/0029-ordinary-borrow-result-continuation.md) |
| 关联 ADR | 无新结果 ABI 或语言决定 |
| 阻塞项 | 无语义阻塞；实现及验收进行中 |
| 影响范围 | lang-frontend single/unit ownership facts、lang-codegen lowering、正常source/SSA/native测试 |
| 语言语义变更 | 否 |

## 1. Goal 与输入

`observe(view(source))` 在已支持的具名同步调用中，把普通借用结果作为真实 Borrow 实参；
结果只延续到 outer CallReturn，外层返回普通 owned 结果。返回后来源恢复已证明的读取、
移动及清理能力。不是完整 M2B，不引入新的语言或公共 SSA/LLVM ABI。

从 actual main `967bf98e0cb370265bc2587a26b0ff48f7b85847`、tree
`9b88a8a95fe44da4fa6cffaf2e49408093a44abd` 建立 `feature/spec-0307`。
完整 API commit JSON 重建原字节并核 SHA；ordered parents 为
`bfdb00bd455d6847a7f14cf7391a198664ee6f1e` 与 `c2142a9f1bf03cf572f6958f8d403fc768d6a775`。
0306 已由 [PR84](https://github.com/Halckon/Koven/pull/84) 合并；其
[actual-main CI](https://github.com/Halckon/Koven/actions/runs/38368833991)独立跟踪，冻结时仍运行中。
0307 在 active/draft/archive 库存与文件中未占用；本片保持 in-progress，不提前归档。

## 2. 有界合同

- inner 沿0305/0306：无 receiver、唯一 Borrow 参数来源、其余已支持 Borrow/Value、表达式体、
  具名包装、具体泛型、字段返回及透明 Group。来源是稳定 named owner、Borrow entry 或已有
  显式 borrow parent；source canonical lease 和真实 payload storage 分开保持。
- outer 仅具名普通 DirectCall，目标实参由 typed mapping 证明为同步 Borrow，结果为 Unit 或
  已支持的独立 owned 值。按源码顺序求值一次、立即施加契约，再按 parameter_index 装配 ABI。
- frontend 发布普通 call-use、source continuation、parent 与明确 outer CallReturn 终止事实。
  保存 inner实际call与outer原始operand identity；不伪造binding symbol、不把result当owned临时。
- inner 返回只结束无关本次新建 loan并清理对应owner；source/result在后续outer operand及
  callee运行期间保持活性。后续 own move/Inout重叠应报L0135，先消费再借用保持L0131。
- outer返回时每条result子loan先于其本次新建祖先结束；全部保护某owner的loan结束后才drop
  该owner。独立链可按逆序逐链结束；同root不同实参身份不可合并，既有外部parent不可提前结束。
- Resource依既有drop facts与词法逆声明序清理；Borrow temporary按所属call归还，Value
  temporary只有新owner清理；结果不产生payload owner/drop/copy/retain。
- single/unit事实等价、ID边界封闭，错误/deferred原子撤销新事实；lowering缺事实结构化拒绝。
  借用结果交own（包括Int）、owned返回、普通val等继续L0163，不能用隐式copy绕过身份。
- 复用 BorrowCall、Loan operand、DirectCall、既有verifier和storage pointer ABI；single增加
  局部祖先链运输，unit复用现有Vec调用帧。verifier不放宽，正常source生成真实Program后验证。

## 3. 非目标

temporary source、outer借用结果继续绑定/返回（包括 `view(view(source))`）、receiver/Inout
来源或辅助参数新能力、CFG/loop/提前退出运输、间接/callable或跨线程调用、新range/carrier、
条件/可选借用、任意表达式上下文、完整NLL、新std API。保留既有结构化诊断与Span；
若发现Guide不足或需新ABI/ADR，先报告并停在该边界，不自行补语义。

本地仅运行正常source或语言非法source→frontend→真实SSA/verifier→原版LLVM/object/link/run。
禁止fault注入/校准、sanitizer、坏IR、allocator/LLVM改写及aggregate驱动；坏IR测试若有只静态
编写交CI，不在云上执行。Cargo串行独占现有target，不cargo clean。

## 4. 实施与验收账本

先冻结Spec与actual-main正常红基线，再添加行为失败测试，接frontend事实/cleanup与两入口
lowering。稳定snapshot即送独立审查，不等最终收尾才review。小提交与完整恢复材料留仓库外。

| 验收项 | 目标/选择 | 实际结果 |
|---|---|---|
| actual-main/Spec冻结 | commit/tree/parents、库存、docs/diff | 精确对象恢复与编号核验通过；文档检查待执行 |
| 正常source红测 | 准备45组×single/真正跨文件unit在actual-main重跑；新正常测试 | 进行中；旧0306本地准备证据不冒充本基线 |
| frontend事实与拒绝 | 定向新suite；borrow origins/continuation相关source测试 | 未执行 |
| SSA/Loan/来源/清理 | 正常source两入口矩阵，检查未改写Program及原版LLVM | 未执行 |
| native行为 | 同源single/unit object/link/run、逐字节stdout/空stderr/deinit | 未执行 |
| 共享消费者 | 既有0305/0306、普通Borrow调用、父绑定与cleanup相关正常source测试 | 未执行 |
| 静态门禁 | fmt、frontend/codegen严格clippy、跨crate check、docs/尺寸/diff | 未执行 |
| 独立审查 | 稳定snapshot、最终变化复查 | 未执行 |
| 远端双宿主/交付 | root统一publisher、精确head/merge CI | 未发布、未执行；merge另需具体批准 |

验收必须覆盖来源首/中/末、内外named反排、同root多次借用、不同root、混合Borrow/Value辅助、
内外层临时Resource、wrapper/泛型/字段/Group、已有borrow parent后续使用、outer后显式move，
并核解除L0164后的真实L0135/L0131、L0163与拒绝原子性。source被投影到不同payload类型时必须
检查实际返回storage身份，不能只以输出偶然正确代替SSA来源证明。全部相关loan结束先于owner
drop，每个资源deinit一次且顺序精确。只记录实际运行结果，不把过滤/未测/macOS待CI写成通过。

## 5. 交付边界

先批准范围内本地红绿/回归与独立审查，再由root协调分支push与Draft PR；本worker不GitHub写。
Spec保持in-progress，归档及未来合并按具体交付授权与实际CI证据处理。原始日志、源码探针、
二进制与恢复bundle置于仓库外；库内只保留真正持续测试fixture及本简要账本。
