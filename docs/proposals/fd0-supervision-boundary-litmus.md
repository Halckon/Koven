# FD0：显式监督边界预留与“每连接一线程”服务器试金石

> **性质**：非规范候选设计（FD0 预留核对与试金石） · **状态**：未启用（不修改 Guide v0.47，不提前启用栈展开或监督构造） · **读取时机**：核对[演进总纲之十一](evolution-11-failure-interfaces-units.md) §1.4 FD0 的四项预留、评审行为草案第 3–4 条，或设计 FD3 监督设施与 C4 锁/channel 异常终止接口时 · **唯一真源**：本页维护 FD0 的试金石源码、行为草案第 3–4 条收口建议与四项预留核对表；三级失败与 J48 决断见[演进总纲之十一](evolution-11-failure-interfaces-units.md) §1.4，失败安全条款见 [U0 合同](unsafe-aliasing-contract-outline.md) §4，在途 IO 与终止窗口模型见[失败与清理模型](failure-cleanup-model.md) §5–§6

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权实现。现行 v1 仍以进程为失败单位，任何线程 abort 均直接终止整个进程且不执行待运行析构（[Guide 09](../guide/09-nullability-errors.md)）。

## 1. FD0 的目标与范围

按[演进总纲之十一](evolution-11-failure-interfaces-units.md) §1.4 与[之十三](evolution-13-implementation-roadmap.md) §4.2（文档先行）：

1. **手写“每连接一线程”回显服务器试金石**，验证动态数量监督句柄的创建、非阻塞回收已结束工作单元、失败隔离与优雅停机是否写得自然；
2. **用试金石反推并收口 J48 行为草案第 3、4 条**（监督句柄的结果接口、未取结果句柄析构时的等待/回收语义，以及与普通 `thread(move { ... })` 的边界）；
3. **逐项核对 J48 的 4 项现行预留**，确认 v1 及其后继演进不会把 J48 的可行路径提前封死。

## 2. 试金石：每连接一线程的监督回显服务器

本试金石以 [Guide v0.47](../guide/README.md) 为语法基线，仅在监督边界与阻塞式网络/锁接口处使用候选占位名（`SupervisorPool`、`SupervisedHandle`、`Failure`、`LockError`、`TcpListener`、`TcpStream`）。关键目标：**某个连接在处理过程中触发程序错误（如越界、`!!`、`error()`）时，只展开并清理该连接对应工作单元的栈与资源，其余连接与主接受循环继续运行**。

### 2.1 候选监督原语签名（FD3 目标表面）

```kotlin
// 候选签名（占位拼写，仅用于验证调用形态；不启用）
enum class FailureKind {
    Abort,
    OutOfBounds,
    ArithmeticOverflow,
    NullAssertion
}

class Failure(
    val kind: FailureKind,
    val message: String
)

enum class LockError {
    Poisoned
}

// 单个监督句柄：跨线程/监督边界的所有捕获与返回值 T 均须满足 Transferable（防止 Rc 等线程局部值跨边界交付）
class SupervisedHandle<T : Transferable> {
    // 阻塞等待该执行单元结束并返回 Result；不自动传播失败
    own fun joinCatching(): Result<T, Failure> { ... }

    // 非阻塞查询该执行单元是否已结束（用于主循环顺带回收已完成句柄）
    fun isFinished(): Boolean { ... }

    // 未调用 joinCatching() 即离开作用域时：按草案第 4 条等待单元结束，不传播失败
    deinit() { ... }
}

// 监督池：管理运行时数量不定的监督句柄，解决“只有阻塞 join 无法在 accept 循环中回收”的问题
class SupervisorPool<T : Transferable> {
    companion {
        // Guide 08：companion 不捕获外层类型参数，泛型关联函数须自行声明 <T : Transferable>
        fun <T : Transferable> empty(): SupervisorPool<T> { ... }
    }

    // 启动一个隔离的监督工作单元（捕获环境与返回值 T 均须满足 Transferable）；
    // 内部普通失败展开至边界并被记录为 Failure
    inout fun spawn(own task: move () -> T): Unit { ... }

    // 非阻塞回收当前已结束的工作单元结果，供日志/指标统计，避免句柄无限增长
    inout fun drainCompleted(): MutableList<Result<T, Failure>> { ... }

    // 停机时等待全部剩余工作单元结束并收集结果
    own fun joinAll(): MutableList<Result<T, Failure>> { ... }
}
```

