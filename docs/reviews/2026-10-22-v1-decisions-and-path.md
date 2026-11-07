# Koven v1 待决事项建议与推进路径

> **性质**：评审建议，非规范 · **日期**：2026-10-22 · **基线**：PR #120 头 `aef3f4f8`、Guide v0.45
> **范围**：v1 范围文档 D1–D12 中仍待决的项、E4、线程、错误模型，以及 Kotlin 近期语言演进中可借鉴的部分；最后给出按依赖关系排列的推进路径。
> **相关**：[v1 范围](2026-10-13-v1-scope.md) · [10-22 增量审计](2026-10-22-incremental-review.md) · 本地候选 `docs/proposals/global-and-static-state.md` · 仓内候选 `docs/proposals/thread-transfer.md`、`text-input-and-io.md`

## 0. 一览

| ID | 问题 | 建议 | 落点 |
|---|---|---|---|
| 错误模型 | 是否需要 try/catch；能否不"什么都 abort" | **不引入异常**；补齐不 abort 的标准 API 变体，`main` 可返回 `Result`，abort 可观测 | Guide 09、13；P0/P3 |
| E4 | Result/Pair 是否进标准环境 | **Result/Ok/Err 进内建环境**（两模式相同）；Pair 随 `to`/`mapOf` 一起进 | Guide 02、09；P0 |
| D12 | abort 是否向 stderr 输出 | **输出消息与源码位置**，写失败忽略，然后 SIGABRT | Guide 09、13；P0 |
| D9 | 退出码 | 正常 0；`main` 返回 Err 时为 1；abort 保持 SIGABRT；`kovenc run` 透传子进程退出码 | Guide 13；P0 |
| D4 | 顶层 `val`/`var` | **v1 只允许 `const val`**（候选 G1）；运行时顶层值改为前端语言诊断 | Guide 05；P0 |
| D5 | 线程是否进 v1.0 | **受限版本进 v1**：spawn/join + channel，只转移所有权，无共享内存 | Guide 10、13；P4 之后 |
| D1 | 值语义协议 | v1 定 Equatable、Hashable、Display；value class/enum 自动推导相等与哈希 | Guide 03、04、08；P3 |
| D2 | 字符串插值 | v1 落地，`"${x}"` 要求 `x: Display` | Guide 04；P3 |
| D7 | 闭包可变捕获 | 非逃逸闭包可用独占借用捕获 `var`；move/逃逸闭包不可 | Guide 07；P3 |
| D6/L14 | unsafe/FFI、线程以外的预声明 API | 移出 Guide 正文到 proposals（v2） | Guide 13；随 v0.46 |
| 新 D13 | companion 语法 | 改为 Kotlin 2.5 的 `companion { }` 写法，语义不变 | Guide 08；随 v0.46 |
| 新 D14 | 未使用的 `Result` | 丢弃 `Result` 返回值报诊断（must-use） | Guide 09；P3 |
| 新 D15 | value class 解构 | 由"按字段顺序"改为"按字段名" | Guide 11、12；v1 冻结语法前 |

建议把上表除线程实现外的规则一次写成 **Guide v0.46**，一起审、一起启用，避免分多轮改同几页。

## 1. 错误模型：不引入 try/catch，但不再只有 abort

### 1.1 现状

Guide 09 已经定了三分法：合法的"无值"用 `T?`，可恢复失败用 `Result<T, E>` + 后缀 `?`，破坏不变量用 `error()` abort；语言里没有 `throw/try/catch`。
"什么情况都 panic"的感受，并不是这套模型本身导致的，而是来自四个缺口：

1. **标准 API 只有 abort 版本**：整数溢出、越界、`toXxx` 转换失败都只能 abort，没有对应的 `T?` 版本。
2. **`Result` 用不起来**：Result 不在用户环境里（s01/s02 L0080），`?` 原生路径未通（p16、p16b）。
3. **`main` 不能返回 `Result`**：顶层只能自己 `when` 再 `error()`，所以 `?` 在入口处断了。
4. **abort 不可观测**：没有消息、没有位置（p45），用户只看到退出码。

### 1.2 建议

