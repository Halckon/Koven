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

## parameter-report

三个文件共同使用上一节的 `project.toml`，entry为 `app.main`：
`kovenc build --project project.toml --entry app.main -o program`，
`kovenc run --project project.toml --entry app.main -- alpha 你好 tail`。
本例四组argv已由SPEC-0268在双宿主通过真实build、artifact与run验收。

`src/app/model.ko`：

```koven parameter-report-model
package app

class Report(var text: String)
```

`src/app/processor.ko`：

```koven parameter-report-processor
package app

fun reportArguments(args: Array<String>): Unit {
    val report = Report("start")
    for (argument in args) {
        val previous = replace(&report.text, argument.clone())
        println(report.text)
    }
    println("processed")
}
```

`src/app/main.ko`：

```koven parameter-report
package app

fun main(args: Array<String>): Unit {
    reportArguments(args)
    println("done")
}
```

跨文件 `Report` 的字段通过 `replace` 更新，`previous`接收并清理旧owner；
`println(report.text)`直接借用字段。循环binding借用argv元素，显式clone生成独立字段owner。
同一源码验证空argv、`alpha`、`alpha 你好 tail`及一个空字符串参数；每组均build、执行产物、run。
`processed`与`done`保证循环后代码和caller均继续执行。完整输出见 `examples.json`。

## numbers-bitwise

本例使用固定 Int，验证不同 radix 和分隔符表示同一数值，以及32位移位计数屏蔽。
语义以 [表达式与运算符](../guide/04-expressions-operators.md)为准；没有演示全部整数宽度或溢出情况。

```koven numbers-bitwise
fun main(): Unit {
    val decimal = 42
    val hexadecimal = 0x2A
    val binary = 0b10_1010
    if (decimal == hexadecimal && hexadecimal == binary && 1_024 == 1024) {
        println("literals")
    }
    if ((1 shl 33) == 2 && (binary and 0b1111) == 10) {
        println("bits")
    }
}
```

## scope-cleanup

资源保留到对应函数作用域退出；callee 的第二个资源先析构，随后是第一个资源，再继续 caller 语句。
最后 caller 的资源析构。这里观察的是 resource 规则，不据此推定纯内存值的 ASAP 清理时机。

```koven scope-cleanup
class Resource(val name: String) { deinit() { println(this.name) } }
fun inner(): Unit {
    val first = Resource("first")
    val second = Resource("second")
    println("inner")
}
fun main(): Unit {
    val outer = Resource("outer")
    inner()
    println("after")
}
```

## unit-loop-cleanup

两个文件使用前述 `project.toml`，entry 为 `app.main`。
本例组合已支持的 unit native for、临时 MutableList provider、借用调用、局部资源及三条退出路径。
mode 0 走 continue，两次访问；mode 1 在第一次访问后 break；mode 2 在第一次访问后 return。
每轮局部资源均先清理；容器元素随后按逆序清理。return 跳过 `after`，但外层资源与 caller 仍有可见输出。
不推导 Map、任意 provider 或全部嵌套控制流支持范围。

`src/worker/Work.ko`：

```koven unit-loop-cleanup-worker
package worker

class Leaf(val name: String) { deinit() { println(this.name) } }
fun inspect(leaf: Leaf): Unit { println("visit") }
fun work(mode: Int): Unit {
    val outer = Leaf("outer")
    for (item in mutableListOf(Leaf("first"), Leaf("second"))) {
        val local = Leaf("local")
        inspect(item)
        if (mode == 2) { return }
        if (mode == 1) { break }
        continue
    }
    println("after")
}
```

`src/app/Main.ko`：

```koven unit-loop-cleanup
package app

fun main(): Unit {
    println("continue")
    worker.work(0)
    println("break")
    worker.work(1)
    println("return")
    worker.work(2)
    println("caller")
}
```

## reject-immutable-place

`replace` 需要可变 place；`val` root 不满足要求。JSON 合同固定完整诊断和 Span。

```koven reject-immutable-place
fun main(): Unit {
    val number = 1
    val previous = replace(&number, 2)
}
```

## reject-iteration-move

循环从 provider 借用元素期间，不能把 provider owner 移交给 own 参数。
这与下面普通 use-after-move 负例不同：拒绝的是活跃迭代借用期间的移动。

```koven reject-iteration-move
class Item()
fun consume(own items: Array<Item>): Unit {}
fun main(): Unit {
    val items = arrayOf(Item())
    for (item in items) {
        consume(items)
    }
}
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

## gap-scope-branch

下面的嵌套分支资源例在 SPEC-0271 的固定 Mac 基线实际 build 失败：
退出码2，stdout为空，stderr为
`error: native build failed: native object InvalidModel: frontend lowering failed with InvalidSsa`。
这是 native 实现缺口，不是语言不合法的诊断示例；暂列 planned，CI 不执行，不计通过。
本批保留失败，不为取得教程通过而修改编译器或 Guide；前述函数作用域正例不能证明此分支组合可运行。

```koven gap-scope-branch
class Resource(val name: String) { deinit() { println(this.name) } }
fun main(): Unit {
    val outer = Resource("outer")
    if (true) {
        val first = Resource("first")
        val second = Resource("second")
        println("inner")
    }
    println("after")
}
```
