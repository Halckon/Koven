# SPEC-0196：pointer-like nullable handle lowering

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `approved` |
| Goal ID | `KOV-P4-196` |
| 所属 Phase | Phase 2/3/4 纵向切片 |
| 语言规范 | 现行 [`guide/01-design-decisions.md` nullable、smart cast 与 §30.2](../guide/01-design-decisions.md) |
| 批准依据 | 当前持续 Goal“继续推进 guide 和分阶段实施 specs，先审计 roadmap，再根据依赖图推进”的站立授权 |
| 前置 Spec | SPEC-0045、0195 `done` |
| 前置 ADR | [ADR-0017](../adr/0017-nullable-handle-ssa-abi.md) `accepted` |
| 关联 ADR | ADR-0006、0008、0015、0016 |
| 阻塞项 | SPEC-0195 尚在实施 |
| 影响范围 | `lang-codegen` nullable SSA/verifier/LLVM、frontend flow lowering、native tests、Architecture/Roadmap |
| 语言语义变更 | 否；实施既有 nullable/smart-cast 与 Rc null-niche 语义 |

## 2. Goal

完成后，普通 class、Box 与 Rc 的 nullable handle 使用独立 typed SSA identity 和 LLVM null niche；
null 比较/smart cast、conditional drop 与 Rc 非空分支 `.share()` 可走真实 native 主线。

## 3. 范围与需求

- 新增 `NullableHandle<inner>` 及 wrap/null/is-null/take/non-null-view operations，ownership 继承
  pointer-like inner owner但不与其 SSA 类型合并。
- lowering 消费 frontend nullable/flow facts，建立 null/non-null CFG；smart cast 只产生绑定原 owner
  的 non-owning view，不隐式复制或 retain MoveOnly handle。
- nullable drop 在 null 分支无操作、非空分支调用 inner drop glue；Rc 仅非空 release-to-zero。
- LLVM 使用单 pointer null niche，无 tag、wrapper allocation或额外 retain；target preflight 与
  verifier-before-LLVM 正反测试覆盖。
- 源码/native 覆盖 null、非空 wrap、if/when smart cast、`!!` 的已批准子集和不同 drop 顺序。

## 4. 非目标

- 不实现 inline value/enum/function nullable ABI、safe-call、完整 Elvis/when nullable lowering。
- 不改变 nullable 类型推导、穷尽性或 smart-cast 语言规则。
- 不实现 Weak/Arc、GC 或 cycle collector。

## 5. 验收标准

- [ ] NullableHandle type/operation/render/type/ownership verifier 正反矩阵通过。
- [ ] null/non-null edge proof 阻止无证明 unwrap、owner 提前 move/drop与 view 逃逸。
- [ ] LLVM IR 对 class/Box/Rc 使用单 pointer niche，conditional drop 正确且无额外 allocation/tag。
- [ ] `Rc<T>?` null 与非空 native build/run 退出 0，非空 branch 可 share/read并最终只 free 一次。
- [ ] inline nullable 与未授权 control forms 明确拒绝，不发生 compiler panic。
- [ ] Architecture、Roadmap 与 workspace 标准基线同步。

## 6. 技术方案与边界

严格实施 ADR-0017。先建立通用 pointer-like nullable SSA/verifier，再接 frontend flow facts和 LLVM；
不得把 nullable Rc 作为 SharedOwner adapter 的字符串/指针特例。

## 7. 实施计划

1. [ ] 建立 NullableHandle、operation、non-null edge/view 与 verifier。
2. [ ] 接 frontend nullable/null comparison/smart-cast/drop facts。
3. [ ] 接 LLVM null niche、conditional drop 与 class/Box/Rc native 测试。
4. [ ] 同步 Architecture、Roadmap、验证记录与 workspace 基线。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | nullable handle SSA/verifier | `feat(codegen): verify nullable handles (SPEC-0196)` |
| 2 | frontend/LLVM/native 与完成文档 | `feat(codegen): lower nullable handles to native (SPEC-0196)` |

## 9. 未决问题

- 无。inline nullable 与其余 control forms 已明确排除并留待后续独立 Spec。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-26 前置审计 | 等待 | ADR-0017 已 accepted；等待 SPEC-0195 完成 internal reference/non-null view 基础 |
