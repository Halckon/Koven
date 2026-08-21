# SPEC-0104: 强化 Pratt 运算符矩阵前端产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-104` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25 运算符层级](../guide/03-grammar-core.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0007、SPEC-0072、SPEC-0093–0103 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Pratt operator matrix、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只强化既有优先级、结合性与不结合诊断验收 |

## 1. Goal

完成后，240 个 Pratt 运算符 case 均执行两次生产 Lexer 与两次公开 expression Parser，既有
局部 AST 语义断言之外还验证完整产物结构和确定性；不结合 case 不再以过滤方式掩盖额外诊断。

## 2. 范围与需求

- 保持 110 个跨层双向组合、36 个 postfix / prefix / cast 组合、54 个结合性组合和 40 个
  不结合组组合不变，测试内现有闭式计数断言继续生效。
- 240 个源码分别执行两次生产 Lexer，共验收 480 个 Lexer 产物；逐次验证 source identity、
  连续完整 byte 覆盖、末尾唯一 EOF、diagnostic primary / label Span 与完整公开产物确定性。
- 240 个源码分别执行两次公开 expression Parser，共验收 480 个 Parser 产物；逐次验证
  source identity、四张 AST table、diagnostic primary / label Span 与可解析 typed root。
- 200 个合法结构 case 的两次 Lexer / Parser 产物均保持零诊断，原有根与关键子树断言不变。
- 40 个不结合 case 的完整合并诊断序列必须恰好一个 `L0012`，primary Span 精确指向第二个
  运算符；不得只过滤目标 code 而忽略额外级联诊断。
- 同一源码两次完整 Parser `Debug` 产物必须一致；不增加 corpus、依赖、生产 API 或语义。

## 3. 非目标

- 不改变运算符集合、优先级、结合性、TypeRef 歧义、AST 或诊断。
- 不复制生产 binding-power 数值，也不扩大矩阵语料。
- 不替代 expression 领域测试对全部 payload、恢复和资源预算的精确验收。

## 4. 验收标准

- [x] 四组 case 计数保持 110 / 36 / 54 / 40，总计 240。
- [x] 480 个 Lexer 产物全部满足完整覆盖、EOF、source、diagnostic Span 与确定性不变量。
- [x] 480 个 Parser 产物全部满足 AST、diagnostic、typed root 与确定性不变量。
- [x] 200 个合法结构 case 两阶段零诊断且既有 AST 断言保持通过。
- [x] 40 个不结合 case 的完整诊断序列均恰好一个精确 Span 的 `L0012`。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

矩阵复用 SPEC-0103 的 `lex_source_twice` 与共享 Lexer 产物验证器；局部 `parse_case` 对同一
lexed file 执行两次 `parse_expression`，逐次验证 AST / diagnostic / root 后比较完整 `Debug`。
合法 wrapper 要求零诊断；不结合分支读取完整诊断切片并断言唯一 `L0012`。

## 6. 实施计划

1. [x] 审计 Pratt operator matrix → 验证：确认单次运行、产物验证缺失与诊断过滤缺口。
2. [x] 强化双 Lexer / 双 Parser 及完整不结合诊断 → 验证：240 个源码、960 个产物通过。
3. [x] 运行直接相关窄验收 → 验证：58/58，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0104`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Pratt 双运行产物与完整诊断不变量、Architecture 与完成记录 | `test(frontend): strengthen Pratt operator invariants (SPEC-0104)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_operator_matrix --test parser_expression --locked --offline` | 通过 | 58/58；240 个源码、960 个前端产物 |
| 同一组两个 integration tests 的窄 Clippy | 通过 | `-D warnings`；0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
