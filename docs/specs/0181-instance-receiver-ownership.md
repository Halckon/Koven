# SPEC-0181：instance receiver ownership

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-181` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 [v0.34 §34.2–34.3](../guide/01-design-decisions.md#342-调用顺序this-与所有权能力) |
| 批准依据 | 2026-08-31 持续 Goal 明确要求按 Phase 2→3→4 推进 v0.34 receiver 实施；SPEC-0180 已完成 |
| 前置 Spec | SPEC-0029、0032、0180 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0016](../adr/0016-interprocedural-borrow-abi.md) |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` ownership receiver/place/loan/drop/capture facts；Architecture/Roadmap |
| 语言语义变更 | 否；消费 SPEC-0180 typed facts |

## 1. Goal

完成后，一般 member call 和 member body 中的 `this` 按 Borrow/Inout/Value receiver contract
进入现有 loan、move、partial-move、capture 与 ASAP drop 模型，不再留下 MemberReceiver deferred。

## 2. 范围与需求

- receiver expression 先求值一次；在首个显式 argument 前建立 shared/exclusive loan或 Value
  copy/move，并让 receiver loan 覆盖全部 argument 求值和同步 call。
- Borrow temporary 延命到返回；Inout 要求 §26.3 的可独占 receiver place且拒绝 temporary；
  Value 对 Copyable 复制、对 MoveOnly 移动，后续使用沿用 L0131。
- `this` 按 callable receiver mode 形成 non-owning shared/exclusive binding 或 owned local；检查字段
  读写、reborrow、普通字段部分移动、正常退出 drop 与提前控制转移。
- Value `this` 与普通 Value 参数一样是不可变 owned root：允许读、shared reborrow 与整体
  Value 交付，拒绝字段写入、exclusive reborrow 和 Inout member call；需要修改时必须先整体
  移入显式 `var` local，并对原 `this` 形成普通 use-after-move 事实。
- Inout receiver loan 覆盖整个 owner place，`this` 不可重绑或完整替换。普通 class val handle
  可独占同一 handle storage并修改 payload `var` 字段，但不得写回另一 handle；内联 value/enum
  仍递归要求 mutable root。receiver 与显式 argument place 重叠统一使用 L0134/L0135。
- shared `this` capture、Inout/Value call、move closure 与 receiver owner 冲突复用 SPEC-0032；
  三种 receiver 的 `this` 都不能被 move closure 直接捕获。Value `this` 可先整体移入 local，
  再按 local 的 capture、`Transferable`、drop 与 use-after-move 规则处理。
- Borrow delegate forwarder 只 shared-borrow outer receiver 与 delegate field，转发结束后同时
  结束 loan，不生成 retain、隐藏 owner或字段 move。
- 发布 codegen 可直接消费的 receiver delivery/loan/end/drop facts；删除一般 member路径上的
  `OwnershipDeferredReason::MemberReceiver`，失败节点不发布成功 plan。

## 3. 非目标

- 不 lower SSA/LLVM，不实现非 Borrow interface delegation。
- 不实现 safe-call、callable reference、borrow-return、cross-thread borrow 或 iterator lifecycle。
- 不改变显式参数、字段 place、closure capture 或 drop 的既有诊断含义。

## 4. 验收标准

- [x] Borrow/Inout/Value receiver 的 stable place、temporary、Copyable/MoveOnly 正反矩阵通过。
- [x] receiver loan 与显式 arguments 的 evaluation order、重叠冲突和 call-return end 精确。
- [x] Borrow `this` 只读、Inout `this` 可写 `var` 字段；Value `this` 的读/shared/整体移动合法，
  字段写入与 Inout reborrow 被拒绝，Value→`var` local 后可按普通 mutable-root 规则继续。
- [x] class val handle 与 inline mutable-root 差异、L0131–L0135 primary/label 稳定。
- [x] verifier/facts 拒绝 Inout receiver 重绑或替换 handle；class payload field mutation保持同一 owner。
- [x] direct `this` move capture 使用 L0138；Value `this`→local→move closure 的
  `Transferable`/use-after-move/drop，以及 Borrow delegate 与手写转发 ownership facts 等价。
- [x] 无一般 MemberReceiver deferred；受影响 frontend 窄测试及 workspace Layer 2 静态门禁通过并同步 Architecture。

## 5. 技术方案与边界

扩展现有 call-loan planner，使 receiver 成为有独立 source order 的第一个 delivery operand；
复用 `RootPlace`、field projection、loan conflict、parameter binding 和 closure capture 状态机，
不创建 receiver 专用 alias checker。成功产物必须与 SPEC-0180 analysis owner identity 匹配。

## 6. 实施计划

1. [x] 接 receiver delivery/order/temporary → 验证：call ownership 窄矩阵。
2. [x] 接 member-body `this`、field/capture/drop → 验证：body/closure 正反矩阵。
3. [x] 接 Borrow delegate并移除 deferred → 验证：facts 等价与失败原子性。
4. [x] 同步 Architecture/Spec 并运行分层验收。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | receiver loan/move/drop/capture ownership facts | `feat(frontend): check receiver ownership (SPEC-0181)` |

## 8. 未决问题

- 无；native lowering 由 SPEC-0191 承接。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 receiver 审计 | 通过 | 现有 place/loan/capture 基元可复用；一般 member receiver 当前明确 deferred |
| 2026-08-26 候选闭合审计 | 通过 | Value `this` 固定为 owned-but-immutable；在本候选内，Value→`var` local 是取得 mutable root 的显式路径 |
| 2026-08-31 重基审计 | 通过 | v0.34 已重基到完整 v0.33；SPEC-0180 已完成，当前顺序实施授权进入 Phase 3 |
| `cargo test -p lang-frontend --test multifile_ownership_checking` | 47/47 通过 | 覆盖 receiver order/loan/move/temporary/this/drop/capture/delegation 与失败原子性 |
| `cargo test -p lang-frontend --test ownership_checking` | 15/15 通过 | 单文件显式 member receiver 不再发布一般 `MemberReceiver` deferred |
| `cargo clippy --workspace --lib -- -D warnings` | 通过 | Layer 2 workspace 库级静态门禁 |
| `cargo check --workspace --lib` | 通过 | Layer 2 workspace 库级构建门禁 |
| Layer 3 全 targets / 全量 frontend | 未升级 | 本变更未触发依赖、feature、target、CLI/protocol 或平台风险；按分层策略不运行约一小时的全量 frontend |
| 独立高风险复核 | 通过 | 发现并关闭 shared `this` capture 可用性/borrowed move 与 capture trial 半提交问题；最终无 P1/P2 |
