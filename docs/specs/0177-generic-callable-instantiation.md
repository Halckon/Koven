# SPEC-0177: 泛型 callable 实例化与实例 identity

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-177` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.27](../guide/00-index.md)；实施候选为尚未启用的 [v0.28 §28](../guide/01-design-decisions.md#28-泛型-callable-实例化与-overload-lambda-隔离v028-候选未启用) |
| 批准依据 | 当前持续 Goal 对 Spec 有站立授权，但新 guide 不在授权范围内；v0.28 明确启用后才能批准 |
| 前置 Spec | SPEC-0020、SPEC-0067、SPEC-0022、SPEC-0032 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 用户尚未明确启用 v0.28；候选 L0140–L0141 不是现行诊断 |
| 影响范围 | `lang-frontend` callable candidate/typed model、泛型 bound 查询、Phase 2 fixture、Architecture |
| 语言语义变更 | 否；启用后只实施 v0.28 封闭的泛型调用契约 |

## 1. Goal

完成后，Phase 2 能为源码顶层和实例 member 泛型 callable 解析完整显式类型实参，或只从已
定型的非 lambda 实参得到唯一结构化推导，并为每个成功调用发布后续单态化可消费的稳定实例
identity、已替换参数与返回类型。

## 2. 背景

SPEC-0020 已保存 callable 与 classifier 类型参数及 bound，SPEC-0067 已完成单态候选映射，
但 `checker/callable.rs` 遇到显式类型实参或任一 generic candidate 时会把整个调用保留为
`DeferredReason::Call`。Phase 4 的 typed SSA 不能从该状态恢复具体参数/返回类型，也不能
建立单态实例 key，因此本 Spec 是进入 SPEC-0033 前的类型层前置。

## 3. 范围与需求

- 只处理源码具名顶层函数和非 safe 实例 member 函数；receiver 的 classifier 替换先于
  callable 自身类型参数替换。
- 显式类型实参必须完整且 arity 精确；完全省略时只从已定型非 lambda 实参按 v0.28 §28 的
  invariant 结构规则提取候选替换。
- 对每个候选独立执行映射、推导、替换、assignability 和 interface / `Copyable` /
  `Transferable` bound 验证；trial 失败不得泄漏候选局部诊断。
- 扩展成功 call 产物，保存静态 target、owner 参数在前/callable 参数在后的有序实例实参、
  已替换参数和返回类型；同一 target + 类型序列形成同一规范实例 key。
- 唯一 generic candidate 推导不完整/冲突使用 L0140；`Transferable` bound 失败使用 L0141；
  显式 arity、interface bound、`Copyable` bound 分别复用 L0091、L0093、L0115。
- 所有权检查继续只消费最终 `CallDescriptor` 的参数 mode/type；不得把候选 trial 或未替换的
  type parameter 当作成功调用。

## 4. 非目标

- 不实现 overload lambda candidate isolation；由 SPEC-0174 承接。
- 不从返回 expected type、lambda body、后续使用或跨文件信息推导类型实参。
- 不实现部分类型实参、`_`、默认类型实参、型变、隐式转换或 Kotlin 完整约束求解。
- 不实现 constructor、safe call、callable reference、外部泛型环境签名、body clone、SSA、
  单态实例图展开或无限实例增长诊断。

## 5. 验收标准

- [ ] 显式 `identity<Int>(1)` 与实参推导 `identity(1)` 产生相同 source target、实例实参和返回类型。
- [ ] member callable 同时替换 owner 与 method 类型参数，实例 key 顺序稳定且重复检查确定。
- [ ] nullable、function、nominal、intrinsic 嵌套位置的结构推导有正例；缺失、重复冲突和禁止
  的返回上下文推导产生 L0140，并断言 primary / declaration label。
- [ ] interface、`Copyable`、`Transferable` bound 具有 compile-pass 与分别对应
  L0093/L0115/L0141 的 compile-fail/Span。
- [ ] overload 候选的推导/bound 失败只淘汰候选；零/多匹配仍使用 L0123/L0124，失败 trial
  不污染 expression/call facts。
- [ ] Phase 2 pass/fail fixture、`type_callable` 领域测试与 workspace 标准基线通过。
- [ ] Architecture、guide roadmap、Spec 状态与实际实现同步。

## 6. 技术方案与边界

- 在 `type_checking::call` 扩展公开只读实例 descriptor；候选替换与推导仍收敛在
  `checker/callable.rs`，复用现有 `substitute_type`、bound descriptor 和规范化 `TypeTable`。
- 推导结果使用按 `SymbolId` 排序语义明确的映射，实例产物按声明顺序转换成 `Vec<TypeId>`；
  不依赖 hash 迭代或类型名称文本。
- 非 lambda operand 仍只建立一次基础类型事实；候选层只读取该事实提取约束和验证替换，
  不重复执行源码表达式的可观察语义。
- L0140–L0141 只有 v0.28 启用后才加入生产诊断 catalog。

## 7. 实施计划

1. [ ] 扩展 callable instance model 与 L0140–L0141 → 验证：model/diagnostic 窄测。
2. [ ] 实现显式类型实参、结构推导与三类 bound → 验证：`type_callable` 正反例。
3. [ ] 接入 overload 淘汰、member owner 替换与确定性 instance key → 验证：领域矩阵及重复运行。
4. [ ] 补 Phase 2 fixture并运行 workspace 标准基线 → 验证：实际退出状态。
5. [ ] 同步 Architecture、guide、Spec 验收并检查 staged diff → 验证：提交仅含 SPEC-0177。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | instance model、显式类型实参与结构推导 | `feat(frontend): instantiate generic callables (SPEC-0177)` |
| 2 | bound/overload 集成、fixture、Architecture 与 done 验收 | `feat(frontend): validate generic call instances (SPEC-0177)` |

## 9. 未决问题

- v0.28 尚未明确启用；启用前不得改变生产代码或诊断 catalog。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-24 现状审计 | 通过 | 确认 generic call 当前统一保留 `DeferredReason::Call`，现有模型已保存 callable/owner 类型参数与三类 bound |
| 实现验收 | 未执行 | 等待 v0.28 明确启用 |
