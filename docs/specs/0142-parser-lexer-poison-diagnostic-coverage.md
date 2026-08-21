# SPEC-0142: 锁定 Parser 的 lexical poison 诊断覆盖

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-142` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0006–0009、SPEC-0065、SPEC-0129、SPEC-0135–0141 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser lexical recovery、engine 私有单元测试、Architecture |
| 语言语义变更 | 否；只拒绝缺失生产 Lexer diagnostic 的内部 poison 产物 |

## 1. Goal

完成后，Parser 的 Lexer diagnostic/lexeme anchor 契约为双向：每条诊断必须锚定正确 lexeme，
每个 `InvalidKind` 或 `ReservedWord` poison lexeme 也必须由对应生产诊断覆盖，不再允许静默
Error AST。

## 2. 范围与需求

- 复用 SPEC-0140 的 diagnostic anchor 二分定位结果，按 lexeme index 记录诊断覆盖。
- diagnostic 扫描完成后线性检查全部 lexeme；任何未覆盖的 `Invalid(_)` 或 `ReservedWord(_)`
  均返回 `ParserInternalError::InvalidLexemeStream`。
- 从 6 份独立双运行生产 Lexer 产物移除 L0001、L0002、L0003、L0006、L0007、L0008，保留
  UnexpectedCharacter、ReservedWord、UnterminatedBlockComment、InvalidStringEscape、
  InvalidCharLiteral、InvalidNumericLiteral poison，共验证 12 个正常 Lexer 产物。
- recovery index 与 expression、declaration、block、file 四入口对每类各执行两次，共验证
  60 个确定的 `InvalidLexemeStream`。
- 新增校验保持 O(D log L + L)，不改变合法 Lexer / Parser 产物、诊断或语言语义。

## 3. 非目标

- 不要求 L0004 / L0005 对应 poison；它们由 StringStart / InterpolationStart owner anchor 表达。
- 不改变 diagnostic catalog、消息、排序或 Lexer recovery。
- 不修改 Parser grammar、AST、用户错误恢复或公开 API。
- 不新增随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 五种 `InvalidKind` 与 `ReservedWord` 均要求对应生产 diagnostic 覆盖。
- [x] 6 类破坏从独立双 Lexer 产物派生，共验证 12 个正常产物。
- [x] 全部输入保持 lexeme 结构有效，且精确移除原始 L0001/L0002/L0003/L0006/L0007/L0008。
- [x] recovery index 与四入口双运行拒绝全部输入，共验证 60 个精确错误。
- [x] SPEC-0140–0141 的正反向矩阵与合法 Lexer / owner 测试保持通过。
- [x] `lang-frontend` library 单元测试增至 30 项。
- [x] 新增校验复杂度保持 O(D log L + L)。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

把私有 `has_diagnostic_anchor` 收敛为返回 lexeme index 的 `diagnostic_anchor_index`；每条已验证
diagnostic 在等长布尔向量中标记 anchor，随后单次遍历 lexeme 检查 poison 覆盖。既有 anchor
二分定位、diagnostic 分类与 owner recovery 不重复扫描，空间复杂度 O(L)。

## 6. 实施计划

1. [x] 审计 poison 消费路径 → 验证：六类 lexeme 可在无 diagnostic 时被当作已诊断错误消费。
2. [x] 物化 Spec 与反向覆盖实现 → 验证：索引位图与线性检查保持既有阶段顺序。
3. [x] 增加六类 test-only corpus 与五消费者双错误矩阵 → 验证：12 个 Lexer 产物、60 个错误。
4. [x] 运行直接相关窄验收 → 验证：30/30，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0142`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | poison 诊断覆盖、负向矩阵、Architecture 与完成记录 | `test(frontend): require diagnostics for lexer poisons (SPEC-0142)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：30 passed，0 failed / ignored /
  measured / filtered。
- `cargo test -p lang-frontend --test lexer --test parser_lexical_owner_matrix --locked --offline`：
  21 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 434 passed，0 failed / ignored / measured / filtered。
