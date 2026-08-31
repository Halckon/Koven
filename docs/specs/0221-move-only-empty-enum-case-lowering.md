# SPEC-0221：MoveOnly enum 空 case owner lowering

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P4-221` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.34](../guide/00-index.md) 的既有 `enum class`、constructor 与所有权规则 |
| 批准依据 | 2026-09-01 持续 Goal 要求继续按 Phase 推进 guide 对应 Specs、简化验收并避免 `lang-frontend` 全量测试；SPEC-0191 enum receiver characterization 隔离出空 case construction 缺口 |
| 前置 Spec | SPEC-0184、0188、0198、0199 `done` |
| 前置 ADR | ADR-0008、0010 `accepted` |
| 关联 Spec | SPEC-0191 |
| 阻塞项 | 无 |
| 影响范围 | `lang-codegen` compilation-unit enum construction/owner transfer/drop 与 native tests；Architecture/Roadmap |
| 语言语义变更 | 否；修复既有 validated enum construction/ownership facts 的 Phase 4 消费 |

## 1. Goal

完成后，若 non-generic enum root 因任一其他 case 的 payload 而为 MoveOnly，零 payload case 仍可作为
合法的唯一 tagged owner 完成 construction、local/return/Value delivery、receiver operand 与精确 drop，
而不会因“当前 case 无字段”被误判为无 owner 或 unsupported。

## 2. 范围与需求

- 只处理已 concrete、non-generic `enum class` 的零 payload case；root tagged type、case identity、variant
  index 与 `ConstructionRootKind::Inline` 必须继续来自 frontend validated descriptor/ownership plan。
- 空 case construction 仍生成其零字段 payload aggregate 与 root `TaggedConstruct`；root copyability 按完整
  enum 推导，不按当前 case payload 局部重算。
- MoveOnly root 在 local、return、Value call/receiver 的 temporary/place delivery 中保持唯一 owner；不得因
  空 payload 跳过 take、复制 tagged value或丢失 drop obligation。
- LLVM drop glue 继续按 runtime tag 分支：空 case 不 drop payload，含 owner payload 的 case 只 drop 对应
  字段；不增加 allocation、tag 之外的状态或特殊 calling convention。
- validated-before-LLVM 不变；construction descriptor、ordered delivery/root obligation、case/root identity
  或 drop fact 不一致时必须带 Span fail loud，不能按 AST 名称猜测。

## 3. 非目标

- 不开放 generic enum storage/monomorphization、inline recursion、MoveOnly enum `when` branch-qualified drop、
  partial payload move 或结构化消费。
- 不新增 enum receiver IR；SPEC-0191 已证明 Borrow 与 Value receiver 共用现有 root tagged identity。
- 不改变 frontend type/copyability/ownership 规则、SSA tagged-union ABI、runtime ABI、依赖或诊断码。

## 4. 验收标准

- [ ] 红测锁定 `enum class Owned { Empty, Full(value: String) }` 的 `Owned.Empty` 不再以
  `UnsupportedNode` 失败，且与 `Owned.Full` 共用唯一 root tagged identity。
- [ ] empty temporary/local/return/Value call 与 Value receiver 至少覆盖三类边界，SSA verifier 锁定唯一
  take/drop；Copyable 全空 enum 与既有 non-empty MoveOnly case 回归不变。
- [ ] LLVM 锁定 empty variant construction、tag-dispatch drop 与 Full payload 精确析构；真实
  source→object→Clang link→run 同时执行 Empty 与 Full 路径，无 leak/double drop。
- [ ] generic enum 与 MoveOnly enum `when` 继续确定性拒绝；不修改 frontend facts。
- [ ] 运行 `unit_lower_enum_tests`、相关 receiver 与 `native::unit_tests`，再运行 workspace library
  check/clippy、fmt/diff及独立复核；不运行约一小时的 `lang-frontend` 全量测试，除非窄测证明公共事实错误。
- [ ] Architecture、Roadmap、SPEC-0191 与本 Spec 验证记录同步。

## 5. 技术方案与边界

先用最小 compilation-unit fixture 对比 Empty/Full 的 `UnitConstructionDescriptor`、
`UnitConstructionOwnershipPlan` 与 drop facts，定位 `UnsupportedNode` 的首个消费点。修复优先限定在现有
`lower_enum_construction` / ordered-field delivery / local transfer 组合，不修改通用 receiver 或 tagged IR；
若事实本身缺失才回到 Phase 3 建立独立前置，而不在 codegen 重推。

## 6. 实施计划

1. [ ] 建立 Empty/Full fact 与 red-test 对照 → 验证：错误 Span、descriptor/root obligation 差异明确。
2. [ ] 修复空 case owner construction/transfer → 验证：enum SSA 正反矩阵与 verifier。
3. [ ] 接 LLVM/native Empty+Full drop 闭环 → 验证：tag 分支、析构次数与真实运行。
4. [ ] 独立复核、同步 Architecture/Roadmap/SPEC-0191 并运行精简分层验收。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Spec 范围、红测证据与精简验收门禁 | `docs(spec): stage empty enum owner lowering (SPEC-0221)` |
| 2 | enum empty-case owner/LLVM/native 与完成文档 | `fix(codegen): lower move-only empty enum cases (SPEC-0221)` |

## 8. 未决问题

- 无。generic enum 与 MoveOnly `when` 已明确排除，不阻塞本切片。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-09-01 receiver characterization | 缺口已隔离 | 拆分 Copyable enum Borrow 与 MoveOnly enum Full Value receiver 后均通过；把二者合并为 `Signal { Ready, Full(String) }` 时首个 `Signal.Ready` local construction 以带 Span `UnsupportedNode` 失败，证明问题位于空 case MoveOnly root 而非 receiver ABI |
