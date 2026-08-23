# Koven 语言设计规范 · 开发阶段路线图与工程规范

> 本文档是 Koven 语言设计规范多文档结构的一部分（原单文件 guide 第五、六部分），完整
> 文档地图、版本治理规则与跨文件索引见 [`00-index.md`](./00-index.md)。内容版本：v0.26。
> 本文档是拆分后变化最频繁的一份——每验收一个 Spec 就需要勾选对应 checkbox，请优先
> 到这里确认“现在该做哪一项”。

## Phase 0：项目骨架（已完成）

- [x] 建立 Cargo workspace：`lang-frontend` / `lang-codegen` / `lang-cli` / `lang-lsp` / `lang-std`
- [x] 定义 AST 数据结构（索引式节点）
- [x] 搭建诊断输出框架（错误码格式 `L0001` 等，从第一天就用）
- [x] 建立真实 Cargo test target，枚举至少一个 `.ko` fixture，并让零用例成为配置错误

**验收标准**：五个 workspace member 均有有效 Cargo target，workspace 基线命令可执行；测试
驱动能枚举并读取至少一个 `.ko` fixture，统一 source / `Span` 基础设施能对人工构造的 AST
和结构化诊断做稳定断言。Phase 0 不引入临时 parser，也不要求源码已能解析或执行。

## Phase 1：词法 + 语法分析（Lexer/Parser）

- [x] **SPEC-0006**：实现 [v0.5 词法基线](./02-lexical-spec.md)：ASCII 标识符、42 个硬
      关键字、当时的 2 个软关键字、11 个未来保留字、trivia、字面量、字符串插值、当时的固定符号、
      EOF 和错误恢复；v0.14 新增的单字符 `&` 已由 SPEC-0012 增量实施
- [x] 所有 token / trivia / invalid 区域保留精确 UTF-8 字节 `Span`，非法源码返回结构化
      诊断而不 `panic!`
- [x] **SPEC-0007**：实现 [`03-grammar-core.md`](./03-grammar-core.md) 独立表达式入口、全部运算符层级、`type_ref`、仅位置实参的
      basic call、单表达式索引、局部恢复及表达式 AST `Span`
- [x] **SPEC-0008**：实现 [`04-grammar-declarations-blocks.md`](./04-grammar-declarations-blocks.md)
      第 7 节的独立 `val` / `var` / `const val` / `fun` 声明、单一内联泛型上界与签名
      `type_ref`，以及按 [`03-grammar-core.md`](./03-grammar-core.md) 第 2、3 节无副作用试探
      规则成立的调用点类型实参
- [x] **SPEC-0009**：只实现 [`04-grammar-declarations-blocks.md`](./04-grammar-declarations-blocks.md)
      第 8 节的独立 block、局部 `val` / `var` 与 expression
      statement 序列、嵌套 block，以及具名函数 block body
- [x] **SPEC-0010（前置：SPEC-0009 `done`）**：只实现
      [`05-grammar-calls-lambda.md`](./05-grammar-calls-lambda.md) 第 9 节 lambda literal
- [x] **SPEC-0011（前置：SPEC-0009 `done`）**：只实现具名函数无体 / block body 省略返回
      标注时固定为 `Unit`；表达式体仍要求显式标注
- [x] **SPEC-0012（前置：SPEC-0010、0011 `done`；v0.14 已明确启用）**：增量实现单字符
      `&` 的 Lexer token，以及统一 callable 参数 marker、函数类型参数、typed call argument、
      命名实参与模式实参
      （声明侧关键字 `borrow` / `inout`；调用点关键字 `borrow` 与符号 `&`）的 Phase 1
      AST / parser；不实现 Phase 2 / 3 合法性检查
- [x] **SPEC-0176（v0.26 已明确启用）**：在 SPEC-0012 基线上让声明侧接受 `own`，把普通
      callable / function-type 的无标记 mode 改为 Borrow，并保留显式 `borrow` 的同义源码
      形态；调用点仍只接受 `borrow` / `&`，`own` 继续拒绝。同步迁移 typed contract、
      预声明 API 与 lambda expected mode；不实现调用期 loan 或 drop-point
