# L4 独占视图试金石与反例矩阵（演进总纲之二 L4）

> **性质**：非规范候选试金石与反例矩阵（仅源码评审，不求当前编译器可编译） · **状态**：未启用（不修改 Guide v0.47，不批准 Spec/ADR，不授权实现） · **读取时机**：起草 L4（独占 origin 与独占视图）形式化小演算、评审 `inout T` 类型与 `splitAt`/访问器边界，或核对 R4.1–R4.7 验收项时 · **唯一真源**：本页维护 L4 三组试金石源码与六类反例矩阵；L4 规则草案见[演进总纲之二](evolution-02-route-loan-kernel.md) L4，受信任合同与别名条款见 [U0 合同](unsafe-aliasing-contract-outline.md)，现行语法基线以 [Guide v0.47](../guide/README.md) 为准

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权实现。三组试金石均以 [Guide v0.47](../guide/README.md) 为表面语法基线（`.size` 属性、`Ok.success`、无 `when (val ...)`、枚举与参数列表无末尾逗号、`inout` 仅写声明侧、调用点写 `&`、`borrow` 调用点不写标注），仅在 L4 候选处使用[之二](evolution-02-route-loan-kernel.md) L4 的占位语法（`inout T` 返回类型、含 `inout` 字段的独占 `view class`、`trusted` 受信任转发签名）。

## 0. L4 候选语法基线与验收映射

| 试金石 / 矩阵 | 对应之二 L4 规则 | 核心检验目标 |
|---|---|---|
| 1. 原地排序（`MutSlice.splitAt` + `swap`） | R4.1、R4.5、受信任转发 | 独占视图 `MoveOnly`；受信任 `splitAt` 拆分两个不相交独占视图，内核不做下标整数推理 |
| 2. 二维矩阵行视图与用户自定义索引 | R4.2、R4.3、R4.4、R4.7 | 经 `inout` 访问器交付可写视图与元素 `inout Double`；子 loan 结束后父恢复可写 |
| 3. 链表可写游标（遍历中插删） | L4 形式化小演算必测用例 | 对比独占 Holder 重绑定为所指对象一部分的难点与安全递归对照写法 |
| 4. 六类 L4 反例矩阵 | R4.1–R4.7 | 并发双独占、独占期读来源、非受信任转发、两阶段不穿透访问器、逃逸与复制拒绝 |

## 1. 试金石一：原地排序（`MutSlice.splitAt` + `swap`）

对应[之二](evolution-02-route-loan-kernel.md) L4 验收项与 R4.5：含 `inout` 字段的 `view class` 恒为 `MoveOnly` 且不可逃逸；普通安全函数无法从同一独占视图一次转发两个并存的独占子视图，`splitAt` 由标准库在内部用 `unsafe` 保证两半区间不相交，并以 `trusted` 签名向借用内核声明转发来源。

### 1.1 库作者侧源码（受信任视图层）

```kotlin
// 候选源码（依赖 L4 inout T / 独占 view class、U1 裸指针与 U0 受信任合同）
// L4 阶段尚未放开 Pair<view A, view B>（归 L5），故使用专用视图聚合承载拆分结果
view class MutSlicePair<T>(
    val left: MutSlice<T>,
    val right: MutSlice<T>
)

view class MutSlice<T>(
    private inout val source: Array<T>,
    private val start: Int,
    private val length: Int
) {
    fun len(): Int = this.length

    fun get(index: Int): borrow T from this {
        if (index < 0 || index >= this.length) {
            error("MutSlice index out of bounds")
        }
        return this.source[this.start + index]
    }

    // R4.2：要求 inout this，调用期间与结果存活期间 source 上保持 Exclusive loan
    inout fun at(index: Int): inout T from this {
        if (index < 0 || index >= this.length) {
            error("MutSlice index out of bounds")
        }
        return this.source[this.start + index]
    }

    inout fun swap(i: Int, j: Int): Unit {
        if (i < 0 || i >= this.length || j < 0 || j >= this.length) {
            error("MutSlice swap index out of bounds")
        }
        if (i == j) {
            return
        }
        this.source.swap(this.start + i, this.start + j)
    }

    // R4.5：普通函数不能转发独占字段，只有受信任签名可声明拆分出两个不相交的独占子视图；
    // 消耗当前 MutSlice（own this），保证原视图不能再与两个子视图并存。
    // 不相交性（[start, start + mid) 与 [start + mid, start + length)）由库作者按 U0 AL1/AL4 证明。
    trusted own fun splitAt(mid: Int): MutSlicePair<T> from source {
        if (mid < 0 || mid > this.length) {
            error("MutSlice splitAt out of bounds")
        }
        unsafe {
            // 内部经受信任拆分构造两个互不重叠的子视图（略去底层裸指针构造细节）
            return intrinsic_split_mut_slice(this, mid)
        }
    }
}
```

