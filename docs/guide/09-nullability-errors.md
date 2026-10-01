# Koven v0.38：空安全与错误值

> **性质**：规范性语言规范 · **状态**：current（v0.38） · **读取时机**：实现或评审 nullable、Nothing、Result、error 与 postfix ? 时 · **唯一真源**：本页

本页是现行 Koven v0.38 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## `error()` 与空安全运算符

Koven 使用标准函数 `error()` 表示不可恢复终止：

```kotlin
fun divide(a: Int, b: Int): Int {
    if (b == 0) error("division by zero")
    return a / b
}
```

- `error` **不是关键字**，而是标准库顶层函数：`fun error(message: String): Nothing`。`Nothing`
  的 bottom-type 规则适用于所有返回 `Nothing` 的 callable，不按函数名特判。
- `error()` 不抛出可捕获异常，而是终止进程（abort）。
- `Nothing` 参与 bottom-type 类型推导：`if` 一个分支返回 `Nothing`，另一分支返回 `T`，整体类型推导为 `T`。

**非空断言 `!!` 保留**，定义为纯语法糖，不引入新的运行时机制：

```kotlin
val len = name!!.length
```

`e!!` 精确求值一次，类型从 `T?` 收窄为 `T`；失败效果等价于标准 abort。
assertion 的 Abort 身份由编译器绑定，不受同名 `error` 遮蔽；显式 `error(...)` 保持普通名称解析。

## `Result<T, E>` 与 Postfix `?`

v1 把可预期、可恢复的失败表达为普通返回值，不提供异常体系。源语言精确没有 `throw`、
`try`、`catch`、`finally`、`throws`、可捕获异常类层级或异常栈展开；函数返回类型
`Result<T, E>` 本身就是完整失败契约，不再用第二个关键字重复声明。合法但没有值使用 `T?`，
可恢复失败使用 `Result<T, E>`，程序不变量破坏使用不可捕获的 `error()` abort，三者不得
混用。

```kotlin
fun readConfig(path: String): Result<Config, IoError> {
    val text = File.readText(path)?    // Err 分支在此提前返回
    val parsed = parse(text)?
    return Ok(parsed)
}
```

- `expr?` 只传播 `Result`。它所在最近 callable 的返回类型必须是 `Result<T, E>`，operand
  类型必须是 `Result<U, E>`，其中 `E` 精确相同。v1 不把 `?` 开放为用户可实现协议，不传播
  `T?`，也不做 `Into`/`From` 式隐式错误转换；错误类型变化必须由显式 `when` 或后续标准库
  `mapError` 完成。
- 语义上，`expr?` 等价于：

  ```kotlin
  val __tmp = expr
  when (__tmp) {
      is Ok -> __tmp.value
      is Err -> return __tmp
  }
  ```

  即：`expr` 只求值一次；`Ok` 分支时整个表达式的值是内部的 `value`（按 `Copyable` 规则
  复制或移动，和[构造与结构移动规则](11-copyability-layout-construction.md)一致）；`Err` 分支时
  从最近 callable `return` 整个 `__tmp`（而不是重新构造一个新 `Err`）。具名函数和 lambda
  都是 callable boundary；lambda 内的 `?` 只退出该 lambda，绝不从外层具名函数非局部返回。
- `?` 的优先级与 `!!` 相同，归入[表达式与运算符规则](04-expressions-operators.md)运算符层级表的第 1 级（postfix，左结合，
  可连续）。由于 `?.` 是 Kotlin safe-member 的单一最长匹配 token，传播后立即访问普通成员
  必须显式分组为 `(foo()?).bar`；`foo()?.bar` 永远表示 safe member，不解释为 `foo()?` 后
  接 `.bar`。
- `?` 与 `!!` 的差异：`!!` 面向 `T?`，失败时 `error()`（abort，不可恢复）；`?` 面向
  `Result<T, E>`，失败时是**callable 级别的普通提前返回**，把错误值交还给调用者，不终止进程。
  二者不能混用（不能对 `Result<T, E>` 用 `!!`，也不能对 `T?` 用 `?`）。
- 与[所有权规则](10-ownership-borrowing-drop.md)的交互：`expr?` 对不满足 `Copyable` 的 `T` 同样成立，`Ok`
  分支消费 `__tmp` 并移出其 `value`（单一分量的消费式解构，复用[结构移动规则](11-copyability-layout-construction.md)的
  机制）；`Err` 分支整体移动 `__tmp` 用于 `return`。

Parser 只建立 postfix AST，不拥有 callable 返回类型或 `Result` 名称绑定信息，因此在所有
expression context 接受 `?`；上述 callable、operand 与 `E` 约束由类型检查形成诊断。所有权
检查把 `Err` 传播视为普通 return 路径，lowering 在该路径生成正常 drop，
不得引入 unwind cleanup。应用边界使用 `when` 选择恢复、转换、报告或调用 `error()`；
`Result` 不“接住异常”，因为该模型中没有异常被抛出。

## Nullable flow 与 extraction