- [x] **SPEC-0013（前置：SPEC-0012 `done`）**：只实现 block / lambda body 内局部 `val` 解构
- [x] **SPEC-0014（前置：SPEC-0011、0013 `done`；v0.15 已明确启用）**：只把 SPEC-0007 至 SPEC-0013 的既有
      节点组合为完整文件，并实现声明
      分隔、跨声明同步与级联抑制；单个语法错误不得导致整个文件解析中断，但本 Spec 不以
      尚未定义的控制流或 class-family 范例为验收条件
- [x] **SPEC-0062（前置：SPEC-0014 `done`；v0.16 已明确启用）**：把 `;` 加入固定符号，
      实现顶层声明的换行 / 分号分隔、同行缺分号诊断及 owner-aware 恢复；不改变 block 与
      独立声明入口
- [x] **SPEC-0015（前置：SPEC-0014、SPEC-0062 `done`；v0.17 已明确启用）**：解析
      [`04-grammar-declarations-blocks.md`](./04-grammar-declarations-blocks.md)第 11 节的可选
      `package`、exact / wildcard / alias `import`、文件头顺序和 owner-aware 恢复；只保存
      Phase 1 AST，不实现名称解析或文件系统映射
- [x] **SPEC-0016（前置：SPEC-0009 `done`；v0.18 已明确启用）**：解析 `if` / `when`、
      loop-family、jump 与 `super`，实现 §12 的 AST、位置敏感 `else` 诊断和恢复契约。
- [x] **SPEC-0063（前置：SPEC-0016 `done`；v0.19 已明确启用）**：解析
      [`01-design-decisions.md`](./01-design-decisions.md)第 19 节与
      [`03-grammar-core.md`](./03-grammar-core.md)第 2、4、6 节的 postfix `?`，只建立
      `Propagate` AST 与消歧 / 恢复证据；`Result<T, E>`、最近 callable 和错误类型检查延后
      到 Phase 2。
- [x] **SPEC-0017（前置：SPEC-0014 `done`；v0.20 已明确启用）**：按
      [`04-grammar-declarations-blocks.md`](./04-grammar-declarations-blocks.md) 第 13 节解析
      `value class` / `class` / `interface` / `enum class` / 具名 `object` / `companion object`，
      只交付 Phase 1 AST、诊断与恢复；不实现接口委托或 Phase 2/3 语义检查。
- [x] **SPEC-0064（前置：SPEC-0017 `done`）**：增量解析 `Interface by field` 接口实现委托；
      `by` 仍由 Lexer 产出 identifier，只在 class supertype entry 的确定上下文中提交。不得扩张
      为属性委托、任意 delegate expression、动态代理或运行时 `dyn` 分发。
- [x] **SPEC-0066（前置：SPEC-0006、0007 `done`；v0.22 已明确启用）**：识别 `L`、`u` /
      `U`、`uL` / `UL`、`f` / `F` 数值后缀，在 Lexer token 与 Parser AST 保存规范化身份；
      不解析数值或决定默认类型。

SPEC-0006 词法基线、SPEC-0007 至 SPEC-0017 以及增量 SPEC-0062、0063、0064、0066 已完成。
未勾选状态不
表示已经批准或已有代码；各 Spec 必须按实际依赖顺序独立验收和提交。后续阶段使用
[`05-grammar-calls-lambda.md`](./05-grammar-calls-lambda.md) 第 9 节的新编号映射。
`type_ref` 的 Phase 1 反例必须拒绝含值实参的 `Array<Int, 4>`；`Array<Int, Size>` 的两个实参
都可先解析为类型引用，内建 `Array` 的 arity 则由 Phase 2 判断，Parser 不提前做名称或类型
判定。

**Lexer 基线与 v0.14 增量验收标准**：SPEC-0006 已穷举验证全部硬 / 软 / 未来保留字及
前后缀边界；正例覆盖 ASCII
标识符、Unicode `Char` / `String` 内容、全部字面量、`${...}` 嵌套插值、LF / CRLF、两类
注释、全部固定符号和最长匹配；反例逐类覆盖上表八种错误，并断言稳定错误码、精确字节
`Span`、恢复后的后续 token 及确定性顺序。数字测试必须锁定 `1..2` / `1..<2` 与
`1e3` / `0x10` / `1l` / `1_0` 的边界；v0.22 的 `1L` / `1u` / `1f` 正例及错序后缀归
SPEC-0066 增量验收。`&`/`&&` 最长匹配边界同样需要锁定测试
（v0.14 新增固定符号，归 SPEC-0012 增量验收）：`&x` 是单字符 `&` token 后跟 `x`，
`&&x` 是单一 `&&` token
后跟 `x`，`& &x`（trivia 分隔）是两个独立 `&` token，三者必须分别断言 lexeme 数量与
类型。空文件只产生一个 EOF，所有非 EOF lexeme
非空、不重叠并覆盖全部输入字节；真实 pass / fail `.ko` fixture 必须被 Cargo test target
枚举，零用例必须失败。

