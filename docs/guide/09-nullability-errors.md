# Koven v0.34：空安全与错误值

> **性质**：规范性语言规范 · **状态**：current（v0.34） · **读取时机**：实现或评审 nullable、Nothing、Result、error 与 postfix ? 时 · **唯一真源**：本页

本页是现行 Koven v0.34 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

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

脱糖规则：`e!!` 等价于 `e ?: error("Non-null assertion failed")`，类型从 `T?` 收窄为 `T`。

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
