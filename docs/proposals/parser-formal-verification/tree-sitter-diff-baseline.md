# F4b：Tree-sitter 共同子集、已知漂移与差分基线

> **性质**：非规范验证账本 · **状态**：静态交集与漂移基线已核对，首片实跑差分由 [SPEC-0481](../../specs/active/0481-tree-sitter-difftest.md) 承接（TS-06 已随 [SPEC-0479](../../archive/specs/0479-explicit-block-and-crossline-lambda.md) 闭环；TS-03/TS-05 分号部分/TS-07 已随 [SPEC-0488](../../specs/active/0488-tree-sitter-drift-closure.md) 闭环；TS-01/TS-02/TS-04 已随 [SPEC-0491](../../archive/specs/0491-tree-sitter-remaining-drift-closure.md) 闭环；TS-05 同行无分隔符语句拒绝已随 [SPEC-0497](../../specs/active/0497-ts05-block-separator-closure.md) 闭环，基线登记的 TS-01 至 TS-07 共 7 项漂移已全部收口） · **读取时机**：设计 F4b Tree-sitter 差分切片或同步编辑器文法前 · **唯一真源**：本页维护 Tree-sitter 共同子集、已核对漂移清单与差分验收边界

基点 `main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，Guide v0.47（后续随 `SPEC-0479` 合入 `main@8207b212`，TS-06 尾 lambda 已同步纳入 `editors/tree-sitter` 与共同子集，首片实跑验证见 [SPEC-0481](../../specs/active/0481-tree-sitter-difftest.md)）。
核对对象为仓库既有资产 `editors/tree-sitter/grammar.js`、`editors/tree-sitter/src/scanner.c`、
`editors/tree-sitter/test/corpus/koven.txt` 与 `editors/tree-sitter/test/test_contract.py`。
本轮不修改 Tree-sitter 文法、生产 Parser、Guide 或 CI，不新增依赖。

## 1. 角色定位与入口边界

Tree-sitter 文法服务于编辑器增量高亮与结构导航，**不是第二份语言规范**。
F4b 只在双方明确声明共同支持的子集上核对接受性与粗粒度结构一致，并将未支持构造或已知文法漂移显式归因，
不能把未解释差异算作通过，也不能以 Tree-sitter 接受作为 Guide 合法性依据。

- **入口映射**：`editors/tree-sitter/grammar.js` 只提供文件根规则 `source_file`，不提供独立的
  `parse_expression`、`parse_declaration` 或 `parse_block` 入口。因此 F4b 差分通道须统一通过完整文件
  输入对齐生产 `parse_file`（例如用 `fun f(): Unit { ... }` 包装块或表达式样本），不与 F4a 的独立
  `parse_block` 入口混计。
- **GLR 冲突事实**：`grammar.js` 显式声明了 8 组 GLR `conflicts`（含 `[binary_expression,
  prefix_expression, call_expression]`、`[block, lambda_expression]`、`[qualified_name,
  _primary_expression]` 等）。因此 Tree-sitter 仅提供交叉观察信号，不提供无歧义证据。

## 2. 共同子集（首片候选范围）

以下构造在 Guide v0.47、`parse_file` 与 `editors/tree-sitter/grammar.js` 均有对应规则，且不含 D1–D3，
适合作为 F4b 首个 advisory 差分切片的共同子集：

| 类别 | 共同子集范围 | 排除项（不在首片交集） |
|---|---|---|
| 文件头与顶层声明 | `package`、`import`（含 `.*` 与 `as`）、`val`/`var`/`const val`、`fun`（表达式体与普通块体）、类族声明（`class`/`value class`/`interface`/`enum class`/`object`）、文件级 `\n`/`;` 分隔 | 裸 `\r`/`\f`、未终止注释、恢复场景 |
| 表达式与运算符 | 名称、字面量、括号分组、前缀 `!`/`+`/`-`、类型转换 `as`/`as?`、乘除模、加减、左结合 `to`、右结合 `?:`、逻辑 `&&`/`||`、右结合赋值、括号调用/命名参数/`&` 实参、索引、`.`/`?.` 成员、`!!`/`?` 后缀、函数引用 `::` | 具名位运算（`shl`/`shr`/`ushr`/`and`/`xor`/`or`）、链式不结合运算符、无空白相邻禁符（`++`/`--`/`<<`/`>>`/`...`）、尾 lambda |
| 控制流与软词 | `if`/`else`、`when`、`while`、`for`、`loop`、`return`/`break`/`continue`、同行 `move { ... }` 与 `move (...) -> R`、参数/接收者模式 `own`/`borrow`/`inout`、普通解构与保留字拒绝 | 块内分号分隔、跨行 `move\n{}`、块注释内换行触发的分隔/续行、D1–D3 草案规则 |

## 3. 静态审查发现的已知漂移（差分必须显式归因）

逐项核对 `grammar.js`、`scanner.c` 与现行 Guide v0.47 后，确认以下 **7 类真实差异**；
正式实跑差分时，落入下表的样本须归类为已知漂移或范围外，不得静默过滤：

| 编号 | 差异类别 | Guide v0.47 规范来源 | `editors/tree-sitter` 现状 | 差分影响 |
|---|---|---|---|---|
| TS-01 | 裸 `\r` 与 `\f` 空白处理 | [01 词法](../../guide/01-lexical.md#trivia换行与注释)（普通模式裸 CR 为词法错误） | `grammar.js` `extras` 将 `/[ \t\r\f]/` 当普通空白跳过；`scanner.c` 同步跳过 `\r`/`\f` | Tree-sitter 接受含裸 `\r`/`\f` 的非法源码；生产 `parse_file` 报词法错误 |
| TS-02 | 块注释内换行不可见 | [01 词法](../../guide/01-lexical.md#trivia换行与注释) + [06 边界](../../guide/06-blocks-control-flow.md#block-element结构边界与值语义) | `block_comment` 整体属于 `extras`，不触发 `_newline` | `a/*\n*/-b` 在生产侧拆分为两元素，在 Tree-sitter 侧仍视为同行二元减法 |
| TS-03 | 缺少 6 个具名位运算符 | [04 运算符层级](../../guide/04-expressions-operators.md#运算符层级与结合性)（优先级 6–9：`shl`/`shr`/`ushr`/`and`/`xor`/`or`） | `PREC`、`binary_expression` 与 `scanner.c` 均未定义这 6 个中缀运算符 | 合法位运算表达式在 Tree-sitter 产生 `ERROR` |
| TS-04 | 四个不结合组写成左结合 | [04 运算符层级](../../guide/04-expressions-operators.md#运算符层级与结合性)（`..`/`..<`、`in`/`!in`/`is`/`!is`、`<`/`>`/`<=`/`>=`、`==`/`!=` 分别不结合） | `grammar.js` 对这四组全部使用 `prec.left` | `a < b <= c`、`a == b != c`、`a..b..<c` 等非法链式表达式在 Tree-sitter 被接受 |
| TS-05 | 块内缺少分号与同行分隔约束 | [06 边界](../../guide/06-blocks-control-flow.md#block-element结构边界与值语义) | `block`、`control_block`、`lambda_expression` 仅写 `repeat(choice($._newline, $._statement))`，未纳入 `;`，也未强制同行相邻语句间出现分隔符 | 合法块内分号（`{ a; b }`）被 Tree-sitter 拒绝；同行无分隔语句可能被错误接受 |
| TS-06 | 不支持尾 lambda 调用 | [07 尾 lambda 调用糖](../../guide/07-calls-lambdas-closures.md#尾-lambda-调用糖) | `call_expression` 要求必有 `argument_list`，不支持 `f { ... }`、`f<T> { ... }`、`f() { ... }` | 合法尾 lambda 调用在 Tree-sitter 无法形成单一 `call_expression` |
| TS-07 | 跨行 `move` 与禁符组合 | [04 运算符层级](../../guide/04-expressions-operators.md#运算符层级与结合性)（`++`/`--`/`<<`/`>>`/`...` 整体拒绝）与 [07 Lambda](../../guide/07-calls-lambdas-closures.md#lambda-literal) | `scanner.c` 的 `peek_context` 不跨 `\n`；`grammar.js` 未单独拦截无空白相邻 `++`/`--` 等禁符 | `move\n{}` 与相邻 `--x`/`++x` 的接受性与生产侧不一致 |

## 4. 本机环境核对与后续实跑门槛

- **本机现状（macOS AArch64）**：`node`、`npm`、`cc`、`clang`、`python3` 均可用，但 PATH 中**未安装 `tree-sitter` CLI**，
  Python 环境亦无 `tree_sitter` 模块（`editors/tree-sitter/test/test_contract.py` 依赖 `TREE_SITTER` 指向的 CLI）。
  因此**本轮未执行 Tree-sitter 二进制解析，未宣称 F4b 差分已运行或通过**。
- **既有语料资产**：`editors/tree-sitter/test/corpus/koven.txt` 现有 10 组语料用例，`test_contract.py` 现有 9 个
  契约测试方法，可作为后续 F4b 实跑时验证 Tree-sitter 侧运行环境正常的前置冒烟检查。
- **进入实跑的放行条件**：
  1. 确定 F4b 离线运行方式（在本地/容器提供固定版本 `tree-sitter` CLI，或复用已生成 `src/parser.c` + `src/scanner.c`
     与固定版本 `libtree-sitter`；按[依赖治理](../../development/dependencies.md)审查，不进入默认 `cargo test`）。
  2. 先用 `editors/tree-sitter/test/test_contract.py` 与 `corpus/koven.txt` 验证 Tree-sitter 侧基线可执行。
  3. 对第 2 节共同子集构造完整文件正例语料，比较 `parse_file`（无错误诊断、无恢复节点）与 Tree-sitter XML/S-expr
     输出（无 `ERROR`/`MISSING`、顶层声明数与受支持表达式绑定形状一致），同时对 TS-01 至 TS-07 各附至少 1 条
     显式归因的已知差异对照用例。
  4. 明确结论边界：Tree-sitter 侧的漂移（如 TS-03/TS-04/TS-05/TS-06）是否修复属于编辑器文法维护议题，
     与生产 Parser 正确性解耦，不在 F4b 差分 Spec 中混改生产代码。
