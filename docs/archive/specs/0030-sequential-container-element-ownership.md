# SPEC-0030：顺序容器 element place 所有权

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-030` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.26](../guides/v0.34-pre-restructure/00-index.md)：[顺序容器、内存表示与索引语义](../guides/v0.34-pre-restructure/01-design-decisions.md#8-顺序容器内存表示与索引语义array--list--mutablelist) |
| 批准依据 | 用户要求继续推进 guide 主线并分阶段实施 Specs；当前持续 Goal 的站立授权适用 |
| 前置 Spec | SPEC-0023、SPEC-0029 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无；Phase 5 mutator API 尚未定义，不进入本 Spec |
| 影响范围 | `lang-frontend` container construction / element place 所有权、loan/drop facts、L0136、Phase 3 fixture、Architecture |
| 语言语义变更 | 否；实施现行 guide 已封闭的 Phase 3 核心容器契约 |

## 2. Goal

让 Phase 3 消费既有顺序容器 typed facts：列表式构造按元素 `Copyability` 复制或移动；内建
`container[index]` 形成逻辑 element place，支持共享/独占调用期 loan、拒绝不可复制元素的
普通 owned 读取，并按 guide 固定顺序检查 replacement 和发布旧元素 drop fact。

## 3. 范围与需求

- 消费 `ContainerConstructionDescriptor` 的源码有序参数模式：列表式构造逐项 `Value`，
  runtime-length 构造的 `size` / `initializer` 为 `Borrow`，空 `MutableList` 无实参；不建立
  第二套构造或 callable 参数映射。
- 把有 typed descriptor 的内建 index 从 deferred 收敛为 element place。稳定 named receiver
  使用 root `SymbolId` + field path + 逻辑 index identity；不同已知整数索引可证明不重叠，
  相同或无法证明不同的索引保守视为可能重叠。temporary receiver 保留独立求值 identity。
- `Copyable` 元素普通读取或交给 `Value` 参数时取得 owned copy；`MoveOnly` 元素只能作为
  `Borrow` / 合法 `Inout` 实参，普通读取或 `Value` 交付产生 L0136，不移动 container、不留下洞。
- `Borrow` element loan 对三种容器合法；`Inout` element loan 只接受 Phase 2 已证明可变的
  `Array` / `MutableList` place。element loan 与 root/parent、同一或未知索引应用既有 L0135；
  不同已知索引允许并存。
- element assignment 先求值 receiver 和 index，再求值 RHS，最后确认 owner 仍有效并检查
  冲突；RHS 移动 receiver 产生既有 L0131。成功替换 MoveOnly 元素时发布提交后旧元素 drop fact，
  不把 replacement 当成整个容器重新赋值，也不恢复已移动 owner。
- temporary container 的 Copyable element read 在 index 完成后析构 container；element Borrow /
  Inout 则把 temporary owner 延长到同步 call return。整个 container owner 继续按普通 MoveOnly
  规则移动和 ASAP 析构。
- 成功 intrinsic index 不再发布 `IndexPlace` deferred；没有 typed element descriptor 的普通
  index、post-index field projection 和未来用户自定义索引仍保持明确 deferred。

## 4. 非目标

- 不预声明或实现 `MutableList.add` / `removeAt` / 重排等 Phase 5 API，也不按成员名猜测
  relocation effect。后续 API Spec 必须发布显式 mutation/relocation effect，并复用本 Spec
  的“任意有效 element loan 阻止 owner relocation/销毁”不变量。
- 不实现缓冲区、边界检查、allocator、元素实际 move/store/drop glue、异常清理或 LLVM；
  这些属于 Phase 4。
- 不实现 Map、自定义 indexing、slice、消费式迭代、借用返回、引用类型、完整 NLL、closure
  capture 或 `Transferable`。
- 不改变 Phase 2 的容器类型、构造推导、index mutability、L0125–L0130 或 callable 语义。

## 5. 验收标准

- [x] `listOf(endpoint)` 移动 MoveOnly place，随后使用产生 L0131；Copyable 元素构造不移动源。
- [x] Copyable element 普通读取 / Value 交付 compile-pass；MoveOnly element 普通读取 / Value
      交付产生精确 L0136，且 container 仍保持初始化。
- [x] 三种容器 element Borrow、Array/MutableList element Inout compile-pass；List Inout 继续由
      Phase 2 拒绝。loan facts 可查询 element index identity 与 call begin/end。
- [x] 同一、parent/element 和未知索引冲突产生 L0135；不同整数常量索引不误报；shared/shared
      保持合法，nested call 与 later-argument owner move 纳入同一源码顺序检查。
- [x] replacement 的 receiver/index/RHS/commit 顺序由测试锁定；RHS 移动 owner 产生 L0131，
      有效 loan 阻止重叠 replacement，成功 MoveOnly replacement 发布旧元素 drop fact。
- [x] temporary receiver 的 Copyable read 与 call-scoped element borrow 分别产生正确的
      AfterExpression / CallReturn container drop fact；moved owner 不重复 drop。
- [x] intrinsic element place 不再 deferred；未知 index 仍保守 alias，非 intrinsic index 保持
      `IndexPlace` deferred；重复检查的 diagnostics/loan/drop facts 确定。
- [x] Phase 3 pass/fail fixture 真实枚举 L0136 与相邻 L0131/L0135；容器类型、既有 ownership
      测试和一次 workspace 标准基线通过，Architecture/roadmap/Spec 只陈述实际事实。

## 6. 技术方案与边界

- 扩展 `OwnershipPlace` 的 terminal element identity，不复制 Phase 2 容器类型图。字段路径仍
  使用 `SymbolId`；element identity 只证明不同整数字面量不重叠，其余动态索引保守 alias。
- 前向 checker 继续维护 invocation-local loans；构造参数模式并入既有
  `calls_by_expression`，index operand 只求值一次，contract 在 operand 完成后建立 loan。
- liveness/drop planner 复用相同 call modes 和 place query；replacement old element 使用 AST
  identity 形成 Phase 4 可查询 fact。任何 ownership 诊断继续阻止有效 loan/drop plan 发布。
- 不新增依赖、crate、LLVM 类型或 runtime 假设；所有集合和输出顺序保持 `Vec` / `BTreeMap`
  确定性。

## 7. 实施计划

1. [x] 扩展 container call modes、element place identity 与公开 drop target → 验证：model/query 窄测。
2. [x] 实现 element read/loan/overlap、L0136 与 temporary owner lifetime → 验证：read/loan matrix。
3. [x] 实现 replacement 顺序、owner invalidation 与 old-element drop fact → 验证：replacement matrix。
4. [x] 补 Phase 3 fixture、deferred 和确定性回归 → 验证：frontend 受影响测试批次。
5. [x] 同步 Architecture/roadmap/验收并运行 workspace 标准基线 → 验证：实际退出状态。
6. [x] 暂存并审查本 Spec 独立 diff → 验证：无跨 Spec 或无关改动。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | element place、构造效果、loan/replacement/drop、测试与事实文档 | `feat(frontend): check container element ownership (SPEC-0030)` |

## 9. 未决问题

- 无。Phase 5 mutator/relocation API 的名称和签名尚未批准，已明确排除，不阻塞核心内建
  construction/index/replacement 的 Phase 3 实施。

## 10. 验证记录

- `cargo test -p lang-frontend --all-targets`：通过；无 ignored / filtered 用例。
- `cargo fmt --all -- --check`：通过。
- `cargo check --workspace --all-targets`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace --all-targets`：通过；无 ignored / filtered 用例。
- `cargo build -p lang-cli`：通过。
