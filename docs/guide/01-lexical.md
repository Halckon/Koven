# Koven v0.38：词法

> **性质**：规范性语言规范 · **状态**：current（v0.38） · **读取时机**：实现或评审 Lexer、token、字面量、注释与词法恢复时 · **唯一真源**：本页

本页是现行 Koven v0.38 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 词法错误与 Parser 交接

Lexer 产生的 invalid 或 reserved-word token 已有词法诊断。parser 在任何语法位置遇到它们
都须消费并在当前结构中放置 error node 以保证前进，不得在相同 `Span` 再发一条 parser
诊断；这也适用于独立入口的尾随位置和 delimiter / 名称恢复位置。除此之外，
若 Lexer 已对未终止 string / interpolation 报错并以恢复后的 segment 流抵达 EOF，parser
保留可构造的 string / error AST，但不得再为同一缺失结束符报告 expected closing delimiter；
Lexer 的对应诊断已经完整表达该根因。这项抑制不吞掉发生在字符串内容中的独立 parser 错误。
表达式规则至少区分下列语法错误含义；稳定 `L` 码、固定消息与精确恢复用例由诊断目录统一维护：

| 类别 | 最小局部恢复语义 |
|---|---|
| expected expression | 消费一个不可能开始表达式的 token 形成 error node；若已到当前 stop token / EOF，则不消费并形成空 error node |
| expected closing delimiter | 保留已解析的内部节点；在当前结构的匹配闭合符、调用 / 索引分隔符、插值 stop 或 EOF 处停止，不跨越外层安全边界 |
| expected member/reference name | `.`、`?.` 或 `::` 后缺名称时，在下一个 postfix / binary 边界前结束该 suffix，不把后续结构误挂为名称 |
| non-associative chain | 消费同组第二个运算符及其可解析右操作数作为错误区域，并保留第一段合法 AST |
| unexpected trailing token | 独立入口已得到表达式后仍有 token 时，从首个尾随 token 前进到当前 stop token，不静默成功 |
| expected type reference | 在 cast / type 参数需要类型处消费一个非法起始 token；遇当前 delimiter / stop 时不越界 |
| unsupported operator | 消费无 trivia 相邻的整个 `++`、`--`、`<<`、`>>` 或 `...` 组合，不把组合拆成合法 AST |
| unsupported argument form | L0016 是保留的兼容诊断码，现行 parser 不再产生且不得复用或改变含义；非法实参形态使用[调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)的专用类别 |

