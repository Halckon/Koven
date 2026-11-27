# 内核演进两项悬决策略的裁决纪要与 3A 取消接口对比（2026-11）

> **性质**：非规范候选决策与对比稿 · **状态**：讨论收口（未启用；不修改现行 Guide，非正式 Spec/Guide 批准） · **读取时机**：核对 OD1（`A1 / 1A-ext`）与 OD2（`B1 / 首个已提交的普通失败`）三轮讨论收口条款、按 Guide 合法位置审查示例，或评审 3A 取消接口两种候选方案时 · **唯一真源**：本页维护 OD1 / OD2 的三轮讨论收口结论、3A 取消接口调用例对比与待补原型验收清单；实验细节见[借用内核演算 §84](loan-kernel-calculus.md) 与[失败与清理模型 §6.4](failure-cleanup-model.md)，现行语义以 [Guide v0.47](../guide/README.md) 为准

本文不修改或启用任何 Guide 规则，不批准 Spec/ADR，不授权修改编译器。

2026-11-26 三轮设计讨论（Cooper、Grok、Claude、Gemini 四方确认）形成明确拆分：**OD1 · A1（1A-ext）收口，OD2 · B1（首个已提交的普通失败）只收策略，3A 取消接口暂缓**，并明确 3A 仍欠两项证据（取消接口调用例对比 + 多任务扩充原型验收）。

## 1. OD1 · A1（1A-ext）收口：禁止对裸借用绑定槽形成二层借用

### 1.1 原型对照证据（`Z46.lk` 六处漏报）

[借用内核演算 §84](loan-kernel-calculus.md)（`scripts/loan_kernel/cases/Z46.lk`，演算原型记法）对 `undecided` / `forbid`（1A-ext）/ `second_order` 三策略做了对照，证实默认扁平化 `undecided` 存在 **6 处**可复现的类型/借用漏洞：

| # | 原型复现写法（`Z46.lk`，演算记法） | 旧默认 `undecided` | `A1 / forbid` | 漏洞性质 |
|---|---|---|---|---|
| 1 | `val fa = &hb.a; write_item(&mut fa)` | 漏报（接受） | `TYPE` | 共享借用经局部绑定槽取址提权为可写 `inout Item` |
| 2 | `val fa = &hb.a; take_escapable_mut(&mut fa)` | 漏报（接受） | `TYPE` | 不可逃逸借用绕过 `<T>` 可逃逸上界 |
| 3 | `val rb = &mut fa; *rb = Item(9)` | 漏报（接受） | `TYPE` | 向 `borrow Item` 槽位写入非借用值 `Item` |
| 4 | `val n = 1; val r = &n; write_int(&mut r)` | 漏报（接受） | `TYPE` | 标量形参路径下共享借用提权为可写 `inout Int` |
| 5 | `val fa = &a; val s = Slot(&mut fa); write_item(s.p)` | 漏报（接受） | `TYPE` | 视图构造器消除一层借用，将 `inout (borrow Item)` 误作 `inout Item` |
| 6 | `val g = copy_inner(&fa)`（`T from x.origins`）后写 `&mut a` | 漏报（接受） | `TYPE` | `.origins` 丢失内层借用来源 `a` |

### 1.2 收口规则与 Guide 合法位置示例

四方确认采纳 **A1（1A-ext）**：

1. **禁止对象**：首版不对裸借用绑定槽（包括 `borrow` / `inout` 参数及显式 `borrow val` 局部绑定）形成二层借用；**显式取址**和**落到该绑定槽的隐式调整**都不能绕过。
2. **不受影响的合法位置**（按 [Guide 10](../guide/10-ownership-borrowing-drop.md) 现行合法语法与位置）：
   - **对所指对象的合法接收者重借**：经 `borrow` / `inout` 参数或 `borrow val` 绑定直接调用成员方法或传给兼容参数（重借的是所指对象，不借绑定槽本身）；
   - **`borrow val` 显式只读借用绑定**：`borrow val item = view(source)`；
   - **字段投影**：经合法 root/receiver 投影到字段（例如只读投影 `item.field`，或对普通 class 可变字段的 `&node.next`）；
   - **`for` 来源**：按 Guide 10/12 对合法集合或区间建立的迭代来源；
   - **合法 mutable place 的 `replace` / `swap`**：`replace(&this.state, nextState)`、`swap(&a, &b)`（要求操作数本身是 `var` 绑定、`inout` 参数或满足 Guide 10 封闭规则的 mutable field place，而不是借用绑定槽）。
