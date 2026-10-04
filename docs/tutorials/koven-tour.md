# Koven v0.40 可执行 tour

> **性质**：当前教程 · **状态**：current · **读取时机**：运行最小示例时 · **唯一真源**：本页源码，Guide 定义语义

每例使用 `kovenc build source.ko -o program`，运行生成产物，再使用 `kovenc run source.ko`。
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
fun main(): Unit { for (value in [1, 2, 3]) { if (value == 2) { println("two") } } }
```

## arguments

```koven arguments
fun main(args: Array<String>): Unit { for (arg in args) { println(arg) } }
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