这里仅要求表达式内部的最小、确定性恢复，并保证每次错误都消费输入或抵达明确 stop token；
完整文件、跨声明同步和“单个语法错误后继续解析后续声明”的策略见
[完整文件与声明分隔](02-names-files-packages.md#完整文件与声明分隔)。

## 硬关键字

**声明相关**
```
class       companion   const       enum        extern
fun         import      interface   object      package
typealias   val         var         vararg
```

**控制流**
```
break       continue    else        for         if
in          is          return      when        while
```

**所有权 / 借用 / 安全**
```
unsafe
```

**可见性**
```
internal    private     public
```

**其他（字面量 / 表达式相关）**
```
as          false       null        operator    override
super       this        true
```

> `error` **不在此表中**——它是标准库顶层函数，不是关键字（[空安全与错误值规则](09-nullability-errors.md)）。

> `package` 只在完整文件头产生式中有特殊含义；`module` 是普通 `Identifier`。Koven 不接受
> Rust 风格的 `mod` / `use`，也不把 `module` 作为它们的同义词。

> `unsafe` 同样是硬关键字，但 v0.38 没有使用它的产生式。

## 上下文关键字与软关键字

下列拼写在词法分析阶段均保持普通 `Identifier`，仅由 Parser 在特定产生式与语法上下文中赋予特殊关键字含义；在其他语法位置（包括字段名、成员访问、函数名、局部变量名等）继续作为普通标识符使用：

### 声明修饰与控制流上下文关键字

```
value       loop        own         borrow      inout       move
```

- `value`：仅在类型声明起始处修饰 `value class` 时作为关键字；在成员名（如 `node.value`、`owner.value`）、函数参数名（如 `println(value: String)`、`Ok(value: T)`）或局部变量名时均为普通 `Identifier`。
- `loop`：仅在无条件循环语句（`loop { ... }`）起始处识别为控制流关键字；在成员名或变量名处为普通 `Identifier`。
- `own` / `borrow` / `inout`：仅在具名函数参数模式（如 `own x: T`、`borrow y: T`、`inout z: T`）或函数类型签名（如 `(own T) -> R`）的参数模式位置识别为模式关键字；在方法调用或标识符位置（如 `cell.borrow()`、`own()`）均为普通 `Identifier`。
- `move`：仅在 lambda 表达式前缀（`move { ... }`）或逃逸函数类型前缀（`move () -> R`）识别为逃逸/移动捕获关键字；在成员名或方法调用位置（如 `player.move()`）均为普通 `Identifier`。

### 表达式中缀与接口软关键字

```
to          by          infix（仅标准库内部使用）
and         or          xor         shl         shr         ushr
```

- `to`：只在[表达式与运算符规则](04-expressions-operators.md)规定的表达式位置解释为构建 `Pair` 的中缀运算符。
- `by`：只在[class-family 与成员规则](08-class-family-members.md)普通 class 的 supertype entry 中解释为接口委托标记。
- `and` / `or` / `xor` / `shl` / `shr` / `ushr`：只在整数表达式位置解释为具名中缀位运算符（见[表达式与运算符规则](04-expressions-operators.md)），不占用 `&` 符号。
- `infix`：在表达式中仍是标识符，仅由后续标准库声明语法在其专用上下文解释。

> `get` / `set` 不是软关键字，lexer 始终把它们作为普通
> `Identifier`。[集合、索引与解构规则](12-collections-destructuring.md)的顺序容器索引是预声明原语，parser 不查找这两个
> 名称或任何用户 operator 声明来建立 `[]` AST；现行语言也不因这两个普通名称
> 开放自定义索引能力。

## 未来保留字

```
async       await       suspend     actor       spawn
sealed      dyn         where       yield       macro
reify
```

> `dyn` 已在[一致性与 Phase 边界](15-conformance-and-staging.md)明确说明：关键字保留，但对应语法降级到 v2，v1 不实现。

即便这些拼写没有现行语法功能，lexer 也必须将其识别为保留字并拒绝用作变量、函数或类型名。

## 源文本与标识符

- Koven 源文件必须是有效 UTF-8。文件加载器在进入 lexer 前拒绝非 UTF-8 字节；lexer
  处理 Unicode scalar 并使用已有的 UTF-8 字节 `Span`。
- 非转义标识符的首字符匹配 ASCII `[A-Za-z_]`，后续字符匹配 ASCII
  `[A-Za-z0-9_]*`。单独 `_` 也是普通标识符；v1 不为它赋予丢弃或通配含义。
  Unicode scalar 仍可出现在注释、`Char` 和 `String` 中，但不能组成标识符。
- 标识符大小写敏感，不做 case folding。
- lexer 必须先扫描完整标识符，再与硬关键字和未来保留字做大小写敏感的
  精确比较。例如 `class` 和 `className` 分别是硬关键字和标识符；`classβ` 扫描为
  硬关键字 `class` 后跟非法字符 `β`，不得把整段当作 Unicode 标识符。
- 硬关键字产生对应 keyword token。软关键字在 lexer 中仍产生普通 identifier token，
  由 parser 在已定上下文解释。未来保留字当前没有合法语法位置；lexer 每次遇到
  都产生“未来保留字不可用”诊断，同时保留 reserved-word token 以便恢复。
- `error`、`get`、`set`、基础类型名和 `Copyable` 都按普通标识符扫描。v1 不支持
  反引号或其他转义标识符，也不允许通过转义绕过硬关键字或保留字。

后续若扩展为 Unicode 标识符，必须由新 guide 明确字符属性、Unicode 数据版本、规范化和
升级兼容策略，不得通过宿主语言或依赖版本静默扩大合法标识符集合。

## Trivia、换行与注释

- 普通词法模式中的 trivia 包括 ASCII space `U+0020`、tab `U+0009`、换行以及注释。
  其他 Unicode 空白字符不是隐式分隔符，按非法字符处理；这条限制不把注释、`Char`
  或 `String` 内容中的同一 scalar 重新解释为 trivia 或非法字符。
- 换行只可写为 LF 或 CRLF，每个序列是一个 newline trivia。裸 CR 产生非法字符诊断，且
  不单独开始新行；这与现行 source / `Span` 位置模型一致。换行不会改写或插入 `;` token，
  但完整文件 parser 会把顶层声明之间实际出现的 LF / CRLF 识别为声明分隔，且 block 内
  在不存在换行续行条件时由行末换行结束当前 element（见 Block 与控制流）；独立表达式与
  独立声明入口仍按各自产生式处理。
- `//` 开始行注释，直到 CR、LF 或 EOF 之前；终止换行不属于注释 token。
- `/*` 开始块注释，并由遇到的第一个 `*/` 结束；块注释不嵌套。v1 不区分 doc
  comment；`///`、`/** ... */` 与普通注释相同。未终止块注释产生对应词法诊断。
- 普通词法模式不接受 BOM 或 shebang；其中不属于任何合法 token 的字符产生非法字符
  诊断。注释或字面量内容中的 `U+FEFF` 仍按该模式的普通内容处理。
- lexer 保留每个 trivia 及其 `Span`：连续 space / tab 合并为一个最大 whitespace
  trivia；每个 LF / CRLF、行注释和块注释分别形成独立 trivia。parser 可以确定性跳过，
  后续 formatter 可以复用；trivia 不会附着到 AST 节点或改变表达式语义。

## 字面量

### 布尔与空值

`true`、`false` 和 `null` 是硬关键字 token，不由通用标识符或数字规则解释。

### 整数与浮点

- v1 整数字面量支持十进制、十六进制与二进制形态，并支持可选的数字内部分隔下划线：
  - 十进制：一个或多个十进制数字 `[0-9]`，允许在数字之间包含单下划线 `_`（如 `1_000_000`）。前导零不改变进制；
  - 十六进制：以 `0x` 或 `0X` 前缀引导，后跟一个或多个十六进制数字 `[0-9a-fA-F]`，允许在数字之间包含单下划线 `_`（如 `0xFF_AA_00`）；
  - 二进制：以 `0b` 或 `0B` 前缀引导，后跟一个或多个二进制数字 `[01]`，允许在数字之间包含单下划线 `_`（如 `0b1010_0110`）。
  - 下划线 `_` 仅能出现在两个数字之间；不得紧邻 `0x`/`0b` 前缀首位（`0x_1` 非法）、不得出现在数字末尾（`100_` 非法），亦不得连续出现（`1__0` 非法）。
  - 负号始终是独立 `-` token。
- v1 浮点字面量是 `[0-9]+ '.' [0-9]+`，允许在整数与小数部分内部包含数字间下划线（但小数点两侧不得紧邻下划线，`1_.0` 与 `1._0` 非法）。因此 `1.0` 合法，`.5` 和 `1.` 不是浮点 token。
- 整数字面量（十进制、十六进制、二进制）可紧邻以下精确后缀：`L` 表示 `Long`；`u` / `U` 表示无符号整数约束；`uL` / `UL` 表示 `ULong`。不接受小写 `l`，也不接受 `ul` / `Ul` 或错序组合。
- 浮点核心可紧邻 `f` / `F` 表示 `Float`；无后缀浮点固定为 `Double`。纯十进制数字 `[0-9]+` 紧邻 `f` / `F` 也直接构成 `Float` 字面量，因此 `1f` 合法。
- 数字扫描在范围运算符前停止；`1..2` 必须切分为 `1`、`..`、`2`，`1..<2` 切分为 `1`、`..<`、`2`。
- v1 不支持八进制、科学计数法指数或上述集合之外的类型后缀。lexer 保留原始文本并规范化记录进制与后缀身份，不进行数值解析、溢出、默认类型或 expected-type 判定；这些属于后续阶段。
- 扫描完整数或浮点候选后，如果紧邻非法 ASCII `[A-Za-z0-9_]`（如 `1e3`、`1.0e3`、`1l`、`1LU`、`1.0L` 或错位下划线），且它们不构成上面的完整合法后缀，lexer 把数字候选及其后的最大连续 ASCII `[A-Za-z0-9_]*` 后缀合为一个非法数字区域并产生诊断，不得静默拆成合法数字和标识符。

### `Char`

- `Char` 字面量由单引号包围，解码后必须恰好是一个 Unicode scalar。
- 可用转义是 `\\`、`\'`、`\"`、`\n`、`\r`、`\t` 和 `\0`。v1 不定义数字形式的
  Unicode 转义。
- 空字符、多个 scalar、未知 / 非法转义、未闭合字面量以及未转义 CR / LF 都产生
  `Char` 字面量诊断。

### `String` 与插值

- 常规字符串由双引号包围，不得包含未转义 CR / LF。可用转义与 `Char` 相同，
  并额外允许 `\$`。v1 不支持三引号 / raw / 多行字符串。
- `${` 开始字符串插值表达式，与它嵌套深度匹配的 `}` 结束插值并返回字符串
  模式。插值内使用完整普通 token 规则，并可包含块、调用或嵌套字符串；只有插值普通
  模式产生的 `{` / `}` 改变该层深度，注释、`Char` 和嵌套字符串中的花括号不参与匹配。
- `$name` 简写不属于 v1；未紧跟 `{` 的 `$` 是普通字符串文本。`\$` 可在需要明确表达
  字面 `$` 时使用，尤其可用 `\${` 表示不会开始插值的字面 `${`。
- lexer 保留 string-start、string-text、interpolation-start、插值内普通 token、
  interpolation-end 和 string-end 的独立 `Span`。转义的值解码属于 parser / literal 边界，
  token 保留原始源文本。每个 string-text 是结束引号、`${`、非法转义或未转义 CR / LF
  之间的最大非空区域；合法转义保留在所属 string-text 中，不产生空 text token。
- 未闭合字符串在该字符串开始到未转义 CR / LF 之前或 EOF 的范围产生未终止字符串诊断，且
  不消费换行。插值表达式内的换行按普通 trivia 处理；只有到 EOF 仍未找到与 `${`
  匹配的 `}` 时，才以该 `${` 到 EOF 为主 `Span` 产生未终止插值诊断。EOF 处存在
  多层未闭合词法模式时只报告最内层错误，不再为外层字符串 / 插值产生级联诊断。
  非法转义产生独立的字符串转义诊断。

## 固定运算符、标点与最长匹配

lexer 识别下列固定符号：

```text
( ) [ ] { } , : ; @
. ?. ? ?: !! !
:: ->
* / % + -
.. ..<
< > <= >= == !=
& && ||
+= -= *= /= %= =
```

- 在同一起点可匹配多个固定符号时必须取最长匹配。因此 `..<`、`?.`、`?:`、
  `!!`、`::`、`->`、`<=`、`&&` 和复合赋值均不得拆分——`&` 后紧跟 `&` 时必须整体识别为
  `&&`，不能先产生一个 `&` token 再产生第二个 `&` token。注释开始符 `//` / `/*` 优先于
  单个 `/` 或 `/=` 规则。
- `as?`、`!in`、`!is` 是需要字符相邻的复合 token；`as ?`、`! in`、`! is` 按各自
  token 扫描。`!in` / `!is` 只有在 `in` / `is` 后为 EOF 或下一 scalar 不匹配
  ASCII `[A-Za-z0-9_]` 时才匹配；
  `!inside` 是 `!` 加 identifier `inside`。
- `(` / `)` 和 `[` / `]` 始终是各自独立的 delimiter token，不是成对复合 token。
- `inout`、`borrow`、`own`、`move`、`as`、`in`、`is` 的 keyword token 只记录词法分类；它们的
  合法语法位置由 parser 决定。`own` 只可作为具名值参数或函数类型参数的声明端 mode，
  不能出现在调用实参或普通表达式中。单字符 `&`
  token 同样只记录词法分类，不预设语法位置：v1 唯一接受它的产生式是
  [调用、lambda 与 closure 规则](07-calls-lambdas-closures.md)的调用实参 `Inout` 标注入口（如 `foo(&x)`），其余位置遇到 `&`
  一律是语法错误，不是词法错误；Koven 的位运算采用 Kotlin 风格的命名中缀操作符（`and`、`or`、`xor` 等，见[表达式与运算符规则](04-expressions-operators.md)），不使用 `&` 作为按位与运算符。
- `;` 在完整文件的顶层声明分隔以及 block / lambda 内同行多语句分隔时合法；它不属于
  expression，独立声明入口仍要求匹配至 EOF。
- `@` 是为 Phase 5 内建 `@Test` 预留的单字符 token；v1 不因此开放通用注解语法，
  在后续 guide 定义 `@Test` 的语法位置前，parser 应拒绝任何 `@` 用法。
- v1 不支持把分号用作无条件的全局 statement terminator，也不支持 `++`、`--`、C 风格符号移位/位运算符（`<<`、`>>`、单个 `|`、`^`、`~` 等，Koven 统一使用具名中缀操作符 `shl`、`shr`、`ushr`、`and`、`or`、`xor`）、`#`、shebang 或 `...`；
  它们不得因 Kotlin 或 C/Rust 中存在而被默认接受。若其中字符各自是合法固定符号（如 `++`、
  `--`、`<<`、`...`），lexer 只产生逐个最长合法 token，由 parser 拒绝该组合；没有单字符
  token 的 `#`、单个 `|` 等产生非法字符诊断。

## Token、`Span`、EOF 与错误恢复

- 每个普通 token、trivia、string segment 和 invalid token 都保留其原始非空 `Span`。EOF 是唯一
  固定的空 token，范围为 `[source.len(), source.len())`。空文件只产生 EOF。
- 所有 lexeme 按源文本顺序返回，它们的 `Span` 不重叠且联合覆盖 EOF 之前的全部有效
  UTF-8 字节。该顺序不依赖 hash 集合、文件路径或加载顺序。
- lexer 返回 token 序列和结构化诊断序列；发现一条非法用户输入时不得 `panic!`。
  诊断按既有全序输出，且每条至少有稳定错误码、主消息与精确主 `Span`。

词法错误类别和恢复边界如下。实现必须在集中目录为每一行分配独立稳定错误码，不得
把不同含义复用为同一错误码：

| 类别 | 主 `Span` 与恢复 | Lexeme 形态 |
|---|---|---|
| 非法字符 | 覆盖并消费一个不属于合法 token 的 Unicode scalar；裸 CR 也按此处理 | 同范围 invalid |
| 未来保留字 | 覆盖并消费完整保留字 | 同范围 reserved-word token，不另造 invalid |
| 未终止块注释 | 从 `/*` 覆盖并消费到 EOF | 同范围 invalid，不保留 block-comment trivia |
| 未终止字符串 | 从开始引号覆盖到未转义 CR / LF 之前或 EOF，不消费换行 | 保留所有已产生的 string / interpolation lexeme，不另造重叠 invalid |
| 未终止插值 | 从最内层未闭合 `${` 覆盖并消费到 EOF | 保留已产生的 string / interpolation lexeme，不另造重叠 invalid |
| 非法字符串转义 | 后有 scalar 时覆盖反斜杠和该 scalar 并继续字符串；反斜杠后直接是 CR / LF / EOF 时只覆盖反斜杠、结束当前字符串恢复，且不再追加未终止字符串诊断 | 同范围 invalid；前后 string-text 仍分别取最大非空区域 |
| 非法 `Char` 字面量 | 覆盖从开始引号到结束引号、未转义 CR / LF 之前或 EOF；不消费换行 | 整个范围为一个 invalid，不再拆 `Char` 内部 token |
| 非法数字 | 覆盖并消费本节定义的完整非法数字区域 | 同范围 invalid |

错误恢复后的 lexeme 流仍须覆盖全部输入。若错误产生 invalid lexeme，它只能覆盖尚未由其他
lexeme 表示的已消费字节，不得为覆盖整条诊断而与既有 string segment 等 lexeme 重叠，也
不得产生零长度错误 token 或在同一 offset 循环。EOF 处多层模式按上一节规则只报告最内层
错误；诊断 `Span` 可以包含多个相邻 lexeme 的范围，但 lexeme 自身不得重叠。

字符串模式恢复还必须满足以下例子，其中 `<LF>` / `<EOF>` 表示输入边界而不是源码文本：

- `"abc<LF>next` 产生未终止字符串诊断，弹出当前字符串模式且不消费 LF；随后 LF 是普通
  newline trivia，`next` 按普通模式扫描。
- `"${ "abc<LF>next }"` 在内层字符串上产生一次未终止字符串诊断，只弹出内层字符串；
  LF 及后续 `next` 继续按当前插值模式扫描，匹配 `}` 后回到外层字符串模式。
- `"abc\<EOF>` 只产生一次非法字符串转义诊断，反斜杠是 invalid lexeme；不再追加
  未终止字符串诊断。若该字符串嵌套于未终止插值，EOF 也不再为外层模式追加级联诊断。

---
