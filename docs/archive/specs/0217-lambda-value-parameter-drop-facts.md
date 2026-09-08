# SPEC-0217：lambda Value 参数入口析构事实

| 字段 | 值 |
|---|---|
| 状态 | `done` |
| Goal ID | `KOV-P3-217` |
| 所属 Phase | Phase 3 |
| 语言规范 | 现行 v0.32；§18 lambda 独立 callable 返回边界、§26 ASAP 析构 |
| 批准依据 | 当前持续 Goal“继续推进 guide 和分阶段实施 specs，简化验收流程和测试并行”的站立授权 |
| 前置 Spec | SPEC-0029、0032、0197、0198、0215、0216 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0009 `accepted` |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` compilation-unit liveness/drop planner；Architecture/Roadmap |
| 语言语义变更 | 否 |

## 1. Goal

完成后，validated compilation-unit ownership 能为 MoveOnly Value lambda 参数发布 source-qualified
entry/last-use/exit drop facts，使无用参数立即析构、读取参数在最后使用后析构、消费参数转交而不析构，
并解除 lambda body plan 的参数级门禁。

## 2. 背景

SPEC-0215 已为 lambda body 发布独立 liveness 与隐式结果 Consume，但遇到 MoveOnly Value 参数时会
跳过整个 body plan，因为现有 `FunctionEntry(UnitItemId)` 不能表达 lambda callable entry。SPEC-0199
因此仍必须拒绝 MoveOnly Value callable 参数。本 Spec 只补齐 Phase 3 source-qualified drop point 与
规划，不改变既有 Value 参数语义或 callable ABI。

## 3. 范围与需求

- liveness 为每个 lambda expression 保存独立 `live_in`，不把参数或 body-local symbol 泄漏到外层
  closure formation。
- 新增 lambda callable entry drop point；MoveOnly Value 参数在 body state 建立后按声明顺序进入，
  未进入 `live_in` 的参数按逆声明顺序在 entry 析构。
- 读取参数沿既有 last-use liveness 生成精确 `AfterExpression` / `CallReturn` 等 drop；传给
  Value、显式/隐式 return 或 control result 的参数按 Consume 转交，不生成重复 drop。
- Borrow 参数与 Copyable Value 参数不进入 owner state；body-local、capture、control result 与输入
  置换事实保持既有行为。

## 4. 非目标

- 不实现 SSA/LLVM/native lowering；由 SPEC-0199 后继切片消费。
- 不实现 `Inout` lambda 参数、nested lambda codegen、`for` 或新的 closure ABI。
- 不改变 guide、公开诊断、runtime 或单文件 legacy ownership API。

## 5. 验收标准

- [x] compile-pass 覆盖 unused/read/Consume/return 的 MoveOnly Value lambda 参数。
- [x] entry drop 使用 lambda expression identity，多个参数按逆声明顺序且输入置换后 facts 一致。
- [x] Borrow String 与 Copyable Value 参数不产生 entry owner drop；body composite/control drop 不回归。
- [x] 受影响的 `multifile_ownership_checking` lambda 窄测与 workspace Clippy 通过；不运行
  `lang-frontend` 全量测试。
- [x] Architecture 与 Spec 路线图同步为实现事实。

## 6. 技术方案与边界

`liveness::Liveness` 增加按 lambda `ExpressionId` 索引的 `lambda_live_in`；root traversal 仍不进入
lambda body，统一的第二遍以空 live-after 独立计算并保存结果。drop planner 为 lambda 建立与 body
同一 scope frame 的 MoveOnly Value 参数 state，先依据 `lambda_live_in` 生成 entry facts，再复用现有
statement/expression、control-transfer、last-use 与 tail Consume 规划。公开 `UnitDropPoint` 使用
source-qualified lambda expression，不把 lambda 伪装成顶层 `UnitItemId`。

## 7. 实施计划

1. [x] 发布 lambda live-in 与 entry drop point → 验证：unused 多参数精确 facts。
2. [x] 解除 body plan 参数门禁并锁定 Read/Consume/return → 验证：lambda ownership 窄矩阵。
3. [x] 同步 Spec 验收记录与 Architecture → 验证：文档和实现一致。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lambda Value 参数 entry/last-use/drop facts 与回归 | `feat(frontend): plan lambda parameter drops (SPEC-0217)` |

## 9. 未决问题

- 无；Phase 4 消费继续由 SPEC-0199 保持独立提交边界。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-30 实施前审计 | 通过 | guide 已定义 Value 参数所有权与 ASAP 析构；缺口仅为 lambda callable entry 的 source-qualified fact |
| `cargo test -q -p lang-frontend --test multifile_ownership_checking lambda_value_parameters_publish_entry_read_and_transfer_drop_facts` | 1 passed | unused 逆序 entry drop、Borrow read、Value call Consume、显式/隐式 return、Borrow/Copyable 排除与输入置换 |
| `cargo test -q -p lang-frontend --test multifile_ownership_checking lambda_` | 4 passed | 相关 lambda body/drop 矩阵；未运行 `lang-frontend` 全量 |
| `cargo test -q -p lang-codegen --lib unsupported_closure_surfaces_remain_atomic_boundaries` | 1 passed | 显式前置 gate 仍拒绝 MoveOnly Value lambda 参数，不按 body 形状意外放宽 Phase 4 |
| `cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` | passed | workspace 静态基线；后续仅补测试 fixture |
| `cargo clippy -p lang-frontend --test multifile_ownership_checking --locked --offline -- -D warnings` | passed | 最终 fixture 状态的定向静态检查 |
| `cargo fmt --all -- --check` / `git diff --check` | passed | 格式与空白错误检查 |
| 独立 fresh-context 评审 | clean | 基于显式 codegen gate 撤回 P1；补齐 Value-call Consume P2 后复审无剩余发现 |
