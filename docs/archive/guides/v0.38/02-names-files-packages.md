# Koven v0.38：名称、文件、Package 与 Import

> **性质**：规范性语言规范 · **状态**：current（v0.38） · **读取时机**：实现或评审文件组合、名称解析、可见性与跨文件绑定时 · **唯一真源**：本页

本页是现行 Koven v0.38 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 单文件名称解析

名称解析只处理一份已经成功解析的
`ParsedFile`，建立稳定符号身份、词法作用域和名称引用；不从文件路径猜测 package，也不
展开 `import`。名称解析与类型检查是两个阶段：前者回答“这个拼写指向哪个声明或候选组”，
后者才回答“该声明是否适用于这里”。

### 双命名空间与声明身份

- 每个词法作用域分别维护**类型命名空间**和**值命名空间**。类型参数以及
  `value class` / `class` / `interface` / `enum class` / 具名 `object` 进入类型命名空间；
  `val` / `var` / `const val`、参数、局部绑定、函数和 enum 变体进入值命名空间。
- 同一拼写可以在同一作用域的两个不同命名空间各出现一次；引用所处的语法上下文决定查询
  哪一个命名空间。`type_ref` 查询类型空间，普通名称表达式查询值空间。classifier 在调用
  或 `Type.member` 位置形成的构造器 / 关联命名空间候选由后续类型检查从已解析的 type
  symbol 建立，不把 classifier 复制成第二个用户 value 声明。具名 `object` 额外产生同名
  singleton value，但两个身份仍共同追溯到同一声明。
- 每个成功收集的声明、参数和局部绑定获得一个只在本次解析产物内有效的稳定 `SymbolId`；
  每个作用域获得 `ScopeId`。ID 按确定性的源码遍历顺序分配，不等同于名称哈希或跨编译
  持久标识，也不得暴露随机集合的迭代顺序。
- 同一值作用域中的多个函数声明形成一个源码有序的 overload set。函数与非函数值同名、
  两个非函数值同名、同一类型作用域两个类型同名，均使用 L0079。两个函数是否具有重复
  签名、哪个 overload 最终适用，由类型检查在获得参数类型后判断；名称解析不按参数
  数量或源码 TypeRef 文本自行淘汰候选。

Kotlin 风格的 `UpperCamelCase` / `lowerCamelCase` / 常量大写只是源码风格，不是名称身份
的一部分，也不产生语言错误；语言仍按[词法规则](01-lexical.md)的 ASCII、大小写敏感 Identifier 精确匹配。

### 作用域与可见时点

- 文件作用域先按源码顺序收集全部顶层声明，再解析任何签名或 body，因此顶层类型、函数、
  常量和变量允许同文件前向引用与函数递归。package/import 不是本地声明；跨文件分析根据
  compilation-unit package index 构造环境，并继续使用本节相同查询规则。
- 每个 classifier 建立独立成员作用域。主构造器字段与 body 成员在成员 body 解析前完整
  收集，允许成员前向引用；函数可形成 overload set。companion 拥有单独的类型级值作用域，
  不把关联函数 / 常量注入实例成员作用域，实例成员也不反向注入 companion。
- 具名函数建立类型参数作用域和值参数作用域；参数在整个 body 中可见。函数自己的递归名称
  来自外层已收集的 overload set。类型参数在其后续上界、参数、返回类型和 body 内可见；
  同一参数列表或类型参数列表重名使用 L0079。
- lambda 参数在该 lambda body 内可见；未被 lambda 自己声明、但解析到外层值的引用记录为
  外层 symbol，捕获方式与 `move` 合法性仍由[所有权规则](10-ownership-borrowing-drop.md)判断。`for` binding 只在 loop body
  可见，不在 iterable/source 表达式中可见。
- block 按 element 源码顺序解析。局部 `val` / `var` 的 initializer 先在当前可见环境中
  解析，随后才把新名称加入当前 block；局部解构的所有 binding 同时在 initializer 完成后
  加入。名称不在自己的 initializer 或同 block 更早 element 中可见。