3. **局部换来源与固定目标写入的边界**：
   - 局部需要切换到另一只读来源时，用**新的 `borrow val` 绑定**（例如 `borrow val first = view(a)`、`borrow val second = view(b)`；Guide 10 禁止 `borrow var` 与 `inout` 局部，因此不写 `fa = &b` 这类非法重绑）；
   - 需要跨调用持续写入一个固定目标时，使用显式 `view class Slot(inout target: Item)` 视图字段；
   - 明确**两者都不能原地换绑调用方的借用来源**。
4. **重开条件与范围边界**：
   - 只有真实标准库或典型应用 API 必须原地换绑调用方的借用来源、又无法用新绑定或返回新视图表达时，才重开 A2（`second_order`）；
   - `<view T>` 泛型位置上的借用绑定交互单列另案，不在本条一并放宽。

按 Guide v0.47 合法位置书写的正反对照示意（`view class` 为候选语法，其余为 Guide 10 现行语法）：

```kotlin
// 合法：接收者重借、borrow val、新绑定切换局部来源、合法 mutable place 的 replace/swap
fun inspectAndSwap(borrow a: BoxPair, borrow b: BoxPair, inout left: Item, inout right: Item) {
    borrow val first = peekLeft(a)      // 合法：显式 borrow val 绑定
    borrow val second = peekLeft(b)     // 合法：用新绑定换局部来源（不原地重绑 first）
    readItem(first)                     // 合法：对所指对象做共享重借
    readItem(second)
    swap(&left, &right)                 // 合法：对 inout 槽位所指的合法 mutable place 做 swap
}

// 合法：对所指对象的字段投影与 replace
fun detachNext(inout node: Node, own replacement: Node?): Node? {
    return replace(&node.next, replacement)   // 合法：mutable field place
}

// 拒绝（A1 / 1A-ext）：禁止对裸借用绑定槽形成二层借用（无论显式取址或隐式调整到绑定槽）
fun badBindingSlot(borrow h: Item) {
    borrow val cur = identity(h)
    writeItem(&h)        // 拒绝：h 是 borrow 绑定槽，不得对槽取址提权
    writeItem(&cur)      // 拒绝：cur 是 borrow val 绑定槽，不得对槽取址或隐式调整为 inout
}
```

## 2. OD2 · B1（首个已提交的普通失败）策略收口

四方确认 **OD2 只收策略，采纳 B1（首个已提交的普通失败）**，取消接口形态暂缓（见 §3）：

1. **仲裁规则与成功提交点**：默认不聚合多个错误，也不强制公开清理期错误回调。单次运行内所有可提交的终态事件——包括**作用域成功提交**（作用域体产出 `Ok(v)` 且最后一个子任务已实际终止、未发生普通失败或致命失败）、**首个普通失败提交**与**外部取消提交**——按同一**提交全序（commit total order）**逐个仲裁，先提交者胜出；**不用墙钟时间倒推首错或成功**。
2. **外部取消与子任务响应取消**：观察到外部取消且作用域尚无主结果时，**当步提交取消**，不等子任务排空后再挑结果；子任务响应取消返回取消结果不算普通失败，不参与竞争主错。
3. **后到事件与清理义务**：后到的普通失败、成功值或取消均不覆盖已提交的主结果；所有未返回的值（包括被取消压过的成功值与后到的普通错误值）及局部资源必须**恰好清理一次**；必须等到所有子任务**实际终止**且必要清理完成后，才释放父帧借用。
4. **致命失败与重试边界**：`deinit` 失败或已定义的致命失败（如跨 FFI 失败、runtime 内部错误）仍**直接终止进程**，绝不被主错、成功或取消覆盖；仅收到取消或主错**不能作为安全重试的依据**（子任务在取消或失败前可能已产生外部副作用）。

