# SPEC-0486：四入口统一观察适配器与恢复/边界契约矩阵（F0b）

> **性质**：交付合同 · **状态**：in-progress · **读取时机**：实施或验收四入口统一观察分类与恢复契约时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| 所属 Phase | Phase 1 测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@d6976cf0` |
| 分支 | `feature/spec-0486-parser-observation-adapter` |

本 Spec 承接 [F0b 结果分类与恢复验收基线](../../proposals/parser-formal-verification/observation-baseline.md) 中登记的两项未实现缺口：
四入口统一观察适配器（含 `parse_file` 的 foreign-`SourceMap` `InternalFailure` 见证）以及 `parse_file` 对抗性确定性/产物完整性/R4 扫描。
不修改生产 Parser/Lexer、公开 API、Guide 或 CI，不新增 Cargo 依赖，默认随 `cargo test` 在双宿主运行。

## 交付合同

1. **四入口四分类观察适配器与零隐蔽错误节点合同**：[parser_observation_adapter.rs](../../../crates/lang-frontend/tests/parser_observation_adapter.rs)
   统一封装 `parse_file`、`parse_expression`、`parse_declaration`、`parse_block`，按 `Accept` / `SyntaxError` / `ResourceLimit` /
   `InternalFailure` 分类；对零诊断 `Ok` 产物递归遍历全部可达 AST 节点、标记与 `Span`，若发现任何可达 `Error`/`Missing` 恢复节点或越界 `Span`
   则直接判定合同失配失败。
2. **补齐 `observation-baseline.md` 两项显式缺口**：
   - `all_four_entries_reject_foreign_source_map_as_internal_failure`：补齐全部四入口（含 `parse_file`）的 foreign-`SourceMap` `InternalFailure` 见证；
   - `all_four_entries_classify_nesting_overflow_as_resource_limit`：验证全部四入口超深嵌套均精确归类为 `ResourceLimit { limit: 1024 }`；
   - `parse_file_adversarial_corpus_satisfies_determinism_integrity_and_r4_r5`：以 128 组对抗性完整文件输入（含词法 poison、保留字、未闭合字符串/注释、裸块、残缺类头与后继哨兵声明）验证 `parse_file` 确定性、零静默错误节点与 R4（相同主 `Span` 不重复报告词法错误）。
3. **故障负对照**：`fault_controls_detect_undiagnosed_recovery_nodes_and_misclassification` 证明适配器能拦截零诊断却含恢复节点的合同失配及四类结果混淆。

## 本地运行

```bash
cargo test --locked -p lang-frontend --test parser_observation_adapter
```

## 检查点

- [x] 四入口统一观察适配器、foreign-SourceMap / ResourceLimit 见证与 128 组 `parse_file` 对抗矩阵实现
- [x] 本地门禁（fmt / clippy / test / check_docs / check_rust_sizes）实跑验收
- [ ] PR CI 验收与归档
