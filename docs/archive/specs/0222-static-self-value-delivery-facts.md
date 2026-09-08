# SPEC-0222：StaticSelf Value receiver 条件交付事实

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-222` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.34 §34](../guides/v0.34-pre-restructure/01-design-decisions.md#34-显式-instance-receiver-契约与静态分发调用v034) |
| 批准依据 | 2026-09-01 持续 Goal 要求继续按 Phase 推进 guide 对应 Specs、简化验收；SPEC-0191 完成审计把该缺口隔离为 Phase 3 前置 |
| 前置 Spec | SPEC-0180、0181 `done` |
| 前置 ADR | 无 |
| 关联 Spec | SPEC-0191、0223 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` compilation-unit ownership facts 与窄测试；Architecture/Roadmap |
| 语言语义变更 | 否；只发布现行 Value receiver 规则的可验证所有权事实 |

## 1. Goal

完成后，Value interface default 中直接交付 `this`，或以隐式 `this` 调用另一 Value receiver 时，
validated compilation-unit ownership 产物会发布精确、条件化的 receiver Value delivery fact，供 Phase 4
在 concrete specialization 下决定 Copy/Move，而不会让后端从 AST 猜测交付来源。

## 2. 范围与需求

- 只处理 typed descriptor 已解析为 `StaticSelf` template、Value receiver 且 target 唯一的 direct member call。
- fact 保存 source unit、call expression、调用点、当前 receiver binding、selected target、原始 receiver
  template 与 origin；frontend 不保存尚不存在的 concrete specialization，也不提前决定 Copy/Move。
- conditional delivery 与现有 conditional receiver-drop fact 保留可交叉核对的 point/template identity；
  concrete specialization 下的互斥消费由 SPEC-0223 决定。
- `this` 显式交付与隐式 receiver call 共用同一状态转换，正常/提前 return 与 control merge 保持确定性。
- 任一 ownership error 或 deferred boundary 原子清空本 callable 的可执行 receiver delivery facts。

## 3. 非目标

- 不修改 SSA、LLVM、runtime ABI 或 drop glue；由 SPEC-0223 消费。
- 不开放 Borrow-return、部分移动、MoveOnly field read、nullable member form 或非 Borrow delegation。
- 不改变 receiver 语法、overload/static target 选择或 copyability 推导。

## 4. 验收标准

- [x] compile-pass 覆盖 MoveOnly/Copyable `StaticSelf` 的显式 `this` delivery 与隐式 Value receiver call。
- [x] conditional delivery 与 receiver-drop fact 的 point/template identity 可交叉核对，frontend 不发布
  concrete Copy/Move 结论。
- [x] Borrow/Inout receiver 不进入 conditional delivery；selected target 必须解析为当前 interface 或其传递
  父 interface 的 Value `StaticSelf` callable，target/template identity 不匹配及重复 fact 的内部 guard
  fail loud，Span 指向调用点。
- [x] ownership error 与 deferred boundary recovery 不发布可执行 conditional delivery，既有 receiver
  ownership 窄回归通过。
- [x] Architecture、Roadmap 与 SPEC-0223 前置状态同步；未运行 `lang-frontend` 全量测试。

## 5. 技术方案与边界

扩展现有 `UnitConditionalReceiverDropFact` 相邻的 receiver ownership 产物，以独立 conditional delivery
fact 表示“该调用可能向 callee 交付当前 receiver”；frontend 只核对 source/point/template identity，
concrete Copy/Move 与 drop 互斥由 SPEC-0223 在单态化后决定。事实沿用 compilation-unit validated gate。

## 6. 实施计划

1. [x] 建立显式/隐式 Value receiver 红测与现有 drop fact 对照 → 验证：唯一缺失事实明确。
2. [x] 发布、验证并恢复 conditional receiver delivery fact → 验证：identity、恢复与 Span 正反矩阵。
3. [x] 运行 ownership 窄回归并同步 Architecture/Spec → 验证：SPEC-0223 可只消费 validated fact。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | receiver delivery fact、验证与窄回归 | `feat(frontend): publish static-self value delivery facts (SPEC-0222)` |

## 8. 未决问题

- 无。Phase 4 operation 选择明确后置到 SPEC-0223。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test multifile_ownership_checking static_self_value_calls_publish_conditional_receiver_deliveries --locked --offline -- --test-threads=1` | 通过，1/1 | 覆盖显式/隐式、inherited/`super`、Borrow/Inout 排除与 concrete Copy/Move |
| `cargo test -p lang-frontend --test multifile_ownership_checking --locked --offline -- --test-threads=1` | 通过，56/56 | compilation-unit ownership 窄目标；未运行 `lang-frontend` 全量测试 |
| `cargo check --workspace --lib --locked --offline` | 通过 | workspace library 基线 |
| `cargo clippy --workspace --lib --locked --offline -- -D warnings` | 通过 | 无 warning |
| `cargo fmt --all -- --check`、`git diff --check` | 通过 | 格式与 patch whitespace |
| 独立 fresh-context 审查 | 通过 | 第三轮无 P1/P2；保留非阻塞 P3：无公开源码路径可伪造 unrelated target/重复 fact，生产 guard 已 fail loud |