## 3. 3A 欠项一：取消接口两方案调用例对比（暂缓裁决）

OD2 已固定运行时仲裁策略（B1），但用户看到的取消接口签名暂缓。本节用两个对称调用例，对照 [Guide 09](../guide/09-nullability-errors.md) 的现行约束（`Result<T, E>`、`?` 仅传播 `Result` 且要求 `E` 精确相同、无异常与隐式错误转换），比较两种候选表面：

- **方案 C-I：`E` 显式承载取消**——`scoped` 返回 `Result<T, E>`，业务错误类型 `E` 自身显式包含取消变体（例如 `enum class AppError { Io(IoError), Parse(ParseError), Cancelled }`），作用域在需要把外部取消编码为 `E` 时通过显式构造器或约定接口（如 `onCancel = { AppError.Cancelled }` 或 `E : FromCancelled`）产出 `Err(AppError.Cancelled)`。
- **方案 C-II-a：独立三态取消出口**——`scoped` 返回与业务错误 `E` 正交的三态结果 `enum class ScopedResult<T, E> { Ok(T), Err(E), Cancelled }`（拼写占位，未冻结），`E` 保持纯业务错误，不含取消变体。
- **方案 C-II-b：独立嵌套 `Result<Result<T, E>, Cancelled>`**——与三态枚举在 Guide 09 `?` 传播上**不等价**（2026-11-27 OD3 讨论更正）：在返回 `Result<_, Cancelled>` 的外层函数中可直接对 `scoped { ... }` 写 `?` 原生向上传播取消，代价是业务错误 `E` 需额外拆一层 `Result`。因此与 C-II-a 单列比较，不混为一谈。

### 3.1 调用例 1：多子任务汇总并接入调用方 `?` 传播链

场景：`loadDashboard` 需返回 `Result<Dashboard, AppError>`；它先用 `File.readText(path)?`（经 `mapError` 转为 `AppError`），再进入 `scoped` 并行拉取两路数据，最后组装结果。

**方案 C-I（`E` 显式承载取消）**：

```kotlin
enum class AppError {
    Io(err: IoError),
    Parse(err: ParseError),
    Cancelled                  // 取消必须作为业务错误枚举的显式变体（无末尾逗号）
}

fun loadDashboard(path: String): Result<Dashboard, AppError> {
    val cfg = loadConfig(path)?                      // Result<Config, AppError> -> ? 满足同一 E
    val parts = scoped(
        onCancel = { AppError.Cancelled },
        body = { sc ->                               // 统一命名实参传法，不混用无名尾 lambda
            val a = sc.launch { fetchProfile(cfg) }  // 子任务返回 Result<Profile, AppError>
            val b = sc.launch { fetchMetrics(cfg) }  // 子任务返回 Result<Metrics, AppError>
            Ok(Pair(a.join()?, b.join()?))
        }
    )?                                               // scoped 返回 Result<Pair, AppError> -> 直接 ?
    return Ok(buildDashboard(parts))
}

// 调用边界区分取消与普通失败：
fun handleRequest(path: String) {
    val res = loadDashboard(path)                    // 现行 Guide 要求 when 主体为表达式，先独立绑定
    when (res) {
        is Ok -> render(res.success)
        is Err -> when (res.error) {
            is AppError.Cancelled -> logCancelled()  // 靠枚举分支区分；若忘写容易混入通用错误日志/重试
            is AppError.Io -> reportIo(res.error.err)
            is AppError.Parse -> reportParse(res.error.err)
        }
    }
}
```

**方案 C-II（独立的类型化取消出口）**：

