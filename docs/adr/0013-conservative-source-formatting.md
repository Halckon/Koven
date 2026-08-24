# ADR-0013: 保守、语法保持的源码格式化

## 状态

accepted

## 接受依据

2026-08-25 依据当前持续 Goal“继续推进 guide 主线，分阶段实施 specs”的站立授权接受。
本决策只封闭 SPEC-0057 的工具行为，不改变 Koven token、语法、换行分隔或注释语义。

## 背景

Phase 6 需要稳定、幂等的格式化器。生产 Lexer 已保留每个 token、whitespace、LF/CRLF、行注释
和块注释的原始 `Span`；Parser 已能对完整文件建立具体 AST 并发布恢复诊断，但 trivia 不附着到
AST。Koven 的换行不是任意空白：它可分隔顶层声明、终止裸 `return`，并影响完整文件恢复。

因此，首版 formatter 若像普通 pretty-printer 一样自由重排换行，需要额外 CST/trivia attachment
和对每条换行敏感语法的完整证明；若只做正则替换，则容易改写字符串、注释或 token 边界。项目
需要在不新增 crate/依赖的前提下，先建立一个覆盖全部合法文件、可证明不改 token 序列和换行
边界的格式化基线。

## 决策

### formatter 位于 frontend，CLI 只负责 IO

- `lang-frontend::formatting` 接收调用方拥有的 `SourceMap` / `SourceId`，复用生产 Lexer 与完整文件
  Parser。只有 Lexer/Parser 均无诊断的文件才格式化；恢复 AST、非法 token 和内部错误原样转为
  结构化 formatter error，不猜测修复。
- formatter 直接消费完整 lexeme 流和原始 source slice；字符串/字符/数字/标识符等非 trivia
  字节逐字保留，行/块注释正文也逐字保留。不得复制关键字或操作符拼写表。
- `lang-cli` 提供最小 `kovenc format <path>`（格式化文本写 stdout）和
  `kovenc format --check <path>`（不写文本，规范时退出 0、有差异时退出 1）边界；源文件永不
  原地覆盖。参数/读取/内部错误退出 2。该子命令不定义 compiler build 参数。

### 首版只规范安全空白

- 保留每一个原始 newline lexeme 的具体 `LF` / `CRLF` 字节和数量；formatter 不插入、删除或
  移动换行。这样顶层分隔、裸 `return` 和 comment termination 不会被重解释。
- 行首 whitespace 统一为每层四个 ASCII space，不输出 tab。基础层级由 `{}` 决定；未闭合输入
  已在 Parser 门禁拒绝。多行 `()` / `[]` 内增加一层 continuation indent，行首 closing delimiter
  先减去自身层级。字符串插值 owner 不作为 block indentation。
- 同一行 token 之间的 whitespace 折叠为零或一个 ASCII space。`.` / `?.`、postfix `?` / `!!`、
  `,` / `;` 前、opening delimiter 后和 closing delimiter 前不留空格；`,` / `;` / `:` 后及关键字、
  标识符、字面量和二元/赋值/arrow 操作符之间使用一个空格。`@`、unary `!` / `+` / `-` 与 operand
  相邻。
- `<` / `>` 和 `+` / `-` 等上下文相关 token 采用“保留邻接类别”规则：原来两侧均无水平空白
  时保持紧邻，否则按普通操作符留一格。首版不借格式化器重新判定泛型、比较或 unary 语义。
- comment 不重排。行注释前若同一行已有 token 则保留一个空格，后继 newline 原样输出；块注释
  作为一个不可改写 lexeme，同行两侧按普通 word 边界处理。

### 可执行不变量优先于风格扩张

- `format(format(source)) == format(source)` 必须对代表性 corpus 和全部现有 parser-pass fixture
  成立。
- 格式化前后非 trivia lexeme 的 kind 与原始字节序列完全相同，comment lexeme 的 kind/正文/
  顺序完全相同，newline lexeme 字节序列完全相同；格式化结果重新 Lexer/Parser 必须零诊断。
- 不设置行宽、不折行、不合并声明、不排序 import/member、不改引号/数值后缀/分号、不添加尾随
  逗号。任何这些扩张都需要后续 ADR/Spec，并继续证明换行敏感语义。

## 替代方案

### 直接从 AST pretty-print

暂不采用。现有 AST 没有 trivia attachment；comments 无法稳定归属，且完整恢复节点会迫使
printer 发明源码。将来若引入 lossless CST，可由新 ADR 取代本决策。

### 基于正则替换空白

不采用。正则无法可靠区分字符串文本、插值 owner、comments、unary/binary token 和换行敏感
边界，容易改变合法程序。

### 引入 Tree-sitter Rust runtime 作为 formatter CST

不采用。仓库已有 Tree-sitter grammar 用于编辑器，但生产语义真源仍是 frontend Lexer/Parser；
新增 runtime 依赖和第二 parser 不能消除与生产 AST 的漂移验证，首版 lexeme 方案已足够。

### 默认原地改写文件

不采用。首版 CLI 尚无备份、批量发现和原子替换契约。stdout 与 `--check` 足以形成可测试工具，
且不会因 formatter bug 破坏用户文件。

## 后果

收益：

- 全部合法语法可在不复制 token 拼写、无 CST 依赖的情况下获得稳定空白与缩进；
- token/comment/newline 不变量能直接证明首版不会改变语法输入；
- 非破坏性 CLI 可用于编辑器管道和 CI，失败路径明确；
- 后续 pretty-print 扩张有可比较的保守基线。

代价与风险：

- 首版不主动折行、合并空行或重排 import，某些长行和用户换行选择会保留；
- 泛型/比较和 unary/binary 的既有邻接类别会保留，不能保证所有输入收敛到审美上唯一的操作符
  风格，但同一输入仍收敛且幂等；
- format API 依赖完整 Lexer/Parser，非法文件不会得到 best-effort 输出；
- 未来引入 lossless CST 或原地写入时需要新 ADR，并迁移现有 corpus 不变量。

## 关联

- 相关 Spec：SPEC-0057
- 相关 ADR：[ADR-0003](./0003-diagnostic-architecture.md)、
  [ADR-0004](./0004-source-span-position-model.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