### 2.2 服务器主循环与连接处理源码

```kotlin
// 候选试金石：每连接一线程回显服务器
class ServerStats(
    var okConnections: Int,
    var failedConnections: Int
)

enum class ReadLineError {
    Timeout,
    Closed,
    IoFailed
}

fun handleConnection(
    own stream: TcpStream,
    statsLock: Arc<Mutex<ServerStats>>,
    shutdownFlag: Arc<AtomicBoolean>,
    connId: Int
): Unit {
    // 连接内若因 bug 触发 error() / 越界：
    // 1. 先向 stderr 写出 abort 报告；
    // 2. 在本监督边界内自内向外执行待运行析构（stream.deinit 关闭套接字，锁守卫释放并标中毒）；
    // 3. 监督边界接住失败并打包为 Err(Failure)，主线程与其他连接不受影响。
    while (!shutdownFlag.load(MemoryOrder.Acquire)) {
        // 使用带超时的读操作，确保空闲连接在收到停机信号后按有界延迟退出，不阻塞 joinAll()
        val lineResult = stream.readLineWithTimeout(4096, 100)
        val line = when (lineResult) {
            is Ok -> lineResult.success
            is Err -> when (lineResult.error) {
                is ReadLineError.Timeout -> continue
                else -> return
            }
        }

        // 模拟单连接程序错误：只终止本连接单元，不拖垮整个进程
        if (line == "__trigger_bug__") {
            error("simulated per-connection invariant failure")
        }

        // 写路径同样设置总截止超时，避免对端停止读取导致写阻塞而无法观察 shutdownFlag
        val writeResult = stream.writeLineWithTimeout(line, 100)
        when (writeResult) {
            is Ok -> {
                val lockResult = statsLock.withLock { inout stats ->
                    stats.okConnections = stats.okConnections + 1
                }
                when (lockResult) {
                    is Ok -> { }
                    is Err -> return // 对端曾在持锁时异常终止（LockError.Poisoned），本连接安全退出
                }
            }
            is Err -> return // 含写超时、对端关闭或 IO 错误，直接结束本连接
        }
    }
}

fun recordCompleted(
    own completed: MutableList<Result<Unit, Failure>>,
    statsLock: Arc<Mutex<ServerStats>>
): Unit {
    var i = 0
    val n = completed.size
    while (i < n) {
        borrow val outcome = completed[i]
        when (outcome) {
            is Ok -> { }
            is Err -> {
                println("connection unit failed: ${outcome.error.message}")
                val lockResult = statsLock.withLock { inout stats ->
                    stats.failedConnections = stats.failedConnections + 1
                }
                when (lockResult) {
                    is Ok -> { }
                    is Err -> println("stats mutex poisoned")
                }
            }
        }
        i = i + 1
    }
}

fun runEchoServer(
    listener: TcpListener,
    shutdownSignal: Receiver<Unit>
): Unit {
    var pool = SupervisorPool.empty<Unit>()
    val statsLock = Arc.new(Mutex.new(ServerStats(0, 0)))
    val shutdownFlag = Arc.new(AtomicBoolean.new(false))
    var nextConnId = 1

    while (true) {
        // 1. 检查停机信号（非阻塞）；置位停机标志以唤醒并结束所有空闲连接线程
        val stop = shutdownSignal.tryReceive()
        if (stop != null) {
            shutdownFlag.store(true, MemoryOrder.Release)
            break
        }

        // 2. 非阻塞回收已结束连接句柄，防止长运行服务内存泄漏
        val finished = pool.drainCompleted()
        recordCompleted(finished, statsLock)

        // 3. 接受新连接（带短超时以便定期轮询停机与回收）
        val accepted = listener.acceptWithTimeout(100)
        val stream = when (accepted) {
            is Ok -> accepted.success
            is Err -> continue
        }

        val connId = nextConnId
        nextConnId = nextConnId + 1
        val lockHandle = statsLock.share()
        val stopHandle = shutdownFlag.share()

        // 4. 在显式监督边界内派发独立工作单元
        pool.spawn(move {
            handleConnection(stream, lockHandle, stopHandle, connId)
        })
    }

    // 5. 优雅停机：读/写均带超时时，空闲或写阻塞连接在截止后观察到 shutdownFlag 或超时退出，joinAll() 等待回收
    val remaining = pool.joinAll()
    recordCompleted(remaining, statsLock)
}
```

