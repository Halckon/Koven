# SPEC-0006: 建立确定性 Lexer

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-006` |
| 所属 Phase | Phase 1 |
| 语言规范 | [`agent-language-design-guide-v0.5.md`](../agent-language-design-guide-v0.5.md) |
| 前置 Spec | SPEC-0002、SPEC-0003、SPEC-0005 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend`、语言 fixture、Architecture |
| 语言语义变更 | 否；只实现生效后的 guide，不由本 Spec 创设语义 |

## 1. Goal

完成后，`lang-frontend` 能把任意已加载的 Koven UTF-8 源码确定性转换为保留精确 `Span`
的 lexeme 流和结构化诊断；非法源码不会触发 `panic!`，结果可直接作为 SPEC-0007 的唯一
词法输入。Parser 仍与 Lexer 共享 `SourceMap`，通过 `Span` 回查原文，不要求 LexedFile
复制源码文本。

## 2. 背景

Phase 0 已提供 source / `Span`、结构化诊断与真实 fixture target，但当时尚无 Lexer。历史
v0.4 未完整定义标识符字符集、字面量、trivia、最长匹配和错误恢复，且其“完整”硬关键字表
漏掉正文已经使用的 `move`，因此不能据此猜测实现。现行 v0.5 已收敛这些规则，并为本 Spec
提供可执行的语言与恢复契约。

## 3. 范围与需求

- 新增 `lang_frontend::lexer`。入口接收同一 `SourceMap` 的 `SourceId`，返回词法产物或具体
  内部错误；非法 Koven 源码保留为产物中的用户诊断，不走内部错误路径。
- 词法产物按源码顺序保存 token、trivia、invalid 区域和唯一 EOF。每个 lexeme 包含 kind
  与原始 `Span`，源码文本通过 `SourceMap` 回查，不复制 token 文本。
- 完整实现 v0.5 第三部分：ASCII 标识符、42 个硬关键字、2 个软关键字、11 个未来
  保留字、trivia、十进制整数 / 浮点、`Char`、单行 `String` / `${...}` 插值、固定符号、
  最长匹配、EOF 和逐类恢复。
- 在生产诊断目录集中分配下列一一对应的稳定错误码：

  | 错误码 | 严重级别 | 稳定含义 | 主消息 |
  |---|---|---|---|
  | `L0001` | error | 非法字符 | `unexpected character` |
  | `L0002` | error | 未来保留字不可用 | `reserved word is not available in Koven v1` |
  | `L0003` | error | 未终止块注释 | `unterminated block comment` |
  | `L0004` | error | 未终止字符串 | `unterminated string literal` |
  | `L0005` | error | 未终止字符串插值 | `unterminated string interpolation` |
  | `L0006` | error | 非法字符串转义 | `invalid escape in string literal` |
  | `L0007` | error | 非法 `Char` 字面量 | `invalid character literal` |
  | `L0008` | error | 非法数字 | `invalid numeric literal` |

- 每类诊断使用固定、非空、单行主消息和 v0.5 规定的主 `Span`；恢复必须前进，且 lexeme
  联合覆盖全部输入字节。诊断沿用 SPEC-0003 的稳定全序。
- 扩展真实 Cargo fixture target，新增 `phase1/lexer-pass/` 和 `phase1/lexer-fail/`。
  pass case 断言零词法诊断及完整字节覆盖；每个 fail `.ko` 必须有同名 `.diag`，每行格式为
  `Ldddd<TAB>start_byte<TAB>end_byte`：例如 `foo.ko` 只与 `foo.diag` 配对，offset 是十进制
  UTF-8 半开字节范围，行序就是诊断全序。每个 fail sidecar 至少包含一条记录，可使用 LF
  或 CRLF；空行、缺失 / 孤立 sidecar、非法行、额外或缺失诊断均失败。sidecar 只属于
  仓库测试，不是公共机器诊断协议。
- 使用 Rust 标准库完成扫描；ASCII 标识符无需 Unicode 属性库、regex 或其他新依赖。

## 4. 非目标

- 不实现 Parser、Pratt binding power、AST 构造或任何语法位置合法性判断。
- 扫描时可以临时解码转义以验证 `Char` 恰好是一个 Unicode scalar，但不在 token 中存储
  解码后的字面量值，也不判断数字溢出、默认类型或目标类型。
- 不把 Unicode 标识符、指数 / 进制数字、数字分隔符或类型后缀作为合法语法；数字形式仍须
  按 guide 聚合为非法数字区域并产生 `L0008`。不实现规范化、同形字符诊断、raw / 三引号 /
  多行字符串、嵌套块注释或 `$name` 插值。
- 不接入 CLI renderer、stderr、退出码、LSP 或公共机器诊断协议。
- 不实现增量 lexing、rope、formatter trivia 附着或任何 v2+ 语义。

## 5. 验收标准

- [x] 穷举测试 42 个硬关键字、2 个软关键字和 11 个未来保留字；`move` 是硬关键字，
      `error` 是标识符，关键字前后缀和大小写边界符合 guide。
- [x] 标识符测试覆盖首 / 续字符、单独 `_`、ASCII 边界，以及 Unicode scalar 只能用于
      注释和字面量内容、不能组成标识符的反例。
- [x] trivia 测试覆盖最大 space / tab 段、LF、CRLF、裸 CR、Unicode 空白、行注释、
      非嵌套块注释、普通模式 BOM 和未终止块注释，并锁定每段 `Span`。
