# 路线 S 挂起与生成器试金石（S3 生成器与 S4 异步作用域取消）

> **性质**：非规范候选试金石与反例矩阵（仅源码评审，不求当前编译器可编译） · **状态**：未启用（不修改 Guide v0.47，不批准 Spec/ADR，不授权实现） · **读取时机**：评审路线 S 的 S3（生成器 `yield`）、S4（`suspend` 函数、单线程任务作用域、值式取消与完成式在途 IO）的源码体验与安全边界时 · **唯一真源**：本页维护演进总纲之七 §3 与之十三 §4.6 要求的两组出口试金石与五类反例矩阵；并发与异步决断见[演进总纲之七](evolution-07-concurrency-async-io-ffi.md) §2（J14–J21、J34–J36、J39、J49–J50），取消仲裁策略见[裁决纪要](kernel-open-decisions-2026-11.md)，在途 IO 交接见[IO 资源借用、责任交接与清理](io-resource-handoff-cleanup.md)，现行语法基线以 [Guide v0.47](../guide/README.md) 为准

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权实现。源码以 [Guide v0.47](../guide/README.md) 为表面语法基线（`.size` 属性、`Ok.success`、无 `when (val ...)`、无末尾逗号、`inout` 仅写声明侧、调用点写 `&`），仅在 S3/S4 候选处使用[之七](evolution-07-concurrency-async-io-ffi.md)的占位语法（`suspend fun`、`yield`、`IndirectFrame` 显式间接帧、`coroutineScope` / `supervisorScope`）。

## 0. 试金石与验收映射

| 试金石 / 矩阵 | 对应之七步骤与决断 | 核心检验目标 |
|---|---|---|
| 1. 二叉树中序遍历生成器 | S3（KB8、J20、§2.2） | `yield` 产出借用元素、跨 `yield` 保持只读借用、递归挂起显式间接帧 |
| 2. 单线程异步回显服务器与作用域取消 | S4（J16–J19、J39、§2.7、OD2 · B1） | Kotlin 式无 `await` 顺序写法、默认单线程免 `Arc`/`Mutex`、值式取消与操作持有在途 IO 缓冲 |
| 3. 五类 S3/S4 反例矩阵 | S0.1–S0.3、J18、J19、§2.7 | `deinit` 挂起、并行入口跨挂起点持 `Rc`、提前释放未终结 IO 缓冲、隐式递归帧、帧销毁悬垂借用 |

## 1. 试金石一：二叉树中序遍历生成器（验证 S3 / KB8）

对应[之七](evolution-07-concurrency-async-io-ffi.md) §3 S3 与[之十三](evolution-13-implementation-roadmap.md) §4.6：“树的中序遍历写成生成器；跨 `yield` 持有借用”。同时体现 §2.2 的硬规则：**顺序调用的挂起帧默认嵌入调用者帧（零堆分配）；递归调用因帧大小无法静态确定，首版须显式写出间接帧（占位 `IndirectFrame`），不隐式装箱**。

```kotlin
// 候选源码（依赖 L3 借用视图、S3 生成器 yield 与显式间接帧）
class TreeNode(
    val value: Int,
    val left: TreeNode?,
    val right: TreeNode?
)

// 生成器函数：产出的元素为只读借用 borrow Int，来源绑定到输入树 node；
// 每次 yield 暂停当前帧，恢复时局部借用仍有效且指向稳定存储。
generator fun inOrder(node: TreeNode?): borrow Int from node {
    if (node == null) {
        return
    }
    // 递归生成器无法静态内嵌帧大小，按之七 §2.2 显式使用间接帧逐项转发
    for (v in IndirectFrame.run { inOrder(node.left) }) {
        yield(v)
    }
    // 跨 yield 持有来自 node 的只读借用；挂起期间调用方不得修改或移动整棵树
    yield(node.value)
    for (v in IndirectFrame.run { inOrder(node.right) }) {
        yield(v)
    }
}

fun sumTreeInOrder(tree: TreeNode): Int {
    var total = 0
    for (v in inOrder(tree)) {
        total = total + v
    }
    return total
}
```

**验证要点**：
- **不用不付（J36）**：非递归生成器把被调帧直接嵌入调用者帧，不触发堆分配；只有递归 `inOrder` 在显式 `IndirectFrame` 处分配。
- **跨 `yield` 状态守恒（S0.3）**：`yield` 暂停期间，源树 `tree` 上的共享 loan 持续存活，直至生成器自然结束或被提前销毁；若调用方在消费生成器中途试图修改 `tree`，报借用冲突（L0135）。
- **提前 `break` 清理**：若调用方在 `for (v in inOrder(tree))` 中途 `break`，生成器帧按当前挂起状态执行逆序析构，释放内层间接帧并结束所有 loan。

