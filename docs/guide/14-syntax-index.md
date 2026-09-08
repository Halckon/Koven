# Koven v0.34：语法索引

> **性质**：规范性导航索引 · **状态**：current（v0.34） · **读取时机**：从 token 或语法形式定位规则时 · **唯一真源**：链接指向的领域正文

本页不重复关键字表、产生式或优先级，只提供到唯一规则正文的入口。

| 领域 | 语法形式 | 唯一规则 |
|---|---|---|
| 词法 | 硬关键字 | [硬关键字](01-lexical.md#硬关键字) |
| 词法 | `to`、`by`、`infix` 软关键字 | [软关键字](01-lexical.md#软关键字) |
| 词法 | 未来保留字 | [未来保留字](01-lexical.md#未来保留字) |
| 词法 | Identifier 与源文本 | [源文本与标识符](01-lexical.md#源文本与标识符) |
| 词法 | trivia、换行与注释 | [Trivia、换行与注释](01-lexical.md#trivia换行与注释) |
| 词法 | 数值、字符、字符串与插值字面量 | [字面量](01-lexical.md#字面量) |
| 词法 | 固定运算符、标点与最长匹配 | [固定运算符、标点与最长匹配](01-lexical.md#固定运算符标点与最长匹配) |
| 词法 | token、Span 与词法恢复 | [Token、Span、EOF 与错误恢复](01-lexical.md#tokenspaneof-与错误恢复) |
| 文件与名称 | 完整文件和声明分隔 | [完整文件与声明分隔](02-names-files-packages.md#完整文件与声明分隔) |
| 文件与名称 | `package` / `import` 文件头 | [Package 与 Import 文件头](02-names-files-packages.md#package-与-import-文件头) |
| 文件与名称 | 限定名称、alias 与 wildcard import | [跨文件 Package 与 Import](02-names-files-packages.md#跨文件-package-与-import) |
| 类型 | `type_ref`、泛型实参与函数类型 | [TypeRef 与函数类型语法](03-types-generics.md#typeref-与函数类型语法) |
| 表达式 | primary 与 bound reference | [Primary 表达式](04-expressions-operators.md#primary-表达式) |
| 表达式 | member、call、index、postfix 与函数引用 | [Postfix、调用、索引与函数引用](04-expressions-operators.md#postfix调用索引与函数引用) |
| 表达式 | prefix、binary、range、Elvis、assignment | [运算符层级与结合性](04-expressions-operators.md#运算符层级与结合性) |
| 声明 | `val`、`var`、`const val`、`fun` | [声明语法](05-declarations-callables.md#声明语法) |
| 声明 | callable 参数 marker 与函数返回标注 | [Callable 与函数值](05-declarations-callables.md#callable-与函数值) |
| 声明 | 无体、表达式体与 block body | [函数 Body 三形态](06-blocks-control-flow.md#函数-body-三形态) |
| 控制流 | block 与 block element | [Block 与函数 Body](06-blocks-control-flow.md#block-与函数-body) |
| 控制流 | `if`、`when`、`while`、`for`、`loop`、jump | [控制流语法与语义](06-blocks-control-flow.md#控制流语法与语义) |
| 调用与 Lambda | lambda literal、header 与 `return` 边界 | [Lambda literal](07-calls-lambdas-closures.md#lambda-literal) |
| 调用与 Lambda | 尾 lambda | [尾 lambda 调用糖](07-calls-lambdas-closures.md#尾-lambda-调用糖) |
| 调用与 Lambda | 隐式 `it` | [无显式 header lambda 的隐式 `it`](07-calls-lambdas-closures.md#无显式-header-lambda-的隐式-it) |
| 调用与 Lambda | named、`borrow`、`&` 调用实参 | [Typed call argument](07-calls-lambdas-closures.md#typed-call-argument) |
| 类型声明 | class/value/interface/enum/object/companion | [Class Family 声明](08-class-family-members.md#class-family-声明) |
| 类型声明 | instance receiver marker | [Receiver Grammar](08-class-family-members.md#receiver-grammar) |
| 类型声明 | `Interface by field` | [接口委托边界](08-class-family-members.md#接口委托边界) |
| 空安全与错误 | `?:`、`!!`、nullable 与 `error()` | [`error()` 与空安全运算符](09-nullability-errors.md#error-与空安全运算符) |
| 空安全与错误 | postfix `?` 与 `Result<T, E>` | [`Result<T, E>` 与 Postfix `?`](09-nullability-errors.md#resultt-e-与-postfix-) |
| 所有权 | Value/Borrow/Inout、loan 与 place | [调用期借用与 ASAP 析构](10-ownership-borrowing-drop.md#调用期借用与-asap-析构) |
| 构造 | class/value/enum/Box 构造 | [名义值与 Intrinsic Box 构造](11-copyability-layout-construction.md#名义值与-intrinsic-box-构造) |
| 集合与解构 | 顺序容器构造、index 与 element place | [顺序容器的表示与索引语义](12-collections-destructuring.md#顺序容器的表示与索引语义) |
| 集合与解构 | 局部 `val` 解构 | [局部 `val` 解构语法](12-collections-destructuring.md#局部-val-解构语法) |
| 程序入口 | conventional `main` | [Conventional `main`](13-program-runtime-standard-library.md#conventional-main) |
| 程序入口 | project selector 与 process entry | [Project 与 Process Entry](13-program-runtime-standard-library.md#project-与-process-entry) |
