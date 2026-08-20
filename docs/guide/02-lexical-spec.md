# Koven 语言设计规范 · 完整词法规范

> 本文档是 Koven 语言设计规范多文档结构的一部分（原单文件 guide 第三部分），完整文档
> 地图、版本治理规则与跨文件索引见 [`00-index.md`](./00-index.md)。内容版本：v0.14。
> v0.5 词法基线已由 SPEC-0006 实现并验收；v0.14 新增的单字符 `&` 尚待 SPEC-0012
> 实施。除这个明确增量外，本文档属于稳定基线，预期变更频率是全部拆分文档中最低的一份。

## 1. 硬关键字（42 个，按用途分类，不可作为标识符）

**声明相关**
```
class       companion   const       enum        extern
fun         import      interface   module      object
typealias   val         value       var         vararg
```

**控制流**
```
break       continue    else        for         if
in          is          loop        return      when
while
```

**所有权 / 借用 / 安全**
```
borrow      inout       move        own         unsafe
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

> `error` **不在此表中**——它是标准库顶层函数，不是关键字（[01-design-decisions.md](./01-design-decisions.md)第 3 节）。

> `own` **仍在此表中，数量不变**——v0.10/v0.11 候选一度让它出现在函数类型、具名参数与
> 调用实参三处产生式里；v0.12 取消了这一独立契约（并入 `Value`，见[01-design-decisions.md](./01-design-decisions.md)第 5 节与
> [05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节），`own` 因此不再被任何产生式接受。它**不**被移入第 3 节“保留但当前
> 版本未使用”表：那份表的规则是“lexer 每次遇到都无条件产生诊断”，只适用于从未在任何
> 已发布版本语法产生式中出现过的关键字；把已实现 lexer 行为不变的 `own` 塞进那份表会
> 让它凭空获得那条无条件诊断规则，属于不必要的行为变化。`own` 目前的状态与同样“已是
> 硬关键字、但没有任何产生式使用”的 `unsafe` 一致：词法层照常识别为 keyword token，
> parser 找不到匹配产生式时按普通语法错误处理，不触发第 3 节的专属诊断。

## 2. 软关键字（仅特定上下文有特殊含义，其余场景可作普通标识符）

```
to          infix（仅标准库内部使用）
```

两者在 lexer 中都始终是普通 `Identifier`。parser 只在[03-grammar-core.md](./03-grammar-core.md)第 4 节规定的表达式位置把拼写 `to`
解释成中缀运算符；`infix` 在表达式中仍是标识符，只能由后续标准库声明语法在其专用上下文
解释。

> `get` / `set` 自 v0.2 起就不是软关键字，lexer 始终把它们作为普通
> `Identifier`。[01-design-decisions.md](./01-design-decisions.md)第 8 节的顺序容器索引是预声明原语，parser 不查找这两个
> 名称或任何用户 operator 声明来建立 `[]` AST；本版也不因这两个普通名称
> 开放自定义索引能力。

## 3. 保留但当前版本未使用（预留给 v2/v3，禁止用作标识符）

```
async       await       suspend     actor       spawn
sealed      dyn         where       yield       macro
reify
```

> `dyn` 已在[01-design-decisions.md](./01-design-decisions.md)第 15 节明确说明：关键字保留，但对应语法降级到 v2，v1 不实现。

> Agent 注意：即便这些保留字当前版本没有对应语法功能，**lexer 阶段也应将其识别为保留标识符并拒绝用户用作变量/函数/类型名**，防止未来版本引入新语法时出现向后兼容问题。

## 4. 源文本与标识符

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

## 5. Trivia、换行与注释

- 普通词法模式中的 trivia 包括 ASCII space `U+0020`、tab `U+0009`、换行以及注释。
  其他 Unicode 空白字符不是隐式分隔符，按非法字符处理；这条限制不把注释、`Char`
  或 `String` 内容中的同一 scalar 重新解释为 trivia 或非法字符。
- 换行只可写为 LF 或 CRLF，每个序列是一个 newline trivia。裸 CR 产生非法字符诊断，且
  不单独开始新行；这与现行 source / `Span` 位置模型一致。换行不会自动插入分号，
  v1 也不接受源码分号。parser 通过语法边界而不是换行结束语句。
- `//` 开始行注释，直到 CR、LF 或 EOF 之前；终止换行不属于注释 token。
- `/*` 开始块注释，并由遇到的第一个 `*/` 结束；块注释不嵌套。v1 不区分 doc
  comment；`///`、`/** ... */` 与普通注释相同。未终止块注释产生对应词法诊断。
- 普通词法模式不接受 BOM 或 shebang；其中不属于任何合法 token 的字符产生非法字符
  诊断。注释或字面量内容中的 `U+FEFF` 仍按该模式的普通内容处理。
