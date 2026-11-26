# SPEC-0487：F4c Formatter 全 AST 往返差分扩展

> **性质**：交付合同 · **状态**：done（历史归档） · **读取时机**：实施或验收 F4c 全 AST 格式化往返差分扩展时 · **唯一真源**：本页验收账本

| 字段 | 值 |
|---|---|
| 状态 | done |
| 所属 Phase | Phase 1 测试；不改变语言行为 |
| 语言规范 | [Guide v0.47](../../guide/README.md) |
| 基线 | `main@5ea16bb8` |
| 分支 | `feature/spec-0487-f4c-formatter-difftest-expansion` |

本 Spec 承接 [SPEC-0477](0477-parser-formatter-difftest.md) 与 [F4c 实施计划](../../proposals/parser-formal-verification/difftest.md#4-后续引入顺序)，
保留 `formatting/roundtrip.rs` 首片契约不变，在同一已接入双宿主 `Targeted Tests` 的 `formatting` 测试目标中新增全 AST 往返差分模块
`crates/lang-frontend/tests/formatting/full_roundtrip.rs`。不修改生产 Parser、Formatter、Guide 或 CI，不新增 Cargo 依赖。

## 交付合同

1. **全 AST 规范化投影与逐臂 `Span`  containment 核验**：[full_roundtrip.rs](../../../crates/lang-frontend/tests/formatting/full_roundtrip.rs)
   覆盖全部合法 `PackageHeader`、`ImportDirective`（含 `.*` 与 `as`）、`Item`（`Modified`/`Variable`/`Constant`/`Function`/`Classifier`/`Companion`/`Deinit`）、
   `Statement`（`Block`/`LambdaBody`/`ControlBody`/`LocalVariable`/`LocalDestructuring`/`While`/`For`/`Loop`/`Expression`）、
   `Expression`（全部字面量、字符串插值、lambda、`if`/`when`、跳转、`super<T>.m`、前缀/中缀/转换/类型测试/赋值/成员/调用/索引/`!!`/`?`/`::`）与
   `TypeRef`（限定类型、泛型实参、可空、函数类型、`borrow`/`carrier` 返回来源）；两臂独立核验每个节点的 `SourceId`、UTF-8 边界与父子 `Span` 包含关系，
   遇任意 `Error`/`Missing` 恢复节点显式失败。
2. **全量完整文件语料往返**：`full_ast_roundtrip_covers_every_complete_file_pass_fixture` 自动遍历 `tests/fixtures/` 下全部
   `is_complete_file_pass` 完整文件正例（含 `parser-file-pass`、`name-pass`、`type-pass`、`ownership-pass`、`structural-pass`、`closure-pass`、
   `formatter-pass`），验证每份文件格式化前后全 AST 投影严格一致且二次格式化幂等。
3. **SPEC-0479 与 F3c/F3d/F3e 专项往返**：`full_ast_roundtrip_preserves_spec_0479_and_f3c_f3d_f3e_constructs`
   验证显式 `block { ... }`、同行与跨行/注释分隔尾 lambda、`when` 复合条件、`({ ... })`、`move` lambda/类型、具名位运算、字符串插值、
   `by` 委托、`deinit`、`private companion`、`enum class` 父类型与成员以及 `borrow T from x` 在格式化往返后 AST 严格不变。
4. **独立故障负对照**：`full_projection_fault_controls_detect_semantic_mutations_and_ignore_offsets` 验证前导注释位移不改变投影，
   同时单项语义变异（可见性、委托目标、接收者模式、借用返回来源、可空 `?`、空安全 `?.`、`as` vs `as?`、二元运算符、尾 lambda 脱离与非法裸块）均能被精确路径检出。

## 本地运行

```bash
cargo test --locked -p lang-frontend --test formatting
```

## 检查点

- [x] 全 AST 投影器、逐臂 `Span` containment 校验与全量 complete-file 语料/SPEC-0479 往返实现
- [x] 独立故障负对照与本地门禁（fmt / clippy / test / check_docs / check_rust_sizes）验收
- [x] PR 双宿主 CI 验收与归档

**归档记录。** 已随 PR #458（合并提交 `f50e90e2`） 合入 `main` 并完成有界交付，本批次按维护者确认收口归档至 `docs/archive/specs/`。
