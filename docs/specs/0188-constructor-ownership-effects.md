# SPEC-0188：构造 Value delivery 与所有权效果

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P3-188` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.28](../guide/00-index.md)；尚未启用的 [v0.29 §29 候选](../guide/01-design-decisions.md#29-名义enum-case-与-intrinsic-box-构造v029-候选未启用) |
| 批准依据 | 无；语言 guide 不适用站立授权，等待用户明确启用 v0.29 |
| 前置 Spec | SPEC-0029 `done`；SPEC-0183 `done` 后方可实施 |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) |
| 阻塞项 | v0.29 未启用且 SPEC-0183 未完成 |
| 影响范围 | `lang-frontend` ownership checker/model、Phase 3 fixtures；Architecture、Roadmap |
| 语言语义变更 | 否；只消费已启用 guide 与 SPEC-0183 typed facts |

## 1. Goal

完成后，ownership checker 能把每个成功 construction 作为一次源码有序的 Value delivery 与
新 owner 建立动作，准确发布 copy/move、construction temporary、ASAP drop 和递归字段 drop
facts，并拒绝构造参数移动后的再次使用或从 non-owning binding 移出。

## 2. 背景

构造器字段/payload 虽已由 v0.26 定义为天然-owned Value 参数，但 SPEC-0029 只消费普通
`CallDescriptor`。把 constructor 直接塞进 Phase 4 会跳过 MoveOnly operand 失效、临时 owner
延寿与路径析构证明，因此在 SPEC-0183 typed facts 和 SPEC-0184 lowering 之间需要独立的
Phase 3 Goal。

## 3. 范围与需求

- 只消费成功且 owner 一致的 `ConstructionDescriptor`；descriptor 缺失、foreign analysis 或
  参数 identity 不一致是内部错误，不按源码名称重新选择 constructor。
- operand 始终按 descriptor 的 evaluation index 源码顺序各处理一次；每个 Value 参数对
  `Copyable` 建立 owned copy，对 MoveOnly place 转移 owner，对 temporary 直接交付。
- 较早参数移动/loan/drop 效果在较晚 operand 求值时已生效；命名参数映射不能重排所有权动作。
- construction 成功后建立 result owner：value/enum 是内联 owner，class/Box 是唯一 heap-owner
  handle。直接 return/继续 Value 交付可以转移该 owner；未转移的 local/temporary 在 SPEC-0029
  的最早安全点析构。
- 为正常路径发布按实际单态字段/payload 的递归 drop facts；无 payload/全 Copyable 形态不制造
  虚假唯一析构义务。abort 路径继续不 unwind，不发布部分构造 cleanup。
- 非 owning Borrow/Inout 参数中的 MoveOnly 值构造交付复用 L0133；已移动 operand 后使用复用
  L0131；普通字段部分移动继续 L0132。成功结果与现有 loan/place 冲突使用既有 L0135。

## 4. 非目标

- 不选择 constructor、推导类型、映射参数或发类型诊断；这些属于 SPEC-0183。
- 不定义物理布局、分配、enum tag、LLVM drop glue 或 object/run；这些属于 SPEC-0184。
- 不实现 partial initialization recovery、异常 cleanup、用户 destructor、placement construction、
  receiver、delegation forwarding、Map 或 MutableList relocation。

## 5. 验收标准

- [ ] Copyable/MoveOnly field、payload 与 Box operand 分别产生 copy/move，MoveOnly 源的后续使用
      L0131，Borrow/Inout 来源非法 owned delivery 产生 L0133，primary/label 精确。
- [ ] 位置/命名参数的求值与 effect 始终按源码顺序，参数声明顺序只决定最终字段槽位；嵌套
      construction、`Nothing` operand 和较早移动影响较晚 operand 均有测试。
- [ ] construction result 的 local/temporary/return/再次 Value delivery/分支/loop 正常路径 owner
      与 ASAP drop facts 精确；MoveOnly 字段每条正常路径恰好一个最终 drop obligation。
- [ ] 无 payload enum、全 Copyable value/enum 不获得虚假 drop；class/Box handle 只产生一个
      root owner，字段不变成可独立部分移动状态。
- [ ] Phase 3 fixtures、ownership 白盒窄测和 workspace 五项标准基线通过；Architecture/Roadmap/
      Spec 同步，production 文件遵守 1000 行软上限。

## 6. 技术方案与边界

- 在 ownership checker 建立 constructor 专项入口，把 descriptor arguments 规范化为既有
  Value delivery primitive，并复用 `PlaceState`、copyability、loan 与 drop-point 数据流。
- construction result 使用 expression identity 作为 temporary owner key；字段 drop 仍附属于
  root owner，不扩张为普通源码 place 的部分移动状态。
- 产物发布 source expression → ordered delivery/drop plan，供 SPEC-0184 直接消费。

## 7. 实施计划

1. [ ] 建立 construction ownership plan/model → 验证：foreign/invalid descriptor 内部边界。
2. [ ] 接入 Value copy/move、result owner 与 ASAP drop → 验证：领域正反矩阵和诊断 Span。
3. [ ] 接入递归字段/payload drop facts与 Phase 3 fixtures → 验证：路径/顺序/确定性白盒测试。
4. [ ] 同步 Architecture/Roadmap/Spec，运行 workspace 基线 → 验证：全部实际退出码为 0。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | construction ownership model/checking、fixtures 与完成文档 | `feat(frontend): check construction ownership (SPEC-0188)` |

## 9. 未决问题

- 无设计留白；版本门禁与 SPEC-0183 前置未解除。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 等待前置 | SPEC-0029 已具备 Value delivery/ASAP drop 基元；v0.29/0183 尚未解除门禁 |
