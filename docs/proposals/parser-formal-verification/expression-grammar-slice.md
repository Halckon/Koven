# F3c：18 级表达式运算符层级参考文法与漂移校验

> **性质**：非规范试验账本 · **状态**：独立表达式 18 级层级已实跑零冲突与漂移注入，独立参考解析差分由 [SPEC-0485](../../specs/active/0485-f3c-expression-difftest.md) 承接 · **读取时机**：扩展表达式参考文法或对照 `operator.rs` 优先级表时 · **唯一真源**：本页维护 Guide 04 运算符层级规则 ID 映射、外部谓词清单与离线校验结果

基点 `main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，Guide v0.47。
接续[真实词法流首片](token-stream-probe.md)（块内换行与窄子集）并面向独立表达式入口 `parse_expression`。
本切片不含 D1–D3，不修改生产 Parser、Guide、依赖或 CI；离线试验使用 macOS 自带 GNU Bison 2.3，
探针在临时目录运行后已清理，未进入仓库。

## 1. Guide 04 规则 ID 映射（纯结构 CFG，无 `%left`/`%right`）

以下规则 ID 直接对应 [Guide 04「Primary 表达式」](../../guide/04-expressions-operators.md#primary-表达式)、
[「Postfix、调用、索引与函数引用」](../../guide/04-expressions-operators.md#postfix调用索引与函数引用)与
[「运算符层级与结合性」](../../guide/04-expressions-operators.md#运算符层级与结合性)，
并与生产单真源 `crates/lang-frontend/src/parser/engine/operator.rs` 的 18 级绑定顺序及四个不结合组逐一核对。

| 规则 ID | 层级 | Guide 04 产生式与运算符 | 结合性 |
|---|---:|---|---|
| `E-ATOM` | Primary | `ID`、`INT`、`true`、`false`、`null`、`this`、`( expression )`、`:: ID` | 基础项 |
| `E-TYPE-SIMPLE` | TypeRef 子集 | `ID`、`ID ?`（仅用于 `as`/`as?`/`is`/`!is` 右侧） | 右侧类型 |
| `E-P01` | 1 | `. ID`、`?. ID`、`( args? )`、`[ expr ]`、`!!`、`?`、`:: ID` | 左结合，可连续 |
| `E-P02` | 2 | prefix `!`、`+`、`-` | 右结合 |
| `E-P03` | 3 | `as`、`as?`（右操作数为 `E-TYPE-SIMPLE`） | 左结合 |
| `E-P04` | 4 | `*`、`/`、`%` | 左结合 |
| `E-P05` | 5 | `+`、`-` | 左结合 |
| `E-P06` | 6 | 移位软词 `shl`、`shr`、`ushr` | 左结合 |
| `E-P07` | 7 | 按位与软词 `and` | 左结合 |
| `E-P08` | 8 | 按位异或软词 `xor` | 左结合 |
| `E-P09` | 9 | 按位或软词 `or` | 左结合 |
| `E-P10` | 10 | `..`、`..<` | **不结合** |
| `E-P11` | 11 | 精确软词 `to` | 左结合 |
| `E-P12` | 12 | `?:` | **右结合** |
| `E-P13` | 13 | `in`/`!in`（右表达式）、`is`/`!is`（右 `E-TYPE-SIMPLE`） | **不结合**（同组互斥） |
| `E-P14` | 14 | `<`、`>`、`<=`、`>=` | **不结合** |
| `E-P15` | 15 | `==`、`!=` | **不结合** |
| `E-P16` | 16 | `&&` | 左结合 |
| `E-P17` | 17 | `\|\|` | 左结合 |
| `E-P18` | 18 | `=`、`+=`、`-=`、`*=`、`/=`、`%=` | **右结合** |

## 2. 外部依赖与未覆盖清单（不计入 G1）

以下构造依赖上下文谓词、邻接 Span 或未启用裁决，显式留在纯 CFG 之外：

1. **块级换行续行**：`.`/`-` 跨行语义已在 [F3a](grammar-tool-probe.md) 与 [F3c 真实词法流首片](token-stream-probe.md) 独立验证；
   本切片面向独立表达式，不混入块级 `NL` 分隔。
2. **邻接禁符谓词**：`++`、`--`、`<<`、`>>`、`...` 须由 Span 邻接检查整体拒绝（`operator.rs::unsupported_operator`），
   不能由纯无空白 CFG 替代。
3. **调用实参上下文谓词**：未加括号的命名参数 `name = expr` 与 `&` 模式标注属于调用实参专用入口，
   与普通赋值表达式 `=` 需在 `call_argument` 规则层隔离。
4. **F3d / F3e 延后构造**：泛型 `typed_call_suffix` `<T>(...)` 与尾 lambda（D1）、lambda `return`/`move`（D2/D3）、
   `if`/`when`/jump/`super`、字符串插值与完整泛型/函数类型 `TypeRef`。

## 3. 离线实跑与漂移注入结果

环境：macOS AArch64，GNU Bison 2.3（`%expect 0`，无优先级声明，纯结构化产生式），`cc -Wall -Wextra -Werror`。
词法器为本试验内置最小扫描器（仅用于验证 CFG 优先级与不结合结构，不替代生产 Lexer 或 F3b 词法见证）。

| 验证项 | 结果 | 代表性见证 |
|---|---|---|
| LALR(1) 原始冲突检查 | 退出 0，**0 shift/reduce、0 reduce/reduce** | 全 18 级层级与 7 种基础后缀在 `%expect 0` 下零冲突 |
| 16 个手写正例 | **16/16 通过**，绑定树与 Guide 04 手填预期精确一致 | `a!!.b(c)[i]::ref!!`、`-a as Int * b`、`a + b shl c and d xor e or f`、`a or b..c to d ?: e ?: f`、`a ?: b in c < d == e && f \|\| g`、`a = b += c`、`(a < b) < c`、`(a..b)..<c`、`::top()` |
| 10 个手写反例 | **10/10 拒绝** | 覆盖全部 4 组不结合链（`a..b..<c`、`a..b..c`、`a in b is Int`、`a is Int !in b`、`a < b <= c`、`a > b >= c`、`a == b != c`、`a !is Int is Long`）及 `a +`、`a b` |
| 漂移注入 1（歧义） | 将 `additive_expression` 右部改为自递归 `additive '+' additive` → **Bison 退出非 0**，报告 `conflicts: 2 shift/reduce` | 证明 `%expect 0` 可直接拦截优先级/结合性引入的歧义 |
| 漂移注入 2（不结合失效） | 将 `comparison_expression` 改为左递归 → 反例 `a < b <= c` 被误接受为 `(<= (< a b) c)` | 证明反例见证可检出不结合组退化（对照 F4b `TS-04`） |
| 漂移注入 3（优先级互换） | 互换 `range_expression` 与 `to_expression` 层级 → `a..b to c` 漂移为 `(.. a (to b c))`（预期 `(to (.. a b) c)`） | 证明绑定树预期可检出静默优先级漂移 |

## 4. 证据边界与未运行项

- **证据等级**：本切片仅证明所列 18 级独立表达式子集的纯 CFG 在 Bison 2.3 下零冲突且 26 个正反例与 3 项漂移注入符合预期；
  **不构成全语言无歧义证明，也不替代真实生产 Lexer 流对比或 F4a 同语料差分**。
- **未运行项**：本轮沙箱 PATH 中无 `cargo`，未重跑生产 `parser_expression` 测试套件（既有基线见
  [F3c 真实词法流首片](token-stream-probe.md#3-验证与后续门槛) 的 `85 passed` 记录）；未运行远端 CI。
- **后继**：待 D1–D3 修订草案决定启用后，进入 F3d（泛型调用、尾 lambda、软词与完整 `TypeRef`）；
  或先将本切片的 18 级优先级/不结合用例接入 F4a 独立参考差分。
