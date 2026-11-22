# SPEC-0483：F3d 泛型调用、Lambda 与 SPEC-0479 独立参考解析差分（F3d→F4a）

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收 F3d 高风险构造与 SPEC-0479 独立参考解析差分时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@8207b212` |
| 分支 | `feature/spec-0483-f3d-reference-difftest` |

本 Spec 承接 [Parser 分阶段形式化验证计划](../../proposals/parser-formal-verification-plan.md) 的 **F3d→F4a 扩展切片**与
[F3d 冲突与外部谓词合同](../../proposals/parser-formal-verification/generic-lambda-grammar-slice.md)。
在 `SPEC-0479` 合入 `main@8207b212` 后，本片为 F3d 四类必选外部谓词建立不共享生产 Parser 内部逻辑的独立参考解析器差分测试，
不修改生产 Parser/Lexer、Guide 或 CI，不新增 Cargo 依赖，默认随 `cargo test` 在双宿主运行。

## 交付合同

1. **独立参考解析器与归一 AST 投影**：[parser_f3d_reference_diff.rs](../../../crates/lang-frontend/tests/parser_f3d_reference_diff.rs)
   消费仅含物理换行事件（`NL`）的词法流，独立实现 F3d + SPEC-0479 子集参考解析器并输出规范化 S-expression；
   生产 `parse_block` 独立验证所有节点 `Span` 包含关系后投影至同一 S-expression，比对接受性与完整结构树。
2. **四类 F3d 外部谓词全覆盖**：
   - 无副作用试探：`<...>` typed-call 提交（后接 `(` 或同行 `{`）与 `<` 比较回退，以及 lambda header 试探；拒绝 `f<T> move { x }` 与链式比较 `a < b > c`；
   - 上下文软关键字：`move { ... }` / `move\n{ ... }` vs 普通 `move(1)` / `obj.move()`，语句起点同行 `block { ... }` vs 表达式位置 `block { ... }`（`L0009`）与普通 `block(1)` / `obj.block { ... }`；
   - 换行谓词：`return` 操作数换行截断（LF / 含换行块注释）、闭合调用跨行与注释分隔尾 lambda 附着、`when` 复合条件前瞻（`{ -> true }() -> g()`、`&& flag -> g()`、`, other -> g()`）与合法附着分支；
   - 语句/表达式 `{ ... }` 分派：块内与 lambda 体内裸 `{ ... }` 拒绝（`L0030` / `L0032`），`({ ... })` 与 `move { ... }` 接受。
3. **故障负对照与组合矩阵**：`fault_controls_detect_f3d_structural_and_predicate_drift` 证明比较器可检出
   `nested-block`/`lambda` 混淆、尾 lambda 脱离、`return` 截断失效、typed-call/比较混淆与接受/拒绝漂移；
   `combinatorial_f3d_matrix_matches_reference` 覆盖 80 组调用/注释换行/尾 lambda 组合样本。

## 本地运行

```bash
cargo test --locked -p lang-frontend --test parser_f3d_reference_diff
```

## 检查点

- [x] F3d + SPEC-0479 独立参考解析器、故障负对照与 80 组组合矩阵实现
- [x] 本地门禁（fmt / clippy / test / check_docs / check_rust_sizes）实跑验收
- [ ] PR CI 验收与归档
