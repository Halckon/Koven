# SPEC-0360: String when 清理事实的 refinement 消费

> **性质**：有界后端变更合同 · **状态**：in-progress · **读取时机**：修改或验收 String when refinement 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0360` |
| 所属 Phase | Phase 4/5；不变更 Phase 3 合同 |
| 语言规范 | [Guide v0.46](../../guide/README.md) |
| 批准依据 | 2026-10-27 用户“继续推进”；独立实现 p41、Draft PR 与官方 CI，禁止本地动态验证 |
| 前置 Spec | [0357](0357-string-when-ownership-facts.md)、[0356](../../archive/specs/0356-refine-when-family.md) |
| 关联 ADR | [0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[0018](../../adr/accepted/0018-string-owner-runtime-abi.md) |
| 影响范围 | refinement subset / when CFG、真实源测试、p41 oracle 与事实文档 |
| 语言语义变更 | 否 |

## 1. Goal 与基线

从 main `5b7f3e977aacfc72f4419095ec3bc4fb75f2f73d` 新建
`feature/spec-0360-string-when`。0359 的未合并 Rc 分支
`44d1e11c2a764d41289c919c05901b9eb8e81cd5` 不作为输入，不修改其工作树。
交集为 subset、测试注册/准入、conformance 与架构/路线图；不交叉改变 Rc 语义。

以现有通用 refinement 真正运行原 `conformance/probes/p41_when_string.ko`，不改其字节与
`when-str\n` 强 stdout oracle。无 legacy 捷径、无 frontend 新释放点。

## 2. 事实消费

- subject 求值一次。String Group 按 producer 同样归一到内部真实表达式身份。
- `StringEqual` 已计算 Boolean 后、CFG 分流前，消费 source-qualified
  `WhenComparisonCompleted { control, entry, alternative }`；候选 owner 恰一次。
- 成功边消费既有 `WhenAlternativeMatch`；失败边消费 `WhenAlternativeNoMatch`。
  所有目标与位置只来自 validated facts，不由最后一次比较等语法推测释放。
- 构造互斥边时恢复同一 subject temporary 的原 ID/槽位；不发新 drop，不修改 fact。
  零比较沿已有 `AfterExpression(subject)`，不伪造 alternative。
- 直接 Borrow String 参数 subject 使用 caller shared loan，不 Read 为 owned String，不结束
  或释放 caller loan；callee 的其他 MoveOnly Borrow 用法仍由既有 subset 限制。
- 完整事实消费检查、通用槽位 CFG/SSA liveness、SSA 与 LLVM verifier 均保留。

## 3. 范围与原观察

仅无插值 String literal/Group candidates、producer 已证明且属于现有 refine 表达式子集的
普通 named/temporary String subject；包含逗号、早/晚/无匹配、else/无 else、零比较、
body/后续使用、既有 Return/Break/Continue。非 String MoveOnly、复杂 candidate、field/index、
borrow-result call、显式 borrow-result binding、一般 Abort call 不扩展。

0357 两条 consumer 拒绝 fixture 原源码与 UnsupportedNode 断言保留，使用既有 test-only
`with_per_family_lowering` 钉住仍拒绝新事实的旧路径；公开路径另加同源成功测试。subset 原
String 源码原字节移入成功准入矩阵。这是阶段能力门的迁移，不删除原语义观察或其他负控。
旧 per-family 对未消费事实的拒绝门保留，防止其他未迁入构造落入漏清理路径。

## 4. 验收矩阵

| 原始来源/场景 | 预期行为与证据 |
|---|---|
| p41 原文件 include | `when-str\n`；公开单文件 native + 单/双 source SSA/LLVM |
| 动态 temporary + early/late/miss + Group/逗号 | subject 一次求值；每次 1 allocation/1 free；未求值后缀无 drop |
| named subject body + after | 两次原字符串输出；1 allocation/1 free |
| no else、only else、empty | `done\n`；每条 1 allocation/1 free |
| Borrow 参数 + caller 后续使用 | callee 仅 candidate drop、无 owned Read；caller 两 allocation/free |
| Return/Break/Continue | `return\nloop\n`；3 allocation/free，无重复清理 |
| 候选清理精确次序 | SSA equality 紧跟候选 drop；native String drop hook 为 1/3/3 候选加固定4 |
| 显式 borrow-result binding | 保持 subset false；不扩大既有能力 |

每个新增 source 经 single 与两个 source 的正逆输入公开分析；双 source 输出确定性比较。
计数器与 native 命令只由官方 CI 执行。allocation/free 不冒充 literal owner drop 证明，后者
另用 String drop hook 与 SSA 身份/次序断言。新测试位于 `ssa/refine_string_when_tests.rs`。

## 5. 验证账本

- 本地 Cargo/build/check/clippy/test/native/test-list：全部未执行（用户禁止）。
- 本地只做官方定向 rustfmt、文档生成/结构、尺寸/字节与 diff 静态检查。
- exact Draft PR 的双宿主官方 CI 待发布与核验；未宣称编译/运行通过。
- 保持 in-progress；未 Ready、未 merge、未 Slack 通知；父任务负责一次 review。

## 6. 普通临时 owner 与后续实参双出口（有界补片）

仅准入无 receiver、非特化的直接具名尾 `return`：第一个实参是无 runtime 字段、无
`deinit`、无构造实参或 delivery 的普通整根 HeapOwner，按精确匹配的共享借用参数交付；
第二个实参是动态 Boolean 参数的 `when`，`true` 臂提前返回、`else` 臂正常产生值。
两个叶值都只读取已知 `Int` 绑定。相同 source-qualified lender、owner、transfer 必须同时具有
正常 `CallReturn` 和提前 `ControlTransfer` 的唯一 `EndCallLoan → Drop`，无其他该 owner drop。
准入不产生前端事实，不改变 builder、SSA verifier 或普通不匹配路径。

Mac + LLVM 21.1.8：真实具名函数 Refine/SSA/LLVM 通过；表达式体、非尾位置及有 runtime 字段
对照不进入本补片。实际 SSA 边传递同一 fresh owner 和共享 loan，两个出口均先 End 后恰好一次
Drop，提前退出 0 次派发、正常退出 1 次。native 两个调用的数值、stdout 和按指针分配身份的
两个释放均断言；相邻混合 Rc native 对照通过。未执行 Linux、全量测试或 CI，不关闭本 Spec。

## 7. 直接不可变局部结果（限函数体末尾）

在 §6 的所有证明照常成立时，另准入函数体最后两句为无修饰 `val result = 直接调用`
和 `return result` 的形态。初始化调用、绑定、返回引用和函数返回类型均为同一非空 Owned
`Int`，返回引用绑定到同源声明，且正常路径可完成初始化。带包装或括号的初始化、`var`、
返回其他符号或两句不相邻均不由本补片支持。前端事实、普通绑定/返回 builder 和 verifier
保持不变。

验收使用带 native 入口的同一源码，只观察一次实际 lowering；入口调用的具体函数必须匹配
Refine 记录，从该 Program 同时验证 SSA、LLVM 和 native。String 命中/不命中与 Boolean
提前/正常双轴的结果为 `1/10/0/9`。提前出口不派发调用；两个出口都依次结束同一
loan、恰好释放同一 Node；正常返回的是调用结果实体。

Mac / Rust 1.96 / LLVM 21：基线新增阳性在原 String when 处得到 UnsupportedNode；限定
准入后 `mixed_string_when` 定向 6/6（含原尾调用、Rc native 对照、嵌套和近邻拒绝）。
同一观测 Program 的 SSA、LLVM 与链接执行通过，精确各释放四次分配身份中的一次。
Linux、全量和 hosted CI 未执行。


## 8. 单层括号初始化（同一尾部局部结果）

在 §7 的限制内，初始化式可为 `val result = (直接调用)`，最后仍为裸
`return result`。只接受一层 Group，Group 与真实内层调用同为非空 `Int` 且各有正常边，
无窄化或独立 Group 调用描述。原验证仍要求内层调用返回 Owned、匹配同一不可变声明与
函数结果，并仍以该内层调用的身份核对唯一的两条 End → Drop；不在 Group 制造清理点。
生成普通 Group、绑定和返回的 builder 不变。

Mac / Rust 1.96 / LLVM 21：原实现对该阳性用例在前置 String `when` 处报
`UnsupportedNode`。本片准入后，`mixed_string_when` 定向 8/8；同一经过路由记录的
Program 验证 SSA、渲染 LLVM 并链接执行，真实入口四条分支分别返回 `1/10/0/9`，
正常调用只派发一次、提前返回不派发，两路均在结束同一 loan 后只释放自己的 Node 一次。
双层括号、返回式括号、其他语法位置及其他近邻仍待单独核对 Refine 覆盖，不表示
括号语法非法。全量、hosted CI 与 sanitizer 未执行。

### 括号位置的通路审计

同一混合 String `when` 夹具记录九种尾部写法：裸尾调用、裸局部初始化和单层括号初始化进入
Refine 并通过 SSA/LLVM；`return (直接调用)`、双层括号初始化、`return (result)`，
以及可变绑定、不同返回符号、非连续语句在当前通路落到原 String `when` 上的
`UnsupportedNode`。后两类括号返回位置不要和已交付的单层括号初始化混同。

每个用例先通过前端检查，逐个核对真实内层调用、同一临时 owner 与提前返回的语义身份，
以及两个精确出口各一次的 End → Drop；括号表达式和内层值同为非空 `Int`，
不拥有调用描述。这个矩阵只刻画当前降低覆盖，不新增语法或生产准入。
Mac 使用 Targeted CI 同一 feature 的 `mixed_string_when` 定向 8/8，含原 native 正控；
该矩阵没有在 Linux 或 hosted CI 运行。