## 3. 由试金石反推的行为草案第 3、4 条收口建议

对照[演进总纲之十一](evolution-11-failure-interfaces-units.md) §1.4 行为草案，手写服务器试金石暴露了三处必须定形的细节：

1. **行为草案第 3 条（监督结果与显式升级）**：
   - `joinCatching(): Result<T, Failure>` 与 `SupervisorPool.drainCompleted(): MutableList<Result<T, Failure>>` 均**只返回结果值，不自动向调用者传播失败**。若监督者希望“任意工作单元失败就级联失败”，由监督者显式匹配 `Err` 后调用 `error(...)`。
   - **关键区分**：若工作单元内部触发的是**致命错误**（`deinit` 中失败、展开试图穿过 `extern "C"` 帧、runtime 内部不变量损坏），按 [U0 合同](unsafe-aliasing-contract-outline.md) FS6 / FS7 与 FFI5，**直接终止整个进程且不执行待运行析构**，绝不被 `SupervisorPool` 拦截为 `Err(Failure)`。
2. **行为草案第 4 条（未取结果的监督句柄析构）**：
   - 单个 `SupervisedHandle<T>` 在未调用 `joinCatching()` 就离开作用域时，`deinit` **阻塞等待该单元结束且不传播失败**（失败报告已写入 stderr），保持无 detach 承诺。
   - 但对动态数量连接的服务器而言，仅靠单句柄阻塞 `join` 无法在主循环中回收已结束连接；因此 FD3 必须把 **`SupervisorPool`（含非阻塞 `drainCompleted` + 停机 `joinAll`）**作为一等监督设施纳入标准库，而不是让用户把 `SupervisedHandle` 塞进普通 `MutableList` 后在停机前无法回收。
3. **与普通 `thread(move { ... })` 及借用型子任务（S2）的边界**：
   - 普通 `thread(move { ... })` 不是监督边界，其 `join(): T` 签名不变；其中的普通失败继续按“无监督边界即终止进程”处理，保证 v1 行为零变化。
   - 借用型子任务/子线程（S2）不是监督边界，必须遵守[失败与清理模型](failure-cleanup-model.md)模型 A3 与 OD2 · B1：等待全部子任务实际终止并完成必要清理后才释放父帧借用。

## 4. J48 四项现行预留核对表

对照[演进总纲之十一](evolution-11-failure-interfaces-units.md) §1.4 末尾列出的预留清单（原第 2 项已于 2026-11-14 取消，实际有效为 4 项）：

| # | 预留项 | 目标归属 | 当前状态 | 核对结论 |
|---|---|---|---|---|
| 1 | `unsafe` 合同从第一版起纳入“失败安全”条款（FS1–FS8） | [U0 合同](unsafe-aliasing-contract-outline.md) §4 | **已落实草案** | 已写清监督边界内展开与 v1 / 致命终止不跑 drop 的区分，以及 FS1–FS8 义务 |
| 2 | 普通线程句柄维持 `join(): T`，不引入隐式捕获 | v1 受限线程（P4） / [Z1](v1-freeze-decisions.md) | **已登记 Z1** | 与 `SupervisedHandle.joinCatching(): Result<T, Failure>` 正交，v1 零破坏 |
| 3 | 所有权图（IR0）语句集为可能失败调用预留失败出口边 | [之八](evolution-08-compiler-ir-escape-simd-selfhost.md) IR0 | **待 IR0 规格落盘** | 起草 IR0 规格与新 ADR 时须保留 `unwind/failure` 边位，即使阶段一不激活 |
| 4 | 锁与 channel 的库设计为“持有者异常终止”预留可辨别返回位 | [之四](evolution-04-route-capability-unsafe-suspend.md) C4、[之七](evolution-07-concurrency-async-io-ffi.md) S1、[Z1](v1-freeze-decisions.md) | **已登记 Z1 与模型 F** | `Mutex.withLock` 返回 `Result<R, LockError>`；channel 全发送端异常终止后关闭对端唤醒接收者（[失败与清理模型](failure-cleanup-model.md) §6.2 模型 F 已验） |

## 5. 阶段落位、阻塞点准入与仍待 FD1–FD3 收口的遗留项（2026-11-27 OD6 共识）

