# SPEC-0168: 建立 Parser 超长块注释换行矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-168` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006–0009、SPEC-0014、SPEC-0073、SPEC-0093、SPEC-0103、SPEC-0129、SPEC-0150–0151、SPEC-0160–0167 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 长 UTF-8 block-comment trivia、逻辑换行与四个公开 Parser 入口测试、Architecture |
| 语言语义变更 | 否；只锁定 block comment 不嵌套、首个 closer 终止及内部 LF / CRLF 参与语法换行判定的既有行为 |

## 1. Goal

完成后，含 nested-looking opener、约 65 KiB UTF-8 payload 且在尾部含 LF / CRLF 的块注释必须
形成单一完整 BlockComment trivia；Parser 必须在表达式中跨该逻辑换行保留中缀运算，同时在
block / file 中把同一注释内换行作为相邻声明的合法分隔证据。

## 2. 范围与需求

- 两类 comment 分别包含 LF 与 CRLF；共同正文先放 nested-looking `/*`、string/interpolation /
  line-comment-like 标记，再放 21,845 个 `界`（65,535 bytes），然后才出现换行与唯一 `*/`。
- Lexer 必须按 v1 非嵌套规则把唯一 `*/` 视为当前注释的首个 closer，产生恰好一个覆盖完整
  comment 的 `TriviaKind::BlockComment`，零诊断且不把正文标记泄漏成 token。
- expression 入口固定解析 `left <comment> + right`；declaration 入口把同一表达式放入 initializer。
  两者必须保留完整 `BinaryOperator::Add`、左右 Name 与真实 operator Span，不因注释内换行拆分。
- block / file 分别解析 `val first = 0 <comment> val after = 1`；源码在 comment 外没有换行或分号，
  两个声明必须仅凭 comment 内 LF / CRLF 合法分隔，且零 Parser 诊断。
- 两类 comment 分别投放到 expression、declaration、block 与 file 四个公开入口，共执行 8 个源码。
- 每个源码先执行两次独立生产 Lexer 并检查 comment trivia，再由对应 Parser helper 执行双 Lexer /
  双 Parser，共验证 32 个 Lexer 和 16 个 Parser 产物的覆盖、EOF、source-local Span 与确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现 comment / line-break 缺陷时只修复直接根因。

## 3. 非目标

- 不重复短普通 block comment、长无换行 comment、unterminated L0003 或大量 sibling trivia 压力。
- 不改变 v1 block comment 的非嵌套规则，也不引入 doc-comment、注释 AST 或格式化语义。
- 不覆盖裸 CR、Unicode line separator 或无效 UTF-8 输入。
- 不验证 renderer 的行列展示，不设置 wall-clock / 内存阈值。
- 不检查类型、所有权或运行时求值。

## 4. 验收标准

- [x] LF / CRLF 两个长 comment 均形成唯一完整 BlockComment trivia，正文 marker 不泄漏且零诊断。
- [x] 65,535-byte UTF-8 payload、换行与 comment Span 均可按精确 byte boundary 切片。
- [x] expression / declaration 保留 `left + right` Binary，operator 与两侧 Name Span 准确。
- [x] block / file 仅凭 comment 内换行保留两个声明且零 Parser 诊断。
- [x] 8 个源码共验证 32 个 Lexer 与 16 个 Parser 公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 block-comment line-break target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 comment builder 记录 UTF-8 payload、LF / CRLF 和完整
trivia 的相对 byte offset。测试复用 `lex_parser_source_twice` 直接检查生产 LexedFile，再复用四个
Parser 双运行入口检查 typed AST；只读取公开 lexeme / AST table 与 SourceMap 切片，不读取私有
Scanner / Parser 状态。

## 6. 实施计划

1. [x] 审计短 comment 非嵌套、短 comment 换行与长 comment 证据 → 验证：nested-looking opener × 长 UTF-8 × 深处 LF/CRLF × 四入口交叉边界缺失。
2. [x] 建立两个 comment × 四入口矩阵 → 验证：唯一 trivia、8 个源码与上下文相关 typed 结构。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0168`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长块注释换行矩阵、必要修复、Architecture 与完成记录 | `test(frontend): preserve long block comment line breaks (SPEC-0168)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_long_block_comment_line_breaks` integration target：LF / CRLF 两种 comment 分别经过
  四个 Parser 公共入口，8 个源码均保留唯一完整 BlockComment trivia、精确 UTF-8 payload / 换行
  Span 与上下文相关 typed AST；独立 Lexer 加 Parser helper 双运行共覆盖 32 个 Lexer 和 16 个
  Parser 产物。
- nested-looking opener 与其他 token-like marker 均留在 comment 内，唯一 `*/` 按非嵌套规则关闭
  comment；expression / declaration 保留 `left + right`，block / file 仅凭 comment 内换行保留两个
  声明，全部零诊断。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_long_block_comment_line_breaks --locked --offline`：
  1 passed，0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_long_block_comment_line_breaks --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：473 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。
