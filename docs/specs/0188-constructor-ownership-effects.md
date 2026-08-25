# SPEC-0188：构造 Value delivery 与所有权效果

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-188` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.29 §29](../guide/01-design-decisions.md#29-名义enum-case-与-intrinsic-box-构造v029) |
| 批准依据 | 用户于 2026-08-25 明确启用 v0.29；实现状态仍按本 Spec 推进 |
| 前置 Spec | SPEC-0029 `done`；SPEC-0183 `done` 后方可实施 |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) |
| 阻塞项 | 无；SPEC-0183 已完成 |
| 影响范围 | `lang-frontend` ownership checker/model、Phase 3 fixtures；Architecture、Roadmap |
| 语言语义变更 | 否；只消费已启用 guide 与 SPEC-0183 typed facts |

## 1. Goal

完成后，ownership checker 能把每个成功 construction 作为一次源码有序的 Value delivery 与
新 owner 建立动作，准确发布 copy/move delivery effect、construction temporary 与 root ASAP
drop obligation，并拒绝构造参数移动后的再次使用或从 non-owning binding 移出。

## 2. 背景

构造器字段/payload 虽已由 v0.26 定义为天然-owned Value 参数，但 SPEC-0029 只消费普通
`CallDescriptor`。把 constructor 直接塞进 Phase 4 会跳过 MoveOnly operand 失效、临时 owner
延寿与路径析构证明，因此在 SPEC-0183 typed facts 和 SPEC-0184 lowering 之间需要独立的
Phase 3 Goal。

## 3. 范围与需求

- 只消费成功且 owner 一致的 `ConstructionDescriptor`；descriptor 缺失、foreign analysis 或
  参数 identity 不一致是内部错误，不按源码名称重新选择 constructor。
- 在普通 `Name` / `Member` / `Call` ownership 分派前按 expression identity 拦截 construction；
  type/case callee 或 receiver 不求值，只按 descriptor 的 operand 序列检查实际运行时表达式。
- operand 始终按 descriptor 的 evaluation index 源码顺序各处理一次；每个 Value 参数对
  `Copyable` 建立 owned copy，对 MoveOnly place 转移 owner，对 temporary 直接交付。
- 较早参数移动/loan/drop 效果在较晚 operand 求值时已生效；命名参数映射不能重排所有权动作。
- construction 成功后建立 result owner：value/enum 是内联 owner，class/Box 是唯一 heap-owner
  handle。直接 return/继续 Value 交付可以转移该 owner；未转移的 local/temporary 在 SPEC-0029
  的最早安全点析构。
- 发布 source-ordered `ConstructionDeliveryEffect` 与 construction root 的唯一 drop obligation；
  field/payload 不形成独立源码 `DropFact` 或部分移动状态。SPEC-0184 按 descriptor 的完整单态
  result type 派生递归 drop glue。无 payload/全 Copyable 形态不制造虚假唯一析构义务；abort
  路径继续不 unwind，不发布部分构造 cleanup。
- 每个 `ConstructionDeliveryEffect` 保存 construction/argument/parameter identity、evaluation
  index 与 `Copy` / `Move` / `DeliverTemporary` kind。operand 为 `Nothing` 时计划在该 operand
  终止，不发布后续 delivery 或 result root obligation；存在所有权诊断时不发布半成品 plan。
- 非 owning Borrow/Inout 参数中的 MoveOnly 值构造交付复用 L0133；已移动 operand 后使用复用
  L0131；普通字段部分移动继续 L0132。成功结果与现有 loan/place 冲突使用既有 L0135。

## 4. 非目标

- 不选择 constructor、推导类型、映射参数或发类型诊断；这些属于 SPEC-0183。
- 不定义物理布局、分配、enum tag、LLVM drop glue 或 object/run；这些属于 SPEC-0184。
- 不实现 partial initialization recovery、异常 cleanup、用户 destructor、placement construction、
  receiver、delegation forwarding、Map 或 MutableList relocation。

## 5. 验收标准

- [x] Copyable/MoveOnly field、payload 与 Box operand 分别产生 copy/move，MoveOnly 源的后续使用
      L0131，Borrow/Inout 来源非法 owned delivery 产生 L0133，primary/label 精确。
- [x] 位置/命名参数的求值与 effect 始终按源码顺序，参数声明顺序只决定最终字段槽位；嵌套
      construction、`Nothing` operand 和较早移动影响较晚 operand 均有测试。
- [x] construction result 的 local/temporary/return/再次 Value delivery/分支/loop 正常路径 owner
      与 ASAP drop facts 精确；含 MoveOnly 字段的 root 每条正常路径恰好一个最终 drop obligation。
- [x] 无 payload enum、全 Copyable value/enum 不获得虚假 drop；class/Box handle 只产生一个
      root owner，字段不产生独立源码 DropFact 或可部分移动状态。
- [x] 白盒测试证明 payload `Call` 与无 payload `Name` / `Member` 都跳过 type/case callee 求值；
      position/named operand 的 ordered delivery effects 可由 SPEC-0184 直接消费。
- [x] Phase 3 fixtures、ownership 白盒窄测和 workspace 五项标准基线通过；Architecture/Roadmap/
      Spec 同步，production 文件遵守 1000 行软上限。

## 6. 技术方案与边界

- 在 `ownership_checking/checker/construction.rs` 建立 constructor 专项入口，把 descriptor
  arguments 规范化为既有 Value delivery primitive，并复用 `PlaceState`、copyability、loan 与
  drop-point 数据流；现有接近 1000 行软上限的 `checker.rs` 只保留 expression dispatch。
- 在 `ownership_checking/construction.rs` 保存 ordered effect/root plan model，并由现有门面最小
  re-export；不把新的独立职责继续堆入集中 `model.rs`。
- construction result 使用 expression identity 作为 temporary owner key；字段 drop 仍附属于
  root owner，不扩张为普通源码 place 的部分移动状态。
- 产物发布 source expression → ordered delivery effects + root drop plan，供 SPEC-0184 直接消费；
  recursive field/payload glue 不是 Phase 3 source fact。

## 7. 实施计划

1. [x] 建立 construction ownership plan/model → 验证：foreign/invalid descriptor 内部边界。
2. [x] 接入 Value copy/move、result owner 与 ASAP drop → 验证：领域正反矩阵和诊断 Span。
3. [x] 接入 root drop obligation 与 Phase 3 fixtures → 验证：路径/顺序/确定性白盒测试，确认
   不生成独立字段 DropFact。
4. [x] 同步 Architecture/Roadmap/Spec，运行 workspace 基线 → 验证：全部实际退出码为 0。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | construction ownership model/checking、fixtures 与完成文档 | `feat(frontend): check construction ownership (SPEC-0188)` |

## 9. 未决问题

- 无设计留白；版本门禁与 SPEC-0183 前置均已解除。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 通过 | v0.29 已生效，SPEC-0029/0183 `done`；现有 Value delivery、loan 与 ASAP drop 基元可直接复用，无 ADR 或新诊断前置 |
| `cargo test -p lang-frontend --test ownership_checking --test ownership_structural --test ownership_containers --test ownership_closures --test ownership_construction` | 通过 | 47 项 Phase 3 integration tests；construction 新增 6 项，覆盖 ordered effects、root obligation、诊断原子性、路径与 fixture |
| `cargo test -p lang-frontend ownership_checking::checker::construction::tests::invalid_argument_identity_is_an_internal_error` | 通过 | 生产单元测试锁定无效 descriptor 内部错误；其余 test binaries 以 filter 运行 0 项 |
| `cargo clippy -p lang-frontend --all-targets -- -D warnings` | 通过 | 无 warning；主 checker 保持 1000 行软上限，新职责位于独立 construction 模块 |
| 2026-08-25 workspace 五项标准基线 | 通过 | `cargo fmt --all -- --check`、workspace check、Clippy `-D warnings`、workspace all-target tests、`cargo build -p lang-cli` 均退出 0；codegen 101 passed / 1 个既有 LLDB 权限测试 ignored，frontend unit 40 passed，construction integration 6 passed |
