# M6 线程所有权转移：Spec 起草材料

> **性质**：待编号 Spec 起草材料 · **状态**：draft / 等待 runtime 基线及线程合同补齐 · **读取时机**：评审 v1 线程转移首片及未来共享边界时 · **唯一真源**：本页维护待决问题和候选验收；现行线程语义以 Guide 为准

## 1. Goal 与阶段

Goal：在 v1 已规定的所有权转移模型下，交付可验证的线程启动、完成和资源清理切片。
先完成合同缺口决策，再实施 Phase 2–5 与 Phase 6 宿主验证。
共同启动/调整规则见[计划](post-governance-milestones.md)，用户本轮授权仅为起草。

前置为稳定的 closure/runtime 交付边界、[M4](memory-safety-validation-spec-draft.md)适用证据
与实际并行工作需求；不因计划排序就把全部 Map 或发行能力作为硬前置。
首片具体平台、任务输入和结果观察方式在正式 Spec 中固定。

## 2. 已有规范与实现核验边界

[线程规范](../guide/13-program-runtime-standard-library.md)已经规定 `thread(move { ... })`
及只转移所有权的 v1 范围；跨线程 closure 参数要求显式 move，全部捕获满足 Transferable。
能力判定依据[所有权规范](../guide/10-ownership-borrowing-drop.md)，
不能仅凭使用 move closure 就认定捕获可跨线程。

现行非原子 Rc 始终不可 Transferable，与其 payload 是否可转移无关；
不能隐式改成原子引用计数、复制 handle 或通过 raw pointer 绕过检查。
borrowed closure 的既有同线程存活规则不因线程首片而缩减。
Guide 示例不是 native 已实现证明：实施前重新核对声明、能力检查、SSA、runtime 和测试入口。

## 3. 必须封闭的线程合同

| 决策 ID | 待核对或补齐的边界 | 决策归属 |
|---|---|---|
| H1 转移判定 | 捕获的结构递归规则、泛型实例、用户类型与容器能力；拒绝借用捕获及 Rc | 复用 Guide；缺失的可观察规则先补 proposal/Guide |
| H2 启动与提交 | 参数求值、closure 环境转移点、线程创建失败时的归属和可观察失败 | 语言规则先确定；内部 ABI 再按 ADR/工程合同承接 |
| H3 handle 生命周期 | join 的接收模式、重复 join、未 join 的 handle 被丢弃/移动时如何处理 | 示例不足以决定 detach、阻塞或诊断，需要明确语义 |
| H4 完成与错误 | 正常完成、worker 内 Abort、宿主线程错误及进程终止边界 | 不把 Abort 擅自变成 Result Err 或异常展开 |
| H5 资源与可见性 | worker capture/drop 的唯一责任、join 后可观察完成关系、主线程提前退出 | 可观察合同与平台实现分开；不能凭 OS 经验补语言承诺 |
| H6 runtime 接线 | 栈/启动桥接、最小 unsafe、目标 ABI、错误转换、无全局隐式状态 | 按现有 crate 边界决定；生产输出仍走用户输出边界 |

未定义的语言行为先形成 proposal，并由用户明确启用 Guide 增量；ADR 不能代替这一前置。
H1–H6 不能用“内部实现细节”名义跳过用户可观察的失败和资源行为。
不预设 join 返回结果类型，不复用 Rc 承载内部共享生命周期来绕过其线程限制。

## 4. 候选首片与验收矩阵

候选首片为一个无借用捕获、满足 Transferable 的 move closure，启动后显式 join，
使用批准的可观察结果确认任务完成，并检查捕获的正常清理。
具体源码要等 H1–H6 完成后再落盘，避免候选语法变成实现依据。
Channel 只有在观察结果确有需要且其自身合同闭合时纳入，否则独立 Spec 承接。

| ID | 验收 | 关键反例或独立预期 |
|---|---|---|
| H-A | 合法 capture 与启动贯通 | 转移后 caller 的 moved 状态、worker 获得唯一 owner |
| H-B | 不可转移值稳定拒绝 | 直接/嵌套/泛型内 Rc、借用 capture、错误能力事实；诊断位置可追溯 |
| H-C | 创建失败清理正确 | 可重复的失败注入，按 H2 判断 capture/handle 的责任，无双重释放 |
| H-D | join 和 handle 生命周期 | 正常完成、重复使用、移动、丢弃及提前退出按 H3 接受或拒绝 |
| H-E | worker 正常/异常边界 | 按 H4/H5 检查输出、进程状态和必要清理，不假定 Abort 会 unwind |
| H-F | source/closure 身份与 SSA 合同 | 多文件、泛型实例和捕获槽位不串用；伪造产物被 verifier 拒绝 |
| H-G | 线程资源及检测接线 | 多次创建/join 与并发执行，批准路径唯一清理，适用检测器真实生效 |
| H-H | 两宿主用户入口 | 同一项目的编译、运行、输出/错误；记录目标差异与不支持项 |

用 join 或批准的同步关系确定观察点，不依赖 sleep 或固定调度次序。
跨线程日志顺序只有规范保证时才作 golden；测试计数器自身必须线程安全，并与生产实现隔离。
资源 oracle 不从相同 drop plan 生成；检测工具不能自动替代 Transferable 的负向用例。
线程创建失败注入只用于测试边界，不能变成生产可配置故障开关。

## 5. 后续共享与平台工作

Arc、Shareable、共享可变状态和 Weak 均不属于本首片。确有需求时先比较消息转移与共享模型，
形成独立 proposal/Guide 决策，定义原子计数、别名、数据竞争和清理合同后再计划实现。
不把未来共享所有权当作实现现行 v1 线程 API 的隐含前置。

Channel 的阻塞、关闭、发送失败与消息 owner 归属需要自己的完整合同；
Windows、musl、交叉编译是各自目标平台工作。async/await、Future、执行器与协程仍在未来范围。

## 6. 可调整项与记录

正式实施可缩小首片捕获类型、宿主范围或将 runtime 接线单列交付，但必须明确限制，
不能悄悄改变 v1 的 Transferable 含义，或用成功跳过某宿主冒充双宿主支持。
需求不成立时可推迟整个 M6，并记录原因及对其他里程碑的影响。

H1–H6、正式编号、首片源码和 H-A–H-H 均未完成或未运行。
文档验证统一见[计划 §11](post-governance-milestones.md#11-文档起草记录)。