- **不引入异常与 try/catch。** 理由：
  - Koven 的所有权与析构只走正常控制流，没有 unwind。引入 catch 需要栈展开、landing pad，以及"展开路径上的 drop 和 loan 结束"，所有 lowering 都要多一条路径，E3 的难度会成倍上升。
  - `inout` 独占借用在展开途中被打断时，被借对象的不变量无法保证，这正是 Rust 选择 panic=abort 也能成立的原因。
  - Kotlin 自己也在往"错误作为值"走：Rich Errors（KEEP-0462）用 `T | Error` 联合类型承载可预期失败，并明确不打算替代异常。Koven 没有 JVM 兼容包袱，直接用 `Result` 即可。
- **补齐不 abort 的变体（规则化）。** 在 Guide 写一条原则：凡标准 API 可能因为输入而 abort，必须同时提供返回 `T?` 或 `Result` 的版本。v1 首批：
  - 转换：`toIntOrNull()` 等 OrNull 族（即 L3b）；
  - 索引：`getOrNull(i)`；
  - 整数运算：保留默认 checked abort，另提供显式的 checked 变体（例如 `a.checkedPlus(b): Int?`，只做 `+ - *`，名称在 Guide 中确定）；
  - IO 一律返回 `Result`（L6）。
- **`main` 增加第三种合法签名**：`fun main(): Result<Unit, E>` 与 `fun main(args: Array<String>): Result<Unit, E>`，要求 `E: Display`。Err 时向 stderr 打印 `error: <display>`，退出码 1。这样命令行程序可以从入口一路用 `?`，这也是 Rust 的做法。
- **abort 可观测（D12）。** stderr 输出 `abort: <message>` 与 `  at <file>:<line>:<col>`。`!!`、越界、溢出、转换失败使用固定消息；写 stderr 失败时忽略；之后照常 SIGABRT。需要在产物中保留源码位置表，代价很小。
- **线程中的 abort 使整个进程 abort。** v1 不提供隔离；需要隔离时，用多进程实现（v2 再议）。
- **Result 的最小 API**：`isOk`、`map`、`mapError`、`getOrElse`；不做 `Into/From` 式的隐式错误转换（Guide 09 已是此立场）。

## 2. E4：Result 进入内建环境

- **Result/Ok/Err 进入内建环境**，单文件与 project 相同。这需要改写 v0.45 Guide 02 "不额外提供 Pair/Result"那句，以及 0341 §2 的同名约束（D10 收尾后再改，避免干扰切换门槛）。
- **Pair 延后到 P3**，与 `to` 中缀和 `mapOf("a" to 1)`（S6）同批进入。Map 遍历用的是两个具名分量，不依赖 Pair。
- 标准错误类型 `IoError` 随 L6 一起定义，不在 E4 里预造。

## 3. D4：顶层状态先只做 `const val`

同意你的判断，并与本地候选 `global-and-static-state.md` 的 G1 一致：

- 顶层、`object`、companion 中的值声明 **只接受 `const val`**（已交付，B06）。
- 顶层运行时 `val`/`var` 改为 **Phase 2 的结构化语言诊断**，例如 "unsupported runtime global"，help 给出两条迁移路径：改成 `const val`，或在 `main` 里构造后作为参数传递。目前它是 L0165"尚未完成"加后端 UnsupportedSource，会让人以为以后会支持。
- G2（静态不可变存储，例如只读查找表）放到 v1 后期，有真实需求再开；G3–G5（惰性初始化、共享可变状态、线程局部）放到 v2，依赖 `Shareable`。

## 4. D5：线程做受限首版

同意进 v1，但限定范围，并把它放在关键路径之外。下面同时回答 `thread-transfer.md` 的 H1–H6：

| 项 | 建议 |
|---|---|
| API | `thread(move { ... }): Thread<T>`，要求 `T: Transferable`；`join()` 消费 handle 并返回 `T`；`channel<T>()` 得到 `Sender<T>`/`Receiver<T>`，`send(own v)`，`receive(): T?`（所有 sender 都销毁后返回 null）；`Sender` 可显式 `clone()`，形成多生产者单消费者 |
| H1 转移判定 | 沿用 Guide 10 的 `Transferable` 结构递归推导；`Rc`、借用捕获一律拒绝 |
| H2 创建失败 | 视为 operational failure：abort 并输出消息；捕获的值由 runtime 按正常 drop 清理 |
| H3 handle 生命周期 | **未 join 的 handle 在 drop 时隐式 join**（同 C++20 `jthread`），与确定性析构一致，也避免主线程先退出时 worker 只跑了一半；不提供 detach |
| H4 worker abort | 整个进程 abort（§1.2），不转成 Err |
| H5 可见性 | `join` 返回即建立 happens-before；channel 的 send/receive 同理 |
| H6 runtime | 只做最小 pthread 桥接，不暴露 unsafe |
| 不做 | 共享内存、Mutex、Atomic、Arc（属于 v2 的 `Shareable`）、scoped borrow 线程、线程池、async |