## 2. 试金石二：单线程异步回显服务器与作用域取消（验证 S4 / J16–J19 / §2.7）

对应[之七](evolution-07-concurrency-async-io-ffi.md) §3 S4 与[之十三](evolution-13-implementation-roadmap.md) §4.6：“单线程回显服务器承载大量空闲连接；取消一个作用域后全部子任务以取消错误返回且资源全部释放；覆盖‘操作已提交、取消未完成、执行器失败’的在途 IO 交接”。

> 注：3A 取消接口的具体表面形态（`E` 显式承载 `Cancelled` 还是独立 `ScopedResult<T, E>`）按[裁决纪要](kernel-open-decisions-2026-11.md) §3 仍暂缓裁决；本试金石暂用 `AsyncIoError.Cancelled` 展示值式取消（J19）与 `?` 传播的调用形态，不代表已提前锁定方案 C-I。

```kotlin
// 候选源码（依赖 S4 suspend fun、单线程任务作用域、值式取消与 IO4 操作持有缓冲+句柄）
enum class AsyncIoError {
    Cancelled,
    Closed,
    IoFailed
}

class SingleThreadStats(
    var activeConnections: Int,
    var echoedLines: Int
)

// RefCell.borrowMut 的参数模式 inout 由期望函数类型声明；调用处写 { s -> ... }（遵守 Guide 07）
// 签名示意：fun <T> RefCell<T>.borrowMut(body: (inout T) -> Unit): Unit

// 完成式在途 IO 交接包装（落实之七 §2.7）：
// 提交读/写时同时把 stream 句柄与 IoBuf 的所有权一并移交底层完成式操作；
// 仅当收到 OS 终结证据（完成通知）后才经 Ok(ReadTurn / WriteTurn) 交还二者。
// 若在取消或执行器失败时尚未收到 OS 终结证据，反应器继续独占持有该 stream 与 IoBuf，
// 直到内核完成通知到达后才关闭描述符并释放缓冲；绝不在在途期由任务帧 deinit 关闭句柄或释放缓冲。
value class ReadTurn(
    val stream: AsyncTcpStream,
    val buf: IoBuf,
    val lineLen: Int
)

value class WriteTurn(
    val stream: AsyncTcpStream,
    val buf: IoBuf
)

// 单连接处理：Kotlin 式 suspend fun，调用点无 await；
// 按 J18 默认留在所属单线程执行器上，因此 Rc<RefCell<SingleThreadStats>>
// 可直接跨挂起点使用，无需 Arc/Mutex 或原子开销。
suspend fun handleAsyncConnection(
    own initialStream: AsyncTcpStream,
    stats: Rc<RefCell<SingleThreadStats>>
): Result<Unit, AsyncIoError> {
    stats.borrowMut { s ->
        s.activeConnections = s.activeConnections + 1
    }

    var stream = initialStream
    var buf = IoBuf.alloc(4096)
    while (true) {
        // 同时移交 stream 与 buf；若取消或失败且尚无终结证据，反应器保留二者直至内核终结，不触发提前关闭
        val readRes = readLineWithOwnership(stream, buf)
        val turn = when (readRes) {
            is Ok -> readRes.success
            is Err -> {
                stats.borrowMut { s ->
                    s.activeConnections = s.activeConnections - 1
                }
                return Err(readRes.error) // 含 AsyncIoError.Cancelled，经普通值路径退出
            }
        }
        val (stream = returnedStream, buf = returnedBuf, lineLen = lineLen) = turn
        stream = returnedStream
        buf = returnedBuf
        if (lineLen == 0) {
            break
        }

        val writeRes = writeFromWithOwnership(stream, buf, lineLen)
        val writeTurn = when (writeRes) {
            is Ok -> writeRes.success
            is Err -> {
                stats.borrowMut { s ->
                    s.activeConnections = s.activeConnections - 1
                }
                return Err(writeRes.error)
            }
        }
        val (stream = writtenStream, buf = writtenBuf) = writeTurn
        stream = writtenStream
        buf = writtenBuf

        stats.borrowMut { s ->
            s.echoedLines = s.echoedLines + 1
        }
    }

    stats.borrowMut { s ->
        s.activeConnections = s.activeConnections - 1
    }
    // 正常退出路径上已无任何在途 IO，stream 离开作用域时安全关闭描述符并释放 buf
    return Ok(Unit)
}

// 主服务循环：supervisorScope 隔离单连接失败；收到停机信号后取消整个作用域并等齐全部子任务
suspend fun runSingleThreadEchoServer(
    listener: AsyncTcpListener,
    shutdown: AsyncReceiver<Unit>
): Result<Unit, AsyncIoError> {
    val stats = Rc.new(RefCell.new(SingleThreadStats(0, 0)))

    return supervisorScope { scope ->
        // 监听停机信号：收到后请求取消整个作用域（J19 / J39）
        scope.launch {
            val sig = shutdown.receive()
            when (sig) {
                is Ok -> scope.cancel()
                is Err -> scope.cancel()
            }
            Ok(Unit)
        }

        // 接受循环：被 scope.cancel() 唤醒后返回 Err(AsyncIoError.Cancelled)，经 ? 退出循环体
        while (true) {
            val stream = listener.accept()?
            val statsHandle = stats.share()
            // 使用 move 显式把本轮循环的 stream 与 statsHandle 移入子任务（任务活过本轮循环）；
            // supervisorScope 隔离各子任务失败，因此在子任务内部显式记录并处理 IoFailed，不静默丢弃失败；
            // 作用域返回前仍须等齐全部子任务实际终止（且无在途句柄/缓冲遗留给任务帧）后才释放父帧借用。
            scope.launch(move {
                val res = handleAsyncConnection(stream, statsHandle)
                when (res) {
                    is Ok -> Ok(Unit)
                    is Err -> when (res.error) {
                        is AsyncIoError.Cancelled -> Err(AsyncIoError.Cancelled)
                        is AsyncIoError.Closed -> Ok(Unit)
                        is AsyncIoError.IoFailed -> {
                            println("connection failed with IoFailed")
                            Ok(Unit)
                        }
                    }
                }
            })
        }
        Ok(Unit)
    }
}
```

