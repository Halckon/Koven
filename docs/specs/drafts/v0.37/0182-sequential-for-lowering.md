# SPEC-0182：顺序容器 `for` frontend→SSA→native 集成

> **性质**：draft Spec · **状态**：draft（blocked by unapproved v0.37 guide） · **读取时机**：评审 v0.37 proposal 或对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-182` |
| 所属 Phase | Phase 4 |
| 语言规范 | 重基基线 v0.36；候选 [v0.37 §37](../../../proposals/v0.37-sequential-iteration.md) |
| 批准依据 | 无；候选已重基到 v0.36，v0.37 尚未启用 |
| 前置 Spec | SPEC-0034、0036、0184、0192、0195 `done`；SPEC-0179/0211/0212 待完成 |
| 前置 ADR | [ADR-0023](../../../adr/proposed/0023-borrowed-sequential-iteration-provider.md) 待 `accepted` |
| 阻塞项 | 明确 temporary source 首轮 native 范围（proposal §37.4 与本 Spec §3/§5 存在差异）；v0.37 启用；ADR-0023 `accepted`；SPEC-0179/0211/0212 `done` |
| 影响范围 | `lang-codegen` frontend lowering/SSA/LLVM/native tests；Architecture |
| 语言语义变更 | 否；实现启用后的 v0.37 executable `for` |

## 2. Goal

完成后，真实 Koven `for` AST 只消费 validated typed/ownership/provider facts，lower 为无分配的
preheader/header/body/continue/exit SSA CFG，并对三种顺序容器、名称/discard/解构及全部退出路径
完成 LLVM object/link/run 闭环。

## 3. 范围与需求

- source 在 preheader 精确 lower 一次；按 ownership plan 建立/复用 source loan 与 temporary owner，
  length snapshot 一次，cursor 从零开始。
- header 先比较 cursor/length；body edge 建立 current element place/loan 并安装名称或 value-class
  component Borrow bindings。`_` 不生成无用 owner/binding。
- Copyable binding read 生成 copy，MoveOnly 只作为 loan 使用；lowerer 不读取 copyability 来重推所有权，
  不从 container 移出 element。
- normal/continue edge 按 facts 逆序 drop body-local owner/结束其派生 loan，再结束 element/component
  loans、递增 cursor 并回 header；break/exhaustion 随后 finish provider、结束 source loan 并在
  要求处 drop temporary source。return 先交付 operand，再按 body-local → element → provider →
  source → temporary → outer-scope 顺序清理。
- 扩展 loop control，使 break/continue 消费 `DropPoint::ControlTransfer` 与 iteration cleanup facts；
  最近 loop/nested provider 顺序精确，return 沿用先求值返回 operand 再 cleanup。
- lowerer 对缺失/mismatched typed、ownership 或 provider facts 在 LLVM/object 写盘前 fail loud；不按
  source 类型名、`iterator` 方法或 AST 形状重推 provider。

## 4. 非目标

- 不实现 Inout 或 field source native lowering（等待一般 source place lowering 后继 Spec）、receiver/
  公开容器 API、Map/range/String/IO/user-defined/consuming iteration、iterator object 或新 runtime ABI。
- 不改变 SPEC-0179/0211 诊断语义、SPEC-0212 provider verifier 或现有 container layout/drop glue。
- 不实现循环优化、bounds-check elimination、vectorization 或 allocation elimination。

## 5. 验收标准

- [ ] owned named 与 Borrow 参数的 Array/List/MutableList，empty/single/multi 及 source-call-once 正例通过。
- [ ] Int Copyable binding、String/nominal MoveOnly Borrow binding、`_` 与 mixed value-class 解构产生
  预期 SSA loan/read/projection 且无 element consume。
- [ ] normal/continue/break/exhaustion/return/nested loop CFG 的 body-local/derived/element/source loan
  结束与 owner drop 顺序由 SSA/render 断言锁定；temporary source 每条可达退出精确 drop 一次。
- [ ] named source 循环后可读；compile-fail 的 move/mutate source、MoveOnly binding consume/return、
  escaping capture 由 frontend 既有 L0133/L0135/L0137/L0138 拒绝且不进入 lowering。
- [ ] native stdout 覆盖递增顺序、Unicode String Borrow、continue/break/early return；ZST 逻辑次数和
  container/element drop 次数准确，无 iterator runtime symbol/allocation。
- [ ] malformed/mixed analysis product 在落盘前失败；SSA/LLVM 文本确定，受影响契约回归通过，
  Architecture 与实现事实同步。

## 6. 技术方案与边界

在 `lower_frontend::loop_control` 增加只接受前置阶段 validated facts 的 sequential `for` path。
source owner/loan、length 与 cursor 显式作为 CFG entities 传递；borrow bindings 使用现有 loan
binding map，value-class projection 复用 `SharedFieldLoan`，drop emission 只查 SPEC-0211 facts。
LLVM adapter 不认识 AST。

## 7. 实施计划

1. [ ] 接 preheader/header/body 与名称/discard binding → 验证：SSA CFG/loan 窄测。
2. [ ] 接 borrowed destructuring 与全部 jump cleanup → 验证：projection/drop/nested 矩阵。
3. [ ] 完成 LLVM/native/ZST/determinism 与 Architecture → 验证：object/link/run 及受影响的下游契约检查。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | frontend→verified SSA integration | `feat(codegen): lower sequential for loops (SPEC-0182)` |
| 2 | LLVM/native矩阵与完成文档 | `feat(codegen): run sequential for loops (SPEC-0182)` |

## 9. 未决问题

- temporary source 的首轮 native 范围须先决议，详见 proposal 的“启用前需决定”；不得删除现有验收而宣称完成。

- Inout/field source 的 native lowering 等待一般 source place lowering 后继 Spec；不阻塞本 Spec
  对 owned named source 与 Borrow 参数的首轮 executable 闭环。其他 provider 和优化由后续
  guide/Spec 独立推进。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap/实现审计 | 通过 | loop CFG/checked element/borrow ABI 可复用；当前 lowering 明确拒绝 Statement::For |


2026-09-19 候选重基核对：保留现行 v0.36 的 grammar、nullable/Nothing、所有权与常量契约，
拟议版本取代关系见 proposal。仅更新基线与状态前置，不改变本 Spec 的阶段范围、验收条目或
批准状态；guide 启用与 ADR 接受仍是实施前置。未运行 Rust 测试（本次仅文档）。
