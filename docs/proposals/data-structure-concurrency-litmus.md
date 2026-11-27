# 数据结构与并发试金石程序集（演进总纲之五 §5）

> **性质**：非规范候选试金石（仅源码验证，不求当前编译器可编译） · **状态**：未启用（不修改 Guide v0.47，不批准 Spec/ADR，不授权实现） · **读取时机**：评审[演进总纲之五](evolution-05-data-structure-fitness.md) §4–§5 决定的缺口（G3、G4、G5）、条件变量作用域档（C4）或 `AtomicLong`（C2/C3/U1 合验）的源码形态与边界时 · **唯一真源**：本页维护之五 §5 增补的五组试金石源码与验证要点；缺口裁决见[之五](evolution-05-data-structure-fitness.md) §4，路线依赖见[之二](evolution-02-route-loan-kernel.md)与[之四](evolution-04-route-capability-unsafe-suspend.md)，`unsafe` / 原子合同见 [U0 合同](unsafe-aliasing-contract-outline.md)，现行语法基线以 [Guide v0.47](../guide/README.md) 为准

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权实现。五组试金石均以 [Guide v0.47](../guide/README.md) 为表面语法基线（无 `when (val ...)`、枚举/参数列表无末尾逗号、`companion` 泛型函数自带类型参数、`inout` 仅写声明侧、调用点写 `&`、`borrow` 调用点不写标注），仅在各自目标步骤处使用对应提案的候选拼写。

## 0. 五组试金石总览

| # | 试金石 | 对应缺口 / 路线 | 核心检验目标 |
|---|---|---|---|
| 1 | 百万节点单链表的构造与析构 | G3（`deinit` 中 `this` 改为 `inout`） | 百万节点析构为 O(1) 栈深，不递归爆栈、不产生部分移出 |
| 2 | 迭代式链表遍历 | G4 / L2 R2.6（可重绑定共享借用局部） | `cur = cur.next` 循环推进，退出后原链表自动恢复可写 |
| 3 | `MoveOnly` 元素的原地堆排序 | G5（`MutableList.swap(i, j)`） | 不可复制元素原地排序，不依赖下标不相交静态证明、不 `clone()` |
| 4 | 有界阻塞队列 | C4（条件变量作用域档） | `withLockWhen` 等待期间不交出借用，不依赖 L4 可移动 guard |
| 5 | `AtomicLong` 多线程计数 | C2 + C3 + U1 合验 | `RawCell` 内部可变 + 原子 intrinsic + 受信任 `Shareable`，满足 U0 合同 |

## 1. 试金石一：百万节点单链表的构造与析构（验证 G3）

对应[之五](evolution-05-data-structure-fitness.md) §4 G3（2026-11-10 裁决采用方案 a：`deinit` 中 `this` 改为独占借用 `inout`，允许用 `replace(&this.field, ...)` 摘链，body 结束时全部字段仍保持初始化）。

```kotlin
// 候选源码（依赖 G3：deinit 中 this 为 inout，允许 replace 字段）
class ListNode(
    var value: Int,
    var next: ListNode?
)

class SinglyLinkedList(var head: ListNode?) {
    companion {
        fun empty(): SinglyLinkedList = SinglyLinkedList(null)
    }

    inout fun pushFront(value: Int): Unit {
        val oldHead = replace(&this.head, null)
        this.head = ListNode(value, oldHead)
    }

    // G3 检验点：迭代式摘链析构，栈深恒为 O(1)，避免百万节点递归析构爆栈
    deinit() {
        var cur: ListNode? = replace(&this.head, null)
        while (true) {
            var nodeOpt = replace(&cur, null)
            if (nodeOpt == null) {
                break
            }
            // 先把后继摘给 cur，再让本轮结束时析构单个孤立节点（next == null）
            cur = replace(&nodeOpt.next, null)
        }
        // body 结束时 this.head == null（仍完整初始化），编译器后续字段析构为空操作
    }
}

fun buildAndDropMillion(): Int {
    var list = SinglyLinkedList.empty()
    var i = 0
    while (i < 1000000) {
        list.pushFront(i)
        i = i + 1
    }
    return 1000000
    // 离开作用域触发 SinglyLinkedList.deinit()：迭代释放 100 万节点，不发生栈溢出
}
```