**SPEC-0008 历史 Parser 验收标准**：独立正例至少覆盖带 / 不带类型标注的 `val`、`var`、
`const val`，无体和表达式体 `fun`，空 / 多参数，空缺 / 单个 / 多个泛型参数、递归单一上界
及函数类型签名；反例覆盖 `const` 后缺 `val`、缺名称 / `:` / `=` / initializer / 显式返回
类型、缺 separator 后直接出现 expression / type 起始、`=` / `{` / EOF 与其他非法 token、
不支持的 parameter default、声明列表空项 / 缺逗号 / trailing comma、泛型参数表缺 `>` 后跟
候选函数名与 `(`、多上界、`where`、声明修饰符、block body 与两个连续声明。这里把 block
body 列为反例只记录 SPEC-0008 完成时的测试边界；实施 SPEC-0009 后，按第 8 节
把该用例改为正例，不能继续用历史验收覆盖现行语法。typed call
必须同时覆盖
`f<T>()`、成员及调用链 callee、嵌套 `>>`、`>` 与 `(` 间 trivia，以及失败试探回退为比较的
相邻反例；测试必须证明失败试探不遗留诊断 / AST 节点且 ID 和诊断顺序确定。所有反例断言
稳定错误码和关键 UTF-8 字节 `Span`。名称恢复必须分别锁定 present / missing / error marker
及其非虚构范围。所有 consume-to-current-level 路径至少以 unsupported parameter default
覆盖 string / interpolation 内的 `,`、`)`、嵌套 string / interpolation、`L0004` 只结束内层
string 后继续处于父 owner、terminal `L0006` 及 EOF `L0005` 不越 owner 的用例，并证明只在
所有 owner 退出后才识别声明层 stop、没有 parser 级联。hard closing stop 还须覆盖局部 `[` 未
闭合便遇到外层参数表 `)`、局部 `(` 未闭合便遇到外层 type-parameter `>`，断言外层 closer
保留且错误范围在其之前结束；以 balanced nested `[...]` / `(...)` 后再遇外层 closer 作对照，
断言匹配的局部 closer 先被消费而外层 closer 才停止扫描。长错误区域还须锁定单调单遍
`O(k)` 扫描不发生二次回看。验收同时复跑 SPEC-0007 表达式与 TypeRef 回归。该验收不以
完整文件、类成员、block 语句、跨声明恢复或 Phase 2 名称 / 类型正确性为成功条件。

其中“缺显式返回类型”同样只记录 SPEC-0008 完成时的历史边界。SPEC-0011 只把参数列表后
直接到 EOF / 调用方无体 stop 或 `{` 的分支迁移为隐式 `Unit` 正例；参数列表后直接 `=` 仍是
反例并继续产生 expected explicit return type，不能批量迁移表达式体负例。

**Phase 1 聚合 Parser 验收标准（不归 SPEC-0014 单独承担）**：在 SPEC-0014 以及后续控制流、
class-family 等独立 Parser Spec 全部完成后，能完整解析以下代码为 AST，语法错误有准确的
行列号定位。该范例还依赖 lambda、命名实参、`when` 和 class-family，不能作为提前扩大
SPEC-0009 至 SPEC-0014 范围的理由：

```kotlin
value class Point(val x: Int, val y: Int)

enum class Shape {
    Circle(radius: Double),
    Point;

    fun area(): Double = when (this) {
        is Circle -> 3.14159 * radius * radius
        is Point -> 0.0
    }
}

fun main(): Unit {
    val points: List<Point> = listOf(Point(x = 1, y = 2), Point(x = 3, y = 4))
    val first = points[0]
    val double: (Int) -> Int = { x -> x * 2 }
    println(double(first.x))
}
```