- 同一作用域重复声明使用 L0079；嵌套 block、lambda 或 loop 可以遮蔽外层同命名空间的
  symbol，查询总是选择最近的已可见声明，不产生隐式 warning。声明前若已有可见的外层同名
  symbol，较早引用继续解析到外层；只有没有任何可见声明、但当前顺序作用域稍后会声明同名
  local 时，才使用 L0081，而不是 L0080。

### 引用、外部环境与诊断

- 名称解析规则 的入口必须显式接收一个不可变 `NameEnvironment`。它提供预声明类型、值与函数
  overload，但不读取文件系统、不隐式加载标准库，也不按名称硬编码“可能来自 import”。
  后续 prelude / package 阶段可以构造环境；测试可构造最小环境。环境中不存在且词法作用域
  也找不到的名称使用 L0080。
- 非限定名称从当前作用域向父作用域查询对应命名空间，再查询外部环境。`type_ref` 的首个
  segment 必须解析为类型或外部类型；后续 segment 的名义类型或 package 解释由类型检查与
  本页跨文件规则处理。`.` / `?.` 后的 member、`super<Interface>.member`、constructor、companion
  与 enum variant 的最终选择依赖 receiver 类型，名称解析规则 只解析 receiver 与显式 type
  segment，不对 member name 发未定义诊断。
- `public` / `internal` / `private` 在单文件内部均可见；跨文件与跨 package 可见性由
  跨文件名称规则 执行。`this`、`super`、jump target、override、接口委托目标类型和 smart cast
  都有后续专用检查，不能伪装成普通未定义名称。
- L0079 `duplicate name in scope` 的 primary 覆盖后出现的声明名称，label 指向同命名空间
  的首个冲突声明；函数 overload 组不触发本码。L0080 `unresolved name` 的 primary 覆盖
  引用 Identifier。L0081 `name used before local declaration` 的 primary 覆盖较早引用，label
  指向同一顺序作用域稍后的 local 声明。诊断与 symbol/reference 表均按源码位置和既有
  `ordered_diagnostics` 规则确定性排序。

名称解析规则 不推导表达式类型，不选择 overload / constructor / member，不检查泛型 arity、
visibility 跨文件规则、override、调用实参映射、捕获所有权或 package/import 冲突。它的
输出必须允许类型检查在不重新遍历源码字符串的前提下读取 scope、symbol、overload set
与每个已解析名称引用。

---

## 跨文件 Package 与 Import

### Compilation Unit、Package 与声明身份

- 编译 driver 向 frontend 显式交付一个 compilation unit：有限、显式且枚举顺序无语义的
  source root/source unit 集合，以及每个 source unit 的稳定 root identity、root 内逻辑路径和
  源码。frontend 不读取文件系统，也不从
  进程当前目录、绝对路径或输入枚举顺序猜测 package；路径映射继续遵守 ADR-0005。
- 有 `package a.b` 的文件，其 package 必须精确等于逻辑父目录 `a/b`；省略 package 只允许
  位于 source root 根目录。多个 source root 可以向同一 package 贡献文件，输入顺序不改变
  package 内容、声明身份或诊断顺序。
- frontend 为 package、source unit 和源码 symbol 发布 `PackageId`、`SourceUnitId` 与
  `UnitSymbolId`；可作为声明目标的 symbol 另有 `DeclarationId`。这些身份由规范化
  compilation-unit 输入确定，只在一次分析链内稳定，不是 LLVM symbol、公共 ABI 或持久化
  缓存格式；跨文件引用不得伪装成单文件 `SymbolId` 或外部未知 symbol。文件局部 AST / scope /
  symbol ID 不全局重编号，而是与 `SourceUnitId` 配对使用。
- compilation unit 先收集全部文件的顶层声明，再解析任一声明体。同一 package 沿用本页单文件规则的
  类型/值双命名空间；函数只在同一 package、同一值绑定内形成有序 overload set。非函数
  重名、函数与非函数重名或不可合并的类型重名是跨文件声明冲突，不依赖文件装载顺序。

### 跨文件可见性

- 顶层 `public` 声明可被同一 compilation unit 的其他 package 通过 exact/wildcard import 或
  绝对限定名引用，并作为未来依赖项目的可导出表面。
- 顶层 `internal` 声明对同一 compilation unit 内所有 package 可见；跨 package 使用时仍须
  显式 import 或限定。它不向未来的依赖 compilation unit 导出。source set/依赖图尚未发布
  时，`internal` 的边界就是 driver 本次显式交付的 compilation unit。