```kotlin
enum class AppError {
    Io(err: IoError),
    Parse(err: ParseError)     // E 保持纯业务错误，不混入 Cancelled（无末尾逗号）
}

fun loadDashboard(path: String): ScopedResult<Dashboard, AppError> {
    val c = loadConfig(path)
    val cfg = when (c) {                             // 现行 Guide 09 的 ? 仅作用于 Result<U, E>
        is Ok -> c.success                           // 返回 ScopedResult 时无法直接写 loadConfig(path)?
        is Err -> return ScopedResult.Err(c.error)
    }
    val parts = scoped { sc ->                       // 无需传 onCancel；scoped 返回 ScopedResult<Pair, AppError>
        val a = sc.launch { fetchProfile(cfg) }
        val b = sc.launch { fetchMetrics(cfg) }
        Ok(Pair(a.join()?, b.join()?))               // 作用域体内仍是 Result<..., AppError>，? 照常可用
    }
    return when (parts) {
        is ScopedResult.Ok -> ScopedResult.Ok(buildDashboard(parts.value))
        is ScopedResult.Err -> ScopedResult.Err(parts.error)
        is ScopedResult.Cancelled -> ScopedResult.Cancelled
    }
}

// 调用边界区分取消与普通失败：
fun handleRequest(path: String) {
    val res = loadDashboard(path)
    when (res) {
        is ScopedResult.Ok -> render(res.value)
        is ScopedResult.Cancelled -> logCancelled()  // 类型层强制独立分支，无法静默混作业务错误
        is ScopedResult.Err -> reportBusinessError(res.error)
    }
}
```

**调用例 1 对比要点**：
- **区分取消与普通失败**：方案 C-I 依赖 `E` 内部的约定变体，通用库代码（如只看到泛型 `<E>` 的中间件）若无额外接口上界就无法把取消从 `E` 中分离；方案 C-II 在类型层把 `Cancelled` 与 `Err(E)` 严格正交分离。
- **满足 Guide 09 `?` 的同一 `E` 约束**：Guide 09 规定 `expr?` 只能用于 `Result<U, E>` 且要求外层返回 `Result<T, E>` 中 `E` 精确一致。方案 C-I **零新增语言规则**即可直接用 `?` 贯穿同步调用与 `scoped`，代价是 `Cancelled` 会向纯同步调用方的 `AppError` 渗透；方案 C-II 保持 `AppError` 纯净，但在不扩展 `?` 协议的前提下，外层函数若向上传播 `ScopedResult` 就不能直接对 `Result` 或 `ScopedResult` 使用 `?`（必须显式 `when` 转发，或在入口边界将 `ScopedResult` 显式收敛为 `Result<T, AppError>`）。

### 3.2 调用例 2：取消与普通失败／作用域成功同轮待定时的提交全序

场景：作用域维护单一结果槽 `outcome_slot ∈ { Pending, PrimaryOk(T), PrimaryError(E), Cancelled }`。所有可提交的终态事件——包括**外部取消观测（`event_cancel`）**、**子任务普通失败（`event_fail_A`）**以及**作用域成功提交点（`event_ok`，即作用域体产出 `Ok(v)` 且最后一个活跃子任务已实际终止且无普通/致命失败）**——按本次运行的提交全序原子序列化，**绝不按墙钟倒推**。

**两种方案共享的 B1 提交全序核心**：

