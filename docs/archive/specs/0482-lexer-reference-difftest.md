# SPEC-0482：独立参考词法机差分验证（F3b）

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F3b 独立词法差分时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 测试 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@8207b212` |
| 分支 | `feature/spec-0482-lexer-reference-difftest` |

本 Spec 承接 [Parser 分阶段形式化验证计划](../../proposals/parser-formal-verification-plan.md) 的 **F3b** 阶段与
[F3b 词法输入合同](../../proposals/parser-formal-verification/lexical-input-baseline.md)。
F4a（`SPEC-0478`）共享生产 Lexer，不能替代独立词法验证；本片依据 [Guide 01](../../guide/01-lexical.md)
独立编写状态机参考词法器，不复用 `scanner.rs`，不修改生产 Lexer/Parser 或 Guide，不新增 Cargo 依赖。
双宿主 Targeted Tests 显式运行本 Spec 的 `lexer_reference_diff`。

## 交付合同

1. **独立参考词法状态机**：[lexer_reference_diff.rs](../../../crates/lang-frontend/tests/lexer_reference_diff.rs)
   按 Guide v0.47 §01 独立实现 `Normal` / `String` / `Interpolation { brace_depth }` 三模式状态机，
   覆盖全部 36 个硬关键字、11 个未来保留字、上下文软关键字、44 个固定符号与最长匹配（含 `as?`、`!in`/`!is` 边界）、
   十进制/十六进制/二进制整数与浮点及非法数字吸收、字符字面量、字符串与嵌套 `${...}` 插值及最内层 EOF 错误收敛，
   与生产 `lang_frontend::lexer::lex` 逐项比对完整 `LexemeKind` 序列、UTF-8 字节 `Span` 与诊断主 `Span`。
2. **故障负对照**：`fault_controls_detect_kind_span_and_diagnostic_drift` 证明比较器能检出
   `LexemeKind` 漂移、字节 `Span` 偏移、诊断 `Span` 变化与缺失 EOF。
3. **F3b §2 字节级见证与全族覆盖**：默认随 `cargo test -p lang-frontend --test lexer_reference_diff` 运行，
   覆盖 `lexical-input-baseline.md` 全部 11 项 trivia/换行/块注释见证、全部关键字/保留字/符号、数字/字符正反例、
   字符串插值与错误恢复用例，以及 252 组确定性组合样本（含多字节 UTF-8 `雪`）。

## 本地运行

```bash
cargo test --locked -p lang-frontend --test lexer_reference_diff
```

## 检查点

- [x] 独立参考词法状态机、故障负对照、F3b 字节级见证与 252 组组合差分实现
- [x] 本地门禁（fmt / clippy / test / check_docs / check_rust_sizes）实跑验收
- [x] PR CI 验收与归档

**归档记录。** 已随 PR #456（`5ea16bb8`）与 PR #457（`f3b9cc5a`） 合入 `main` 并完成有界交付，本批次按维护者确认收口归档至 `docs/archive/specs/`。