### 1.2 应用层调用侧源码（原地快速排序）

```kotlin
// 应用层：零 unsafe、零生命周期参数
fun partition(inout slice: MutSlice<Int>): Int {
    val n = slice.len()
    val pivotIdx = n - 1
    val pivot = slice.get(pivotIdx)
    var store = 0
    var i = 0
    while (i < pivotIdx) {
        if (slice.get(i) <= pivot) {
            slice.swap(store, i)
            store = store + 1
        }
        i = i + 1
    }
    slice.swap(store, pivotIdx)
    return store
}

fun quickSort(own slice: MutSlice<Int>): Unit {
    if (slice.len() <= 1) {
        return
    }
    var working = slice
    val p = partition(&working)

    // 使用局部结构化解构一次性完整消费 MoveOnly 视图聚合（遵守 Guide 08 禁止普通字段部分移出）
    val (left, rightWithPivot) = working.splitAt(p)
    // 跳过下标 0 的枢轴元素，只对枢轴右侧 [1, len) 递归
    val (_, right) = rightWithPivot.splitAt(1)

    quickSort(left)
    quickSort(right)
}
```

**验证要点**：
- `quickSort` 的应用层不出现任何 `from` 或生命周期参数。
- `splitAt` 接收 `own this`（消费原 `MutSlice`），同时返还两个依赖原 `source` 的不相交 `MutSlice`，因此不存在“父视图与子视图同时可写”的漏洞，也无需编译器做下标算术证明。

## 2. 试金石二：二维矩阵行视图与用户自定义索引

对应[之二](evolution-02-route-loan-kernel.md) L4 验收项（用户自定义二维矩阵索引、矩阵行视图）与 R4.2–R4.4、R4.7（重借子 loan 结束后的来源恢复）。

```kotlin
// 候选源码（依赖 L4 inout T 与独占 view class）
class Matrix(
    val rows: Int,
    val cols: Int,
    private var data: MutableList<Double>
) {
    companion {
        fun zeros(rows: Int, cols: Int): Matrix {
            if (rows < 0 || cols < 0) {
                error("negative matrix dimensions")
            }
            var buf = MutableList<Double>()
            val total = rows * cols
            var i = 0
            while (i < total) {
                buf.add(0.0)
                i = i + 1
            }
            return Matrix(rows, cols, buf)
        }
    }

    fun get(r: Int, c: Int): Double {
        this.checkBounds(r, c)
        return this.data[r * this.cols + c]
    }

    // R4.2：交出指向矩阵第 r 行的独占行视图；RowMut 存活期间整个 Matrix 被独占借用
    inout fun rowMut(r: Int): RowMut from this {
        if (r < 0 || r >= this.rows) {
            error("matrix row out of bounds")
        }
        return RowMut(&this.data, r * this.cols, this.cols)
    }

    private fun checkBounds(r: Int, c: Int): Unit {
        if (r < 0 || r >= this.rows || c < 0 || c >= this.cols) {
            error("matrix index out of bounds")
        }
    }
}

view class RowMut(
    private inout val data: MutableList<Double>,
    private val offset: Int,
    val cols: Int
) {
    // R4.3 + R4.4：在 RowMut 的独占 loan 下建立子独占 loan，返回单元素 inout Double
    inout fun at(c: Int): inout Double from this {
        if (c < 0 || c >= this.cols) {
            error("row column out of bounds")
        }
        return this.data[this.offset + c]
    }
}

fun scaleElement(inout slot: Double, factor: Double): Unit {
    slot = slot * factor
}

fun scaleRow(inout mat: Matrix, rowIndex: Int, factor: Double): Unit {
    var row = mat.rowMut(rowIndex)
    var c = 0
    while (c < row.cols) {
        // R4.4 重借与 R4.7 恢复：elem 是 row.at(c) 产生的子独占 Holder；
        // scaleElement 返回且 elem 死亡后，row 恢复可写，下一轮循环可继续调用 row.at(c)
        inout val elem = row.at(c)
        scaleElement(&elem, factor)
        c = c + 1
    }
    // row 死亡后，mat 恢复可访问（L1 流敏感 + R4.7）
    if (mat.cols > 0) {
        val check = mat.get(rowIndex, 0)
        if (check != check) {
            error("unexpected NaN after scaling")
        }
    }
}
```

**验证要点**：
- 展示三层嵌套恢复：`mat` → `row`（`RowMut`）→ `elem`（`inout Double`），按后进先出顺序在最后一次使用后依次恢复父权限（R4.7）。
- `RowMut` 把 `data` 声明为 `private` 字段，满足 U0 TR4 与[之二](evolution-02-route-loan-kernel.md) §2 的“受信任/独占视图字段私有”前提。