- lexer 保留每个 trivia 及其 `Span`：连续 space / tab 合并为一个最大 whitespace
  trivia；每个 LF / CRLF、行注释和块注释分别形成独立 trivia。parser 可以确定性跳过，
  后续 formatter 可以复用；trivia 不会附着到 AST 节点或改变表达式语义。

## 6. 字面量

### 布尔与空值

`true`、`false` 和 `null` 是硬关键字 token，不由通用标识符或数字规则解释。

### 整数与浮点

- v1 整数字面量是一个或多个 ASCII 十进制数字 `[0-9]+`。前导零不改变进制；
  负号始终是独立 `-` token。
- v1 浮点字面量是 `[0-9]+ '.' [0-9]+`。因此 `1.0` 合法，`.5` 和 `1.` 不是
  浮点 token。
- 数字扫描在范围运算符前停止；`1..2` 必须切分为 `1`、`..`、`2`，`1..<2`
  切分为 `1`、`..<`、`2`。
- v1 不支持十六进制、二进制、八进制、指数、数字分隔下划线或整数 / 浮点类型后缀。
  lexer 保留原始文本，不进行溢出、默认类型或目标类型判定；这些属于后续阶段。
- 扫描完整数或浮点候选后，如果紧邻 ASCII `[A-Za-z0-9_]`（如 `1e3`、`1.0e3`、
  `0x10`、`1L`、`1_0`），lexer 把完整数字候选及其后的最大连续 ASCII
  `[A-Za-z0-9_]*` 后缀合为一个非法数字区域并产生诊断，不得静默拆成合法数字和标识符。

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

## 7. 固定运算符、标点与最长匹配

lexer 识别下列固定符号：

```text
( ) [ ] { } , : @
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
  `&&`，不能先产生一个 `&` token 再产生第二个 `&` token（v0.14 新增：`&` 作为独立单字符
  token 加入固定符号表，此前 `&` 是非法字符）。注释开始符 `//` / `/*` 优先于
  单个 `/` 或 `/=` 规则。
- `as?`、`!in`、`!is` 是需要字符相邻的复合 token；`as ?`、`! in`、`! is` 按各自
  token 扫描。`!in` / `!is` 只有在 `in` / `is` 后为 EOF 或下一 scalar 不匹配
  ASCII `[A-Za-z0-9_]` 时才匹配；
  `!inside` 是 `!` 加 identifier `inside`。
- `(` / `)` 和 `[` / `]` 始终是各自独立的 delimiter token，不是成对复合 token。
- `inout`、`borrow`、`move`、`as`、`in`、`is` 的 keyword token 只记录词法分类；它们的
  合法语法位置由 parser 决定。`own` 同样始终产生 keyword token（词法层不受本次影响），
  但 v0.12 起没有任何产生式接受它，即合法语法位置的集合为空，见第 1 节。单字符 `&`
  token 同样只记录词法分类，不预设语法位置：v1 唯一接受它的产生式是
  [05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节的调用实参 `Inout` 标注入口，其余位置遇到 `&`
  一律是语法错误，不是词法错误；v1 不提供按位与运算符，`&` 不出现在通用表达式 prefix
  或 binary 层级（该用途留给 v2 位运算符设计）。
- `@` 是为 Phase 5 内建 `@Test` 预留的单字符 token；v1 不因此开放通用注解语法，
  在后续 guide 定义 `@Test` 的语法位置前，parser 应拒绝任何 `@` 用法。
- v1 不支持分号、`++`、`--`、除 `&` 外的 shift / bitwise 运算符、`#`、shebang 或 `...`；
  它们不得因 Kotlin 中存在而被默认接受。若其中字符各自是合法固定符号（如 `++`、
  `--`、`<<`、`...`），lexer 只产生逐个最长合法 token，由 parser 拒绝该组合；没有单字符
  token 的 `;`、`#`、单个 `|` 等产生非法字符诊断。

## 8. Token、`Span`、EOF 与错误恢复

- 每个普通 token、trivia、string segment 和 invalid token 都保留其原始非空 `Span`。EOF 是唯一
  固定的空 token，范围为 `[source.len(), source.len())`。空文件只产生 EOF。
- 所有 lexeme 按源文本顺序返回，它们的 `Span` 不重叠且联合覆盖 EOF 之前的全部有效
  UTF-8 字节。该顺序不依赖 hash 集合、文件路径或加载顺序。
- lexer 返回 token 序列和结构化诊断序列；发现一条非法用户输入时不得 `panic!`。
  诊断按既有全序输出，且每条至少有稳定错误码、主消息与精确主 `Span`。

词法错误类别和恢复边界如下。实施 Spec 必须在集中目录为每一行分配独立稳定错误码，不得
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