依赖：move 闭包原生（p18 目前失败）、`Transferable` 检查、L6 的 runtime 基础、Linux TSan/ASan 接入。
六个验收程序都不需要线程，所以建议把线程列为 v1.0 的"建议"项，并增加第七个验收程序（例如多线程词频或管道）专门验收；若时间紧，可以落到 v1.1 而不影响 v1.0 发布。

## 5. Kotlin 近期演进：可借鉴的部分

| Kotlin 特性 | Kotlin 状态 | Koven 现状 | 建议 |
|---|---|---|---|
| Companion blocks（`companion { }`，编译为静态成员） | 2.5 实验 | Guide 08 的 companion 已是"关联命名空间、无 singleton、无初始化、不实现接口"，语义与 companion block 相同 | **v1 采纳语法**：改用 `companion { }`。`object` 一词暗示存在对象，与 Koven 语义不符；产品未发布，可以直接替换，无需保留别名 |
| Companion extensions（`companion fun T.f()`） | 2.5 实验 | 无；仓内另有受限扩展函数候选 | v1.x，与扩展函数一起评审 |
| 多字段 value class | 2.5 实验（phase I） | 已有多字段、无身份、内联布局 | 借鉴其中三点：结构相等与哈希自动生成（并入 D1）；不生成 `copy`（Kotlin 也把更新语法拆成独立提案，v1 不做）；解构按名称匹配（D15）。`value object` 不需要，无 payload 的 enum 变体已能表达 |
| Rich Errors（`T \| E`、`error class`） | 设计中，尚未进入任何发布版 | `Result<T, E>` + `?` | 不引入联合类型，它会冲击类型推导与布局；只把它当作"错误作为值"方向的印证 |
| 未使用返回值检查 | 2.3 实验 | 无 | **v1 采纳一个子集**：丢弃 `Result` 报诊断（D14）。这是"不用异常"模型最需要的护栏 |
| `when` guard（`is A if cond ->`） | 2.2 稳定 | 无 | v1.x。`when` 家族的原生能力要先补齐（当前失败 9 个） |
| 集合字面量 `[...]`、context parameters | 2.4 实验 / 2.4 稳定 | 无 | 不进 v1 |

## 6. 其余待决项

- **D1 值语义协议**：v1 定义 `Equatable`、`Hashable`、`Display` 三个；`Comparable` 只给内建数值、`Char`、`String`，供排序（S3）使用。
  value class 和 enum 在全部字段满足时自动推导 `Equatable`/`Hashable`，这同时解决 p35（value class `==`，目前 Guide 未定义）。`Display` 不自动推导，用户类型显式实现，避免把调试格式变成兼容承诺。Map 的键要求 `Equatable + Hashable`。
- **D2 字符串插值**：v1 落地，依赖 D1 的 `Display`。目前连 String 插值（p02）原生都失败，这是写命令行工具的基础能力。
- **D7 闭包可变捕获**：允许非逃逸（借用）闭包以独占借用捕获 `var`，移动或逃逸的闭包不允许。这是 `forEach`、`fold` 一类累加写法（L16）的前置条件。
- **D6/L14**：unsafe、extern、RawPtr 等未进入 v1 的内容，从 Guide 13 正文移到 proposals，Guide 只保留 v1 承诺的部分。
- **D15 解构按名称**：现在 `val (a, b) = point` 按主构造器字段顺序对应，调整字段顺序会静默改变含义。Kotlin 的多字段 value class 提案默认按名称解构。建议在 v1 冻结语法前改为绑定名必须等于字段名，按名匹配。

## 7. 推进路径

### 7.1 依赖图