- 顶层 `private` 声明只在其 source unit 内可见，不能由同 package 的其他文件导入或限定。
  member `private` 继续是 declaring classifier 内可见，不因多文件而扩大。
- 默认可见性继续遵守既有声明规则；本节不新增 package-private 修饰符，也不让 import
  绕过可见性检查。

### Exact、Alias 与 Wildcard Import

- exact import 在类型和值命名空间中分别查找：一条路径可以同时绑定同名类型和值，`as Alias`
  同时作用于两者；至少一个命名空间存在可见目标即成功。值目标可以是顶层值声明，或同一
  package 中同名顶层函数构成的完整 overload set。alias 只改变当前 source unit 的本地绑定名，
  不改变目标 identity、声明名或导出表面。
- wildcard import 的目标必须是一个 package；它只按需暴露该 package 的可见顶层声明，
  不递归子 package，不导入类型 member，不形成 re-export，也不在文件头阶段物化无限绑定。
- 类型和值命名空间分别处理冲突。同一个 exact target 以同一个本地名重复导入是幂等的；
  不同 exact target 绑定到同一命名空间/本地名是错误。跨 package 的同名函数不会因两个
  exact import 自动合并为 overload set。
- 当前文件或同 package 自动可见声明若与 exact import 的本地绑定同名，是明确冲突而不是
  静默遮蔽；词法 local 仍按本页单文件规则遮蔽文件级绑定。
- exact import 比 wildcard 候选优先。多个 wildcard 只有在某个名称被实际查询且仍指向多个
  可见 target 时才产生歧义；未使用的潜在冲突不报错。没有隐式 prelude wildcard import。
  编译器显式注入的 builtin/prelude environment 也不视为源码 import，不产生 import reference。

### 限定名称解析

- exact import target 先按最长 package 前缀解析；余下路径必须精确只剩一个可见顶层类型、
  顶层值或同名顶层函数 overload set。enum case 与 object/companion member 不是 import target，
  不得把“已解析顶层 + member 尾部”伪装成成功；`import p.Type.member` 因此使用 L0148。应改为
  `import p.Type` 后在正文使用 `Type.member`，或直接使用绝对静态限定名 `p.Type.member`。
  最长 package 前缀仍优先：若 `p.Type` 本身是 package 且其中有顶层 `member`，同一文本是合法
  顶层 exact import。import 始终是绝对 package 路径。
- 普通静态限定名称同样先按最长 package 前缀解析，再在余下路径中选择一个顶层声明，并可继续
  选择现行允许的静态 member/case；路径必须整体成功，不得保留 deferred 尾部。
- 普通表达式中的裸名称先执行本页单文件名称查询。package 只存在于静态名称路径，不是
  runtime value，不能赋值、传参、捕获或作为 member receiver；本节不引入 Kotlin/Rust
  风格的相对 package 别名、`self` / `super` / `crate` 路径。
- 单段 exact import 可引用默认 package 的顶层声明；同样的 `Foo.*` 仍按 package wildcard
  解释，要求存在 package `Foo`。import 的终端名称、alias 与普通/限定引用都必须发布目标
  identity 和精确引用 Span，供诊断及工具查询复用。

### 诊断与阶段边界

L0146–L0151 分别表示 package 与逻辑路径不匹配、同 package 跨文件声明冲突、import target
未解析、目标不可见、exact import 绑定冲突、wildcard 实际使用歧义。诊断必须包含发生使用或
声明冲突的主 `Span`，并在可用时附带目标/冲突声明位置；排序由稳定 source-unit key、字节
位置和错误码决定。省略 package 但文件不在 source root 根目录时，L0146 的 primary 是文件
起始处的空 Span；无效逻辑路径、重复 `(root identity, logical path)` 属于 driver/unit 输入错误，
不是可归因于 Koven 源码的 L-code。

