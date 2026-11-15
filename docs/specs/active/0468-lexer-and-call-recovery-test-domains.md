# SPEC-0468：Lexer 与调用恢复测试领域归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批lexer/parser测试职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0468` |
| 所属 Phase | Phase 1 lexer/parser测试组织 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；沿fix/spec-0466与#439 |
| 前置 Spec | [0467](0467-type-container-and-lambda-domains.md)，输入105d9ceb |
| 影响范围 | tests/lexer.rs、parser_call_argument.rs及私有领域模块与policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片完整领域

1. lexer数字/字符/字符串：decimal_numbers至invalid_char_forms四个完整测试，移lexer/literals.rs。
2. lexer符号：fixed_symbols、ampersand、unsupported_operator三项完整测试，移lexer/symbols.rs。
3. parser调用恢复：l0033至prior_terminal_escape的十四项完整测试，移parser_call_argument/recovery.rs，
   覆盖marker/separator、nested owner、terminal interpolation与closer边界；不拆断言。

根全部imports、共享helper及唯一重复parse/lex harness留原位。子模块私有声明加use super::*；
原#[test]、正文、fixture与Span/错误码/恢复树及确定性oracle逐字节不变，FQN只增加领域层。
无pub/wrapper/新allow/ignore，不改生产代码或规范。两根与新文件均小于1000，只删除两根
baseline，其他policy等值，不提高尺寸额度或为了数量删除合理保留。

## 2. 验收

- [ ] R1：完整测试及剩余根机械保全，无重复，按test leaf和新增模块FQN一一核对。
- [ ] R2：逐片原完整target前后通过、本地提交，不逐片push、不伪造RED。
- [ ] R3：整组完整frontend、all-target Clippy、fmt/docs/尺寸/policy/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步，集中push同PR，精确新头CI另验，保持active/Draft。

原始日志与快照留仓库外；未运行、ignored/compile/list分别记，不改断言绕过失败。

## 3. 真实证据

待逐片记录。