## Phase 2：类型检查（不含所有权/借用）

- [x] **SPEC-0018（前置：SPEC-0014 `done`；v0.21 已明确启用）**：按
      [`01-design-decisions.md`](./01-design-decisions.md) 第 21 节建立单文件双命名空间、
      确定性 `ScopeId` / `SymbolId`、函数 overload set、顺序 local 可见性、显式
      `NameEnvironment` 与 L0079–L0081；不展开 package/import，不选择 member 或 overload。
- [x] **SPEC-0019（前置：SPEC-0018、0066 `done`；v0.22 已明确启用）**：基础类型、数值
      字面量定型、局部推导、单向 expected type、隐式 `Unit` / 显式返回类型、`Nothing`
      bottom 与 L0082–L0090。
- [x] **SPEC-0020（前置已完成；v0.23 已明确启用）**：建立名义/泛型 identity 与替换、
      interface hierarchy/requirement/default、显式 override 和 `Interface by valField` 静态
      委托检查；诊断 L0091–L0105。
- [x] **SPEC-0021（前置已完成；v0.24 已明确启用）**：建立 enum case type、稳定 place 的
      flow facts、赋值/capture kill、Boolean/enum/nullable 有限域穷尽性、分支 join 与
      L0106–L0114。
- [x] **SPEC-0022（前置已完成；v0.25 已明确启用）**：按现行
      [`01-design-decisions.md`](./01-design-decisions.md) §25 推导条件 `Copyable`，拒绝无限
      内联布局与非法 intrinsic `Box` 实参，并为局部 value-class 解构产出有序的
      Copy/Consume typed descriptor；L0115–L0118 已实施。
- [x] **SPEC-0067（前置已完成；v0.25 callable 契约基线）**：为具名与预声明 callable
      建立有序参数元数据；检查位置 / 命名映射、重复 / 缺失 / 多余
      实参、函数值禁用命名实参、argument 类型与 `Value` / `Borrow` / `Inout` 契约相符，
      并标记类型层面的 place / temporary 类别；不在本 Phase 判定该 place 此刻能否移动、借用、
      独占访问或是否与其他借用冲突
- [x] **SPEC-0173（v0.25 callable 契约实现漂移修复）**：具有唯一期望函数类型的 lambda
      逐项采用 Value/Borrow/Inout 参数契约，并按参数 SymbolId 发布 typed fact；结构错误不
      伪造模式
- [x] **SPEC-0176（v0.26 callable 契约迁移）**：无标记与显式 `borrow` 规范化为同一 Borrow，
      显式 `own` 映射 `ParameterMode::Value`，`inout` 不变；函数类型、override/委托、预声明
      callable、单态 call mapping 与 lambda expected facts 使用同一规范化 mode。Value 参数
      对 `MoveOnly` 实参的调用仍无 marker，并在 Phase 3 形成移动
- [ ] 多 overload 候选对 lambda expected contract/body 的 candidate-isolated 检查；由
      SPEC-0174 独立封闭 trial 与诊断回滚，不把无期望单次检查误报为完整实现
- [x] class-family 的名称、visibility、supertype、`override` 与 `enum class` case type / `when`
      穷尽性检查
- [ ] `object` / `companion object` 关联成员与编译期常量检查；接口 companion 常量不参与继承
      或 override
- [x] 在 SPEC-0064 已建立的委托 AST 上验证 delegate 是同一主构造器的不可变 `val` 字段，
      其静态具体类型满足接口；手写 `override` 优先，拒绝未消歧的多委托冲突
- [x] **智能类型转换（smart cast）**：`is`/`when` 分支内的类型收窄及其失效规则（变量在收窄后被重新赋值则收窄失效）
- [ ] 泛型单态化的类型层面准备（类型替换，不接编译期计算）
- [x] `Nothing` 类型的 bottom-type 特殊处理
- [x] 计算 `value class` / `enum class` 的条件 `Copyable`：允许不可复制字段或 payload，按
      实际类型实参递归推导；支持把预声明的 `Copyable` 用作泛型上界，但不接受用户手动实现、
      覆盖或同名冒充（v0.25 现行契约）