```mermaid
graph TD
  A1[A1 D10 收尾<br/>回退/InvalidSsa 清零, 删旧实现] --> A2[A2 E3 试点<br/>nullable 家族]
  A2 --> A3[A3 E3 推广<br/>when / 闭包 / inout / 泛型 / Result ?]
  G[G Guide v0.46 决策批<br/>E4 D1 D2 D4 D7 D9 D12 D13 D14 D15 L14] --> C2
  G --> L1[L1/L2 Display + 插值]
  G --> L16[D7 + L16 forEach 与集合基础 API]
  C1[C1 abort 消息与位置<br/>kovenc run 透传退出码] --> V1
  C2[C2 Result 进内建环境<br/>main 返回 Result] --> L6
  C3[C3 顶层运行时值改为语言诊断]
  C4[C4 154 probe + 六个验收程序源码入库] --> V2
  A3 --> L1
  A3 --> L16
  A3 --> L7[L7 MoveOnly enum 消费式匹配]
  A3 --> TH[线程受限版]
  L1 --> L5[L5 文本 API]
  N05[N05 Double/Char 原生] --> L5
  L5 --> L6[L6 IO 与进程]
  L6 --> V1[V1 六个验收程序]
  L16 --> V1
  L7 --> V1
  L6 --> TH
  V1 --> V3[V3 ASan/LSan 全覆盖]
  V2[V2 conformance 矩阵] --> V6
  V3 --> V6[V6 v1.0 发布]
  TH -.可落 v1.1.-> V6
```

关键路径：**A1 → A2 → A3 → L1/L2 → L5 → L6 → V1 → V3 → V6**。线程与 V2 在旁路并行。

### 7.2 分阶段与门槛

| 阶段 | 内容 | 可并行 | 完成门槛（可度量） |
|---|---|---|---|
| ① D10 收尾 | A1 | G（写 Guide v0.46）、C1、C3、C4 | 固定集合两模式差异 0、InvalidSsa 0、原成功能力无损失；旧 checker 与 `lower_frontend` 删除 |
| ② E3 试点 | A2：用通用所有权细化 lowering 打通 nullable 家族 | C2（E4）、N05 | nullable 家族 8/8 通过，且没有新增按组合拆分的 lowering 模块 |
| ③ 补齐与语言增量 | A3、L1/L2、D7+L16、L7 | L5 的 Guide 设计 | 74 个合法基础程序 ≥ 70；`when`、闭包家族全部通过 |
| ④ 文本与 IO | L5、L6 实现；`main` 返回 `Result` | 线程实现 | 六个验收程序中至少 3 个能跑 |
| ⑤ 验收与发布 | V1、V3、V5、V6 | V2 收尾 | 六个验收程序双宿主正确且 ASan/LSan 干净；conformance 中合法程序 100% 运行，非法程序 100% 前端诊断，0 内部错误 |

### 7.3 两点提醒

- **E3 先做试点，再定全面路线。** 如果 nullable 家族在不新增按组合拆分的模块的前提下 8/8 通过，就说明通用 lowering 可行，后续家族沿用同一做法；否则退回按家族逐个补齐，但每项只做一次（D10 之后只有一条管线）。
- **每轮审计报告固定给三类数字**：74 个合法基础程序的通过数、六个验收程序能跑几个、内部错误数。这三个数比 Spec 数量和 CI 绿灯更能说明离 v1 还有多远。

## 参考

- [What's new in Kotlin 2.4.0](https://kotlinlang.org/docs/whatsnew24.html)
- [The Companions to Come（JetBrains Blog，2026-09）](https://blog.jetbrains.com/kotlin/2026/09/the-companions-to-come/)；[Kotlin 2.5 companion blocks 报道](https://devnews.news/news/languages/kotlin-2-5-companion-blocks-and-extensions-compile-to-statics/)
- [KEEP-0454 多字段 value class](https://github.com/Kotlin/KEEP/blob/main/proposals/KEEP-0454-better-immutability-value-classes-MFVC.md)
- [Rich Errors 讨论（KEEP #487）](https://github.com/Kotlin/KEEP/discussions/487)；[Rich Errors 现状评述（2026-09）](https://universitasscholarium.substack.com/p/the-question-mark-stays-where-it)
- [What's new in Kotlin 2.3.0](https://kotlinlang.org/docs/whatsnew23.html)；[Kotlin 2.2 新特性](https://softwaremill.com/leveling-up-kotlin-whats-new-in-2-2/)
