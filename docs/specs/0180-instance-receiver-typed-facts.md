# SPEC-0180：instance receiver typed facts

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P2-180` |
| 所属 Phase | Phase 2 |
| 语言规范 | 现行 [v0.34 §34](../guide/01-design-decisions.md#34-显式-instance-receiver-契约与静态分发调用v034) |
| 批准依据 | 2026-08-31 用户明确要求按 Phase 2→3→4 继续实施 v0.34 receiver Specs |
| 前置 Spec | SPEC-0020、0067、0176、0177、0201 `done` |
| 前置 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` name/type checking、receiver/member/delegation model、L0152；Architecture/Roadmap |
| 语言语义变更 | 否；发布候选 guide 已定义的 typed facts |

## 1. Goal

完成后，每个 instance callable、`this`、显式/隐式 member call 与 Borrow-only interface
delegate forwarder 都具有唯一、实例化后的 receiver typed identity，Phase 3 不再从 AST 名称
或函数体推断 receiver mode。

## 2. 范围与需求

- 把缺省/显式 Borrow、Inout、Value 规范化为隐藏 first receiver contract；owner type arguments
  先替换，callable type arguments 后替换，并纳入 stable callable instance key。
- interface callable template 的 receiver type 使用 `StaticSelf(interface instance)`；具体 member/
  `super<I>` call descriptor 继续保存实际 concrete receiver type，不把 interface 物化为 runtime type。
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

- [x] 缺省与显式 Borrow facts 等价；Inout/Value、companion/顶层限制通过，object Inout/Value
  以 receiver marker 为 primary 稳定拒绝。
- [x] class/value/enum/interface default/override/`super<I>` 的 owner+callable 泛型替换与静态 target 精确。
- [x] 裸/显式 `this`、局部遮蔽、member overload、receiver-before-arguments descriptor 精确且 trial 不泄漏。
- [x] receiver mode 不形成 overload，L0099/L0100 contract mismatch 稳定；L0152 primary 为
  `by`/delegate target，label 指向首个仍需转发的不兼容 member。
- [x] Borrow-only delegate forwarder descriptor 与手写等价签名一致；非 Borrow requirement 不发布半成品。
- [x] 受影响 frontend Layer 1 测试及 workspace library Layer 2 静态门禁通过，Architecture/Roadmap
  同步；Layer 3 未升级，因为变更未涉及依赖/feature/发布矩阵且直接测试已覆盖跨 crate API。

## 5. 技术方案与边界

在 callable/member model 中增加职责单一的 receiver/forwarder descriptor；复用现有 member
candidate、argument mapping、generic instantiation 与 `TrialState`，不创建第二套 overload
solver。`this` 使用 callable-local receiver identity，不伪装成普通源码 parameter symbol。

## 6. 实施计划

1. [x] 规范化声明 receiver 并检查 contract → 验证：member/interface typed tests。
2. [x] 扩展 member selection/CallDescriptor/implicit-this → 验证：target/instance/trial 白盒矩阵。
3. [x] 发布 Borrow-only delegate forwarder 与 L0152 → 验证：delegation 正反矩阵。
4. [x] 同步 Architecture/Spec 并运行分层验收门禁。

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
| 2026-08-26 候选闭合审计 | 通过 | v0.34 明确直接基于 v0.32、不包含 v0.33（含后增 grammar §9/SPEC-0213/0214 与 §33）；typed Goal 仍严格等待 0201 与 guide 启用 |
| 2026-08-31 重基审计 | 通过 | v0.34 已重基到完整 v0.33；本 Spec 只剩 SPEC-0201 完成与独立批准门禁 |
| `cargo test -p lang-frontend --test type_checking` | 通过 | 51/51；receiver contract、super、implicit-this 与既有单文件类型矩阵 |
| `cargo test -p lang-frontend --test multifile_type_checking receiver -- --nocapture` | 通过 | 3/3；source-qualified receiver、object 与 delegation 正反矩阵 |
| delegation/source-order 与 visibility 定向回归 | 通过 | L0152 label 锁定首个源码 member；隐式 field projection 更新为 `This(owner)` |
| `cargo test -p lang-frontend --test ownership_structural` | 通过 | 4/4；显式 projection 的既有 ownership 适配无回归 |
| `cargo test -p lang-frontend --test multifile_type_checking` | 基线漂移 | 83/84；唯一失败为既有 `unit_lambda_diagnostics_stop_jumps_and_returns_at_callable_boundary` 旧诊断数量断言，与 receiver 路径无关 |
| `cargo clippy --workspace --lib -- -D warnings` | 通过 | Layer 2 workspace library 静态门禁，零 warning |
| `cargo check --workspace --lib` | 通过 | frontend 公共 receiver projection API 与 codegen/CLI library 下游兼容 |
| `cargo check --workspace --all-targets`、frontend `clippy --lib --tests` | 未升级 | 大型测试 target 长时间无输出后主动停止；直接 test target 已执行，按分层流程不重复作为门禁 |
| 独立复审 | 通过 | 三轮依次发现并关闭 super capability/qualifier/closure、implicit-this/object/generic shape、L0152 source-order 与 unit overload 旁路；最终确认无剩余 P1/P2 |
| `cargo test -p lang-frontend --test multifile_type_checking cross_file_member_bodies_calls_and_fields_publish_source_qualified_facts --locked --offline` | 通过 | 修正 interface callable receiver template 的事实漂移：signature 与 body `this` 都使用同一 `StaticSelf(interface)`，外部 call descriptor 仍为 concrete receiver |
| frontend 分层回归：`multifile_type_checking receiver` / `multifile_ownership_checking` | 4/4、51/51 通过 | 锁定 receiver/default/super/delegation typed contract 及全部 Phase 3 compilation-unit receiver ownership 消费；未运行约一小时的 frontend 全量测试 |
