# SPEC-0029：调用期借用与 ASAP 析构点

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.26](../guide/00-index.md)：[调用期借用与 ASAP 析构点](../guide/01-design-decisions.md#26-调用期借用与-asap-析构点v026) |
| 前置 Spec | SPEC-0176 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` ownership place / loan / liveness、L0133–L0135、Phase 3 fixture、Architecture |
| 语言语义变更 | 否；实施 v0.26 已封闭语义 |
| 批准依据 | 用户已明确启用 v0.26；SPEC-0176 已完成，当前持续 Goal 的站立授权适用 |

## 2. Goal

在不引入引用类型或完整 NLL 的前提下，让 Phase 3 按源码求值顺序执行显式 call argument 的
`Value` / `Borrow` / `Inout` 所有权效果，拒绝重叠 loan 与非法 borrowed move，并为仍由当前
callable 拥有的 `MoveOnly` 值输出确定、路径敏感的 ASAP drop facts。

## 3. 范围与需求

- 为可解析的名称/字段 place 建立稳定 root `SymbolId` + field path identity 和封闭 mutability；
  group 透明，index/receiver/capture 保持专用 deferred。
- 消费 SPEC-0176 规范化后的声明侧 `ParameterMode`：源码 `own` 对应 `Value` owned binding，
  无 marker / 显式 `borrow` 对应 `Borrow`，`Inout` 是 exclusive non-owning binding；按现行
  guide 检查 read/copy/move/reborrow/replacement 能力。
- 调用先检查 callee，再按 argument 源码顺序各求值一次；每个 operand 完成后立即执行已选
  contract。shared/exclusive loan 覆盖其后的 argument 求值、nested call 和 callee 动态期间。
- 对重叠 place 应用唯一冲突矩阵；不同稳定字段允许证明不重叠。失败不移动 owner、不生成
  有效 loan/drop plan，并抑制同根 L0131/L0132 级联。
- 对 owned `MoveOnly` local、temporary 与 `Value` parameter 计算 backward liveness，输出
  expression/control-flow edge 可查询的 drop facts；覆盖普通最后使用、未使用 binding、
  replacement、scope exit、return/`?`、break/continue、branch merge 与 loop backedge。
- temporary Borrow 延长到 call return；Value move 后源不 drop；Borrow/Inout parameter 退出
  callee 时不 drop；Copyable 不产生唯一 drop fact；abort 路径不生成 unwind cleanup。
- 新增 L0133–L0135，保持诊断和 typed/ownership facts 源码有序且可从公开产物查询。

## 4. 非目标

- 不决定 instance member 或接口委托的隐式 receiver mode，也不按方法名/函数体猜测。
- 不实现顺序容器 index place、元素替换或 relocation；这些属于 SPEC-0030。
- 不实现 closure capture、move closure 或 `Transferable`；这些属于 SPEC-0032。被 lambda 引用
  的外层 owner 保持 deferred，不生成提前 drop fact。
- 不实现 borrow-return、引用类型、用户生命周期、跨调用 loan、完整 NLL、部分移动或 codegen。
- 不迁移声明侧 `own`、borrow-default、函数类型、lambda expected contract、预声明 callable 或
  Tree-sitter 参数语法；这些属于 SPEC-0176。本 Spec 不新增其他语法、依赖、crate、LLVM 类型、
  日志框架或与本 Spec 无关的 assignment 类型规则。

## 5. 验收标准

- [ ] shared/shared 同 place、shared 不同字段、Copyable borrowed copy 与合法 Inout replacement
      compile-pass；loan begin/end 和参数 binding kind 可精确查询。
- [ ] shared/exclusive、exclusive/read、loan 后 move/drop、parent/field 前缀重叠和 nested-call
      冲突产生精确 L0135；不同字段不误报。
- [ ] 从 Borrow/Inout binding 移出 MoveOnly 值产生 L0133；`&val`、`&ValueParameter` 或不可变
      field path 产生 L0134，primary/label 精确且无 L0131/L0132 级联；`&temporary` 保持 L0122。
- [ ] unused、last-use、temporary borrow extension、replacement、normal/early scope exit、branch、
      loop 的 drop facts 与候选 guide 顺序一致；moved/Copyable/non-owning binding 不误生成 drop。
- [ ] index、member receiver、lambda capture 保持明确 deferred；已有 use-after-move、结构移动、
      callable 与容器类型测试全部回归通过。
- [ ] Phase 3 pass/fail fixture 被真实枚举；fail 至少断言 L0133–L0135 code、primary byte Span 与
      关键 label，零 fixture 失败。
- [ ] 受影响 frontend 窄测和一次 workspace 标准基线通过；Architecture、roadmap、本 Spec
      验收与验证记录只陈述实际事实。

## 6. 技术方案与边界

- 复用现有 `TypedCallArgument`、`AggregateProjectionDescriptor`、`Copyability` 与
  `DestructuringDescriptor`，不建立第二套参数映射或类型/字段图。
- 从当前 `ownership_checking::checker` 提取本次新增的 place/loan 与 liveness/drop 职责；
  前向检查维护 Available/Moved 和 invocation-local loans，后向分析只消费已验证控制流与
  ownership effects。二者通过封闭 facts 交换，不用隐式全局状态。
- place path、loan、drop point 与诊断聚合均使用 `Vec` / `BTreeMap` 和源码/edge identity；
  不依赖 `HashMap` 随机顺序。每个 AST edge 在前向与后向阶段各访问常数次。
- 所有权错误路径不输出可供 Phase 4 消费的有效 drop plan；恢复状态只用于继续发现独立错误。

## 7. 实施计划

1. [x] SPEC-0176 完成并解除实现门禁，把本 Spec 推进到 `approved` → 验证：typed parameter
       facts、Architecture、路线图和 Spec 状态一致。
2. [ ] 建立 parameter binding、place path、loan/drop 公开产物 → 验证：model 与 identity 单测。
3. [ ] 实现源码顺序 call effects、reborrow、mutability 与冲突诊断 → 验证：L0133–L0135 窄测。
4. [ ] 实现 owned-value liveness 和 ASAP drop facts → 验证：control-flow/drop matrix。
5. [ ] 补 Phase 3 fixture 与相邻回归 → 验证：一次受影响 frontend 测试批次。
6. [ ] 同步 Architecture/roadmap/验收并运行一次 workspace 基线 → 验证：实际退出状态。
7. [ ] 暂存本 Spec 独立范围并审查 staged diff → 验证：无跨 Spec 或无关改动。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | loan/drop model、检查、测试、Architecture 与完成记录 | `feat(frontend): check call loans and drop points (SPEC-0029)` |

## 9. 未决问题

- v0.26 已启用，SPEC-0176 前置已完成。receiver、index 与 capture 已明确拆分，不阻塞本 Spec
  的显式实参范围。

## 10. 验证记录

尚未实施；不得预填通过。