```text
作用域内部单一提交槽：outcome_slot ∈ { Pending, PrimaryOk(T), PrimaryError(E), Cancelled }

（A）取消 vs 普通失败同轮待定 = { event_cancel, event_fail_A（带资源 handle） }：

情形 1：event_cancel 先抢占 outcome_slot
  - 当步提交 outcome_slot := Cancelled（不等子 B 排空再挑）
  - 随后 event_fail_A 到达：见 outcome_slot 非 Pending -> 不覆盖主结果
  - 对未返回的 err_A（含其持有的资源 handle）执行恰好一次清理
  - 唤醒并等待子 B 实际终止（子 B 响应取消不算普通失败；若 B.deinit 失败则终止进程）
  - 释放父帧借用 -> 返回取消出口

情形 2：event_fail_A 先抢占 outcome_slot
  - 当步提交 outcome_slot := PrimaryError(err_A)（保留确切首错 err_A，含其所有权）
  - 随后 event_cancel 到达：见 outcome_slot 非 Pending -> 不覆盖 err_A，仅继续驱动剩余子任务排空
  - 唤醒并等待子 B 实际终止（若 B 清理期又产生普通失败 err_B，不覆盖且对 err_B 恰好清理一次；若 B.deinit 失败则终止进程）
  - 释放父帧借用 -> 将胜出的 err_A 安全返回给调用方（不悬垂到已释放的父帧）

（B）取消 vs 作用域成功同轮待定 = { event_cancel, event_ok（作用域体成功且末子任务完成清理） }：

情形 3a：event_cancel 先抢占 outcome_slot
  - 当步提交 outcome_slot := Cancelled
  - 随后 event_ok 到达：见 outcome_slot 非 Pending -> 不覆盖 Cancelled
  - 对未返回的成功值 v 执行恰好一次清理 -> 释放父帧借用 -> 返回取消出口

情形 3b：event_ok 先抢占 outcome_slot
  - 当步提交 outcome_slot := PrimaryOk(v)（全部子任务均已实际终止且完成清理）
  - 随后 event_cancel 到达：见 outcome_slot 非 Pending -> 不覆盖 PrimaryOk(v)
  - 释放父帧借用 -> 将胜出的成功值 v 安全返回给调用方
```

**两方案在同轮待定出口处的差异**：

| 对比维度 | 方案 C-I：`E` 显式承载取消 | 方案 C-II：独立类型化取消出口（`ScopedResult<T, E>`） |
|---|---|---|
| 情形 1 / 3a（取消先提交）的返回值构造 | 作用域必须调用 `onCancel()` 或 `E.fromCancelled()` 构造一个 `E` 装入 `Err(e)`；**`onCancel` 本身不得挂起或失败**，否则在排空路径引入新的失败源 | 直接返回 `ScopedResult.Cancelled`，**零构造回调、不要求 `E` 实现任何取消接口** |
| 情形 2 / 3b（`err_A` 或 `v` 先提交）的返回值构造 | 直接返回 `Err(err_A)` / `Ok(v)`，不调用 `onCancel()` | 直接返回 `ScopedResult.Err(err_A)` / `ScopedResult.Ok(v)` |
| 泛型库编写成本（不关心具体 `E`） | 任何包装 `scoped` 的泛型函数都须携带 `E : FromCancelled` 上界或透传 `onCancel` 参数 | 泛型函数直接返回 `ScopedResult<T, E>`，对 `E` 零额外约束 |
| 与 Guide 09 `?` 的即时配合 | 可直接 `?`（前提是外层 `E` 已包含取消变体） | 现行 `?` 不支持 `ScopedResult`，需显式 `when` 或后续单独提案扩展传播协议 |
| 取消误被当作普通错误重试的风险 | 较高（需调用方在 `when (err)` 中显式排掉 `Cancelled`） | 低（类型层强制先分流 `Cancelled` 与 `Err(E)`） |

### 3.3 调用例 3（裁决前补充证据）：嵌套 `scoped` + 已取消子任务 `join()` + 泛型重试中间件

2026-11-27 `#koven-design` OD3 讨论确认，在正式拍板取消接口之前须以同一组「嵌套 `scoped` + 已取消子任务 `join()` + 泛型重试中间件」对比两类方案（本例仅作为裁决前证据，**不提前选定 `join()` 或 `ScopedResult` 的具体类型**）：

