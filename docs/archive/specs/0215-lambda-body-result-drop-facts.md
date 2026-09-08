# SPEC-0215：lambda body 隐式结果析构事实

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-215` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 v0.32；§18 lambda 独立 callable 返回边界、§26 ASAP 析构 |
| 批准依据 | 当前持续 Goal“继续推进 guide 和分阶段实施 specs，简化验收流程”的站立授权 |
| 前置 Spec | SPEC-0029、0032、0197、0198 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0009 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` compilation-unit liveness/drop planner；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 1. Goal

完成后，validated compilation-unit ownership 能为无 MoveOnly Value 参数的 lambda body 发布隐式
MoveOnly 结果转移及其内部 owner 的精确 drop facts，使后续 SSA 无需猜测 lambda 退出语义。

## 2. 背景

SPEC-0199 已能把唯一直接 MoveOnly temporary 从 closure thunk 返回，但当前 unit drop planner 只把
lambda 当作 closure formation 表达式，不独立遍历 body。若 codegen 直接放宽 composite tail，String
operand temporary 将缺少 drop facts；因此必须先在 Phase 3 发布事实，再由 Phase 4 消费。

## 3. 范围与需求

- lambda body liveness 在独立 callable 边界内计算，不把 body-local symbol 泄漏到外层形成表达式。
- 最后一个 expression element 按隐式返回的 Consume 使用规划；结果 temporary 不生成
  `AfterExpression` drop。
- composite String tail 的 operand owner 继续按既有 `AfterBinaryOperands` 逆求值顺序析构。
- 事实顺序及 source-qualified identity 不依赖 compilation-unit 输入顺序。

## 4. 非目标

- 不定义或实现 MoveOnly Value lambda parameter 的 entry drop；该表面继续保持现有门禁。
- 不放宽一般 MoveOnly `if`/`when` result、`for` 或 codegen nested-closure surface。
- 不改变 guide 语义、公开诊断、closure ABI 或 runtime。

## 5. 验收标准

- [x] compile-pass 覆盖跨文件 lambda composite String tail。
- [x] tail result 不在 drop targets 中，两个 operand 在 `AfterBinaryOperands` 逆序析构。
- [x] MoveOnly `if` / `when` result 原子回滚整个 body plan，且不抑制受支持 sibling lambda。
- [x] 输入反序后的完整 unit drop facts 一致。
- [x] `lang-frontend` 全量测试与 workspace Clippy 通过。
- [x] Architecture 与 Spec 路线图同步为实现事实。

## 6. 技术方案与边界

root liveness traversal 只把 lambda capture source 加入外层 live set，不下降到 callable body；root
traversal 完成后再统一按 AST identity 枚举全部 lambda body，并以空 live-after 独立计算。drop planner
同样在完成顶层 callable 规划后按 AST identity 顺序规划受支持 lambda body：prefix 沿用普通 statement
规则，tail expression 使用 Consume，body-local state 不与外层 owner state 合并。含 MoveOnly Value
参数的 lambda 暂不发布 body plan，避免在缺少 lambda-entry drop point 时延迟析构。规划期间遇到
本 Spec 明确排除的 MoveOnly `if` / `when` result 时，原子回滚该 lambda body 已生成的全部 drop
facts；相邻 lambda 继续独立规划，不把不完整的分支 result 事实伪装成受支持 surface。

## 7. 实施计划

1. [x] 接独立 lambda body liveness 与 tail Consume drop plan → 验证：单一跨文件窄测。
2. [x] 锁定 operand/result/drop 顺序与输入置换 → 验证：精确 facts 断言。
3. [x] 同步 Spec 验收记录与 Architecture → 验证：文档和实现一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lambda body result/drop facts 与回归 | `feat(frontend): plan lambda result drops (SPEC-0215)` |

## 9. 未决问题

- MoveOnly Value lambda parameter 需要独立 lambda-entry drop point，留给后继切片。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test multifile_ownership_checking lambda_tail_consumes_result_and_drops_only_body_owned_inputs --locked --offline` | 1 passed | composite String tail result 转移、operand 逆序 drop 与输入置换 |
| `cargo test -p lang-frontend --test multifile_ownership_checking lambda_ --locked --offline` | 5 passed | lambda body、参数门禁及受支持/不支持 sibling 隔离 |
| `cargo test -p lang-frontend --test multifile_ownership_checking lambda_drop_planning_defers_move_only_control_results --locked --offline` | 1 passed | checkpoint 前 prefix facts 与 MoveOnly `if` / `when` body facts 原子回滚 |
| `cargo test -p lang-frontend --locked --offline` | passed | affected crate 最终全量，含 38 条 compilation-unit ownership integration tests 与 7 条 doc-tests |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | passed | 与 frontend 全量并行执行，独立进程结果 |
| 独立 fresh-context 评审 | passed | 修复 initializer lambda liveness P1、技术方案漂移及 checkpoint prefix 覆盖缺口后复审无新增问题 |
