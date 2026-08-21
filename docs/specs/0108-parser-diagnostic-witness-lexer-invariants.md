# SPEC-0108: 强化 Parser diagnostic-witness Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-108` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0006–0009、SPEC-0014、SPEC-0076、SPEC-0096、SPEC-0103–0107 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser diagnostic-witness integration test、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有 diagnostic witness 的 Lexer 重复产物验收 |

## 1. Goal

完成后，L0009–L0078 中 69 个现行 Parser 诊断 witness 均执行两次生产 Lexer 与两次对应公开
Parser 入口，统一证明两个阶段的完整公开产物结构与确定性。

## 2. 范围与需求

- 保持 69 个 Lexer-clean witness、expression / declaration / block / file 四入口、完整现行码
  集合以及退役 L0016 排除规则不变。
- 每个 witness 执行两次生产 Lexer，共验收 138 个 Lexer 产物；两次均验证 source identity、
  连续完整 byte 覆盖、唯一末尾 EOF、diagnostic primary / label Span、零诊断与完整 `Debug`。
- 每个 witness 继续执行两次对应 Parser，共验收 138 个 Parser 产物及其 AST、diagnostic、
  typed root / file roots、directive Span 与完整公开产物确定性。
- 两次 Parser 产物继续恰好发出一次目标码，且均不得发出退役 L0016。
- 普通用户输入不得导致 Lexer / Parser 内部错误或 panic。
- 双 Lexer 逻辑复用 SPEC-0103 的共享 helper，不增加 witness、依赖、生产 API 或语言语义。

## 3. 非目标

- 不改变诊断编号、消息、触发条件、Parser 恢复、AST 或 grammar。
- 不扩大 witness 为全部错误上下文的穷举矩阵。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 69 个 witness 与 L0009–L0078 现行码集合保持一一对应，L0016 继续排除。
- [x] 138 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span、零诊断与确定性不变量。
- [x] 138 个 Parser 产物继续满足 AST、diagnostic、root / directive 与确定性不变量。
- [x] 两次 Parser 均恰好发出目标码且不发 L0016。
- [x] 全部 69 个 witness 无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

矩阵 runner 先构造包含诊断码、入口和源码的 context，再复用 `lex_source_twice` 创建 source
map、执行并验证两次 Lexer；首个确定产物继续交给既有两次 `parse_fingerprint`。码集合、精确
目标码计数、typed root / file directive 与完整 Parser 产物比较保持不变。

## 6. 实施计划

1. [x] 审计 diagnostic-witness 矩阵 → 验证：确认 69 个 witness 的 Parser 已双运行，Lexer 仍为单次。
2. [x] 接入共享双 Lexer helper → 验证：69 个 witness、276 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：1/1，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0108`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | diagnostic-witness 双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen diagnostic witness lexer invariants (SPEC-0108)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_diagnostic_witness_matrix --locked --offline` | 通过 | 1/1；69 个 witness、276 个前端产物 |
| `cargo clippy -p lang-frontend --test parser_diagnostic_witness_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