```text
场景：
  1. 泛型重试中间件 retry<T, E>(maxAttempts: Int, op: () -> ScopeOutcome<T, E>)：
     - 仅在收到可重试业务错误 E 时按业务策略决定是否重试；
     - 收到 Cancelled 时必须立刻原样向上传播，绝不重试（落实 OD2 · B1 禁止仅凭取消安全重试）。
  2. 嵌套 scoped：
     - 外层 scoped 启动内层 scoped，内层子任务 a 响应外部取消，随后内层体调用 a.join()；
     - 覆盖 B1 两种提交顺序：
       (i)  外部取消先提交：内层与外层均须保持取消身份原样向上传播，不得经 ? 隐式变成业务错误 E；
       (ii) 子任务业务错误 err_A 先提交：即使外层随后观察到取消，主结果仍保持 err_A。

关键分化点：
  - 方案 C-I（E 显式承载 Cancelled）：
    * 泛型 retry<T, E> 只看见 E，若无额外“判别是否为取消”的谓词接口（仅 FromCancelled 构造器不够），
      无法把 Cancelled 从 E 中分离，且子任务响应取消返回 Err(AppError.Cancelled) 经体内 ? 冒泡后会混作普通失败；
  - 方案 C-II-a（三态枚举）与 C-II-b（嵌套 Result<Result<T, E>, Cancelled>）：
    * 泛型 retry<T, E> 可在类型层直接分流 Cancelled 与 Err(E)，不要求 E 携带取消变体；
    * 但须明确被取消子任务的 join() 是返回独立取消通道还是不返回到作用域体，并分别比较 C-II-a 与 C-II-b 在嵌套 ? 上的样板成本。
```

**暂缓结论（2026-11-27 OD3 讨论更新）**：继续暂缓取消接口定案，等路线 S `suspend` 与[之十一 §1.5](evolution-11-failure-interfaces-units.md) `E2`/`E3`（`?` 错误转换与通用 `Error` 接口）齐平再拍板；暂缓期间**以独立取消通道（C-II-a / C-II-b，单列比较）为优先比较方向**，不冻结 `ScopedResult` 或 `join()` 类型，且已在之十一 E2 侧预先写入硬约束：**`?` 错误转换不得把 `Cancelled` 隐式转为普通业务错误 `E`**。

## 4. 原型验收落地记录（2026-11-26 已完成，不修改编译器或现行 Guide）

三轮讨论收口要求的两组原型证据均已落地并通过双版本 Python 单元测试（共 139 条）：

1. **OD1 原型收口（`scripts/loan_kernel/`）**：
   - 已将 `checker.py` 默认 `local_borrow_ref_policy` 由 `"undecided"` 切换为 `"forbid"`（1A-ext），拒绝裸借用绑定槽的显式取址与隐式调整；同步更新 `Z44.lk`、`Z46.lk`（保留 `undecided` 6 处漏报与 `second_order` 显式对照）；
   - 新增 `scripts/loan_kernel/cases/Z47.lk` 与 `test_od1_binding_slot_rejection_vs_legal_reborrow`，覆盖绑定槽拒绝 vs 合法接收者重借（含两阶段借用）、新绑定换局部来源、字段投影、`for` 来源、合法 mutable place 的 `replace`/`swap` 与显式 `Slot` 正反例。
2. **3A 欠项二：多任务原型验收扩充（`scripts/loan_kernel/failure_model.py` + `scripts/tests/test_failure_model.py`）**：
   - 新增独立状态模型 **A3（`scope_b1_multitask_model`）** 与 `ScopeB1MultitaskModel` 单元测试（详见[失败与清理模型 §6.5](failure-cleanup-model.md)），完整覆盖四类验收且安全 B1 策略零违反、零卡住、状态图严格无环：
     1. **双向竞态**：覆盖两组同轮待定对称分支——① 外部取消观测 vs 子任务普通失败（取消先提交与失败先提交）；② 外部取消观测 vs 作用域成功提交（末子任务完成清理时同轮收到外部取消，验证先提交者胜出且未返回的成功值恰好清理一次）；
     2. **三个以上子任务**：3 个并发子任务（A、B、C）交错失败、阻塞、清理与有界重阻塞，验证全部 3 个子任务实际终止后才释放父帧（并在反例中捕获第三子任务未终止提前释放的 `UAF`）；
     3. **带资源次错的恰好一次清理**：后到普通失败（及取消先提交后的失败/未返回成功值）携带资源句柄，验证未返回的值与资源**恰好清理一次**（反例覆盖泄漏 `UNDISCHARGED` 与重复清理 `DOUBLE_CLEANUP`）；
     4. **胜出返回值在父帧释放后安全返回**：验证胜出主错 `err_A` 与胜出成功值 `v` 在父帧释放后安全交付调用方（反例捕获携带父帧借用的 `DANGLING_RETURN` 与吞掉 `deinit` 失败的 `DEINIT_FAIL_SUPPRESSED`）。