- [x] 字面量测试覆盖整数、小数、range 消歧义、非法数字最大区域、Unicode `Char` /
      `String`、全部合法转义、空 / 多 scalar `Char`、最大非空 string-text、`${...}` 嵌套
      模式、换行 / EOF 恢复，以及 EOF 多层未闭合模式只报告最内层错误。
- [x] 穷举固定符号并验证最长匹配、注释优先、`as?` / `!in` / `!is` 邻接边界和为
      Phase 5 保留但尚无 Parser 语义的 `@` token。
- [x] `L0001`–`L0008` 各自只表达表中一类含义；测试断言全部为 error，以及错误码、固定
      消息、精确字节 `Span`、恢复后的 token 和诊断稳定顺序。
- [x] 空文件只产生唯一 EOF；每个非 EOF lexeme 非空、同 source、不重叠，所有 lexeme
      联合覆盖 EOF 前全部 UTF-8 字节；恢复不产生零长度循环或 `panic!`。
- [x] 相同源码在不同 source 加载顺序和重复运行下产生相同 kind、相对 `Span` 与诊断；
      任何被 `SourceMap` 拒绝的 `SourceId`（至少覆盖来自另一 source map 的 ID）返回具体
      内部错误，不为测试泄漏不安全的 ID 构造器。
- [x] `lexer-pass` / `lexer-fail` 均至少执行一个真实 `.ko` case；零 case、非法配对或期望
      不匹配使测试失败，且 Phase 0 fixture 继续执行。
- [x] `cargo tree -p lang-frontend --edges all --locked --offline` 及 manifest / lock diff 证明
      未新增 normal、dev 或 build 依赖。
- [x] frontend 窄测试和 workspace fmt、check、Clippy、test、CLI build 基线通过，无
      ignored / filtered case 被隐瞒。
- [x] Architecture 更新为实现后的 Lexer 数据流和 Parser 前置边界，不把 Parser 写成已实现。

## 6. 技术方案与边界

建议入口形态为：

```rust
pub fn lex(sources: &SourceMap, source_id: SourceId) -> Result<LexedFile, LexerInternalError>
```

`LexedFile` 持有 source identity、有序 `Vec<Lexeme>` 和结构化诊断；`Lexeme` 使用
`Token(TokenKind)`、`Trivia(TriviaKind)`、`Invalid(InvalidKind)` 与 EOF 的封闭分类。
`TokenKind` 用子枚举集中表达 keyword 和固定符号；软关键字仍为 identifier。关键字表和
固定符号表各自只保留一个实现真源。

非法字符、未终止块注释、非法字符串转义、非法 `Char` 和非法数字使用各自的
`InvalidKind`；未来保留字仍是 reserved token。未终止块注释不保留 trivia。未终止字符串 /
插值保留已经产生且互不重叠的 string / interpolation lexeme，只让诊断 `Span` 横跨这些
lexeme，不另造重叠的 invalid lexeme。换行处只弹出当前字符串模式并让包围的普通或插值
模式继续扫描；EOF 处多层未闭合词法模式只报告最内层模式错误，已经产生的其他独立词法
错误不受抑制。

扫描器基于 UTF-8 `str` 的字节 offset / `char_indices` 前进，不把字面量解析为宿主机数值。
字符串插值使用显式模式栈和插值花括号深度；嵌套字符串复用同一状态模型。正常用户错误
只产生 lexeme 与诊断；source identity 失配、构造诊断失败等实现边界返回具体内部错误。

细粒度 token / `Span` 与恢复测试放在 Lexer integration test；正式 `.ko` pass / fail case
通过 SPEC-0005 的真实 fixture target 执行。无需 facade、feature flag 或第三方扫描框架。

## 7. 实施计划

1. [x] 注册 `L0001`–`L0008` → 验证：目录、severity 与消息单测
2. [x] 实现 lexeme 模型、普通模式、关键字 / 符号 / trivia / 数字扫描 → 验证：Lexer 窄测试
3. [x] 实现 `Char`、字符串 / 插值模式和全部恢复路径 → 验证：逐类诊断与多错误恢复测试
4. [x] 扩展 pass / fail fixture 及 sidecar 自检 → 验证：真实 fixture target
5. [x] 同步 Spec 验收记录与 Architecture → 验证：全 workspace 基线与 staged diff

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 候选规则明确后的 Spec 草案 | `docs(spec): draft deterministic lexer (SPEC-0006)` |
| 2 | v0.5 生效后的 Spec 批准 | `docs(spec): approve deterministic lexer (SPEC-0006)` |
| 3 | Lexer、错误码、测试 / fixture、Architecture 和完成记录 | `feat(frontend): add deterministic lexer (SPEC-0006)` |

v0.5 激活及全仓当前真源指针同步已由本 Spec 之外的独立纯文档提交完成，不与 Lexer 实现
提交混合。

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 3 passed；0 failed / ignored / filtered |
| `cargo test -p lang-frontend --test lexer --locked --offline` | 通过 | 18 passed；0 failed / ignored / filtered |
| `cargo test -p lang-frontend --test fixtures --locked --offline` | 通过 | 15 passed；0 failed / ignored / filtered；三个 suite 均真实执行 |
| `cargo test -p lang-frontend --doc --locked --offline` | 通过 | 7 passed；0 failed / ignored / filtered |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend` 根节点；manifest / lock 无差异，未新增依赖 |
| `cargo fmt --all -- --check` | 通过 | 无输出 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | 全 workspace / target 检查完成 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 零 warning |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 68 passed；0 failed / ignored / filtered |
| `cargo build -p lang-cli --locked --offline` | 通过 | `lang-cli` 构建完成 |
