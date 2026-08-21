# SPEC-0140: 锁定 Parser 的 Lexer diagnostic anchor 契约

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-140` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0006–0009、SPEC-0065、SPEC-0129、SPEC-0135–0139 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser lexical recovery、engine 私有单元测试、Architecture |
| 语言语义变更 | 否；只强化 Parser 对不可能 Lexer 内部产物的拒绝 |

## 1. Goal

完成后，Parser 在消费 Lexer 产物前验证全部 L0001–L0008 diagnostic 的 lexeme anchor 形状；
7 类保留生产诊断但移除精确 anchor 的结构有效产物均被 recovery index 与四个 Parser 入口
重复、精确拒绝。

## 2. 范围与需求

- L0001、L0003、L0006、L0007、L0008 必须与同 Span、同 `InvalidKind` 的 lexeme 对应。
- L0002 必须与同 Span 的 `ReservedWord` token 对应；L0004、L0005 必须分别锚定到诊断起点的
  StringStart、InterpolationStart，后续 owner 栈验证保持不变。
- 从 7 份独立双运行生产 Lexer 产物构造 unexpected character、reserved word、unterminated
  block comment、non-terminal / terminal invalid escape、invalid char、invalid number anchor 破坏，
  共验证 14 个正常 Lexer 产物。
- 每类输入保持 source identity、连续 Span、唯一 EOF 与原始 diagnostic，并先通过通用
  `validate_lexemes`。
- recovery index 与 expression、declaration、block、file 四入口对每类各执行两次，共验证
  70 个确定的 `ParserInternalError::InvalidLexemeStream`。
- 不改变合法 Lexer / Parser 产物、用户诊断、依赖或语言语义。

## 3. 非目标

- 不改变 L0001–L0008 的 code、message、Span、排序或 Lexer recovery 行为。
- 不把具体 diagnostic code 语义并入通用 Lexeme 结构校验。
- 不修改 Parser grammar、AST 或用户错误恢复策略。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] L0001–L0008 的生产 anchor 形状均有统一内部校验。
- [x] 7 类 anchor 破坏从独立双 Lexer 产物派生，共验证 14 个正常产物。
- [x] 全部输入保留精确 diagnostic code，并通过通用 Lexeme 结构校验。
- [x] recovery index 与四入口双运行拒绝全部输入，共验证 70 个精确错误。
- [x] 合法输入与既有 owner / recovery 测试保持通过。
- [x] `lang-frontend` library 单元测试增至 28 项。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 `LexicalRecoveryIndex::new` 的既有 diagnostic 扫描处增加私有 anchor 分类，并利用已验证的
lexeme 起点顺序二分定位对应产物，使校验保持 O(D log L)；
test-only builder 只把生产 Lexer 产物中的对应 lexeme 重分类为 Identifier，不改 diagnostic、Span、
EOF 或 source identity。错误继续收敛为既有 typed `InvalidLexemeStream`。

## 6. 实施计划

1. [x] 审计 diagnostic/lexeme 关联分支 → 验证：六类精确 anchor 尚未统一校验。
2. [x] 物化 Spec 与最小实现 → 验证：L0001–L0008 anchor 规则集中表达。
3. [x] 增加 7 类 test-only corpus 与五消费者双错误矩阵 → 验证：14 个 Lexer 产物、70 个错误。
4. [x] 运行直接相关窄验收 → 验证：28/28，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0140`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | diagnostic anchor 校验、负向矩阵、Architecture 与完成记录 | `test(frontend): validate lexer diagnostic anchors (SPEC-0140)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：28 passed，0 failed / ignored /
  measured / filtered。
- `cargo test -p lang-frontend --test lexer --test parser_lexical_owner_matrix --locked --offline`：
  21 passed，0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 432 passed，0 failed / ignored / measured / filtered。