**验证要点与边界**：
- **不引入部分移出**：整个 `deinit()` 只用现行 `replace(&place, null)` 原语，体末 `this.head` 仍处于合法初始化状态，符合 G3 最小改动原则。
- **不违反 Guide 08 约束**：`this` 未交出给外部存储，`deinit()` 内未对 `this` 再次触发析构。
- **若保留 v1 只读 `this`**：`replace(&this.head, null)` 会因 `this` 非独占而被拒绝，只能退回编译器生成的递归字段析构，在 100 万节点下必然栈溢出。

## 2. 试金石二：迭代式链表遍历（验证 G4 / L2 R2.6）

对应[之五](evolution-05-data-structure-fitness.md) §4 G4 与[之二](evolution-02-route-loan-kernel.md) L2 R2.6（共享借用局部允许重绑定，占位拼写 `borrow var`，正式拼写留给 L2 提案）。

```kotlin
// 候选源码（依赖 L2 R2.6：可重绑定共享借用局部，占位拼写 borrow var）
fun sumList(list: SinglyLinkedList): Long {
    var total: Long = 0
    borrow var cur: ListNode? = list.head
    while (true) {
        val node = cur
        if (node == null) {
            break
        }
        total = total + node.value.toLong()
        // G4 检验点：重绑定使旧 Holder 死亡、新 Holder 产生；循环回边上 Origins 取并集（均来自 list）
        cur = node.next
    }
    return total
}

fun appendAfterTraversal(): Long {
    var list = SinglyLinkedList.empty()
    list.pushFront(10)
    list.pushFront(20)
    list.pushFront(30)

    val before = sumList(list)
    // L1 流敏感 + G4：cur 在 sumList 返回时已死亡，list 恢复可写
    list.pushFront(40)
    val after = sumList(list)
    return before + after
}
```

**验证要点与边界**：
- **循环回边健全性**：`cur = node.next` 每次重绑定产生的新借用仍来自 `list`，因此整个循环期间 `list` 保持共享冻结（若循环体内试图调用 `list.pushFront(...)` 报 L0135）。
- **只覆盖只读游标**：本试金石验证的是共享借用重绑定（L2）；遍历中途原地插入/删除的**可写游标**属于独占 Holder 重绑定，归 L4 形式化小演算验收。

## 3. 试金石三：`MoveOnly` 元素的原地堆排序（验证 G5）

对应[之五](evolution-05-data-structure-fitness.md) §4 G5（2026-11-10 裁决：为 `Array` 与 `MutableList` 补 `swap(i, j)`，下标相等为空操作、越界 abort，容器迁入库 U2 后退为纯库方法）。

```kotlin
// 候选源码（依赖 G5：MutableList.swap(i, j)）
// 普通 class + String 字段：天然 MoveOnly，禁止隐式复制
class JobEntry(
    val priority: Int,
    val label: String
)

fun siftDown(
    inout heap: MutableList<JobEntry>,
    start: Int,
    end: Int
): Unit {
    var root = start
    while (true) {
        val left = root * 2 + 1
        if (left > end) {
            break
        }
        var largest = root
        if (heap[largest].priority < heap[left].priority) {
            largest = left
        }
        val right = left + 1
        if (right <= end && heap[largest].priority < heap[right].priority) {
            largest = right
        }
        if (largest == root) {
            break
        }
        // G5 检验点：原地对换两个 MoveOnly 元素，无需静态证明 root != largest，无需 clone()
        heap.swap(root, largest)
        root = largest
    }
}

fun heapSortByPriority(inout items: MutableList<JobEntry>): Unit {
    val n = items.size()
    if (n <= 1) {
        return
    }

    // 1. 原地建最大堆
    var start = (n - 2) / 2
    while (start >= 0) {
        siftDown(&items, start, n - 1)
        start = start - 1
    }

    // 2. 逐个把堆顶换到末尾并收缩堆区
    var end = n - 1
    while (end > 0) {
        items.swap(0, end)
        end = end - 1
        siftDown(&items, 0, end)
    }
}

fun runMoveOnlyHeapSort(): Int {
    var jobs = MutableList<JobEntry>()
    jobs.add(JobEntry(3, "parse"))
    jobs.add(JobEntry(1, "lex"))
    jobs.add(JobEntry(4, "codegen"))
    jobs.add(JobEntry(2, "typecheck"))

    heapSortByPriority(&jobs)
    return jobs[0].priority * 10 + jobs[3].priority // 预期 14（升序：1, 2, 3, 4）
}
```

