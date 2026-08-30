# SPEC-0212：借用式顺序迭代 SSA/LLVM primitives

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-212` |
| 所属 Phase | Phase 4 |
| 语言规范 | 起草基线 v0.32；候选 [v0.37 §37](../guide/01-design-decisions.md#37-借用式顺序容器迭代-providerv037-候选未启用) |
| 批准依据 | 无；v0.37 尚未启用，且尚未显式重基到现行 v0.33 |
| 前置 Spec | SPEC-0034、0036、0186、0195 `done` |
| 前置 ADR | [ADR-0023](../adr/0023-borrowed-sequential-iteration-provider.md) 待 `accepted` |
| 阻塞项 | 明确 v0.37 对现行 v0.33 的重基与取代关系；v0.37 启用；ADR-0023 `accepted` |
| 影响范围 | `lang-codegen` SSA model/verifier/container LLVM adapter/tests；Roadmap/Architecture |
| 语言语义变更 | 否；实现启用后的 v0.37 provider primitives |

## 2. Goal

完成后，手工 typed SSA 能以 active shared container loan、一次 `Int` length、hidden cursor、
checked element place 和线性 cleanup 表达并验证无分配 sequential provider，LLVM 对 owned/Borrow
container 使用同一 header/runtime ABI；本 Spec 不接真实 `for` AST。

## 3. 范围与需求

- `ContainerLength` operand 扩为 container Value 或 active shared Loan；exclusive/ended/wrong-type
  loan 被 operation/ownership verifier 拒绝，Value 路径保持兼容。
- length result 固定为真正 signed 32-bit Koven `Int`。LLVM header 仍为 target `size_t`，只在
  `logical_length <= 2^31 - 1` invariant 下转换；清除现有 signed i64/`size_t` 冒充 Int 的测试。
- 所有 container 创建边界共同建立 representability invariant：runtime-length construction 先在
  `Int` 域拒绝负数，再无损宽化到 `size_t`；list-form construction 在写 header 前验证元素数
  `<= 2^31 - 1`；未来增长操作也必须在提交新长度前执行同一上限检查。删除现有“语言 length
  与 `size_t` 位宽必须相等”的假设。
- checked index 先在 signed `Int` 域拒绝负数并比较 logical length，再无损宽化为 target index；
  `ContainerLength`/`container.size` 的 `size_t → Int` 只消费上述创建/增长边界已经建立的 invariant，
  不在读取时静默截断或事后修复非法 header。
- provider cursor 为 nonnegative Int；手工 verified CFG 覆盖 acquire once、length snapshot once、
  `cursor < length` 后取 element、increment 不溢出和逐轮 element loan begin/end 的规范形状。
- source Value/shared Loan、cursor、element Loan 可按 ADR-0006/0016 跨合法 edge 传递；verifier 拒绝
  错误 edge entity、inactive/重复 BorrowEnd、active source/element loan 下 replace/drop。
- LLVM loan length 路径先从 pointer load container header；element 继续使用既有 checked-index/
  ZST sentinel，provider 不声明 runtime symbol、不 allocation、不新增 type layout。
- 手工 SSA/LLVM tests 覆盖 empty/non-empty、Copyable/MoveOnly/ZST、normal/continue/break/return shape，
  以及 malformed state/loan/length contracts。

## 4. 非目标

- 不读取 frontend iteration/ownership facts，不 lower `Statement::For`，不生成 executable。
- 不改变 container header/allocator/drop ABI，不实现 Inout source lowering、public iterator object、
  runtime API、自定义或 consuming iteration。
- 不把 Koven `Long`、UInt/ULong 或公开 machine-size type 引入 provider contract。

## 5. 验收标准

- [ ] ContainerLength Value/shared Loan 的 operation、ownership、render 与 LLVM 正例通过；exclusive、
  ended、wrong target 和 result 非 Int 的反例被 verifier 拒绝。
- [ ] runtime-length/list-form construction、未来增长 helper 的共同 guard 与 header size_t→Int bridge
  有边界/不变量测试；既有 container construction/size 测试不再依赖位宽相等或 signed 64-bit
  Koven Int 假设。
- [ ] verifier 接受 owner+source loan+cursor 跨 edge 与逐轮 element begin/end；拒绝错误 edge type、
  inactive/重复 BorrowEnd 和 active-loan replace/drop。
- [ ] checked element place 固定为 signed `Int` 负数检查 → logical upper-bound → target index 无损宽化，
  ZST 按逻辑 index 迭代且不解引用 sentinel。
- [ ] LLVM IR 不含 iterator runtime symbol/allocation；两次 render 确定，SSA/LLVM verifier 与
  codegen/workspace 基线通过。
- [ ] Architecture/Roadmap 同步为已实现事实。

## 6. 技术方案与边界

复用现有 integer compare/arithmetic、block parameter、RootPlace/BorrowBegin/End、ContainerLength、
ContainerElementPlace、Read/Drop；只在现有 operation 无法表达 shared length 时扩大 operand identity。
provider 不新增 first-class SSA type 或专用 runtime operation；普通 typed edge、loan active-state 与
owner conflict verifier 共同拒绝 malformed CFG。职责明确的 canonical builder 构造 length-once、
zero-cursor、guarded-place 与 unit-increment 形状，结构测试和后继 lowering 测试锁定该算法；不要求
通用 verifier 从任意整数 CFG 证明遍历算法。

## 7. 实施计划

1. [ ] 修正 borrowed ContainerLength 与 Int/size_t bridge → 验证：operation/LLVM/边界窄测。
2. [ ] 建 provider CFG/loan 线性 verifier 矩阵 → 验证：positive/negative 手工 SSA。
3. [ ] 接 checked place/ZST/determinism 并同步 Architecture → 验证：codegen、workspace、fmt/clippy。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | borrowed length、provider verifier、LLVM 与完成文档 | `feat(codegen): verify sequential iteration provider (SPEC-0212)` |

## 9. 未决问题

- 无；真实 source integration 由 SPEC-0182 承接。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap/实现审计 | 通过 | existing element place/loan CFG/header 足够；ContainerLength loan operand 与 Int/size_t 契约必须先修正 |
