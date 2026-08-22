# SPEC-0169: 建立 Parser 超长行注释边界矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-169` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0002、SPEC-0006–0009、SPEC-0014、SPEC-0073、SPEC-0093、SPEC-0103、SPEC-0129、SPEC-0150–0151、SPEC-0160–0168 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 长 UTF-8 line-comment / newline trivia 边界与四个公开 Parser 入口测试、Architecture |
| 语言语义变更 | 否；只锁定 line comment 在 LF / CRLF 前结束、换行独立分段并参与语法判定的既有行为 |

## 1. Goal

完成后，含约 65 KiB UTF-8 payload 的 line comment 遇到 LF / CRLF 时必须形成相邻且不重叠的
LineComment 与 Newline trivia；Parser 必须跨该换行保留中缀表达式，并在 block / file 中把它作为
相邻声明的合法分隔，证明 Scanner 边界和语法逻辑换行保持一致。

## 2. 范围与需求

- 两类 carrier 分别为 LF 与 CRLF；共同 comment 正文先放 block/string/interpolation-like marker，
  再放 21,845 个 `界`（65,535 bytes），换行紧邻 payload 且不属于 LineComment trivia。
- Lexer 必须产生恰好一个覆盖 `//` 到换行前的 `TriviaKind::LineComment`，随后恰好一个覆盖完整
  LF / CRLF 的 `TriviaKind::Newline`；两者相邻、零诊断，正文 marker 不泄漏成 token。
- expression 入口固定解析 `left <comment><newline>+ right`；declaration 入口把同一表达式放入
  initializer。两者必须保留完整 `BinaryOperator::Add`、左右 Name 与真实 operator Span。
- block / file 分别解析 `val first = 0 <comment><newline>val after = 1`；源码没有额外换行或分号，
  两个声明必须由该独立 Newline trivia 合法分隔，且零 Parser 诊断。
- 两类 carrier 分别投放到 expression、declaration、block 与 file 四个公开入口，共执行 8 个源码。
- 每个源码先执行两次独立生产 Lexer 并检查两段 trivia，再由对应 Parser helper 执行双 Lexer /
  双 Parser，共验证 32 个 Lexer 和 16 个 Parser 产物的覆盖、EOF、source-local Span 与确定性。
- 不改变 Lexer / Parser 语义、诊断目录、公开 API 或依赖；发现 comment / newline 边界缺陷时只修复直接根因。

## 3. 非目标

- 不重复短 line comment、长 EOF line comment、block comment 或大量 sibling trivia 压力矩阵。
- 不引入 doc-comment、注释 AST、Unicode line separator 或格式化语义。
- 不覆盖裸 CR 或无效 UTF-8 输入。
- 不验证 renderer 的行列展示，不设置 wall-clock / 内存阈值。
- 不检查类型、所有权或运行时求值。

## 4. 验收标准

- [x] LF / CRLF 两个长 comment 均形成精确相邻的唯一 LineComment 与唯一 Newline trivia。
- [x] 65,535-byte UTF-8 payload、comment 与换行 Span 均可按精确 byte boundary 切片。
- [x] expression / declaration 保留 `left + right` Binary，operator 与两侧 Name Span 准确。
- [x] block / file 仅凭独立 Newline trivia 保留两个声明且零 Parser 诊断。
- [x] 8 个源码共验证 32 个 Lexer 与 16 个 Parser 公开产物及确定性不变量。
- [x] 未发现生产缺陷，或缺陷有最小修复与定向回归证据。
- [x] 新 line-comment boundary target 及窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增独立 integration target，以 tests 私有 carrier builder 记录 UTF-8 payload、LineComment 与 LF /
CRLF 的相对 byte offset。测试复用 `lex_parser_source_twice` 直接检查生产 LexedFile，再复用四个
Parser 双运行入口检查 typed AST；只读取公开 lexeme / AST table 与 SourceMap 切片，不读取私有
Scanner / Parser 状态。

## 6. 实施计划

1. [x] 审计短 line-comment 分段、短 Parser 换行与长 comment 证据 → 验证：长 UTF-8 × LF/CRLF 精确双 trivia × 四入口交叉边界缺失。
2. [x] 建立两个 carrier × 四入口矩阵 → 验证：两段 trivia、8 个源码与上下文相关 typed 结构。
3. [x] 运行新 target 与窄 Clippy → 验证：全部通过，0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0169`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Parser 长行注释边界矩阵、必要修复、Architecture 与完成记录 | `test(frontend): preserve long line comment boundaries (SPEC-0169)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- 新增 `parser_long_line_comment_boundaries` integration target：LF / CRLF 两种 carrier 分别经过
  四个 Parser 公共入口，8 个源码均保留精确相邻且不重叠的唯一 LineComment / Newline trivia、
  65,535-byte UTF-8 payload 与上下文相关 typed AST；独立 Lexer 加 Parser helper 双运行共覆盖
  32 个 Lexer 和 16 个 Parser 产物。
- token-like marker 均留在 line comment 内；expression / declaration 保留换行后的 `left + right`，
  block / file 仅凭独立 Newline trivia 保留两个声明，全部零诊断。
- 未发现生产 Lexer / Parser 缺陷；未修改生产代码、公开 API、诊断目录或依赖。
- `cargo test -p lang-frontend --test parser_long_line_comment_boundaries --locked --offline`：
  1 passed，0 failed，0 ignored，0 filtered out。
- `cargo clippy -p lang-frontend --test parser_long_line_comment_boundaries --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线在实现与 Architecture 同步后仅执行一次：
  - `cargo fmt --all -- --check`：通过。
  - `cargo check --workspace --all-targets --locked --offline`：通过。
  - `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`：通过，0 warnings。
  - `cargo test --workspace --all-targets --locked --offline`：474 passed，0 failed，0 ignored。
  - `cargo build -p lang-cli --locked --offline`：通过。