按 `#koven-design` 三轮讨论（OD6 共识，非正式批准）：FD0 **不整体前移至阶段四，也不整体推迟至阶段五后**，而是按阶段拆开——**阶段四**保留 FD0 预留与线程级监督试金石（由 IO3 / FD1 / FD3 经运行时测试验收）；**阶段五及之后**由 IO4 / FD2 承接在途 IO 终止窗口与冻结协议，由 S4 承接异步任务树 `Rc` 隔离。

### 5.1 阶段四阻塞点准入与排除规则

- **直接沿用 3A 排除表**：OS 同步 `sleep` / OS 同步锁 / 不可中断同步 `read`/`accept`、未接入取消信号的孤立 `channel.recv`/`oneshot.await`、以及不可中断同步 FFI，不得默认作为阶段四可展开/可排空的等待依赖。
- **准入点须逐项写明唤醒或有界机制**：
  - `acceptWithTimeout` / `readLineWithTimeout` / `writeLineWithTimeout`：依赖套接字截止超时；
  - **`statsLock.withLock`（暂属 3A 排除项）**：`LockError.Poisoned` 仅区分持有者异常终止，**不证明**活持有者场景下等待方能被停机唤醒或等待时间有界（[失败与清理模型](failure-cleanup-model.md) §6.2 模型 F 仅验持有者异常终止后同伴不卡死，不验活持有者停机唤醒）；在补齐可唤醒/可排空协议前，不得把 `withLock` 默认视为阶段四已接纳阻塞点；
  - **`println` 诊断输出**：在停机路径上尚无时限保证，列为待明确项。

### 5.2 有界停机的三条独立阻塞边与四个计时检查点

源码齐备或“单次套接字读写带超时”均**不能**证明 `joinAll()` 或服务函数有界返回。IO3 / FD1 / FD3 运行时验收必须使用**包含失败连接**的用例（确保触发 `recordCompleted` 的 `Err` 分支，不能只测全成功路径），分别对以下三条阻塞边注入卡滞并独立计时：

1. **工作线程内 `handleConnection` 的 `withLock`**：位于 `joinAll()` 返回之前，若无界等待将直接导致 `joinAll()` 无法有界返回；
2. **主循环内的 `recordCompleted`（失败连接分支的 `withLock` 与 `println`）**：若在运行中回收失败连接时无界阻塞，会挡住下一次 `shutdownSignal.tryReceive()`，推迟置位 `shutdownFlag`，进而使 `joinAll()` 的起点本身无界；
3. **停机后 `joinAll()` 之后的 `recordCompleted`（`withLock` 与 `println`）**：只挡住 `runEchoServer` 最终返回，不回溯改变 `joinAll()` 已返回的事实。

上述测试须分别记录四个独立截止检查点：**① 收到停机信号 → 实际开始停机（置位 `shutdownFlag`）**、**② 工作线程排空**、**③ `joinAll()` 返回**、**④ 服务函数最终返回**。上述三条阻塞边与四个检查点以及“锁中毒 vs 活持有者卡滞 vs 诊断输出阻塞 vs 停机信号起点”的正交区分，现已由离线状态模型 `scripts/loan_kernel/failure_model.py`（模型 G：`fd0_bounded_shutdown_model`，`scripts/tests/test_failure_model.py`）枚举验证；该模型仅枚举离散状态转移与卡滞可达性，**不证明真实运行时的墙钟时限**，真实时限仍由 IO3 / FD1 / FD3 运行时实测计时检验。

### 5.3 其余跨阶段遗留项

- **展开途中等待健康长期线程的唤醒机制**（归 FD1 / FD3）：若监督单元内持有另一个长期阻塞的子线程句柄，展开等待可能退化为挂起，须配合协作取消或 channel 关闭协议明确约束。
- **在途 IO 终止窗口的后端与运行时证明**（归 IO4 / FD2）：[失败与清理模型](failure-cleanup-model.md)模型 D 已验证“隔离边界 + 全线程冻结终止窗口”可防止缓冲区提前复用，具体冻结协议与零终结证据资源保留策略留待 IO4 / FD2 联合落地。
- **任务子树作为监督边界时的 `Rc` 隔离前提**（归 S4）：沿用“监督边界只接受 `Transferable` 捕获”可阻止边界内外共享 `Rc`，异步执行器调度器与任务树的集成证明留待阶段五 S4 复核。
