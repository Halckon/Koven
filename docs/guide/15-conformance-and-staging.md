# Koven v0.35：一致性、Phase 与实施边界

> **性质**：规范性语言规范 · **状态**：current（v0.35） · **读取时机**：判断规范权限、Phase 归属、实现门禁和明确非目标时 · **唯一真源**：本页

本页是现行 Koven v0.35 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 不支持自定义属性 Getter/Setter

真实 Kotlin 的 `val x: Int get() = ...` / `var y: Int set(value) { ... }`（自定义属性访问器）**在本语言 v1 中不支持**，和扩展函数一样列入“去掉的语法糖”清单：属性访问始终直接对应存储字段，不允许拦截读写。原因：自定义访问器要求类型检查器处理访问器返回类型与声明属性类型的一致性校验、`field` 关键字访问底层存储等额外机制，复杂度收益比低，与“去掉语法糖、保留核心”的项目定位一致。

## 动态分发与匿名对象边界

`dyn` 是保留关键字，但对应的动态分发语法在 v1 不实现；v1 泛型与 interface 全部使用
单态化静态分发。异构集合可以显式用 `enum class` 包装各具体类型。

v1 也不提供 Kotlin/Java 风格的匿名内部类或 `object : Interface { ... }` object expression。
lambda 只实现函数类型这一个 callable 行为，不伪装成任意接口实例；单回调 API 应直接接受
函数类型，多方法或有字段的实现使用具名 class，简单包装优先使用上一节的接口委托。
匿名对象必须等 `dyn`、隐藏捕获布局、对象身份与逃逸所有权规则一并在 v2 设计后才能加入。

## AST `Span` 与恢复一致性

所有范围都是现有 UTF-8 字节半开区间，并来自同一 `SourceId`：

| 节点 | 合成范围 |
|---|---|
| identifier / 数值 / `Char` / 关键字字面量 / `this` | 对应 primary token |
| 未绑定引用 | `::` 起点至名称 token 终点 |
| 分组 | 左括号起点至右括号终点；缺右括号时至该组最后消费位置 |
| prefix | 运算符起点至 operand 终点 |
| cast / `is` / `!is` | lhs 起点至 `type_ref` 终点 |
| 其他 binary / assignment | lhs 起点至 rhs 终点 |
| member / bound reference | receiver 起点至名称 token 终点 |
| call / index | receiver 起点至闭合 `)` / `]` 终点；缺闭合符时至该 suffix 最后消费位置 |
| postfix `!!` | receiver 起点至 `!!` 终点 |
| postfix `?` | receiver 起点至真实 `?` 终点；保存 `question_span`，不反查或扩张既有 child |
| string | 开始引号起点至结束引号终点；恢复时至该字符串最后消费位置 |
| interpolation | `${` 起点至匹配 `}` 终点；恢复时至该插值最后消费位置 |
| error | 覆盖本次实际消费的错误区域；只有位于 stop token / EOF 且没有可消费 token 时可以为空 |

TypeRef 节点遵循以下唯一合成规则：

| 节点 | 合成范围 |
|---|---|
| qualified type | 从首段 `Identifier` 起，到末段 `Identifier` 终；若有 type arguments，则改为到匹配 `>` 终；若再有 nullable `?`，最终到该 `?` 终 |
| function type | 若有 `move`，从 `move` 起，否则从 `(` 起；到 return `type_ref` 终；参数模式已包含在各参数完整范围内 |
| function parameter | 若有模式，从真实 `own` / `borrow` / `inout` 起，否则从参数 `type_ref` 起；到参数 `type_ref` 终 |
| type arguments | 从 `<` 起到匹配 `>` 终；若实现不为它单建节点，该范围仍完整纳入所属 qualified type |
| TypeRef error | 与通用 error 相同：覆盖实际消费的错误区域；只有位于 stop token / delimiter / EOF 且没有可消费 token 时可以为空 |

函数类型 AST 的 `parameters` 唯一改为源码顺序的 `Vec<FunctionTypeParameter>`；每项至少保存
`span`、`mode_marker: Option<ParameterModeMarker>` 与唯一 `type_ref: TypeRefId`。marker 缺失
表示 `Borrow`；显式 marker 使用封闭的
`ParameterModeMarker::{Own(Span), Borrow(Span), Inout(Span)}`，不得用多个独立 `Option`
制造“有模式无 Span”或“有 Span 无模式”的半状态。语义层唯一映射为
`None | Some(Borrow) -> ParameterMode::Borrow`、`Some(Own) -> ParameterMode::Value`、
`Some(Inout) -> ParameterMode::Inout`。参数名不进入函数类型 AST；类型身份比较规范化后的
语义 mode，而不是比较 `None` 与显式 `Borrow` 的源码形态。

所有空 Error TypeRef / Expression 都位于下一 non-trivia boundary token 的起点（EOF 则为
EOF offset），但**零宽插入范围不扩大父节点**。父函数类型参数、value parameter 或
CallArgument 的结束位置只取本构造最后实际消费的非 trivia / invalid token 终点；若没有
这样的 token，父节点才可与空 error 同为空范围。因而 `(borrow /*c*/ ,) -> R` 的空 TypeRef
位于逗号起点，而参数 Span 仍只到真实 `borrow` 终；`fun f(borrow x: /*c*/ ,)` 只到真实 `:` 终，
`f(name = /*c*/ )` 的 argument 只到真实 `=` 终。trivia 本身既不扩张父 Span，也不被伪装成
错误 token；child 的零宽插入点可以位于 parent 半开范围终点之后，这是恢复元数据，不表示
父节点消费了中间 trivia 或 boundary。

