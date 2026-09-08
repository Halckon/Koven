# SPEC-0124: 强化错误传播 Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-124` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0016、SPEC-0063、SPEC-0093、SPEC-0103–0105、SPEC-0115–0123 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` postfix 错误传播 Parser integration test、共享测试产物断言、Architecture |
| 语言语义变更 | 否；只强化既有 postfix `?` AST、消歧、恢复与复杂度 suite 的重复产物验收 |

## 1. Goal

完成后，`parser_error_propagation` 的全部用户源码路径均在同一 source identity 上执行两次生产
Lexer 与两次对应公开 Parser，并在既有精确 AST / diagnostic 断言之前验证完整公开产物确定性；
该 suite 没有预期内部错误路径，因此测试文件不再直接调用 Lexer 或 Parser。

## 2. 范围与需求

- 保持现有 8 个 Rust 测试、全部源码 corpus、postfix 左结合、safe call / Elvis / nullable type
  消歧、callable body、恢复、source identity 与长链复杂度断言不变。
- `expression`、`block` 与完整文件三条正常路径分别调用现有 typed 双前端 helper。
- 每个 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、diagnostic
  primary / label Span 与完整 `Debug` 确定性。
- 每个 source 的两次 Parser 均验证 AST 与 diagnostic Span、对应 typed root，并比较完整 `Debug`
  产物；首个确定产物继续进入既有领域断言。
- 仅为现有 typed helper 补齐完整文件 wrapper，并把已经存在的 file-specific 产物校验提取为
  两个 helper 真实复用的单职责测试模块；不增加语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 postfix `?`、nullable type、safe call、Elvis 或 non-null assertion 的语法或优先级。
- 不改变 AST、诊断码、诊断顺序、恢复或测试期望。
- 不实现 Phase 2 的 `Result<T, E>` 类型约束或最近 callable 类型检查。
- 不在本 Spec 同时迁移其他 Parser feature targets。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 8 个错误传播 Parser 测试全部保持通过，无 ignored / filtered。
- [x] expression / block / file 的全部源码路径均经对应双前端 helper。
- [x] postfix 结构、精确 Span、诊断码、恢复、source identity 与复杂度断言保持通过。
- [x] 测试文件不再直接调用 `lex`、`parse_expression`、`parse_block` 或 `parse_file`。
- [x] 全部用户语法错误形成结构化产物，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_error_propagation` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

expression / block 路径直接组合 `parser_test_assertions` 已有 typed wrapper；同一 helper 新增
完整文件 typed wrapper，并与 `frontend_matrix_assertions` 共用新提取的 `file_output_assertions`，
由其承载原有 file-specific 校验逻辑。既有本地领域 helper 仍返回原有产物类型和 `SourceMap`，AST payload、
diagnostic code / Span、恢复与复杂度断言继续读取首个确定产物。

## 6. 实施计划

1. [x] 审计错误传播 suite → 验证：8 个测试；全部路径为成功产物，无内部错误例外。
2. [x] 接入三种双前端 helper → 验证：测试文件无直接 Lexer / Parser 调用。
3. [x] 运行直接相关窄验收 → 验证：8/8，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0124`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 错误传播 suite 三入口重复产物、Architecture 与完成记录 | `test(frontend): strengthen error propagation suite invariants (SPEC-0124)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_error_propagation --locked --offline` | 通过 | 8 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_error_propagation --locked --offline -- -D warnings` | 通过 | 0 warnings；初次组合两个 helper 暴露 `duplicate_mod`，统一模块树后复验通过 |
| `cargo clippy -p lang-frontend --test parser_operator_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings；证明共享 AST 断言的非 file 使用方无 `dead_code` 回归 |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
