# F3b：离线文法的词法输入合同

> **性质**：非规范验证账本 · **状态**：首片合同及词法见证已核对，独立参考词法机差分由 [SPEC-0482](../../specs/active/0482-lexer-reference-difftest.md) 承接 · **读取时机**：准备 F3c 参考表达式文法，或比较 Lexer 与 Parser 的差异时 · **唯一真源**：本页维护首片换行投影及验收边界

基点：`main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`、Guide v0.47。
首片只选普通块中的名称、减号、成员访问、分号及其 trivia；不含 D1–D3，不修改 Lexer 或 Parser。
[Guide 01](../../guide/01-lexical.md#trivia换行与注释)规定词法归属，[Guide 06](../../guide/06-blocks-control-flow.md)
规定何时续行。两者在适配层上不可合并。

## 1. 保留原始词法产物，再派生文法输入

1. 输入是同一个 `SourceMap + LexedFile`。原始记录保留 `SourceId`、lexeme kind、半开 UTF-8 字节 Span、
   原拼写及有序诊断。非 EOF lexeme 必须非空且顺序无缝地覆盖源码，EOF 必须唯一并锚在字节末尾。
   不重编码换行，不从 Unicode 字符索引推导字节 Span。
2. 离线文法视图只从已验证的原始记录派生：普通 token 保留种类和 Span；普通空格、tab、无换行注释
   不产生文法 token，但原始记录不删除。只从 `Newline` 和合法 `BlockComment` 提取 `NL` 事件：LF 一字节，
   CRLF 两字节、一个事件。事件按原始顺序发出，记录换行的子 Span 与 owner lexeme 序号。块注释本身仍为
   一个原始 trivia，拥有多个 `NL` 也不拆分。行注释的终止换行由独立 `Newline` 提供。
3. **`NL` 只报告换行发生，不是分隔符或续行结论。** 不生成 `;`、`CONTINUATION` 或 `MINUS_PREFIX`。
   不调用 Parser 的 `gap_has_line_break` 或基于 AST / stop set 的逻辑。后缀、二元运算、块分隔和括号
   深度由参考文法消费同一有序输入来决定。不得遍历整个源码搜 `\n` 后跨越 owner 归类。
4. 错误诊断、Invalid 或不支持 token 先进入相应拒收/范围外账本，不静默过滤成一个 lexer-clean 输入，
   也不记作参考文法 `SyntaxError`。原始词法错误证据照常保留；本片不定义错误流如何继续投影，
   不对字符串、插值、终止注释或 poison 的内部字节提取 `NL`。参考文法结果仍须完整消费至 EOF。

### 独立性边界

- **Parser 对照通道**：共享生产 Lexer 的词法事实，因此只对照解析、绑定和分隔，不能发现两边继承的 Lexer 错误。
- **词法见证通道**：预期的 owner、字节坐标、是否有诊断必须从输入和 Guide 手工定出；不能从 Parser 输出回填。
- **F3a 原始文本通道**：小型 lexer 自己把裸 CR 当空白，只是工具试验简化，不符合 Koven 普通词法规则。
  F3b 中裸 CR 在普通模式产生 Invalid，故不能把 F3a 的扫描器直接当独立 Lexer 参考。

## 2. 字节级验收见证

下表中的转义代表实际字节（`雪` 占三个 UTF-8 字节）。`NL` 列写事件起始字节；CRLF 事件应使用两字节范围。
对错误输入只校验原始词法事实和首片拒收，不定义后继 Parser 的恢复结构。

| 原始源码 | 原始 trivia Span 或词法诊断观察 | 派生 `NL` |
|---|---|---|
| `a\nb` / `a\r\nb` | 独立 `Newline` 为 `[1,2)` / `[1,3)` | `1` / `1` |
| `a\rb` | `[1,2)` 为 Invalid，存在词法诊断 | 拒收，不发 `NL` |
| `a//雪\nb` / `a//雪\r\nb` | 行注释 `[1,6)`；独立 `Newline` 为 `[6,7)` / `[6,8)` | `6` / `6` |
| `a//雪\rb` | 行注释 `[1,6)`，后续裸 CR 为 Invalid | 拒收，不发 `NL` |
| `a/*雪\nc*/b` / `a/*雪\r\nc*/b` | 一个块注释 `[1,10)` / `[1,11)` | `6` / `6`，owner 未拆分 |
| `a/*雪\rc*/b` | 一个合法块注释 `[1,10)`；注释内容的裸 CR 不是 Invalid | 无 |
| `a/*雪\n\r\nc*/b` | 一个块注释 `[1,12)` | `6, 7`，每个换行独立有序 |
| `a/*雪\n` | 存在未终止注释诊断、仍保留 EOF | 拒收，不从失败内容发 `NL` |

## 3. 执行账本与后继

用仓库外 Rust 探针链接 `lang-frontend` 构建产物，仅调用公开 `lex`，未调用 Parser。上表 **11 个实际输入**
通过：词法流连续覆盖、所有片段可切片、唯一末尾 EOF、手填 trivia 范围和换行起始字节、词法诊断有无，
以及两个普通模式裸 CR 的 Invalid 拼写。探针对诊断只检查存在性，不将诊断码和消息记作本次实测证据；
CRLF 的派生事件结束字节仍由后续适配器测试明确断言。已有 `validate_carrier_lexemes` 独立核对 trivia kind 和拼写。

```bash
cargo test --locked -p lang-frontend --no-fail-fast \
  --test lexer --test lexer_boundary_matrix --test parser_block_line_continuation \
  --test parser_entry_line_break_boundary_matrix --test parser_long_block_comment_line_breaks -- --quiet
```

本机 5 个目标 **55 passed，0 failed、0 ignored、0 filtered**。未启用 Guide 草案，未加入项目依赖，未运行远端 CI。

**后继（F3c）**：[真实词法流首片](token-stream-probe.md)已让离线文法消费 `Token | NL` 流，
用手填预期及四种故障区分结构检出和事件检出。未获对应 Spec 前不提交长期适配器或新增门禁。
