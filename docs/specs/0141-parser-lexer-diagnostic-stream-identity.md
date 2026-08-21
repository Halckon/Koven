# SPEC-0141: 锁定 Parser 的 Lexer diagnostic 流身份

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-141` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002–0003、SPEC-0006–0009、SPEC-0065、SPEC-0129、SPEC-0135–0140 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser lexical recovery、engine 私有单元测试、Architecture |
| 语言语义变更 | 否；只强化 Parser 对不可能 Lexer diagnostic 流的拒绝 |

## 1. Goal

完成后，Parser 在建立 lexical recovery index 时要求每条 Lexer diagnostic 属于当前
`LexedFile` 的 source identity，且 code 严格属于 L0001–L0008；跨 source diagnostic 与
Parser code 注入均在所有入口前确定地收敛为 `InvalidLexemeStream`。

## 2. 范围与需求

- 每条 Lexer diagnostic 的 primary Span 必须与 `LexedFile::source_id` 相同。
- Lexer diagnostic code domain 精确限定为 L0001–L0008；L0009 及后续 Parser / semantic code
  不能从 `LexedFile::diagnostics` 注入。
- 从两份独立双运行 Lexer 产物派生 foreign L0004 primary Span 与 source-local L0009 两类产物，
  共验证 4 个正常 Lexer 产物；保留 lexeme 流结构并先通过 `validate_lexemes`。
- recovery index 与 expression、declaration、block、file 四入口对每类各执行两次，共验证
  20 个确定的 `ParserInternalError::InvalidLexemeStream`。
- 不复制 Lexer diagnostic message，不改变合法 Lexer / Parser 产物、用户诊断或语言语义。

## 3. 非目标

- 不改变 diagnostic catalog、code、message、Span 或排序。
- 不改变 Parser 自身生成诊断的 code domain。
- 不把 diagnostic 流校验并入只负责 lexeme 结构的 `validate_lexemes`。
- 不修改 Parser grammar、AST、恢复策略或公开 API。

## 4. 验收标准

- [x] foreign primary Span 与非 Lexer code 均在 recovery index 入口拒绝。
- [x] 两类破坏从独立双 Lexer 产物派生，共验证 4 个正常产物。
- [x] 两类输入保持 lexeme 结构有效，且保留精确 L0004 / L0009 code。
- [x] recovery index 与四入口双运行拒绝全部输入，共验证 20 个精确错误。
- [x] SPEC-0140 的 7 类 anchor 矩阵与合法 Lexer / owner 测试保持通过。
- [x] `lang-frontend` library 单元测试增至 29 项。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 `LexicalRecoveryIndex::new` 已有 diagnostic 分类点先校验 source identity，并把未知 code
由“无 recovery 语义”改为内部流错误；既有 anchor 二分查找与 owner 验证顺序保持不变。
test-only builder 通过公开 `Diagnostic::new` 构造自身合法、但不属于目标 `LexedFile` 的诊断。

## 6. 实施计划

1. [x] 审计 diagnostic 流身份边界 → 验证：foreign owner diagnostic 与 L0009 注入可越过 recovery。
2. [x] 物化 Spec 与最小校验 → 验证：source identity 与 code domain 集中收敛。
3. [x] 增加两类 test-only corpus 与五消费者双错误矩阵 → 验证：4 个 Lexer 产物、20 个错误。
4. [x] 运行直接相关窄验收 → 验证：29/29，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0141`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | diagnostic 流身份校验、负向矩阵、Architecture 与完成记录 | `test(frontend): validate lexer diagnostic stream identity (SPEC-0141)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：29 passed，0 failed / ignored /
  measured / filtered。
- `cargo test -p lang-frontend --test lexer --test parser_lexical_owner_matrix --locked --offline`：
  21 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 433 passed，0 failed / ignored / measured / filtered。
