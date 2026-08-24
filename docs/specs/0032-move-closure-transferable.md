# SPEC-0032：move closure 与 Transferable 检查

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| Goal ID | `KOV-P3-032` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.27](../guide/00-index.md)：[简化 closure capture 与跨线程转移](../guide/01-design-decisions.md#27-简化-closure-capture-与跨线程转移v027) |
| 批准依据 | 当前持续 Goal 的站立授权适用，但不能替代语言 guide 门禁 |
| 前置 Spec | SPEC-0020、SPEC-0029 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无；v0.27 已明确启用并封闭 capture mode、逃逸、`this` 边界、完整 `Transferable` 域与跨线程 callable effect identity |
| 影响范围 | `lang-frontend` lambda capture facts、ownership state/drop、`Transferable` 推导、跨线程 call effect、Phase 3 fixture、Architecture |
| 语言语义变更 | 否；本 Spec 只能实施后续明确启用的 guide，不能自行补齐当前留白 |

## 2. Goal

在后续 guide 封闭捕获语义后，让 Phase 3 对每个 lambda 发布确定的 capture ownership facts，
在形成 move closure 时执行对应 copy/move/borrow 效果，并只在具有编译器绑定跨线程 effect 的
调用边界检查 capture/value 的结构化 `Transferable` 能力。

## 3. 范围与需求

- 从名称解析的稳定 scope/reference/symbol identity 计算 lambda 的直接与嵌套 capture 集，
  不按标识符文本或 Rust 的隐式规则猜测。
- 按启用后的 guide 区分默认 lambda 与 `move { ... }` 的 capture mode、owner 状态变化、loan
  生命周期和 closure drop；捕获事实保留 lambda、symbol、类型、模式与来源 `Span`。
- 对现行 guide 最终封闭的完整 `TypeKind` 域结构化推导 `Transferable`，并检查源码类型参数
  `Transferable` 上界；同名源码类型不能冒充编译器能力。
- 由 typed callable target 发布跨线程 effect identity；`thread` / `Sender.send` 等只有在其
  预声明 identity 明确绑定该 effect 时才检查，不按函数名或仅凭 `move (...) -> T` 猜测。
- 新增稳定诊断以区分非法 capture、borrowed closure 逃逸（若 guide 采用此规则）和
  non-`Transferable` 跨线程交付，并提供 capture/声明关联 label。
- 所有权诊断存在时不发布可供 Phase 4 使用的有效 capture/drop plan；输出保持确定性。

## 4. 非目标

- 不在本 Spec 决定下方 §9 的语言语义门禁，也不把 Rust closure trait、完整 NLL 或隐式生命周期
  系统移植进 Koven。
- 不实现 closure environment layout、调用 ABI、线程/channel runtime 或 LLVM lowering；这些
  分别属于 SPEC-0038 与 Phase 5 runtime/API 工作。
- 不实现 `Shareable`、跨线程共享引用、async/coroutine、borrow-return 或用户生命周期语法。
- 不实现 Map、Phase 5 容器 relocation API、instance/delegation receiver 或 callable reference。

## 5. 验收标准

- [ ] 默认与 move lambda 的无捕获、Copyable capture、MoveOnly capture、borrowed parameter
      capture 和 nested capture 正反例与启用后的 guide 完全一致。
- [ ] capture facts 可按 lambda/source symbol 查询 mode、类型与 `Span`；重复检查顺序确定。
- [ ] capture 建立后的 owner move/copy、use-after-move、loan 与 ASAP closure/capture drop facts
      由 compile-pass/fail 和领域断言锁定。
- [ ] `Transferable` 覆盖 guide 封闭的 builtin、nullable、value/class/enum、Box、Rc、顺序容器、
      type parameter、function/closure、Any/error/deferred 矩阵，并检查泛型实参上界。
- [ ] 编译器绑定的跨线程 callable 拒绝 non-`Transferable` 值/capture；普通同签名函数和源码
      同名 `thread` 不获得特殊规则。
- [ ] 新诊断的 code、primary/label `Span`、去级联和 Phase 3 pass/fail fixture 均有真实测试。
- [ ] 既有 callable、ownership、copyability、container 测试与 workspace 标准基线通过；
      Architecture/roadmap/Spec 只陈述实际事实。

## 6. 技术方案与边界

- 复用 `NameResolution::scopes/references/symbols`、typed expression/symbol type、function
  `move_only` identity 与现有 ownership Available/Moved/loan/drop 模型，不重新解析源码。
- capture 分析与 `Transferable` 递归各自保持单一职责；跨线程 effect 由 typed call descriptor
  显式携带，ownership checker 不读取 callee 拼写。
- 递归能力查询使用稳定的 nominal/type-argument substitution 与灰/黑集合，不能因环或 deferred
  类型 panic；输出使用 `Vec` / `BTreeMap` 保持确定性。

## 7. 实施计划

1. [ ] 启用封闭下方 §9 未决语义的新 guide，解除本 Spec 门禁 → 验证：索引、正文、changelog 一致。
2. [x] 建立 capture descriptor 与 `Transferability` 公开事实 → 验证：model/query 窄测。
3. [ ] 实现 lambda formation 的 move/copy/loan/drop 效果 → 验证：capture ownership matrix。
4. [ ] 实现 typed cross-thread effect 与能力检查 → 验证：identity/Transferable matrix。
5. [ ] 增加诊断、fixture、确定性与相邻回归 → 验证：frontend 受影响测试批次。
6. [ ] 同步 Architecture/roadmap/验收并运行 workspace 基线 → 验证：实际退出状态。
7. [ ] 暂存并审查本 Spec 独立 diff → 验证：无跨 Spec 或无关改动。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | capture/Transferable facts、检查、诊断、测试与事实文档 | `feat(frontend): check move closure transferability (SPEC-0032)` |

## 9. 未决问题

- 无。原七项门禁已由用户明确启用的 v0.27 §27 一次封闭；本 Spec 不再保留自行解释空间。

## 10. 验证记录

- 2026-08-23：完成 v0.26、roadmap、typed/name/ownership facts 的只读审计；确认七项门禁，
  保持 `draft`，未修改生产代码、未运行实现验收。
- 2026-08-24：用户明确启用 v0.27 与推荐的简化 capture 模型，七项门禁由 §27 解除；本 Spec
  依站立授权进入 `approved`，实现验收尚未执行。
- 2026-08-24：完成第一实现检查点：按 scope/reference/SymbolId 发布 shared/owned
  borrow/copy/move capture facts，将字段引用规范化为 `this`，并对当前完整 `TypeKind` 域公开
  独立于 `Copyability` 的结构化 `Transferability` 查询；新增 4 个领域测试，
  `ownership_closures`、既有 30 个 ownership 相邻测试及 frontend Clippy `-D warnings` 通过。