**验证要点**：
- **低心智与无 `await` 顺序读感（J16 / J50）**：应用层只用到 `suspend fun`、`supervisorScope`、`scope.launch`、`scope.cancel()` 与普通 `Result`/`?`，不出现 `Pin`、手动状态机、`GlobalScope` 或异常式取消；回调参数模式由 `borrowMut` 期望函数类型提供，调用处直接写 `{ s -> ... }`。
- **默认单线程零锁开销（J18 / J36）**：`stats` 使用非原子 `Rc<RefCell<...>>`，编译期不要求 `Transferable`/`Shareable`；只有切到 S5 并行执行器入口时才触发跨线程能力检查。
- **在途 IO 安全性（§2.7，缓冲区与句柄双交接）**：`readLineWithOwnership(stream, buf)` / `writeFromWithOwnership(stream, buf, lineLen)` 在提交时把 **`AsyncTcpStream` 句柄与 `IoBuf` 缓冲区一并移交**给底层完成式操作，仅在内核终结证据到达后才返还。若取消先行返回或执行器异常终止时仍缺终结证据，反应器保留句柄与缓冲直至终结再关闭/释放（证据可丢失时按 §2.7 失败政策保留不释放），任务帧 `deinit` 绝不会提前关闭仍在途的描述符。
- **OD2 · B1 与失败不静默丢失**：子任务经 `move` 显式接管循环局部 `stream`/`statsHandle`，在 `supervisorScope` 隔离策略下由子任务显式分类记录 `IoFailed` 与 `Cancelled`；`scope.cancel()` 唤醒受支持挂起点，作用域等到所有子任务实际终止后才释放父帧借用。

## 3. S3 / S4 五类必测反例矩阵

| # | 反例类别 | 违规写法或时序 | 预期拒绝或安全行为（依据） |
|---|---|---|---|
| M1 | `deinit` 内调用 `suspend` 函数 | 在 `class Conn { deinit() { asyncFlush() } }` 中挂起 | **编译期拒绝（J19 / 演算 §62）**：`deinit` 永不挂起；异步清理须通过显式 `suspend own fun close()` 完成 |
| M2 | 并行执行器跨挂起点持非 `Transferable` | 在 S5 `parallelScope` 中把局部 `Rc` 或 `MutexGuard` 活过 `suspendCall()` | **编译期拒绝（J18 / 演算 §17、§76）**：并行入口对每个跨挂起点存活的值检查 `Transferable`，诊断精确指向该挂起调用点 |
| M3 | 取消未终结时释放帧内 IO 缓冲 | 提交指向帧内局部数组的完成式读请求，收到取消后不等内核完成事件即返回并析构帧 | **模型与运行时均禁止（§2.7 / 失败模型 B–C）**：必须等待终结证据或由操作持有缓冲；证明不足时保留资源不释放 |
| M4 | 递归 `suspend` / `generator` 未显式间接帧 | `suspend fun rec(n: Int)` 直接自调用而不用 `IndirectFrame` | **编译期拒绝（§2.2）**：帧大小无法静态确定，首版须显式写出间接帧分配（不隐式装箱） |
| M5 | 生成器帧提前销毁后仍持有指向其内部存储的借用 | 出借式生成器交出指向帧内缓冲的借用后，生成器变量在借用最后使用前死亡 | **编译期拒绝（演算 §79 K1/K4/K22）**：借用存活期间生成器帧不得被销毁或移动 |
