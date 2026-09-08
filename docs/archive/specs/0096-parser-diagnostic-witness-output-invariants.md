# SPEC-0096: 强化 Parser diagnostic witness 产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-096` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0014、SPEC-0076、SPEC-0093–0095 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser diagnostic witness 集成测试、Architecture |
| 语言语义变更 | 否；只强化公开产物验收 |

## 1. Goal

完成后，69 个 Parser diagnostic witness 除码集合、主 Span 与确定性外，还会逐次验证 Lexer
完整覆盖、AST 全表、诊断 label Span、typed root / 文件 roots 及文件 directive Span，避免
由同样错误的两份 `Debug` 产物形成伪确定性证据。

## 2. 范围与需求

- 保持 SPEC-0076 的 69 个 Lexer-clean witness、四个公开入口和 138 次解析不变。
- 每例验证 Lexer source identity、连续 byte 覆盖、唯一末尾 EOF 与 diagnostic Span。
- 对两次 Parser 产物分别验证 source identity、AST 全表 Span、diagnostic 主 / label Span。
- expression、declaration、block 入口的 typed root 必须能从对应 arena 解引用；file 入口的
  全部 roots 以及 package / import / segment / wildcard / alias Span 必须有效。
- 两次产物都必须恰好发出一次目标码且不发已退役 `L0016`，随后比较完整公开产物确定性。
- 不增加 witness、依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不改变诊断编号、消息、触发条件、Parser 恢复策略或 guide。
- 不把 witness 扩展为全部错误上下文的穷举矩阵。
- 不替代领域测试对精确 AST payload 和特定诊断 Span 的语义断言。

## 4. 验收标准

- [x] 69 个 Lexer 产物均满足 source / coverage / EOF / diagnostic Span 不变量。
- [x] 138 次 Parser 产物均满足 AST / diagnostic Span 与入口 root 不变量。
- [x] file witness 的 roots 与所有实际 directive Span 均有效。
- [x] 两次解析都恰好发出目标码且不发 `L0016`，公开产物确定一致。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

复用 `frontend_output_assertions` 验证通用 Lexer / AST / diagnostic 不变量；入口适配器在生成
现有 fingerprint 前验证 typed root 或文件产物。静态 witness 表与生产诊断集合对齐逻辑保持
不变，不访问 Parser 私有状态。

## 6. 实施计划

1. [x] 审计 diagnostic witness 现有证据 → 验证：确认缺少 Lexer 覆盖、label Span、AST、
   typed root / file roots 与 directive Span 验证。
2. [x] 强化两次解析的结构化产物断言 → 验证：69 case、138 次解析全部通过。
3. [x] 运行直接相关窄验收 → 验证：1/1，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0096`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | diagnostic witness 产物不变量、Architecture 与完成记录 | `test(frontend): strengthen parser diagnostic witness invariants (SPEC-0096)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_diagnostic_witness_matrix --locked --offline` | 通过 | 1/1；69 个 witness、138 次解析 |
| `cargo clippy -p lang-frontend --test parser_diagnostic_witness_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