## 5. 2026-11-27 `#koven-design` 三轮讨论收口纪要（OD3–OD6）

由 `halckon0-dot` 主持，Cooper、Grok、Claude、Gemini 四方在 `#koven-design` 经三轮讨论就 OD3–OD6 达成一致候选方向与验收条件（无保留意见；均为非规范候选共识，不构成 Guide、Spec、原型默认值或实现的启用签核）：

| 编号 | 议题 | 三轮收口方向与验收条件 | 落点文档 |
|---|---|---|---|
| **OD3** | 3A 取消接口表面形态 | **继续暂缓**，等路线 S `suspend` 与之十一 E2/E3 齐平再定；优先比较独立取消通道，不冻结 `ScopedResult` 或 `join()` 类型；三态枚举（C-II-a）与嵌套 `Result`（C-II-b）单列比较 `?` 行为；之十一 E2 预先写死“`?` 不得把取消隐式转成 `E`”；补齐 §3.3 嵌套 `scoped` + 已取消子任务 `join()` + 泛型重试裁决前证据 | 本页 §3、[之十一](evolution-11-failure-interfaces-units.md) §1.5 |
| **OD4** | L2 / L3 候选语法拼写 | F5 本轮**仅演练前三项**：`flows … into`、`view T`（解除默认可逃逸上界）、`p.origins`（合同选择器，非写入目标），补齐 `from s into v` 一词两义、同名 `origins` 字段冲突与误作 `into` 目标负例（已完成 F5 演练与 Bison 2.3 冲突实测）；**`borrow var` 不进本轮 F5、Guide v0.47 不变**，未来 R2.6 另案须分列两项独立验收（换绑后旧别名不可再用 + 绑定槽不可取址或外露）；`any I` 同名消歧仍开放 | [F5 OD4 演练账本](parser-formal-verification/f5-od4-l2-l3-syntax-gate-drill.md)、[数据结构试金石集](data-structure-concurrency-litmus.md) §2、[覆盖与欠账](kernel-prototype-coverage.md) §6 |
| **OD5** | U0 合同定稿与迁移时机 | **留在 `docs/proposals/` 评审，不靠搬目录降低门槛**；满足 U0 §9 两条硬门槛（AL1/AL2 可表述为参考检查器前提 + UB1–UB6、UB9–UB11 具备可执行反例/捕获器/结果记录）后再迁入 `docs/compiler-specs/`。UB6 分两行登记：迁入前须实际跑通 **TSan 数据竞争捕获**（预 U1 代理已跑通），Loom/GenMC 级弱内存检查单列为迁入后的 **U1 验收欠项**，不挡迁入亦不互代；当前 UB1/UB5 捕获与 AL1/AL2 原型接入仍阻塞迁入 | [U0 合同](unsafe-aliasing-contract-outline.md) §9、[U0 硬门槛证据账本](u0-ub-harness-evidence.md) |
| **OD6** | FD 显式监督边界（J48）阶段落位 | **按阶段拆开**：阶段四保留 FD0 预留与线程监督试金石（由 IO3/FD1/FD3 运行时验收），IO4/FD2 在途 IO 冻结与 S4 任务树隔离留后续。准入表沿用 3A 排除表且逐项写明唤醒/有界机制：`LockError.Poisoned` 不等价于活持有者下可唤醒，`withLock` 与 `println` 列为残留阻塞点；须用失败连接分别卡住工作线程锁、主循环 `recordCompleted` 与停机后 `recordCompleted`，独立计时验证“收停机信号→开始停机”、工作线程排空、`joinAll()` 返回与服务函数返回 | [FD0 试金石与预留核对](fd0-supervision-boundary-litmus.md) §5 |