- [x] 检查内联类型结构有限；拒绝未经过 `class`、`Box` 或动态容器等固定大小 handle 打断的
      直接 / 间接递归内联环
- [x] 对内建 `Box<T>` 执行 type-kind 检查：只接受 `value class` 类型实参，拒绝普通 `class`
- [x] 检查字段投影的使用模式：可复制字段可读出 owned copy，不可复制字段只允许投影借用，
      禁止把普通字段读取标记为所有权移出
- [x] 为 `value class` 建立有序结构分量并支持解构类型检查；右值只求值一次，类型结果标记为
      复制式或消费式解构；不可复制类型的消费式解构必须覆盖全部分量
- [x] 按[01-design-decisions.md](./01-design-decisions.md)第 8 节识别 `Array<T>`、`List<T>`、`MutableList<T>` 的精确单类型实参、
      长度 / 可变性角色和非 `Copyable` 独占 owner 能力；拒绝内建容器 arity 错误
- [x] 检查顺序容器元素的 storable type 条件：保留单态化后的具体元素类型，不擦除为 `Any`，
      不把裸 interface 当作 v1 `dyn` 表示，也不隐式改写成 `Box<T>`
- [x] 识别封闭的列表式 / 运行时长度构造操作并推导元素类型；把顺序容器索引结果标记为
      element place，按容器类型检查索引 key、place 可变性和赋值左侧合法性
- [x] 顺序容器索引能力不进入用户可见 `Indexable` / `MutableIndexable` interface 或泛型上界；普通
      `.get(...)` / `.set(...)` 成员调用不得绕过内建 `[]` place 规则

Map 不是 Phase 2 的本版实施项。在后续 guide 定义 key 等价性与所有权契约前，类型检查器
不得自行加入 `V : Copyable`、key 借用、`put` 或下标赋值特例。（v0.11 补充：
[01-design-decisions.md](./01-design-decisions.md)第 18 节给出了一份候选设计，可作为后续 guide 的起点，但在其完成独立评审并进入实施
Spec 之前，本条限制不变。）

**验收标准**：能对 Phase 1 能解析的全部语法结构做类型检查，类型错误有清晰的错误码和
定位；`when (this) { is Circle -> radius }` 这类智能类型转换场景能正确通过类型检查；包含
不可复制字段的 `value class` 以及 `Pair<Sender<Int>, Receiver<Int>>` 均是合法类型，而
`Pair<Int, Int>` 被推导为 `Copyable`；`<T : Copyable>` 可以满足要求该上界的调用或类型
约束，未约束的 `<T>` 不能。未约束 `T` 的普通所有权转移仍然合法，不应在 Phase 2 因缺少
`Copyable` 报错；其移动后使用由 Phase 3 判断。`Box<Node>` 必须产生类型诊断。
`listOf(Point(...))` 必须保留为 `List<Point>`，不得推导为 `List<Box<Point>>`；运行时
`size: Int` 可以用 `Array<Point>(size, initializer)` 形成容器，而
`Array<Int, Size>` 必须产生内建类型 arity 诊断。不可复制元素可以形成合法顺序容器类型，
索引节点保留 place 类别，具体读取和借用合法性由 Phase 3 判断；`List<Any>`、裸 interface 元素和
`list.get(0)` 必须被拒绝。直接
递归或经多个 `value class` 形成的无限内联布局必须报错，经 `Box` 或动态容器打断的递归布局
必须合法。本 Phase 不以 Map 正反例作为验收，也不将任何 Map 所有权策略固化到 typed AST。

## Phase 3：所有权 / 借用检查