## 3. 试金石三：链表可写游标（L4 形式化小演算必测难点）

对应[之二](evolution-02-route-loan-kernel.md) L4 验收项与[之五](evolution-05-data-structure-fitness.md) §4 G4：“可写游标要求独占 Holder 在循环中被重绑定为它当前所指对象的一部分（链表遍历中插入或删除），这是 Rust 借用检查中长期困难的用例，是否支持、支持到什么程度应由 L4 形式化小演算给出结论，而不是在实现中试出来”。

### 3.1 目标难点写法（独占游标原地推进）

```kotlin
class IntNode(
    var value: Int,
    var next: IntNode?
)

// 目标难点（待 L4 形式化小演算裁决是否接受；占位拼写 inout var）：
// 在循环内把独占借用游标 cur 重绑定为 cur 所指对象的字段 &cur.next
fun appendTailIterative(inout head: IntNode?, newValue: Int): Unit {
    inout var cur: IntNode? = &head
    while (true) {
        // 难点 1：对 cur 判空并投影到 cur.next 时，必须保持原 cur 的独占写回能力（当 cur == null 时写入）
        // 难点 2：重绑定 cur = &cur.next 时，新独占 loan 派生自旧 cur 的子字段，
        //        而循环回边要求新旧两轮的独占 loan 在形式化模型中满足保持性且不产生别名重叠。
        if (cur == null) {
            cur = IntNode(newValue, null)
            return
        }
        cur = &cur.next
    }
}
```

### 3.2 现行内核已可表达的对照写法（递归 `inout`）

```kotlin
// 对照写法：无需独占游标重绑定，只用普通递归 inout 重借（L4 之前即可表达；百万深度需改用句柄池）
fun appendTailRecursive(inout slot: IntNode?, newValue: Int): Unit {
    if (slot == null) {
        slot = IntNode(newValue, null)
        return
    }
    // 借用 slot 所指节点的 next 字段做独占重借，本帧返回后恢复
    appendTailRecursive(&slot.next, newValue)
}
```

**小演算裁决清单（L4 实施前必须回答）**：
1. L4 形式化小演算（Oxide 风格）是否支持 `inout var` 在循环内重绑定为自身所指对象的字段（`cur = &cur.next`），并在“条件提前返回 + 分支继续推进”的经典 Polonius 形状下保持健全？
2. 若小演算不支持条件早退的独占重借（或复杂度过高），L4 首版应明确拒绝迭代式可写游标，并把“递归 `inout` + 下标句柄池”作为规范推荐路径，待日后再议。

## 4. L4 六类必测反例矩阵

以下六类反例均须被 L4 借用检查器静态拒绝，且每条拒绝都能按[之二](evolution-02-route-loan-kernel.md) §2 用一句话解释“哪个 loan、被谁持有、与哪次访问冲突”：

| # | 反例类别 | 最小违规片段（候选语法） | 预期拒绝与依据 |
|---|---|---|---|
| N1 | 同一来源两个并存的独占结果 | `inout val a = row.at(0); inout val b = row.at(1); a = 1.0; b = 2.0` | **拒绝（L0135）**：`a` 持有 `row` 上的存活 Exclusive loan，`row.at(1)` 再次请求 `row` 的独占访问冲突（R4.2） |
| N2 | 独占结果存活时读来源 | `inout val a = row.at(0); val len = row.cols; a = 1.0`（或读 `mat.get(0, 0)`） | **拒绝（L0135）**：`a` 存活期间 `row` / `mat` 被独占冻结，禁止并发共享读（R4.2） |
| N3 | 普通非受信任函数转发独占视图字段 | `fun badForward(inout r: RowMut): inout Double from r.data = r.at(0)` | **拒绝（R4.5）**：越过独占字段的借用必然依赖视图本身，普通函数不得声明转发独占字段 `from r.data`（仅 `trusted` 可声明） |
| N4 | 两阶段借用不穿透访问器 | `list.at(i).push(list.size)` | **拒绝（L0135，R4.6）**：`list.at(i)` 已是 Active 独占访问，实参中读 `list.size` 与存活 Exclusive loan 冲突；须先 `val n = list.size` 再调用 |
| N5 | 独占视图或独占借用经重借逃逸 | `class BadBox(val r: RowMut)` 或在 `move { ... }` 闭包中捕获 `row` | **拒绝（L0163 / L0137）**：独占视图与 `inout T` 恒不可逃逸，不得存入普通类字段或被逃逸闭包捕获（R4.1） |
| N6 | 独占 Holder 被隐式复制 | `inout val a = row.at(0); inout val b = a; a = 1.0; b = 2.0` | **拒绝（MoveOnly / L0131）**：`inout T` 与独占 `view class` 恒为 `MoveOnly`，不得复制出两个并存独占持有者（R4.1、R4.5） |
