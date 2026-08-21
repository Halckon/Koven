# SPEC-0098: 强化 Parser line-break 边界产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-098` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0078、SPEC-0093–0097 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer / Parser line-break boundary 集成测试、Architecture |
| 语言语义变更 | 否；只强化 carrier 词法身份与公开 Parser 产物验收 |

## 1. Goal

完成后，80 个 line-break boundary 源码不仅保持诊断与结构分组，还会精确证明 10 个 carrier
在插入 byte 区域内形成预期 trivia 分段，并对 160 次完整文件解析逐次验证 AST、诊断、roots
与 directive Span，避免过滤 trivia 后的间接证据掩盖 Lexer 分段退化。

## 2. 范围与需求

- 保持 SPEC-0078 的 6 个结构 carrier、4 个非结构 carrier、8 类边界 case、80 个源码与
  160 次解析不变。
- 每个 carrier 声明预期 trivia 分段：LF / CRLF、line comment 加独立终止换行、含换行或裸
  CR 的 block comment，以及 space / tab；实际 LexemeKind、源码切片与插入 byte Span 必须一致。
- 每例验证 Lexer source identity、连续 byte 覆盖、唯一末尾 EOF、diagnostic Span 且零诊断。
- 两次 Parser 产物分别验证 source identity、AST 全表、diagnostic 主 / label Span、全部文件
  roots，以及 package / import / segment / wildcard / alias Span。
- 两次产物的 syntax shape 与完整公开 `Debug` 必须确定一致；现有 carrier 分组、诊断码、
  bare return、enum comma 与中缀连续性断言保持不变。
- 不增加语料、依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不改变 line-break、comment、声明分隔或控制流语义。
- 不要求不同长度 carrier 之间的绝对 Span 相等。
- 不替代领域测试对精确 AST payload 或诊断触发条件的验证。

## 4. 验收标准

- [x] 80 个 Lexer 产物均完整覆盖源码、唯一 EOF、零诊断并精确保留 carrier trivia 分段。
- [x] 160 次 Parser 产物均满足 AST / diagnostic Span、roots 与 directive Span 不变量。
- [x] 每个源码的两次 shape / Debug 产物确定一致。
- [x] 结构 / 非结构 carrier 分组及两个反向 case 的既有预期保持不变。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把 carrier 从裸字符串提升为带预期 `TriviaKind + spelling` 分段的静态表；测试只检查插入区间
内的生产 lexeme。通用 Lexer / AST / diagnostic 不变量复用 `frontend_output_assertions`，完整
文件 roots 与 directive Span 在矩阵内以最小 helper 验证。

## 6. 实施计划

1. [x] 审计 line-break matrix 现有证据 → 验证：确认缺少 carrier 精确分段、AST、roots 与
   directive Span 验证。
2. [x] 强化 Lexer carrier 与两次 Parser 产物断言 → 验证：80 case、160 次解析全部通过。
3. [x] 运行直接相关窄验收 → 验证：2/2，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0098`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | line-break carrier / Parser 产物不变量、Architecture 与完成记录 | `test(frontend): strengthen parser line break invariants (SPEC-0098)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_line_break_boundary_matrix --locked --offline` | 通过 | 2/2；80 个源码、160 次解析与 carrier 精确分段 |
| `cargo clippy -p lang-frontend --test parser_line_break_boundary_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