import 诊断按固定优先级选择：完整 exact endpoint 没有可导入的顶层目标使用 L0148；顶层目标
存在但类型/值两个命名空间都没有可见目标时使用 L0149；任一命名空间存在可见目标即成功，且只
绑定可见侧。只有成功 target 与既有文件级绑定冲突时才使用 L0150，失败 directive 不追加
L0150。L0151 只在实际裸名查询时产生，且仅当词法、当前文件、同 package 与 exact import 都未
决定该命名空间中的名称，而多个 wildcard 提供不同可见 target。member/case 形式的非法 exact
import 即使对应 member 存在也使用 L0148，不改用 member/associated visibility 诊断。

阶段边界固定如下：名称解析只建立 package index、声明身份、import/可见性绑定与名称诊断；
类型检查在其后发布 compilation-unit typed facts；所有权检查再发布调用、构造、capture 与 drop
facts；codegen 只消费 validated typed/ownership 产物。LSP source-set 必须复用同一分析链，不能
维护第二套 resolver，也不能把打开 URI 集合或磁盘扫描当成隐式 compilation unit。

本节不定义 manifest、依赖解析、package re-export、模块初始化、增量缓存、跨 compilation-unit
ABI 或多 object 链接策略；也不把跨文件名称规则扩张为项目构建、类型、所有权或 codegen
契约。
manifest→source-set adapter 属于工具层，不因此成为语言语义，也不定义 dependency、target 或
process entry。

---

## 完整文件与声明分隔

```ebnf
source_file = trivia*,
              [ simple_declaration,
                { declaration_separator, simple_declaration },
                [ trivia*, ";" ] ],
              trivia*, EOF ;

declaration_separator = trivia_with_line_break
                      | trivia*, ";", trivia* ;
```

- 空文件合法。文件产物按源码顺序保存零个或多个既有 `ItemId`；`AstFile` 已是文件级容器，
  不增加虚构的根 Item。本节只组合现有 `val`、`var`、`const val`、`fun`，不接纳
  `package` / `import`、control-flow、class-family 或其他顶层产生式；文件头由下节在同一
  `source_file` 中组合。
- `trivia_with_line_break` 是至少包含一个实际 LF / CRLF 的非空 trivia 序列；已终止 block
  comment 内的 LF / CRLF 同样计入，只有 space、tab 或不含换行的注释不构成分隔。声明之间
  必须存在该换行分隔或一个 `;`；因此同一行多个声明必须写 `;`。分隔区域可同时包含换行和
  一个 `;`，最后一个声明后允许一个可选 `;`；文件开头或连续的 `;` 不产生空声明，按非法
  顶层区域恢复。`const val` 仍由同一 constant declaration 消费，不拆成两个声明。
- 上述四个 starter 与 `;` 只在 delimiter stack 为空且 lexical owner 回到文件 baseline 时
  构成恢复用 **soft declaration boundary**。括号、方括号、大括号、string 或 interpolation
  内的同形 token 属于当前声明或错误区，不能提前开始下一声明；EOF 是唯一无条件 hard
  boundary。恢复边界不等于合法分隔：两个同行 starter 之间缺少 `;` 时仍保留后一声明，
  但必须产生 L0047 `expected declaration separator`，主 `Span` 精确覆盖后一声明 starter。
- 文件入口遇到 `val (`、`var (`、`const val (` 继续使用 L0043 和 `Item::Error`，但错误区
  在下一 soft declaration boundary 前结束。其他不能开始既有声明的普通顶层 token 使用
  L0017 `expected declaration`，一次错误区只发一条该诊断并建立覆盖实际消费区的
  `Item::Error`；Lexer 已诊断的 invalid / reserved token 只建立 Error Item，不重复分类。
- 独立声明入口继续以 EOF 为唯一声明 stop，并保留 L0013 `unexpected trailing token` 行为；
  文件入口不得把后续合法声明报告为 trailing token，也不得修改既有声明、block、lambda、
  call argument 或 destructuring 节点内部的诊断含义。内部恢复抵达文件 soft boundary 时
  保留该 starter，交还文件循环。
