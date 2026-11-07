# Koven 增量审计：2026-10-14 晚 → 2026-10-15

> 区间：main `b43a475b`（10-14 18:08，PR #99）→ `3104baa4`（10-15 15:15，PR #105）
> 上一版：[10-14 增量](2026-10-14-incremental-review.md) · [仓库 Review](2026-10-13-repo-milestone-review.md) · [Guide 审计](2026-10-13-guide-v0.43-design-audit.md) · [v1 范围](2026-10-13-v1-scope.md)
> 方法：读 diff 与 4 份新 Spec；分别用两个基线的 release `kovenc` 跑同一套 99 个 probe（原 95 个 + 本轮针对 0320/0321 新增 4 个），逐条对比。
> 未做：`cargo test`、CI、sanitizer、macOS。
> 后续：[10-17 增量审计](2026-10-17-incremental-review.md)。

## 0. 结论

**这一轮方向是对的，但"差不多改完"还谈不上。** review 提出的问题分三层，本轮只动了最外面一层。

| 层次 | review 建议 | 本轮状态 |
|---|---|---|
| 止血：诊断与文档 | 前端放行、ICE、README/教程夸大 | **大部分完成**。9 个"应被前端拒绝"的程序全部改为前端诊断（上轮 4/9）；上轮 6 处 ICE 全部消失；README 与 tour 已按实际能力改写 |
| 结构：管线与 lowering | 合并单文件/project 管线、通用所有权细化层 | **未开始**。0321 仍在单文件管线单独补洞 |
| 语言：Guide 增量 | 14 项语言规则、15 处冲突、D1–D11 决策 | **未开始**。Guide 三轮零改动，决策未记录 |

具体结果：

1. **前端门禁（0319）基本到位。** 新增共享的最终类型准入，single 和 project 两条管线共用同一分类器。未知成员给 L0080，对 nullable 用 `?` 给 L0085，
   仍未实现的类型义务给新的 **L0165"最终类型分析未完成"**。原先落到后端的 `BlockingDeferred`/`MissingFact` 大面积转为前端诊断。
2. **但引入了 3 处新的内部错误**：伴生对象函数调用 `P.origin()`（单文件），以及同一函数里 `?` 之后的 `Result.Ok(...)`（两种模式）。
   报错为 `final type admission is missing original diagnostic for required Error type`，属于新的内部不变量泄漏。
3. **L0165 遮蔽了两个本来正确的语言诊断**：链表 `cur = cur.next` 从 L0133 变成 L0165，单文件闭包改捕获变量从 L0135 变成 L0165。
   原因是类型准入先于所有权检查，而这两处的类型推断本身就没完成。
4. **原生能力：project 模式连续三轮 35/71 不变；单文件 26 → 28。** 增加来自 0321（MutableList 字符串元素）和 0320 的连带效果（无 payload enum 的 `when`）。
   单文件与 project 的能力不对称（99 个 probe）从 16 个（14 个只有 project 能跑 + 2 个只有单文件能跑）变成 17 个（12 + 5）：新增的是"单文件能跑、project 不能"的 enum 用例。

## 1. 本区间交付

| PR | Spec | 内容 | 对应 review 条目 |
|---|---|---|---|
| #100 | — | M2B 收尾整合：清单 5 项全部勾选，附"保留边界与去向"表；README、tour 按实际能力改写 | review §4.5 文档漂移；10-14 增量 §5"M2B 冻结" |
| #101 | 0319 | 最终类型消费准入与源级诊断，新增 L0165 | E1 前端硬门禁（review 中的 r01/r02/p34/s04 被直接用作冻结输入） |
| #102 | 0320 | enum case 构造值作为 Borrow 实参的分类 | review §4.3 project 模式 ICE（p49、q08） |
| #105 | 0321 | 单文件 `MutableList.add` 的 MoveOnly 元素交付记账 | review §3 p38（单文件 InvalidSsa） |
| #106 | 0322 | preview 包对 Ubuntu 外部 libxml2 的有界兼容 | 发布工程 |
| #104 | — | dependabot：toml 1.1.6 → 1.1.8 | — |

规模：104 个提交；crates 生产代码 +1.8k 行（其中约 1.6k 是 frontend 共享的 `final_admission`，单文件 lowering 只加了 24 行），测试 +4.3k（约 1:2.4，上一轮约 1:5）。
批准依据中写明按 review 的"A 诊断阶段封闭 → B enum 实参分类 → C List MoveOnly 转移追踪"推进，0319 直接使用了 review 的 probe 包。

