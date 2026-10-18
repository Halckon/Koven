# 当前 Koven 可执行 tour

> **性质**：当前教程 · **状态**：current · **读取时机**：运行最小示例时 · **唯一真源**：本页源码，Guide 定义语义

本页依据现行 [Guide v0.44](../guide/README.md)，仅覆盖继承的已交付子集；每个具名 Koven fence 都已在库存登记。
可执行与诊断示例有输出合同；`planned-thread` 仅登记状态和原因，尚无输出合同，不计通过。
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

## collections-and-borrow-results

以下说明帮助选择已有能力；不会把任意组合或未列出的标准库 API 当作可用。
具名 fence 的可执行库存不因此增加，源码保存在本页，已登记的输出合同保存在 `examples.json`。

### List 和 Map 从哪里开始

- 顺序迭代先看 [iteration](#iteration) 的 Array 和 [unit-loop-cleanup](#unit-loop-cleanup) 的
  MutableList。List 可用 `val items = listOf(1, 2, 3)` 初始化，再用 `for (item in items)` 顺序借用元素；
  [已有 List 源码](../../crates/lang-codegen/src/native_sequential_for_tests.rs)展示构造后传给 Borrow 参数并迭代。
  具名 owner、Borrow 参数和临时来源有 native 支持；字段和 Inout 参数作为迭代来源仍明确拒绝。
  具体来源与退出限制见[顺序迭代边界](../architecture/finite-sequential-iteration.md)。
- [argv-word-frequency](#argv-word-frequency) 是可运行的 Map 应用：`mutableMapOf<String, Int>()`
  建表，`get` 得到可空计数，`put` 更新，`remove` 移出计数。两次 `for` 都遍历 argv，
  第二遍只在首次成功删除时输出，因此保持首次出现顺序，不依赖 Map 的内部顺序。
- 借用已有槽位另有 `requireValue` 与同步 `withValue`；前者缺失时 abort，后者在命中时调用
  Borrow callback。它们的值类型和清理边界见[Map 访问说明](../architecture/map-native-execution.md)，
  不能把返回借用当成取出一个 owned 值。
- Map 的 `for ((key, value) in source)` 已有有限 single/unit 后端 native 子集：两个具名分量
  共享借用，沿插入链遍历。[具名参数源码](../../crates/lang-codegen/tests/fixtures/map-provider-0331/map.ko)、
  [temporary 源码](../../crates/lang-codegen/tests/fixtures/map-provider-temporary-0331/map-temporary.ko)与
  [local 后用源码](../../crates/lang-codegen/tests/fixtures/map-provider-local-sources-0331/local-later-use.ko)
  展示不同来源；另有有界[Borrow 直接退出](../../crates/lang-codegen/tests/fixtures/map-provider-borrow-exits-0331/map-borrow-break.ko)
  与[Resource 分量正常耗尽](../../crates/lang-codegen/tests/fixtures/map-provider-resource-0331/map-resource.ko)回归源码。
  具体 kind/access、退出和嵌套范围见[Map 支持矩阵](../architecture/map-borrowed-iteration.md)。
  这些是后端回归来源，尚未新增本页公开 CLI 合同；Field/Inout 原生按 Guide 延后；own MutableMap 的 String/String 四种退出及透明 Group
  来源补片，以及含String字段/结构性Resource等既有表示、Pure own参数在退出后继续使用或同根再次遍历
  的有限回归见支持矩阵；其他未验组合
  仍按当前实现边界报告。
- 集合 `forEach`/`map`/`filter` 与未列出的集合/格式化 API 仍未交付。词频里的 `decimal`
  是本页实现的普通函数，不是标准库格式化接口。

### 普通返回与借用返回不同

[function](#function) 的 `greeting(): String` 返回由调用方接收的普通值；
[borrowing](#borrowing) 则只在函数调用期间借用参数，不取走 owner。
若函数要把现有对象的借用交给调用方，声明中须明确结果和唯一来源，例如
`fun view(source: String): borrow String from source = source`。
调用方可用 `borrow val item = view(source)` 保留借用；结果不是独立 owner，来源必须存活，
借用结束前不能移动或冲突修改来源。直线序列的已证明最后使用可结束借用，不能由读者任意猜测。
已有[多参数来源示例](../../crates/lang-codegen/tests/fixtures/multi_borrow/first_parameter.ko)
展示保留结果后继续使用，以及借用结束后再交付来源。

在支持的直接具名同步调用中，也可写 `observe(view(source))`，不必先建局部借用绑定；
见[已有源码](../../crates/lang-codegen/tests/fixtures/nested_borrow/single_direct.ko)与
[同步实参边界](../architecture/synchronous-borrow-result-arguments.md)。这些后端回归源码不等于
本页新增的 CLI 合同；普通借用返回仍限已接通的表达式体、具名包装及具体泛型实例，
不提供可存储引用、任意条件来源、Inout 来源、闭包逃逸或一般跨控制流借用。

普通 Borrow 的非泛型 value-class 方法已能返回 owned Int/Boolean；
[Boolean 字段示例](../../crates/lang-codegen/tests/fixtures/readonly_methods/bool.ko)在公开单文件
CLI 测试中使用 `--entry entry` build/run。此能力不表示单文件入口的普通 class 方法、
任意 receiver 来源或借用返回方法已全部支持。

### List/View 范围与正常短路

project CLI 会加载真实标准库范围源码，可导入 `koven.algorithms.take`、`drop`、`dropLast`。
例如 `borrow val part = take(items, 2)` 建立 List/View 的借用范围，不复制元素；
非负 count 超出长度时截到边界，负数会 abort。View 不是新的 owned List，不能作为 owned 值放入字段/容器或逃逸。
单文件 `kovenc build source.ko` 不加载这些范围标准源码；应使用 [cross-file](#cross-file)
介绍的 project manifest/命令，而不是把单文件后端测试当成该 CLI 入口已支持。

已建立的同根范围可在受限词法块里比较 Int、执行语句 `if`，并使用单层 Boolean `&&`/`||`：
该块的范围绑定须全部在前，短路仅作为直接 Boolean 局部初始化、直接同步调用的 Boolean
实参，或已支持语句 `if` 的完整条件。左侧执行一次，右侧只在选中时执行；合流后可继续读取范围。
这里的调用必须满足只读交付限制，不允许 owning MoveOnly 实参/receiver 或 Inout/mutating 调用。
嵌套短路、循环体内短路、多 root、新分支绑定、nullable/索引等复杂操作仍不属于这个子集。
真实公开 project 来源见[范围短路测试](../../crates/lang-cli/tests/range_project_cli/boolean_short_circuit.rs)，
完整限制见[范围局部控制流](../architecture/range-local-control-flow.md)。

Map 的返回表达式也有窄范围支持：[已有回归源码](../../crates/lang-codegen/tests/fixtures/map-return-last-use-0318/cardinality.ko)
验证单一局部 Map<Int, String> 配合临时 Array<Int> 顺序循环，在 Int return 操作数中
`remove`/`contains` 后正常返回并清理。
这是 single/unit 后端 native 的代表源码，不是 Map 遍历，也不是本页新增的公开 CLI 合同；
不能外推所有容器、嵌套循环或任意返回表达式。具体范围见[Map owner 与循环](../architecture/map-loop-owner-carry.md)。

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
本例集中展示跨文件普通函数调用；跨文件顺序迭代及退出另见 [unit-loop-cleanup](#unit-loop-cleanup)。

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

本例的原始 `InvalidSsa` 失败保存在 SPEC-0271 的历史证据中。
SPEC-0273 补齐控制体正常出口的资源清理后，同一源码加入实际 CLI 合同；
输出依次为 `inner`、`second`、`first`、`after`、`outer`，证明分支内逆序清理及外层资源存活。

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

## argv-word-frequency

三个文件沿用 `source-roots = ["src"]` 的 project，entry 为 `app.main`。
每个参数是一个完整词；按首次出现次序原样输出词、TAB、计数、LF。
空参数也是词；词内的控制字符原样保留，输出不是可逆的行格式。

`src/app/decimal.ko`：

```koven argv-word-frequency-decimal
package app

fun decimal(value: Int): String {
    if (value < 0) { error("negative count") }
    val digit = when {
        value % 10 == 0 -> "0"
        value % 10 == 1 -> "1"
        value % 10 == 2 -> "2"
        value % 10 == 3 -> "3"
        value % 10 == 4 -> "4"
        value % 10 == 5 -> "5"
        value % 10 == 6 -> "6"
        value % 10 == 7 -> "7"
        value % 10 == 8 -> "8"
        else -> "9"
    }
    val prefix = if (value >= 10) { decimal(value / 10) } else { "" }
    return prefix + digit
}
```

`src/app/statistics.ko`：

```koven argv-word-frequency-statistics
package app

fun printWord(word: String, count: Int): Unit {
    println(word + "\t" + decimal(count))
}
```

`src/app/main.ko`：

```koven argv-word-frequency
package app

fun main(args: Array<String>): Unit {
    var counts = mutableMapOf<String, Int>()
    for (word in args) {
        val previous = counts.get(word)
        val count = if (previous == null) { 1 } else { previous + 1 }
        counts.put(word.clone(), count)
    }
    for (word in args) {
        val count = counts.remove(word)
        if (count != null) {
            printWord(word, count)
        }
    }
}
```

统计借用 argv 两遍，共 O(n) 次 Map 操作；String clone、字节哈希/相等与碰撞成本另计，
不据此承诺总时间 O(n)。第一遍 clone 每个词为 Map 提供独立 owned key；第二遍按 argv
顺序 remove，仅首次移出计数时输出，不依赖 Map 遍历顺序。`count` 不超过 `args.size`，
保持在非负 Int 域。decimal 最多递归十层，支持 0 到 Int.MAX_VALUE；负值在正常输出之前
abort。该应用只组合现有 Map、String clone 和 nullable 合同，没有新增集合或格式化 API。
