# 路线 X 存在类型试金石与反例矩阵（X2 借用访问者、X3 容器与 E3 通用错误）

> **性质**：非规范候选试金石与反例矩阵（仅源码评审，不求当前编译器可编译） · **状态**：未启用（不修改 Guide v0.47，不批准 Spec/ADR，不授权实现） · **读取时机**：评审路线 X 的 X2（借用 existential）、X3（owned existential 与容器）及之十一 E2/E3（`Result<T, Error>` + `?`）的源码形态、零分配边界与载荷/槽位可变访问区分时 · **唯一真源**：本页维护演进总纲之三 X2/X3 与之十三 §4.3–§4.4 要求的三组试金石源码与五类反例矩阵；存在类型内核与决断 J37–J38 见[演进总纲之三](evolution-03-route-existential.md)，错误转换见[演进总纲之十一](evolution-11-failure-interfaces-units.md) §1.5，现行语法基线以 [Guide v0.47](../guide/README.md) 为准

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权实现。源码以 [Guide v0.47](../guide/README.md) 为表面语法基线（`.size` 属性、`Ok.success`、无 `when (val ...)`、无末尾逗号、`inout` 仅写声明侧、调用点写 `&`），仅在路线 X 与 E2/E3 候选处使用[之三](evolution-03-route-existential.md)的裸 interface 值类型写法（参数位置为借用 existential，存储/容器/返回位置为 owned existential）。

## 0. 试金石与验收映射

| 试金石 / 矩阵 | 对应路线步骤 | 核心检验目标 |
|---|---|---|
| 1. 借用 existential 访问者 | X2（R2.1–R2.4、`pack_inout`） | 零堆分配动态分发（`value class` 不装箱）；载荷可变访问可改状态但禁整体替换 |
| 2. 事件总线 `MutableList<EventHandler>` | X3（R3.1–R3.6、`pack_owned`） | `class` 载荷零额外分配移交、`value class` 显式 `Box`、槽位整体替换与容器不变性 |
| 3. `Result<T, Error>` 与单 `?` 跨错误源 | X3 + 之十一 E2/E3 | 库保持精确错误枚举，应用层经显式转换 + owned `Error` 用一个 `?` 串联不同错误源 |
| 4. 五类路线 X 反例矩阵 | X1–X3、R2.4、R3.6 | 载荷访问禁替换、容器不协变、借用 existential 禁逃逸、动态不兼容拒绝、移动后使用拒绝 |

## 1. 试金石一：借用 existential 实现的访问者（验证 X2 零堆分配）

对应[之三](evolution-03-route-existential.md) X2 与[之十三](evolution-13-implementation-roadmap.md) §4.3 阶段二出口试金石：参数位置的 `inout visitor: ExprVisitor` 执行 `pack_inout`，无论是普通 `class` 还是栈上 `value class` 实参均**零堆分配**；同时验证 R2.4（载荷可变访问只允许调方法，不能整体替换具体类型）。

```kotlin
// 候选源码（依赖 X1/X2：动态兼容 interface 作参数位置借用 existential）
enum class AstNode {
    Lit(value: Int),
    Add(left: Box<AstNode>, right: Box<AstNode>)
}

interface ExprVisitor {
    inout fun onLit(value: Int): Unit
    inout fun onAdd(): Unit
}

// 栈上值语义访问者：传给 inout visitor: ExprVisitor 时直接取栈地址打包（pack_inout），不装箱、零分配
value class NodeCounter(var litCount: Int, var addCount: Int) : ExprVisitor {
    override inout fun onLit(value: Int): Unit {
        this.litCount = this.litCount + 1
    }

    override inout fun onAdd(): Unit {
        this.addCount = this.addCount + 1
    }
}

// 堆分配引用类型访问者：同样经 pack_inout 借用，不转移所有权
class LitSumVisitor(var sum: Int) : ExprVisitor {
    override inout fun onLit(value: Int): Unit {
        this.sum = this.sum + value
    }

    override inout fun onAdd(): Unit { }
}

// 参数位置 inout ExprVisitor 为载荷可变访问（仅可调用方法，不可整体替换 visitor）
fun walkAst(node: AstNode, inout visitor: ExprVisitor): Unit {
    when (node) {
        is AstNode.Lit -> visitor.onLit(node.value)
        is AstNode.Add -> {
            visitor.onAdd()
            walkAst(node.left.value, &visitor)
            walkAst(node.right.value, &visitor)
        }
    }
}

fun runVisitorLitmus(root: AstNode): Int {
    var counter = NodeCounter(0, 0)
    var summer = LitSumVisitor(0)

    // 两次调用均为零堆分配（X2 分配计数验收点）
    walkAst(root, &counter)
    walkAst(root, &summer)

    // 借用结束后原具体类型变量恢复可用
    return counter.litCount + counter.addCount + summer.sum
}
```

**验证要点**：
- `NodeCounter` 是 `value class`，传给 `inout visitor: ExprVisitor` 时直接把 `(指向局部 counter 的指针, Witness(NodeCounter, ExprVisitor))` 传入，**不触发 `Box` 分配**（R2.2）。
- 递归调用 `walkAst(node.left.value, &visitor)` 转发的是同一载荷可变访问，不会允许被调方把 `LitSumVisitor` 写进 `NodeCounter` 的栈槽位（R2.4）。

## 2. 试金石二：事件总线 `MutableList<EventHandler>`（验证 X3 owned existential）

对应[之三](evolution-03-route-existential.md) X3 验收项（事件总线 `List<Handler>`）：验证普通 `class` 经 `pack_owned` 零额外分配移交、`value class` 须显式 `Box`（X4 才引入隐式装箱糖）、精确析构与槽位整体替换。

