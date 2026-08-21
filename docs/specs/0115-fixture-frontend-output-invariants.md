# SPEC-0115: 强化 fixture frontend 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-115` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0005–0017、SPEC-0062–0066、SPEC-0103–0114 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` fixture integration target、Architecture |
| 语言语义变更 | 否；只强化既有 pass / fail fixture 的 Lexer 与 Parser 重复产物验收 |

## 1. Goal

完成后，fixture harness 的每个 Lexer 或 Parser source case 都执行两次生产 Lexer；六类 Parser
suite 的每个 source case 还执行两次对应公开 Parser，并在既有 sidecar、typed root 与 suite
计数验收之前证明完整公开产物确定性。

## 2. 范围与需求

- 保持十五个固定 suite、发现 / 配对 / 零用例保护、sidecar 格式与 stable report 不变。
- 三个 checked-in Lexer fixture（1 pass / 2 fail）各执行两次 Lexer，共验证 6 个 Lexer 产物。
- 34 个 checked-in Parser fixture 按 expression 4、declaration 5、block 4、lambda 4、implicit-unit
  7、file 10 分组；每例执行两次 Lexer 与两次对应 Parser，共验证 68 个 Lexer 和 68 个 Parser
  产物。
- checked-in fixture 最低合计验收 74 个 Lexer 与 68 个 Parser 产物；临时 fixture harness 自检
  通过同一 source / parse helper 自动获得相同重复产物约束，不以固定临时 case 数作为公共契约。
- 两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF，以及 diagnostic primary /
  label Span 的 source-local 有界性；随后比较完整 `Debug` 产物。
- 两次 Parser 比较完整 `Debug` 产物；首个确定产物继续进入既有零诊断、sidecar 全序、typed root、
  AST table 与 `FunctionForm` 验收。
- 内部错误与重复产物漂移使用不同的 fixture failure variant，避免把确定性失败误报为生产入口错误。
- 不增加 fixture、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 fixture 发现、路径排序、sidecar 解析、诊断期望或现有语料。
- 不把仓库私有 sidecar 固定为公共诊断协议。
- 不重复运行 Phase 0 source-only fixture；它不调用生产 Lexer / Parser。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] checked-in 3 个 Lexer fixture 与 34 个 Parser fixture 的分组计数保持固定。
- [x] checked-in 74 个 Lexer 产物满足覆盖、EOF、source、diagnostic Span 与完整确定性不变量。
- [x] checked-in 68 个 Parser 产物满足完整公开产物确定性。
- [x] 既有 sidecar、diagnostic order、typed root、AST table、stable report 与零用例保护保持通过。
- [x] 临时 pass / fail fixture 自检继续通过共享重复产物 helper。
- [x] 全部 fixture 用户输入无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `fixtures` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`parser_fixture_source` 与 `lex_fixture_source` 在同一 `SourceMap` / `SourceId` 上运行两次 Lexer，
逐次复用扩展后的 `validate_lexemes`，比较完整 `Debug` 后返回首次产物。泛型
`parse_fixture_twice` 接收四个公开 Parser 函数之一，执行两次并比较完整 `Debug`，让十二个
pass / fail runner 调用点共享同一确定性边界，同时保留各 suite 的领域断言。

## 6. 实施计划

1. [x] 审计 fixture harness → 验证：37 个 checked-in frontend source 单 Lexer，34 个 Parser source 单 Parser。
2. [x] 接入双 Lexer / 双 Parser helper → 验证：checked-in 142 个前端产物及动态 harness 自检通过。
3. [x] 运行直接相关窄验收 → 验证：23/23，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0115`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | fixture 双 Lexer / 双 Parser、Architecture 与完成记录 | `test(frontend): strengthen fixture frontend invariants (SPEC-0115)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test fixtures --locked --offline` | 通过 | 23/23；checked-in 37 个 frontend source、142 个前端产物及动态 harness 自检 |
| `cargo clippy -p lang-frontend --test fixtures --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
