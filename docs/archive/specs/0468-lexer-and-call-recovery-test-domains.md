# SPEC-0468：Lexer 与调用恢复测试领域归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批lexer/parser测试职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
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
3. parser调用恢复：l0033至prior_terminal_escape的十七项完整测试，移parser_call_argument/recovery.rs，
   覆盖marker/separator、nested owner、terminal interpolation与closer边界；不拆断言。

根全部imports、共享helper及唯一重复parse/lex harness留原位。子模块私有声明加use super::*；
原#[test]、正文、fixture与Span/错误码/恢复树及确定性oracle逐字节不变，FQN只增加领域层。
无pub/wrapper/新allow/ignore，不改生产代码或规范。两根与新文件均小于1000，只删除两根
baseline，其他policy等值，不提高尺寸额度或为了数量删除合理保留。

## 2. 验收

- [x] R1：完整测试及剩余根机械保全，无重复，按test leaf和新增模块FQN一一核对。
- [x] R2：逐片原完整target前后通过、本地提交，不逐片push、不伪造RED。
- [x] R3：整组完整frontend、all-target Clippy、fmt/docs/尺寸/policy/空白与独立精确diff审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

原始日志与快照留仓库外；未运行、ignored/compile/list分别记，不改断言绕过失败。

## 3. 真实证据

合同38ef37f4，三个切片本地提交后统一验证，不逐片push。

| 切片 | 提交 | 完整target实际结果 | 尺寸 |
|---|---|---|---|
| literals四项 | 3c0e08d4 | lexer前后20/0/0 | 新209行 |
| symbols三项 | b99365fb | lexer前后20/0/0 | 根1178→788，新190 |
| recovery十七项 | f7211d6a | parser_call_argument前后31/0/0 | 根1512→794，新723 |

原51项按leaf与预期模块前缀一一对应，无漏重；24个迁移测试包含原#[test]和全部
fixture/assert正文逐字节保全。两根只删完整块并插入私有显式path模块声明，shared
helper/import/重复lex与parse harness留原位。policy仅删lexer1178与parser1512两项
baseline，其他JSON等值，无exception/allow/ignore变化。

首片初次默认mod寻址失败E0583，随后显式#[path]正确定位tests/lexer/literals.rs，
原失败日志保留，未当行为RED；后续完整lexer20/0/0。初始范围文字误记恢复域十四项，
机械核对与独立审查确认为十七项，已更正合同，不删测满足旧数字。

组末全部frontend目标 **3779 passed、0 failed、13 ignored**（含12实际通过的
compile-fail doctest），ignored不计执行，不重复累计定向51项或此前组覆盖。
frontend all-target Clippy -D warnings、fmt、policy47、docs965、尺寸/空白退出0；
2176手写Rust、历史超限 **18→16**，本PR从21累计退休五根。

独立fresh review完整38ef37f4..f7211d6a真实diff，补查共享helper/harness与前后日志，
No issues found：确认token/span/source-local、committed prefix、nested owner、terminal
cascade与invalid region边界断言保全；审查者只读、不Cargo，不伪称独立重跑测试。
Architecture同步私有测试域，原始证据/机械核对/完整diff与review仓库外保存。

Rust代码组末门禁后未再改。未本地重跑下游codegen/CLI/LSP全套、CI-only fault/真实
校准或sanitizer，不以本批纯测试重组扩大现有能力；R4集中push与精确新头CI另验，保持
active/Draft，不以旧头CI代替，不归档或合入。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