- 每轮文件循环必须消费一个声明或一个非空错误区，或者抵达 EOF。Lexer terminal owner
  事件、异形 closer 与局部 delimiter 沿用
  [声明恢复规则](05-declarations-callables.md#最小诊断与局部恢复)；同一 Lexer / Parser
  根因不得产生文件级级联。诊断按现有全序确定性合并。
- 对含 `n` 个 lexeme 的文件，文件 dispatch 与跨声明恢复合计必须是 `O(n)` 时间、`O(d)`
  owner / delimiter 栈空间；starter、terminal event 与错误区不得从每个声明重新扫描全文件。

换行 / `;` 规则只作用于 `source_file` 顶层声明序列，不把换行或分号提升为通用 expression /
block statement separator，也不改变独立声明入口。

完整文件入口 `parse_file(&SourceMap, &LexedFile) -> ParsedFile` 中，`ParsedFile` 暴露同源
`SyntaxAst`、有序根 `ItemId` 切片及合并诊断。现有 `parse_expression`、`parse_declaration`、
`parse_block` 的公共行为与返回类型保持不变。

## Package 与 Import 文件头

```ebnf
source_file = trivia*,
              [ package_directive ],
              { import_directive },
              [ simple_declaration,
                { declaration_separator, simple_declaration },
                [ trivia*, ";" ] ],
              trivia*, EOF ;

package_directive = "package", trivia+, qualified_name ;

import_directive = "import", trivia+, import_target,
                   [ trivia+, "as", trivia+, Identifier ] ;

import_target = qualified_name
              | qualified_name, ".", "*" ;

qualified_name = Identifier, { ".", Identifier } ;

file_header_separator = trivia_with_line_break
                      | trivia*, ";", trivia* ;
```

- 每个文件最多有一个可选 `package` directive。它必须是文件首个非 trivia 构造；省略时
  文件处于默认 package。零个或多个 `import` 必须紧随可选 `package`，并位于任何普通声明
  之前。只有 exact import 可写 `as Identifier`；wildcard import 的 `*` 必须是最后一个
  segment，不能起别名。
- 名称采用一个或多个点分 ASCII Identifier segment。不得有前导点、尾随点、空 segment，
  也不得以硬关键字或 reserved word 冒充 segment。所有 import 都是绝对名称；v1 不提供
  相对 import。Koven 不接受 Rust 风格的 `mod`、`use`、`::`、`self` / `super` / `crate`
  路径或花括号分组导入。
- 两个文件头构造之间、最后一个文件头与首个声明之间，必须出现实际 LF / CRLF 或 `;`。
  最后一个构造若直接抵达 EOF 可省略分隔符。换行判定与本页
  [完整文件规则](#完整文件与声明分隔)一致，已终止 block comment
  内换行计入，space、tab 或无换行注释不计；同一行继续写下一个文件构造必须显式写 `;`。
  这项规则不改变 block、expression 或独立声明入口。
- `ParsedFile` 内嵌保存 `Option<PackageDirective>` 与源码有序的 `Vec<ImportDirective>`；普通
  声明继续只通过 `roots: Vec<ItemId>` 暴露。directive、点分 segment、wildcard 和 alias
  都保留真实 `Span`，不创建虚构 Item 或第五张 AST table。Parser 只保存源码结构，不解析
  文件系统路径、source root、package identity、名称绑定、可见性、重复或 wildcard 展开。
- `package` 重复或出现在 import / 声明之后使用 L0051 `misplaced package directive`；声明
  后的 `import` 使用 L0052 `misplaced import directive`。缺 package 名、缺 import target、
  缺 alias 分别使用 L0048–L0050；文件头间缺换行 / `;` 使用 L0053；wildcard import 后写
  alias 使用 L0054。主 `Span` 覆盖首个能确定类别的真实 token；EOF 缺失使用同位置空 Span。
- 文件头 starter 只在 delimiter stack 为空且 lexical owner 回到文件 baseline 时作为 soft
  boundary。恢复必须保留下一合法文件头或声明 starter，不能把 nested string、interpolation
  或 delimiter 内的 `package` / `import` 提升到文件级；一次根因不产生重复文件级诊断。
  文件 dispatch、header 解析和跨构造恢复合计保持 `O(n)` 时间与单调游标。

本节只定义 Lexer / Parser / AST / 诊断与恢复。package 到 source root / 文件的
映射、多文件名称解析、exact / wildcard import 的绑定与冲突规则属于本页跨文件名称规则；长期
映射由 package ADR 固定，不能从当前文件名或相对路径静默推导语义。