## 2. 实测对比

只列出结果有变化的 probe（单文件 / project）：

| probe | 内容 | b43a475b | 3104baa4 | 评价 |
|---|---|---|---|---|
| r01 | `a.fooBar()` | BE / BE | **L0080 / L0080** | ✅ 修好 |
| r02 | `s.length` | BE / BE | **L0080 / L0080** | ✅ |
| p36 | `a.toLong()` | BE / BE | **L0080 / L0080** | ✅ |
| s04 | 对 `Int?` 用 `?` | BE / ICE | **L0085 / L0085** | ✅ |
| p34 | `for (i in 0..<5)` | BE / ICE | L0165 / L0165 | ✅ 不再泄漏；Guide 原定 L0159，Spec 选择先报 range 根因，可以接受 |
| p33 | `i in 0..<10` | BE / BE | L0165 / L0165 | ✅ 诚实标为未实现 |
| p11、p21、q03、s03 | interface bound 泛型调用、`Box.value`、`::inc`、`1 to 2` | BE / BE | L0165 / L0165 | ✅ 同上 |
| p23 | 顶层 `var` | BE / BE | BE / L0165 | ◐ 单文件仍落后端 |
| p49 | 无 payload enum `when` | BE / **ICE** | **✅** / BE | ✅ 单文件可运行，project 不再 ICE |
| u01 | enum case 作 Borrow 实参 | BE / **ICE** | **✅** / BE | 同上（0320） |
| u04 | 带 payload 的 enum case 作实参 | BE / **ICE** | **✅** / BE | 同上 |
| q08 | String payload 的 `when` | BE / **ICE** | BE / BE | ✅ 不再 ICE |
| p38 | `MutableList<String>` add | InvalidSsa / ✅ | **✅** / ✅ | ✅（0321） |
| u03 | `MutableList<Item>` add | InvalidSsa / ✅ | **✅** / ✅ | ✅（0321） |
| p16 | 自声明 `Result` + `?` | BE / BE | **内部错误 / 内部错误** | ❌ 新问题 |
| p28 | companion 函数调用 | BE / BE | **内部错误** / BE | ❌ 新问题 |
| p25 | 链表 `cur = cur.next` | L0133 / L0133 | L0165 / L0165 | ❌ 正确诊断被遮蔽 |
| p42 | 闭包修改捕获变量 | L0135 / L0135 | **L0165** / L0135 | ❌ 单文件正确诊断被遮蔽 |

汇总（口径与前两版相同）：

| 指标 | 614a7a7f | b43a475b | 3104baa4 |
|---|---|---|---|
| 71 个合法程序：project 可运行 | 35 | 35 | 35 |
| 71 个合法程序：单文件可运行 | 25 | 26 | 28 |
| 71 个合法程序：两种模式都可 | 23 | 24 | 25 |
| 9 个应拒绝程序：两种模式都得到前端诊断 | 4 | 4 | **9**（其中 2 个诊断码不对，见上） |
| 99 个 probe × 2 模式中的内部错误 | — | 6 | **3**（全是新出现的） |
| 合法程序落到后端拒绝的次数（71 × 2） | 71 | 71 | 60 |
| 其中改为 L0165"未实现" | 0 | 0 | 8 |

## 3. 新问题

### 3.1 最终准入的三处内部错误（应优先修）

最小复现，两种都是 Guide 下的合法程序：

```kotlin
// 单文件：final type admission is missing original diagnostic for required Error type
class P(val x: Int) { companion object { fun origin(): P = P(0) } }
fun main(): Unit { val p = P.origin(); println("e") }
```

```kotlin
// 单文件与 project 均失败
enum class Result<T, E> { Ok(success: T), Err(error: E) }
fun parse(n: Int): Result<Int, Int> = Result.Ok(n)
fun twice(n: Int): Result<Int, Int> { val v = parse(n)?; return Result.Ok(v) }
fun main(): Unit { println("f") }
```

两处的 Span 都指向作为静态限定符的类型名（`P`、`Result`）。guide-conformance 早有记录：构造目标、companion 命名空间这类"非值 expression"在单文件里是 `Error` 类型、没有对应诊断。
0319 合同第 3 条要求"仅在已选事实证明其角色时免除对应值义务"，但伴生函数调用和 `?` 之后的构造这两条路径没有被识别为静态角色。
上一轮它们是后端 `UnsupportedNode`，现在变成内部错误，对用户来说更差。建议补进 0319 的"不误杀对照"并加回归。

