# SPEC-0489：F2a 窄普通括号恢复原型评估与停止/反例契约见证

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：评估 F2a 普通括号恢复原型是否进入生产重写或核对 F2 停止条件时 · **唯一真源**：本页评估账本与反例见证合同

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 恢复评估与测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@5ea16bb8` |
| 分支 | `feature/spec-0489-f2a-paired-paren-prototype` |

本 Spec 承接 [F0a 返工归因检查点](../../proposals/parser-formal-verification/recovery-baseline.md) 与
[F0b 观察与恢复验收基线 §3](../../proposals/parser-formal-verification/observation-baseline.md#3-f2a-普通括号候选现有机制已挡住哪些反例)
中登记的最窄 F2a 候选评估（已证实配对的普通括号错误尾恢复，复用 `LexicalRecoveryIndex`，不改 `LexedFile` 公开形态，不新增公开 owner 树，不启动 F2b–f 多家族迁移）。

## 1. 调用边审计与停止条件结论

对 `crates/lang-frontend/src/parser/engine/` 全部普通括号恢复路径进行逐点审计：

1. **唯一调用点**：`parse_group`（`crates/lang-frontend/src/parser/engine/expression.rs`）是 `skip_to_group_closer` 与 `skip_to_confirmed_group_closer` 的唯一调用方；`GroupClosers`（`crates/lang-frontend/src/parser/engine/recovery/ordinary_groups.rs`）已按需惰性构建单遍线性配对表（`closer`、`enclosing`）与 4 项边界水位（`local`、`caller_else`、`caller_block`、`block_statement_separator`）。
2. **零可缩减边界传递点**：`parse_group` 内 `outer_stops` 参与 6 处上下文判定（内层 `group_stops`、`preserve_block_separator`、`Stops::ELSE`、`recover_poison_before_else`、`skip_to_confirmed_group_closer`、缺闭符 `boundary_span`）。即使将已配对区间封装为私有视图，`confirmed_closer` 仍必须依赖动态调用方上下文 `outer_stops`（例如物理配对组内部嵌套 `[ else next ]` 时，是否允许跳过完全取决于 `outer_stops` 是否含 `Stops::ELSE`），可数调用方边界传递点缩减为 **0**。
3. **前置区间切片违反强制反例**：若在解析内层表达式前就将游标限制在物理配对区间 `opener..closer`，会直接破坏反例 2（`f((value, next)` 唯一 `)` 属于 caller）与反例 3（物理配对组含游标后的文件级声明 `fun saved` 或 caller `else`）。

**正式结论（触发 F2a 停止条件）**：保持生产 Parser 与 `LexedFile` 零改动，不强行包装冗余 owner 区间视图，**不启动 F2b–f 多家族迁移**；将四类强制反例与多组退栈线性工作量/事件计数锁定为独立可执行契约见证。

## 2. 交付合同

1. **内部边界与多组退栈线性工作量见证**：[separator_tests.rs](../../../crates/lang-frontend/src/parser/engine/separator_tests.rs) 新增：
   - `f2a_confirmed_closer_depends_on_enclosing_pairing_and_caller_stops`：直接对 `Parser::skip_to_confirmed_group_closer` 验证反例 1（局部配对跳转）、反例 2（唯一 `)` 拒绝跳转）及反例 3（嵌套 `[ else next ]` 在含/不含 `Stops::ELSE` 时的严格分化，直接证明 `outer_stops` 不可被纯词法配对替代）。
   - `f2a_multi_group_unwind_and_long_poison_keep_linear_visits_and_events`：验证反例 4（32 vs 64 层未闭合普通括号向同一词法 poison 或 caller 逗号退栈），同时断言 `significant_raw_visits`、`declaration_recovery_raw_visits` 与 `declaration_recovery_event_queries_and_applications` 均保持线性增幅。
2. **公开入口四类反例集成见证矩阵**：[parser_f2a_paired_paren_witnesses.rs](../../../crates/lang-frontend/tests/parser_f2a_paired_paren_witnesses.rs) 通过 `parse_expression`、`parse_file`、`parse_block` 端到端锁定反例 1–4 的 AST 结构、诊断码/Span、后继保留（`next`、`fun saved`、`else`、`val saved`/`val later`、`fun sentinel`）及确定性（重复解析完全一致）。
3. **基线账本同步**：更新 `recovery-baseline.md` 与 `observation-baseline.md` §3 指向本 Spec 的评估与停止结论。

## 3. 本地运行

```bash
cargo test --locked -p lang-frontend --lib parser::engine::separator_tests::
cargo test --locked -p lang-frontend --test parser_f2a_paired_paren_witnesses
cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_block_line_continuation --test parser_index_recovery \
  --test parser_standalone_poison_stress_matrix --test parser_token_omission_matrix
```

## 4. 检查点

- [x] 调用边审计确认触发 F2a 停止条件（生产 Parser / `LexedFile` 零改动）
- [x] 四类强制反例与多组退栈线性工作量/事件计数见证落地
- [x] 本地门禁（fmt / clippy / test / check_docs / check_rust_sizes）实跑验收
- [ ] PR CI 验收与归档
