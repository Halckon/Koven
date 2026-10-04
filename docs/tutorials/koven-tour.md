# Koven v0.40 可执行 tour

> **性质**：当前教程 · **状态**：current · **读取时机**：运行最小示例时 · **唯一真源**：本页源码，Guide 定义语义

单文件例使用 `kovenc build source.ko -o program`，运行生成产物，再使用 `kovenc run source.ko`。
输出合同见 `examples.json`；尚未执行的恢复验收不会标为通过。

## hello

```koven hello
fun main(): Unit { println("Hello, Koven!") }
```

## strings

```koven strings
fun main(): Unit { val text = "你好"; println(text.clone() + " Koven") }
```

## branch

```koven branch
fun main(): Unit { if (2 + 3 == 5) { println("five") } else { println("wrong") } }
```

## function

```koven function
fun greeting(): String = "function"
fun main(): Unit { println(greeting()) }
```

## constant

```koven constant
const val answer: Int = 42
fun main(): Unit { if (answer == 42) { println("constant") } }
```

## iteration

```koven iteration
fun main(): Unit { for (value in arrayOf(1, 2, 3)) { if (value == 2) { println("two") } } }
```

## arguments

```koven arguments
fun main(args: Array<String>): Unit { for (arg in args) { println(arg) } }
```

## borrowing

普通参数自动借用；调用后 owner 仍可再次使用。

```koven borrowing
class Cell(val text: String)
fun inspect(cell: Cell): Unit { println(cell.text) }
fun main(): Unit { val cell = Cell("borrowed"); inspect(cell); inspect(cell) }
```

## root-replace-swap

这里只对完整 `var` root 做原地置换，不推导字段或容器 element place 的支持范围。

```koven root-replace-swap
fun main(): Unit {
    var left = 1
    var right = 2
    val old = replace(&left, 3)
    swap(&left, &right)
    if (old == 1 && left == 2 && right == 3) { println("root swap") }
}
```

## deinit

concrete resource class 在作用域退出时析构；deinit 借用读取仍存活的字段。

```koven deinit
class Resource(val name: String) { deinit() { println(this.name) } }
fun main(): Unit { val resource = Resource("drop"); println("body") }
```

## cross-file

将两个 fence 分别保存为 `src/app/Main.ko` 和 `src/app/Values.ko`。
project manifest `project.toml` 内容如下：

```toml
schema = "koven.project"
version = 1

[project]
name = "tutorial"
source-roots = ["src"]
```

使用 `kovenc build --project project.toml --entry app.start -o program`，
或 `kovenc run --project project.toml --entry app.start`。
这里只调用当前 unit 已支持的普通函数，不含 unit-for。

```koven cross-file
package app
fun start(): Unit { println(message()) }
```

```koven cross-file-values
package app
fun message(): String = "cross file"
```

## reject-typed

```koven reject-typed
fun types(): Unit { val item: String = 1
val second: Boolean = 2 }
class Resource()
fun take(own resource: Resource): Unit {}
fun moves(own resource: Resource): Unit {
    val first = take(resource)
    val second = take(resource)
}
fun main(): Unit {}
fun invalid(): Int = 1
```

## reject-ownership

```koven reject-ownership
class Resource()
fun take(own resource: Resource): Unit {}
fun moves(own resource: Resource): Unit {
    val first = take(resource)
    val second = take(resource)
    val third = take(resource)
}
fun main(): Unit {}
fun invalid(): Int = 1
```

## planned-thread

```koven planned-thread
fun main(): Unit { thread(move { println("thread") }).join() }
```

本例属于 planned，不宣称当前 CLI 可执行。