```kotlin
// 候选源码（依赖 X3：owned existential 与容器存储）
interface EventHandler {
    inout fun handle(eventCode: Int): Unit
}

class AuditHandler(val tag: String, var handled: Int) : EventHandler {
    override inout fun handle(eventCode: Int): Unit {
        this.handled = this.handled + 1
    }
}

value class MetricHandler(var count: Int) : EventHandler {
    override inout fun handle(eventCode: Int): Unit {
        this.count = this.count + 1
    }
}

class EventBus(private var handlers: MutableList<EventHandler>) {
    companion {
        fun empty(): EventBus = EventBus(MutableList())
    }

    inout fun register(own handler: EventHandler): Unit {
        this.handlers.add(handler)
    }

    inout fun dispatch(eventCode: Int): Unit {
        var i = 0
        val n = this.handlers.size
        while (i < n) {
            // 经容器元素 place 获取载荷可变访问，按见证表分派 handle
            this.handlers[i].handle(eventCode)
            i = i + 1
        }
    }
}

fun runEventBusLitmus(): Unit {
    var bus = EventBus.empty()

    // 1. 普通 class 实例直接 pack_owned 接管既有堆 owner，零额外分配；原 audit 绑定此后不可用
    val audit = AuditHandler("auth", 0)
    bus.register(audit)

    // 2. value class 在 X3 内核层须显式 Box 成为间接 owner（X4 糖可省去显式 Box）
    val metric = MetricHandler(0)
    bus.register(Box(metric))

    // 3. 拥有存在类型的局部槽位允许整体替换（R3.5）：先求值新值，再经旧见证表精确析构旧值
    var current: EventHandler = AuditHandler("v1", 0)
    current = Box(MetricHandler(1))
    current.handle(42)

    bus.dispatch(200)
    // 离开作用域：bus 析构时逐元素经各自 Witness.drop_in_place 精确析构，ASan/LSan 零泄漏
}
```

## 3. 试金石三：`Result<T, Error>` 与单 `?` 串联不同错误源（验证 X3 + E2/E3）

对应[之十一](evolution-11-failure-interfaces-units.md) §1.5 E2/E3 与[之十三](evolution-13-implementation-roadmap.md) §4.4 阶段三出口试金石：库继续返回精确错误枚举，应用代码写 `Result<Config, Error>` 并只用一个 `?` 串起不同来源的错误。

```kotlin
// 候选源码（依赖 X3 owned existential Error 与之十一 E2 ? 处显式错误转换）
interface Error : Display {
    fun message(): String
}

enum class IoError : Error {
    NotFound(path: String),
    PermissionDenied(path: String);

    override fun message(): String = when (this) {
        is NotFound -> "file not found: ${path}"
        is PermissionDenied -> "permission denied: ${path}"
    }

    override fun toString(): String = this.message()
}

enum class ParseError : Error {
    InvalidSyntax(line: Int);

    override fun message(): String = when (this) {
        is InvalidSyntax -> "invalid syntax at line ${line}"
    }

    override fun toString(): String = this.message()
}

class AppConfig(val port: Int)

// 两个库函数各自返回精确枚举，不强迫库依赖抽象 Error
fun readConfigText(path: String): Result<String, IoError> { ... }
fun parseConfigText(text: String): Result<AppConfig, ParseError> { ... }

// 应用层：两处 ? 分别把 IoError / ParseError 经已声明的 Error 打包/转换规则提升为 owned Error
fun loadAppConfig(path: String): Result<AppConfig, Error> {
    val text = readConfigText(path)?
    val cfg = parseConfigText(text)?
    return Ok(cfg)
}
```

## 4. 路线 X 五类必测反例矩阵

| # | 反例类别 | 最小违规片段（候选语法） | 预期拒绝与依据 |
|---|---|---|---|
| XN1 | 载荷可变访问传给可整体替换的泛型 `inout T` | `fun <T : ExprVisitor> overwrite(inout dst: T, own v: T) { dst = v }` 中对 `inout visitor: ExprVisitor` 调用 `overwrite(&visitor, other)` | **拒绝（R2.4 / 演算 §10、§78）**：`visitor` 是载荷可变访问，隐式打开后 `T` 绑定为未知具体类型，拿不出同类型 `v`（且不允许直接替换 `visitor`） |
| XN2 | 容器不协变：试图把 `List<AuditHandler>` 当作 `List<EventHandler>` | `fun acceptAll(hs: List<EventHandler>)` 传入 `List<AuditHandler>` | **拒绝（R3.6）**：`List<AuditHandler>` 元素布局与定长双字存在类型 `EventHandler` 不同，不存在隐式转换 |
| XN3 | 借用 existential 逃逸或返回缺 `from` | `fun leak(s: ExprVisitor): ExprVisitor = s` 或存入 `class Hold(val v: ExprVisitor)`（当 `s` 为借用参数时） | **拒绝（R2.3 / L0163）**：借用 existential 是不可逃逸 Holder，不能脱离来源存储或无 `from` 返回 |
| XN4 | 动态不兼容 interface 出现在值位置 | `interface EquatableSelf { fun eq(other: Self): Boolean }` 后写 `fun bad(x: EquatableSelf)` | **拒绝（X1 R1.2 / J38）**：含 `Self` 参数或泛型方法的 interface 不可动态分发，只能作泛型约束 |
| XN5 | `pack_owned` 移交后继续使用原绑定 | `val a = AuditHandler("x", 0); bus.register(a); println(a.tag)` | **拒绝（R3.2 / L0131）**：普通 `class` 经 `pack_owned` 转移所有权后原绑定失效 |
