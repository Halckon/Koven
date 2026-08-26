# SPEC-0180：instance receiver typed facts

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-180` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 v0.32；候选 [v0.34 §34](../guide/01-design-decisions.md#34-显式-instance-receiver-契约与静态-member-调用v034-候选未启用) |
| 批准依据 | 无；v0.34 尚未启用 |
| 前置 Spec | SPEC-0020、0067、0176、0177、0201；除 0201 外均 `done` |
| 前置 ADR | 无 |
| 阻塞项 | v0.34 启用；SPEC-0201 `done` |
| 影响范围 | `lang-frontend` name/type checking、receiver/member/delegation model、L0152；Architecture/Roadmap |
| 语言语义变更 | 否；发布候选 guide 已定义的 typed facts |

## 1. Goal

完成后，每个 instance callable、`this`、显式/隐式 member call 与 Borrow-only interface
delegate forwarder 都具有唯一、实例化后的 receiver typed identity，Phase 3 不再从 AST 名称
或函数体推断 receiver mode。

## 2. 范围与需求

- 把缺省/显式 Borrow、Inout、Value 规范化为隐藏 first receiver contract；owner type arguments
  先替换，callable type arguments 后替换，并纳入 stable callable instance key。
- receiver mode 不参与 overload shape，但 interface replacement、override/default/`super<I>`
  contract 精确比较；object 只接受 Borrow，companion/top-level 不产生 receiver。
- 为 `this`、裸 field/member 与显式 `receiver.member(...)` 发布静态 owner/target；局部/参数遮蔽
  优先，显式 `this` 仍能选择 member。成功 member call 不残留 MemberAccess/Call deferred。
- 扩展 `CallDescriptor` 记录 receiver expression或 implicit-this identity、mode、category、实例化
  receiver type、静态 callable target 与显式参数映射；receiver 和 argument facts 原子提交并
  纳入 overload/lambda trial rollback。
- 把 interface-level `DelegationPlan` 展开为源码有序的 Borrow-receiver forwarder descriptor；
  手写 override/default 解析后仍需转发 Inout/Value requirement 时在 `by` 处产生 L0152。
- typed 产物提供声明/使用 Span、receiver place origin 与后续 ownership/codegen 所需的稳定查询，
  不暴露 LLVM 类型。

## 3. 非目标

- 不检查 receiver loan、move、drop、capture 或 mutable-place 可用性。
- 不生成隐藏 AST、SSA/LLVM forwarder、vtable、proxy 或动态 interface value。
- 不实现 safe call、bound callable reference、extension method、borrow-return 或 iteration provider。

## 4. 验收标准

- [ ] 缺省与显式 Borrow facts 等价；Inout/Value、companion/顶层限制通过，object Inout/Value
  以 receiver marker 为 primary 稳定拒绝。
- [ ] class/value/enum/interface default/override/`super<I>` 的 owner+callable 泛型替换与静态 target 精确。
- [ ] 裸/显式 `this`、局部遮蔽、member overload、receiver-before-arguments descriptor 精确且 trial 不泄漏。
- [ ] receiver mode 不形成 overload，L0099/L0100 contract mismatch 稳定；L0152 primary 为
  `by`/delegate target，label 指向首个仍需转发的不兼容 member。
- [ ] Borrow-only delegate forwarder descriptor 与手写等价签名一致；非 Borrow requirement 不发布半成品。
- [ ] frontend 窄测试和 workspace 五项基线通过，Architecture/Roadmap 同步。

## 5. 技术方案与边界

在 callable/member model 中增加职责单一的 receiver/forwarder descriptor；复用现有 member
candidate、argument mapping、generic instantiation 与 `TrialState`，不创建第二套 overload
solver。`this` 使用 callable-local receiver identity，不伪装成普通源码 parameter symbol。

## 6. 实施计划

1. [ ] 规范化声明 receiver 并检查 contract → 验证：member/interface typed tests。
2. [ ] 扩展 member selection/CallDescriptor/implicit-this → 验证：target/instance/trial 白盒矩阵。
3. [ ] 发布 Borrow-only delegate forwarder 与 L0152 → 验证：delegation 正反矩阵。
4. [ ] 同步 Architecture/Spec 并运行 workspace 基线。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | receiver contract、member call 与 delegate typed facts | `feat(frontend): type member receivers (SPEC-0180)` |

## 8. 未决问题

- 无；所有权与 lowering 分别由 SPEC-0181/0191 承接。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 receiver 审计 | 通过 | member target 已可选择，但现有 CallDescriptor 不含 receiver，ownership 明确保留 deferred |