缺失泛型 `>`、函数参数 `)`、`->` 后返回类型或其他 TypeRef delimiter 时，恢复节点止于本构造
最后实际消费位置，不得越过外层 stop token 或 delimiter。所有表达式与 TypeRef 合成范围都
不得用不存在的 token 伪造超出已消费输入的坐标。

函数类型参数模式后的恢复复用现有 expected type reference：`own` / `borrow` / `inout` 后若遇当前参数 `,`、所属
`)`、调用方 TypeRef stop 或 EOF，则在该边界建立空 TypeRef error 并保留边界；遇其他不能
开始 `type_ref` 的 token 时消费到同一组边界前，Error 只覆盖实际消费区域。连续第二个模式
不是新的参数，而是当前参数的错误区域：保留首个模式，消费后续连续模式，并对每个多余
token 发 `duplicate parameter mode`；主 `Span` 精确覆盖该多余 mode token，固定消息为
`duplicate parameter mode`。该类别在现有 L0032 后使用 `L0039`，与调用实参自己的
duplicate-mode 类别不同。现行语言不为函数类型引入空项、缺 separator 或 trailing
comma 的新接受形式，既有 TypeRef list 恢复继续适用。所有路径单调前进，单个函数类型保持
`O(n)`。

调用点类型实参的 strict trial 必须同步识别这里扩展后的 `function_type_parameter`；合法的
`f<(borrow T) -> R>()`、`f<(own T) -> R>()`、嵌套泛型中的模式函数类型以及对应失败候选，都必须继续满足[TypeRef 规则](03-types-generics.md)
既有的无副作用、Match / NoMatch 预算传播与整根 `O(n)` 预索引约束。不得在正式 TypeRef parser
接受参数模式后，让 trial 仍按旧 `Vec<TypeRef>` 语法误回退为比较，也不得为每个 `<` 重新扫描。

## Phase 边界

| Phase | 规范职责 | 最低可验证结果 |
|---|---|---|
| 0 | Cargo workspace、索引式 AST、诊断与 fixture 骨架 | workspace 和测试入口可检查 |
| 1 | Lexer、Parser 与错误恢复 | 正反例产生稳定 AST 或诊断 |
| 2 | 名称、类型、smart cast、穷尽性与结构能力 | compile-pass/fail 与 typed facts |
| 3 | 所有权、借用、capture 与析构计划 | 拒绝非法 move/loan 并发布所有权事实 |
| 4 | typed SSA、LLVM、本机目标与调试信息 | 生成并运行本机程序 |
| 5 | 以 Koven 源码实现的最小标准库 | 标准库源码和 native 行为通过 |
| 6 | project、CLI、LSP、formatter 与编辑器 grammar | 工具有独立可重复验收 |

跨 Phase 功能只实施依赖完备且获授权的部分。保留关键字或候选设计不等于授权提前实现；v2 动态分发与 Shareable、v3 协程、v4+ 自举及未排期语义均不属于 v0.35。

具体测试选择与并行方式见 [测试与分层验收](../development/testing.md)；上表规定阶段产物，
不要求每个实施切片重复执行全量测试。

## `const val` 的 v0.35 边界

v0.35 只启用顶层、具名 object 与 companion 中 `const val` 的声明位置、固定前缀、AST/Span 和
Parser 恢复。initializer 仍解析为普通 expression，但 v0.35 不定义或授权关联常量选择、封闭表达式
子集、编译期求值、依赖图、跨文件常量图、运行时重新物化、所有权事实或 lowering。实现不得把
initializer 当作普通运行时初始化，也不得将仍缺少这些事实的产物送入 ownership/codegen。

上述阶段只有在更高版本 guide 明确启用后才成为语言能力；proposal 或 draft Spec 不能补全本页。

## Nullable 的阶段交接

Phase 2 发布稳定 subject、剩余域、alternative 交集及 assertion Abort descriptor；
Phase 3 消费这些事实发布 view/extraction、move/loan/drop 与 branch join 事实；
Phase 4 只消费已验证产物，不重新解释条件或推导所有权。

Nullable flow 与 extraction 的 frontend facts/ownership 适用于所有已接受的 nullable 类型；首轮 native 实施只覆盖
ADR-0017 已支持、且由 owned whole-root/temporary 承载的普通 class、`Box`、`Rc` pointer-like
nullable。pointer-like Borrow/Inout/field/element subject 的 proof lowering 等待 nullable-place/
loan branch ADR，不能交给 owner-only `NullableBranch`；这不反向否定其 frontend 合法性。
scalar/value/enum/String/顺序容器等 inline/tagged nullable 需要独立 SSA/LLVM ABI ADR 与后继 Spec；
Elvis、safe call、`as?`、nullable function value、nullable borrow-return 和跨 nullable 的 place-return
也继续延后。v0.35 不改变这些类型/语法的既有 frontend 接受边界，只禁止后端凭表示猜测接线。