**验证要点与边界**：
- **为何不用内建 `swap(&items[0], &items[end])`**：运行时下标无法在借用内核中静态证明不重叠，同时对同一容器取两个 `inout` 元素 place 会触发重叠独占冲突（L0135）；`items.swap(i, j)` 把不相交检查收拢到容器内部（`i == j` 时为空操作），内核无需引入整数下标推理。
- **零额外拷贝**：整个排序过程只借用 `heap[i].priority` 做比较，只用 `swap` 交换槽位，不要求 `JobEntry` 实现 `Copyable` 或 `clone()`。

## 4. 试金石四：有界阻塞队列（验证 C4 条件变量作用域档）

对应[之五](evolution-05-data-structure-fitness.md) §3（条件变量作用域档：`withLockWhen` 把等待与条件谓词打包，等待期间不向用户交出任何借用，因此不依赖 L4 的可移动独占 guard；同时响应 FD0 / Z1 的锁中毒返回位）。

```kotlin
// 候选源码（依赖 C3/C4：Arc、Mutex、条件变量作用域档 withLockWhen 与 LockError）
enum class QueueError {
    Closed,
    Poisoned
}

class QueueState<T>(
    var items: MutableList<T>,
    val capacity: Int,
    var closed: Boolean
)

class BoundedQueue<T>(
    private val inner: Arc<Mutex<QueueState<T>>>
) {
    companion {
        // Guide 08：companion 不捕获外层类型参数，泛型工厂函数自行声明 <T>
        fun <T> withCapacity(capacity: Int): BoundedQueue<T> {
            if (capacity <= 0) {
                error("BoundedQueue capacity must be positive")
            }
            val initState = QueueState<T>(MutableList(), capacity, false)
            return BoundedQueue(Arc.new(Mutex.new(initState)))
        }
    }

    fun share(): BoundedQueue<T> = BoundedQueue(this.inner.share())

    // 阻塞入队：等待“未满或已关闭”；等待期间不交出 state 借用，唤醒并重新持锁后才进入 body
    fun put(own item: T): Result<Unit, QueueError> {
        val outcome = this.inner.withLockWhen(
            { state -> state.items.size() < state.capacity || state.closed },
            { inout state ->
                if (state.closed) {
                    Err(QueueError.Closed)
                } else {
                    state.items.add(item)
                    // 通知等待消费的线程（作用域档在释放锁前/后唤醒等待者）
                    state.notifyOne()
                    Ok(Unit)
                }
            }
        )
        return when (outcome) {
            is Ok -> outcome.value
            is Err -> Err(QueueError.Poisoned)
        }
    }

    // 阻塞出队：等待“非空或已关闭”；队列关闭且排空后返回 Ok(null)
    fun take(): Result<T?, QueueError> {
        val outcome = this.inner.withLockWhen(
            { state -> state.items.size() > 0 || state.closed },
            { inout state ->
                if (state.items.size() == 0) {
                    Ok(null)
                } else {
                    val first = state.items.removeAt(0)
                    state.notifyOne()
                    Ok(first)
                }
            }
        )
        return when (outcome) {
            is Ok -> outcome.value
            is Err -> Err(QueueError.Poisoned)
        }
    }

    fun close(): Result<Unit, QueueError> {
        val outcome = this.inner.withLock { inout state ->
            state.closed = true
            state.notifyAll()
        }
        return when (outcome) {
            is Ok -> Ok(Unit)
            is Err -> Err(QueueError.Poisoned)
        }
    }
}

fun runBoundedQueueDemo(): Int {
    val queue = BoundedQueue.withCapacity<Int>(4)
    val producerQueue = queue.share()

    val producer = thread(move {
        var i = 1
        while (i <= 10) {
            val res = producerQueue.put(i)
            when (res) {
                is Ok -> { }
                is Err -> break
            }
            i = i + 1
        }
        producerQueue.close()
    })

    var sum = 0
    while (true) {
        val taken = queue.take()
        val maybeVal = when (taken) {
            is Ok -> taken.value
            is Err -> break
        }
        if (maybeVal == null) {
            break
        }
        sum = sum + maybeVal
    }
    producer.join()
    return sum // 预期 55
}
```

**验证要点与边界**：
- **为何不依赖 L4**：等待与重加锁全部封闭在 `withLockWhen` 内部，用户代码仅在持锁的第二个 lambda 内获得 `inout state`，不会在等待点跨越任何存活独占借用（满足 KB2 与[之五](evolution-05-data-structure-fitness.md) §3）。
- **语法注意**：`withLockWhen` 接受两个位置 lambda 实参（谓词 + 持锁体），避免在命名实参后接无名尾 lambda，lambda 内以 `if/else` 表达式收尾，不依赖尚未启用的 `return@label` 标签返回语法。
- **与 L4 通用档的分界**：仅当调用方需要在一次持锁期间、按运行时动态条件多次交替等待不同条件变量时，才需要 L4 的可移动 `MutexGuard` + `Condvar.wait(own guard): MutexGuard`。