### 3.2 L0165 先于所有权检查，遮蔽了真实的语言错误

- p25（链表迭代）：`cur.v`、`cur.next` 在两种模式下都是 `Deferred(MemberAccess)`，即对 smart cast 后的 `var` 做成员访问的类型推断没有完成。
  此前所有权检查仍能报出正确的 L0133（从借用中移出），现在被 L0165 取代。用户看到的是"编译器还不支持"，而实际上程序本身就不合法。
- p42（闭包改捕获）：单文件里 lambda 内的赋值是 `Deferred(Assignment)`，L0135 被 L0165 取代；project 模式仍是 L0135。

这不是门禁设计的错，门禁暴露了两个前端推断缺口：nullable smart cast 的 `var` 成员访问、单文件 lambda 内赋值。建议把它们列为前端补齐项，而不是放宽门禁。

### 3.3 L0165 的消息直接暴露内部枚举名

当前消息形如 `final type analysis is incomplete: MemberAccess` / `OverloadSelection` / `Call`。
建议改为面向用户的说法，例如"此处的成员访问在当前编译器中尚未实现"，并在 L0165 的诊断目录里列出每个 `DeferredReason` 的对应措辞。

### 3.4 单文件与 project 的不对称在扩大

现在有 12 个程序只有 project 能跑（上轮 14），5 个只有单文件能跑（上轮 2）（p09、p49、u01、u04、q18，基本都是 enum case 条件与泛型 value class）。
0320 修好了两种模式的分类与 ICE，但 project 模式的 enum case 条件 lowering（Litmus 4 的老问题）仍然拒绝。每补一条管线，另一条就多一个差距。

## 4. review 建议的总体跟进状态

| 建议 | 10-14 | 10-15 | 说明 |
|---|---|---|---|
| E1 前端硬门禁、ICE 结构化 | ❌ | **◐ 大部分** | 0319 完成；§3.1 的新内部错误、§3.2 的遮蔽待修 |
| E2 管线合并 | ❌ 反向 | ❌ | 0321 仍是单文件独立补洞；0319 的共享分类器是正面信号 |
| E3 通用所有权细化层 | ❌ | ❌ | — |
| E4 `Result`/`Pair` 注入、`kovenc check`/`--help` | ❌ | ❌ | README 已如实写明 CLI 不加载 prelude；功能未做 |
| L12 `error()` 打印消息 | ❌ | ❌ | p04、p45 仍无消息退出 |
| 文档按实测改写 | ❌ | **✅** | README 亮点逐条加了"原生支持仅覆盖有界组合"；tour 新增"当前能力说明" |
| M2B 收尾后冻结 | 建议 | **✅** | 收尾清单全部勾选并附保留边界表，后续须另立 Spec |
| 高层集合 API、Map 遍历、文件 IO 归入 v1 里程碑 | ✅ | ✅ | 仍"尚未据此建立新 Spec" |
| Guide 审计：14 项语言规则、15 处冲突 | 无变化 | 无变化 | Guide 三轮零改动；C1（`when` 合并到 `Any`）仍被前端接受（p43） |
| v1 范围 §7 决策 D1–D11 | 未答复 | 未答复 | 仓库内未见记录 |

## 5. 建议的下一步

1. **补完 0319**（一个小切片）：修 §3.1 两处内部错误；把 L0165 消息改成用户可读的措辞；为 §3.2 的两处前端推断缺口建 Spec。
2. **决定 D10**。本轮的切片选择（共享前端分类器）说明团队已经在往"共享"方向走；下一步是 lowering 层的合并，否则 §3.4 的不对称会继续增长。
3. **启动 Guide v0.44 的设计**（v1 范围 P3）。前端门禁到位后，L0165 列表本身就是一份精确的"未实现清单"；而 Guide 缺失的规则（相等、转换、range、文本、IO）没有 L0165 可报，只能先进 Guide。
   建议先拍板 D1（相等/显示协议）、D3（`when` 不合并到 `Any`）、D4（顶层 `val`/`var`），这三项都不依赖实现进度。
4. **把 L0165 计数纳入进度度量**：每个 Spec 交付后统计 probe 矩阵中 OK / L0165 / BE / 内部错误四类的数量，能直接看出是在扩能力还是在挪错误类别。

## 附：复现

本轮新增 probe `u01–u04`，两个基线的结果为 `matrix-b43a475b-99.tsv` 与 `matrix-3104baa4.tsv`，
已放入本地 `.local-artifacts/review-2026-10-13/`。