### 35.1 共同求值、证明与 owner 原则

- nullable subject/operand 精确求值一次。null 判别只读取 nullable source，不复制、retain、移动
  或改变 wrapper；成功的 non-null edge 产生绑定到同一 root/place/loan identity 的 non-owning
  proof/view。Borrow/Inout binding、普通字段和容器元素可形成只读 proof，但不因此获得 owned
  inner 或可移动 root。field/element proof 仅绑定一次求值的内部 subject；
  后续重新求值同一字段或元素表达式不因此自动 smart cast。
- proof 只证明同一 source 在当前 CFG edge 非空，不产生第二个 `T` owner。shared read、projection
  或显式 `Rc.share()` 可复用该 view；owner/place move/drop、赋值、冲突调用及 branch join 继续
  按[控制流](06-blocks-control-flow.md)与[所有权规则](10-ownership-borrowing-drop.md) 终止 proof。ADR-0017 首版 SSA 只表示 owned nullable Value 的 branch；loan/place
  subject 的 native proof 需要后继 nullable-place branch ADR，不能把 loan 伪装成 owner。
- 从已证明非空的 `T?` 取得普通 Value `T` 时统一采用 extraction：`T : Copyable` 复制 inner，
  原 nullable root 仍可用；MoveOnly `T` 消费**整个** nullable root 或 temporary，并把唯一 inner
  obligation 转交给结果。不得从 Borrow/Inout binding、普通字段、容器元素或其他不能整体
  Value-deliver 的 place 移出 inner 后留下 wrapper/owner 洞。
- MoveOnly extraction 只能在已有 non-null proof 的 edge 执行 take，消费 wrapper 并转移 inner；
  nullable `when` 直接使用 branch proof，`!!` 则先自行建立 null/non-null branch。`!!` 的 null
  edge不执行 take，而是直接进入 compiler-bound Abort，因此没有正常后继，也不生成 unwind
  cleanup。只有成功 take 的 non-null edge继续执行，原 binding 在该 edge 已 moved，后续使用
  沿用 L0131。

### 35.2 nullable `when` 的剩余域

- subject 仍按[控制流规则](06-blocks-control-flow.md) 只求值一次。对稳定 `T?` subject，显式 `null` condition 的匹配 edge
  获得 null fact，不匹配 edge 获得 non-null fact；后续 entry 接收之前所有未匹配 condition 的
  剩余域，最终 `else` 接收完整补集。
- 同一 entry 的逗号 alternatives 独立从该 entry 的输入域判断，body 只保留所有可到达匹配
  alternative 共同成立的事实。因此 `null, SomeCase -> ...` 不把 body 错误收窄为非空；只有
  每条可达 alternative 都证明非空时，body 才获得 `T` view。
- nullable enum/Boolean 的 case coverage 与 null coverage 组合现行有限域规则；重复覆盖、非穷尽、
  branch type 与非法 condition 继续使用 L0108–L0112。临时 subject 没有可供源码复用的 stable
  binding，但 lowering 仍必须在内部携带同一 subject owner，不能重新求值表达式。
- 分支内只读/借用 subject 使用 non-owning view；若表达式上下文要求 Value `T`，则按 §35.1
  Copy/Consume extraction。未提取 temporary 按完整表达式边界析构；named owner 始终服从现行 ASAP/liveness，
  合流后仍有合法使用时不得在分支出口提前析构。已提取分支不再为原 subject drop wrapper
  或 inner；转移后的结果 owner 保留正常析构义务。

### 35.3 非空断言 `!!`

- `e!!` 继续具有 [空安全规则](09-nullability-errors.md)定义的表面语义：要求 `e : T?`、结果为 `T`，失败等价于
  `error("Non-null assertion failed")`；此等价仅描述失败效果，assertion 使用 compiler-bound Abort，
  不执行 `error` 名称查找，也不受同名声明遮蔽；显式 `error(...)` 仍遵循普通名称解析。
- `e` 为 `Copyable` nullable 时，非空 edge 复制 inner；若 `e` 是可继续访问的 place，原值保持
  可用。`e` 为 MoveOnly nullable 时，`!!` 是 Value extraction，必须整体消费合法 owner
  root/temporary。Borrow/Inout binding extraction 使用 L0133，普通字段 partial extraction 使用
  L0132，顺序容器 element extraction 使用 L0136，active-loan 冲突使用 L0135；这些拒绝不适用于
  Copyable inner 的普通复制。
- `!!` 不提供 place-preserving borrow unwrap，也不根据外层 Borrow receiver/call argument
  静默改变结果契约。需要非消费访问时使用显式 null check/nullable `when` 的 non-null view；
  borrow-return 或可存储 nullable view 等待后续语言设计。
- 非 nullable operand 继续使用 L0085；move/partial-move/loan 失败使用上述
  L0131–L0133/L0135/L0136 稳定分类，不把 L0134 的 mutable-place 含义挪作 extraction。
  本规范不分配 L0153，也不把 codegen 尚未接线伪装成新的源码错误。
