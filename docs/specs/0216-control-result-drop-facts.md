# SPEC-0216：MoveOnly control result 析构事实

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-216` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 v0.32；§18 control expression、§26 ASAP 析构 |
| 批准依据 | 当前持续 Goal“继续推进 guide 和分阶段实施 specs，简化验收流程”的站立授权 |
| 前置 Spec | SPEC-0029、0197、0198、0215 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0009 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` compilation-unit liveness/drop planner；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 1. Goal

完成后，validated compilation-unit ownership 在 MoveOnly `if` / `when` 的 Consume 上下文中把每条
正常分支的 tail owner 转交 control result，并只析构未转交的 branch-local owner 与复合 operand，
使后续 SSA 无需忽略或重写 frontend drop facts。

## 2. 背景

现行 unit drop planner 已把外层 control expression 标记为 Consume，但 `ControlBody` 最后一个
expression 仍通过普通 statement 路径固定按 Read 规划。因此 String branch tail 会产生错误的
`AfterExpression` drop，SPEC-0199 不能安全建立 merge result。SPEC-0215 为 lambda body 建立独立
规划后仍必须对该表面原子回滚；本 Spec 修正 control-body usage 传播，而不改变 guide 语义。

## 3. 范围与需求

- MoveOnly `if` / `when` result 的每条正常 branch tail 始终按内部 Consume 交付 merged result；父级
  Read/Consume/Place 只作用于该 merged temporary，不能原样下传。Copyable result tail 仍按 Read。
- unit ownership checker 与 drop planner 使用同一 normalized branch usage；Consume tail 的 MoveOnly
  temporary 或 named place 不生成结果 drop，复合 operand 继续在既有 `AfterBinaryOperands` point
  逆求值顺序析构。
- 未转交的 branch-local owner 仍按既有 ASAP point 清理，外围 alternative owner 仍按 `BranchExit`
  与合流 liveness 处理；显式 return/divergence 沿用既有 `ControlTransfer`。
- nested `if` / `when` 递归传播同一 usage；事实与 source-qualified identity 不依赖输入顺序。

## 4. 非目标

- 不实现 SSA/LLVM/native lowering；由 SPEC-0199 后继切片消费。
- 不定义 MoveOnly Value lambda parameter entry drop、顺序 `for` provider 或 loop exit-qualified facts。
- 不改变单文件 legacy ownership API、公开诊断、语言 guide、runtime 或 ABI。

## 5. 验收标准

- [x] expression-body、local initializer 与 lambda tail 中的 MoveOnly `if` / `when` 结果均不被 branch
  `AfterExpression` 提前析构；Read 父级只析构 merged result，Borrow 父级只在 CallReturn 析构一次。
- [x] String concat branch operand 按 `AfterBinaryOperands` 逆序析构；named tail owner 转交，未选中的
  alternative owner 按对应 `BranchExit` 析构。
- [x] nested control、显式 return/`Nothing` divergence、受支持 sibling lambda 与输入置换事实稳定。
- [x] checker 与 drop planner 对 named tail 的 Consume 一致；后续使用产生既有 use-after-move 诊断，
  有诊断时不发布矛盾 drop plan。
- [x] control-result affected target 窄测与 workspace Clippy 通过；按路线图简化流程，后续切片默认不
  重跑耗时的 `lang-frontend` 全量。
- [x] Architecture 与 Spec 路线图同步为实现事实。

## 6. 技术方案与边界

main dataflow 与 unit drop planner 各自使用职责对应的 `ControlBody` helper：prefix 复用普通 statement
规则，最后一个 expression 使用由 control result copyability 规范化的 branch usage；MoveOnly 为内部
Consume，Copyable 为 Read。直接 expression branch 与有 braces 的 `ControlBody` 共用该规则，父级
Place 不穿透 result 边界。`if` / `when` 各分支继续独立复制状态并沿既有 merge 规则合流；不新增公开
fact kind，也不从类型名称或 AST 文本推断 owner。`Nothing` call 不形成正常 branch exit 或 CallReturn
drop。SPEC-0215 的临时回滚门禁随完整 control plan 落地后移除。

## 7. 实施计划

1. [x] 传播 control tail Consume 并移除 lambda control-result 回滚 → 验证：if/when 精确 drop 窄测。
2. [x] 锁定 named/composite/nested/divergence 与输入置换 → 验证：compilation-unit ownership 矩阵。
3. [x] 同步 Architecture/Roadmap 与验收记录 → 验证：文档、实现和测试一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | control result Consume/drop facts、测试与完成文档 | `feat(frontend): plan control result drops (SPEC-0216)` |

## 9. 未决问题

- 无；Phase 4 消费边界由 SPEC-0199 保持 fail-loud，直到其独立提交完成。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-30 实施前审计 | 通过 | 现行 guide 已定义 Consume/ASAP 语义；现有错误来自 `ControlBody` tail 固定 Read，前置事实均已完成 |
| `cargo test -q -p lang-frontend --test multifile_ownership_checking control_result` | 通过 | 2 个 control-result 正反矩阵通过；覆盖 expression/local/lambda、Read/Borrow/Consume、nested `Nothing` 与输入置换 |
| `cargo test -q -p lang-frontend --locked --offline` | 通过 | 本切片已取得一次性 affected-crate 全量证据；按用户更新的简化流程，后续不作为默认门禁 |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | 通过 | workspace 严格静态检查通过 |
| 独立 fresh-context 复审 | 通过 | 原 checker/drop usage、Place 穿透、nested `Nothing` 与 Clippy finding 均修复；最终无 P1/P2 |