## 5. 试金石五：`AtomicLong` 多线程计数（验证 C2 / C3 / U1 合验）

对应[之五](evolution-05-data-structure-fitness.md) §3 与 §5，合验 KB6 内部可变标记（C2 `RawCell`）、U1 原子 intrinsic（[U0 合同](unsafe-aliasing-contract-outline.md) MM3）与 C3 受信任 `Shareable` 声明（U0 AL5 / TR2 / TR4）。

### 5.1 库作者侧源码（特权标准库层）

```kotlin
// 候选源码（依赖 C2 RawCell、C3 受信任 Shareable、U1 原子 intrinsic 与 U0 合同）
enum class MemoryOrder {
    Relaxed,
    Acquire,
    Release,
    SeqCst
}

// TR4 审查范围：AtomicLong 的全部成员均可访问 private cell，因此全类型成员均纳入人工审查
// TR2 / AL5：手工断言 Shareable，须证明全部公开 API 组合下无数据竞争（MM1）
trusted Shareable class AtomicLong(
    private val cell: RawCell<Long>
) {
    companion {
        fun new(initial: Long): AtomicLong = AtomicLong(RawCell.new(initial))
    }

    // AL7 / AL8：接收者为 borrow this，但因递归含 RawCell，后端严禁发射 readonly；
    // 方法只做整值原子操作，严禁交出指向内部存储的安全 borrow/inout 引用
    fun load(order: MemoryOrder): Long {
        unsafe {
            return intrinsic_atomic_load_i64(this.cell.rawPtr(), order)
        }
    }

    fun store(value: Long, order: MemoryOrder): Unit {
        unsafe {
            intrinsic_atomic_store_i64(this.cell.rawPtr(), value, order)
        }
    }

    fun fetchAdd(delta: Long, order: MemoryOrder): Long {
        unsafe {
            return intrinsic_atomic_fetch_add_i64(this.cell.rawPtr(), delta, order)
        }
    }

    // 默认最强次序（MM3）：不带次序参数的便捷方法一律走 SeqCst
    fun incrementAndGet(): Long = this.fetchAdd(1, MemoryOrder.SeqCst) + 1
}
```

### 5.2 应用层调用侧源码

```kotlin
// 应用层：零 unsafe、零裸指针、零锁竞争
fun runParallelCounter(workerCount: Int, incrementsPerWorker: Long): Long {
    val counter = Arc.new(AtomicLong.new(0))
    var handles = MutableList<ThreadHandle<Unit>>()

    var w = 0
    while (w < workerCount) {
        val shared = counter.share()
        val handle = thread(move {
            var i: Long = 0
            while (i < incrementsPerWorker) {
                // 纯计数递增无需与其它共享内存建立同步，使用显式 Relaxed（MM3 / MM4）
                shared.fetchAdd(1, MemoryOrder.Relaxed)
                i = i + 1
            }
        })
        handles.add(handle)
        w = w + 1
    }

    // MM2 先后次序：每个 join() 返回后，对应工作线程的全部原子写入对主线程可见
    while (handles.size() > 0) {
        val h = handles.removeAt(handles.size() - 1)
        h.join()
    }

    return counter.load(MemoryOrder.SeqCst) // 预期 workerCount * incrementsPerWorker
}
```

**验证要点与 U0 合同逐条核对**：
- **AL7（`RawCell` 唯一例外）**：`AtomicLong` 通过 `borrow this` 并发修改底层 `Long`，合法性完全建立在 `RawCell<Long>` 之上，且不向外部交出任何内部引用。
- **AL8（后端属性）**：`AtomicLong` 递归含 `RawCell`，后端不得对其 `borrow` 参数发射 `readonly` 或 `noalias`。
- **MM1–MM4（内存模型）**：全部跨线程读写走原子操作，无“良性竞争”；工作线程用 `Relaxed` 累加，主线程经 `join()`（MM2 happens-before）汇合后读取精确总和，TSan 下零数据竞争。
- **TR2 / TR4（审查边界）**：`cell` 必须 `private`；若未来给 `AtomicLong` 新增任何普通非原子读取方法，因全类型成员同属审查范围，评审可直接拦截。