v0.26 已明确启用[默认 Borrow、调用期 loan 与 ASAP drop-point](./01-design-decisions.md#26-调用期借用与-asap-析构点v026)。
SPEC-0176 已迁移 callable 声明与 typed contract；SPEC-0029 已实现名称/字段 place 的同步
call loan 与 owned-value drop facts。以下 element place、receiver 与 capture 项仍按独立 Spec
保持未完成。

- [x] 在当前 named/field call 范围实现简化版单一所有者 + ASAP drop facts（不做完整 NLL）
- [x] 按[05-grammar-calls-lambda.md](./05-grammar-calls-lambda.md)第 9 节已封闭的 callable contract 检查调用点无 marker / `borrow` / `&`
      与声明侧 `own`→Value、无标记 / `borrow`→Borrow、`inout`→Inout 契约，判定 place / temporary、可变性、复制 / 移动
      与借用冲突；不得按函数名猜测例外
- [x] 移动后使用（use-after-move）检测
- [x] 按类型能力区分复制与移动：`Copyable value class` 可以复制；非 `Copyable value class`
      与普通 `class` 转交所有权后都禁止再次使用
- [ ] 接口委托生成的转发调用保持原方法的 `Value` / `Borrow` / `Inout` 契约，并把字段访问、
      移动与借用冲突归入同一套所有权检查；不得把委托隐式升级成共享运行时代理
- [x] 检查消费式解构：不可复制聚合解构后源值不可用，所有分量作为一个所有权动作转移
- [x] 拒绝通过普通字段访问或单独 `componentN()` 移出不可复制分量，不建立部分移动状态
- [ ] 移动顺序容器时转移唯一缓冲区 owner，拒绝再次使用源容器；构造时按 `Copyable`
      能力处理已有 place：已有 place 与临时表达式同样不需要额外标注，按 `Copyable` 复制
      或移动元素，不插入 clone、retain 或 `Box`；initializer 返回值直接交付
- [ ] 检查顺序容器 element place：可复制元素可读出 owned copy；不可复制元素只可借用，
      禁止部分移出；`inout` 仅适用于 `Array` / `MutableList`
- [ ] 跟踪元素借用与 `MutableList` 扩容、缩容、删除、替换、重排的冲突；按
      [`01-design-decisions.md`](./01-design-decisions.md) 第 8 节固定的提交
      顺序检查元素替换；所有正常构造、移动、替换、扩容和析构路径上，每个资源恰好析构一次
- [ ] `move (...) -> T` 函数类型的检查：验证传给此类参数的闭包字面量必须带 `move` 前缀，且闭包体内不能捕获任何借用语义的外部变量
- [ ] `Transferable` 标记能力检查：跨线程 API（`thread` 等）转移的值类型必须满足对应约束；
      `Shareable` 延后到 v2

**验收标准**：能正确拒绝典型的“移动后使用”和“重复可变借用”错误用例；复制
`Pair<Int, Int>` 后源值仍可用，复制 `Pair<Sender<Int>, Receiver<Int>>` 被拒绝，后者消费式
解构后再次使用源值也被拒绝；遗漏任一分量的消费式解构、普通字段读取 `pair.first` 这类
移出不可复制字段的部分移动均被拒绝；能正确拒绝“把借用捕获的普通闭包传给 `thread()`”
这类用例（必须报错要求改用 `move { ... }`）。泛型 `<T>` 交给声明端 `own` 参数后再次使用源值被拒绝，
而 `<T : Copyable>` 的同类操作交付 owned copy，源值仍可用。还必须覆盖 `List<Endpoint>` 的
构造、整体移动和元素借用：`listOf(endpoint)` 对不可复制的 `endpoint` 直接移动该值（调用点
不需要标注），移动后 `endpoint` 不可再使用；移动 List 后再次使用源 owner、把 `list[i]`
用作按值调用实参来移出不可复制元素、元素借用存续期间触发 `MutableList` 重分配都必须
报错。显式 `List<Box<Endpoint>>` 继续按 `Box` 所有权检查，不获得特殊规则。**`&`（`Inout`）
标注的元素借用必须并入同一套借用冲突检查，不能因为拼写是符号而不是关键字就被当成
独立类别**：`mutate(&list[i])` 存续期间对同一 `list` 触发扩容/`add`/`removeAt` 必须报错，
且必须与既有的 `use(borrow list[i])` 场景共享同一条“任意有效元素借用都阻塞重分配”规则
（见[01-design-decisions.md](./01-design-decisions.md)第 8 节），不允许出现“`borrow` 标注的元素借用被拦截、
但 `&` 标注的元素借用被放过”这类不一致；`&list[i]` 与 `borrow list[i]` 在同一调用
的不同实参位置同时出现时（例如 `swapInto(&list[i], borrow list[j])`，`i != j`）必须按
索引证明不重叠才能放行，索引相同或无法证明不同则保守拒绝，与第 8 节“索引确定相同则
冲突，无法证明不同则保守视为可能冲突”的规则完全一致，不因为一边是 `&`、一边是
`borrow` 而有特殊豁免。
Map 所有权检查不在本版 Phase 3 范围内，必须等待第 8 节要求的后续 guide（候选设计见
[01-design-decisions.md](./01-design-decisions.md)第 18 节，尚未批准为实施契约）。

## Phase 4：LLVM 代码生成

- [ ] 自建 SSA IR，从 AST/类型检查结果 lower 到该 IR
- [ ] IR 到 LLVM IR 的映射（用 `inkwell`）
- [ ] `value class`（内联布局）vs `class`（堆分配）的 codegen 差异实现；布局策略与
      `Copyable` 能力保持正交
- [ ] 生成复制/移动/消费式解构：复制只用于 `Copyable` 类型，非可复制内联字段转移后不
      重复析构
- [ ] `Copyable` 复制不调用 retain / clone glue，也不为被复制值生成唯一析构义务
- [ ] 在 typed SSA 中保留顺序容器 owner、构造、length、checked-index、place load / borrow /
      store、relocation 和 drop 基元；owned SSA 值在每条正常退出路径恰好消费或析构一次
- [ ] 为单态化元素生成 size / alignment / stride，以系统堆基线生成固定大小 owner header、
      单个连续缓冲区、受检分配大小和先检查后寻址的索引；不生成逐元素 `Box`
- [ ] 在构造 LLVM 类型前拒绝目标 DataLayout 中的 size / alignment / stride 溢出和超过目标
      可表示对象大小的聚合，返回结构化用户诊断而不是 LLVM 错误或编译器崩溃
- [ ] 大型聚合与容器 header 的 ABI 间接传递不得 lower 为隐式 `Box`，也不得仅因参数或返回
      约定产生堆分配
- [ ] 验证标准顺序容器不存在 small-buffer storage-kind tag、短 / 长双表示或按优化级别改变的
      静态类型
- [ ] 构造和替换保持第 8 节的求值 / 提交 / 析构顺序；正常析构按元素逆序后释放缓冲区，
      ZST 仍按逻辑 `size` 执行 drop；abort 路径不生成异常展开或部分构造 cleanup
- [ ] 在目标布局确定后估算静态栈帧和实际仍存在的隐式大值复制，以对应 Spec 分配的稳定
      warning code 报告目标相关阈值超限，不因 warning 自动改变类型或表示
- [ ] **闭包环境捕获的 codegen**：捕获环境结构体的内存布局设计，`move` 闭包与默认借用闭包在捕获方式上的差异实现，无捕获场景下降级为裸函数指针
- [ ] 析构函数插入（对应 Phase 3 的 ASAP 析构点）
- [ ] `error()` 编译为 abort 语义（不生成栈展开代码）
- [ ] DWARF 调试信息生成

**验收标准**：能编译并运行[01-design-decisions.md](./01-design-decisions.md)附录（原第二部分核心结构声明总览）示例代码，产出正确结果的可执行文件；带副作用的解构
右值只执行一次，消费式解构后的每个不可复制字段恰好析构一次，不可复制 `value class`
移入 `Box` 后源存储不再析构，可复制 `Point` 装箱后源值仍有效，复制
`Pair<Int, Int>` 不调用 glue 且源值仍有效；能用 `gdb`（Linux）或 `lldb`（macOS，现代
macOS 工具链下 `gdb` 需要额外签名权限，`lldb` 摩擦更小，两者都基于 DWARF）设断点单步
调试。IR / runtime 基元验收还必须证明：运行时长度的 `Array<Point>` 使用一个连续堆缓冲区，
`List<Point>` 不为各元素单独分配；大型 `Point` 可以 ABI 间接传递，但不得出现隐式 `Box`
allocation；非 `Copyable` 元素在替换、移动、重分配和正常析构路径上恰好 drop 一次，ZST 的
drop 次数仍等于逻辑长度。越界检查必须发生在地址计算前，负长度、分配大小溢出与 OOM 走
abort 且不生成异常展开。大栈帧 / 大型隐式复制测试必须锁定 warning code 和关键 `Span`，并
证明 warning 不会改变程序的静态类型。分配消除不属于本 Phase 的正确性验收；启用时必须由
独立 Phase 4+ 优化 Spec 与基线结果做差分验证。

## Phase 5：最小标准库（用目标语言自身编写）

- [ ] 在预声明的 `Array`、`List`、`MutableList` 及 Phase 4 基元之上，用目标语言实现
      `MutableList` 增删等普通集合方法与算法；不在 `.ko` 中重新声明 `arrayOf`、`listOf`、
      `mutableListOf`、运行时长度构造、`size` 或 `[]`，也不重新实现容器 header
- [ ] `Result<T, E>`、`Pair<A, B>`（自动解构支持；`Pair` 按类型实参条件满足 `Copyable`）
- [ ] `Rc<T>`/`Box<T>`（`Box<T>` 只接受 value class，参数声明端使用 `own` 并取得传入值所有权；`Rc<T>` 需要
      retain，因此本身不满足 `Copyable`）
- [ ] 高阶函数支持的集合操作：`map`/`filter`/`reduce`/`forEach`
- [ ] 基础 IO：`File`、`BufferedReader`、标准流
- [ ] 线程/channel API，`thread()` 的 task 参数声明 `own`，类型使用 `move (...) -> Unit`；
      `Sender.send` 的 value 参数同样声明 `own`
- [ ] `@Test` 注解 + 断言函数，跑通自身的测试套件

**验收标准**：标准库自身的测试套件全部用目标语言编写并通过；至少覆盖
`Pair<Int, Int>` 的复制、`Pair<Sender<Int>, Receiver<Int>>` 的构造与消费式解构，以及把
可复制 `Point` 交给 `Box` 后继续使用源值、把不可复制 `value class` 移入 `Box` 后禁止再次
使用源值、`Box<Node>` 的 compile-fail 和 `Rc<T>` 不被误判为 `Copyable`。顺序容器测试还
必须覆盖基础标量、可复制 / 不可复制 `value class`、普通 `class` handle 和显式 `Box` 元素：
`List<Point>` 不产生逐元素分配，`List<Box<Point>>` 只因显式 `Box` 产生间接分配；运行时读取
`n` 后，`Array<Point>(n, initializer)` /
`List<Point>(n, initializer)` 必须得到长度 `n`，按索引升序各调用 initializer 一次，
负长度稳定 abort。`Array<Endpoint>` 替换元素必须按提交顺序且
旧值析构一次，`MutableList<Endpoint>` 多次扩容后值保持正确且每个资源最终只释放一次。空容器、
零大小元素、单元素容器、分配大小溢出、移动后使用、不可复制元素的按值构造和索引借用
都必须有正反例；顺序容器 `.get` / `.set` 与 `getOrNull` 均不得作为隐藏的特殊入口出现。
Map 不是本版 Phase 5 验收项；不得为让测试通过而将本版未定义的 key 等价性或
所有权策略固化在标准库中（[01-design-decisions.md](./01-design-decisions.md)第 18 节的候选设计在正式批准前同样不得被当作
既成契约提前固化）。

## Phase 6：工具链完善

- [ ] 包管理器 CLI（`project.toml`/`project.lock`）
- [ ] LSP 基础功能（语法高亮、诊断、跳转定义）
- [ ] 代码格式化工具
- [x] TextMate/Tree-sitter 语法文件

**Phase 6 之后**：并发编译期检查完善、泛型型变、`dyn` 动态分发、`async`/`await` 等 v2/v3 特性按需排期，不在 v1 范围内。

---


---

## 工程规范（原第六部分）

- **crate 划分严格遵循 Phase 0 的 workspace 结构**，`lang-frontend` 不得依赖 `inkwell`/LLVM 相关 crate。
- **每个 Phase 的功能提交都必须配套测试**：语法/类型检查类改动配套 `.ko` 测试用例，编译器内部逻辑配套 Rust 单元测试。
- **诊断信息优先级高于功能完整度**：宁可先实现“检测到错误 + 准确报告”，再实现“错误恢复继续编译”。
- **每个新增关键字/语法结构必须同步更新[02-lexical-spec.md](./02-lexical-spec.md)关键字表**，避免文档与实现脱节。
- **命名规范**：Rust 代码本身遵循标准 Rust 命名约定，与目标语言的 Kotlin 风格命名是两套独立的命名体系，不要混淆。
