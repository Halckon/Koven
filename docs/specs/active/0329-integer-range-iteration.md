# SPEC-0329: 整数 Range 的 owned 值 for

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：实现整数 Range provider 与退出清理时 · **唯一真源**：本片实施范围；语言语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0329` |
| 基线 | 真实 main `3fa567aa39be7ddba421046b2c4ed33d8ce90716`，已合0328值ABI |
| 分支 | 本地 `feature/spec-0329-integer-range-iteration`；授权远端 `fix/spec-0329-range-for` |
| 依赖 | [SPEC-0328](../../archive/specs/0328-integer-range-values.md) |
| 合同 | [Guide12整数Range迭代](../../guide/12-collections-destructuring.md#整数-range-与-map-的独立迭代合同) |

## 1. 范围

只接受环境绑定IntRange/LongRange的普通值for，source求值一次并复制owned快照；
每轮交付Int/Long Copyable值。名称及单`_`沿现语法；整数解构拒绝。
空区间零轮、升序、终值完成后直接退出。闭MAX后不递增，不计算全域长度。
每轮owner、normal/continue/break/return沿既有清理合同；返回值先求值交付，Abort不展开。
不扩Map迭代、用户Iterator/provider、step/reverse、nullable provider或其他循环收窄。
一般nullable值ABI和0328旧borrow-result全函数Binary边界不是本片顺带开放项。

## 2. 实施约束

- typed计划必须区分numeric owned value delivery与Sequential/RangeView Borrow交付；
  真实source/endpoint/binding身份、trial、排序/validate及final admission两入口一致。
- ownership明确快照，不创建元素place或源owner loan，不用虚拟LoanTarget凑既有接口。
  原容器/View借用算法保留，选普通source精确近邻回归。
- SSA沿0328注册Range身份提取first/last/empty；不使用借用provider的LoanId与Int length。
  empty→exit；body后normal/continue先清理，再current==last→exit，否则current+1→body。
  break不递增；所有回边使用真实重绑定/owner身份。LLVM加法须受not-last边支配。
- 不因类型成功冒称ownership/native完成；非法/未完成路径继续结构化拒绝，不能绕final gate。

## 3. 六组普通源码验收矩阵

1. Int/Long×闭/半开×named/call/field来源：source只求一次，精确provider/endpoint/Value binding。
2. empty/reversed/singleton/二三元素次序、discard及嵌套；双入口SSA/LLVM与独立native输出。
3. source打印一次、循环内重赋原Range而仍走初始快照、循环后原值可用。
4. Resource normal/continue/break/return恰一次清理；混合容器/Range内外循环保留外层loan。
5. 两宽度MAX singleton、MAX−1..MAX、MIN..<MIN、MAX..<MAX、全域首轮break/return；
   终轮continue绝不溢出，全域测试有界而不枚举。
6. nullable/nonprovider/整数解构/body真实类型错误/Deferred，与Array/List/View普通近邻不退化。

首批只做三项前端普通exact：来源身份与Value交付、普通退出body类型、provider拒绝/旧借用对照。
完整收集single与unit的names/raw/complete及实际计划后再断言；不引用尚不存在Rust enum变体。
它们不证明source运行次数、snapshot隔离或drop/native。其余矩阵按实现层次继续补齐，不能据首批归档。
清理轨迹先核既有ASAP时点；普通Resource变量未使用时不得猜测它必活到循环末尾。

## 4. 验收与交付顺序

测试先红→FE/ownership→SSA/verifier→LLVM→有限native→共享回归/四工程/actual-base size。
所有新helper与精确argv先独审；本地仅普通source，synthetic非法facts/故障校准留CI。
新增integration target同步精确composition数量与唯一性，不宽跑aggregate。
小提交binary/full-index format-patch、真实远端基线恢复逐树核验；不改Guide，不rebase既有历史。
实现CI、归档head CI、normal merge/main证据分账。初始草案时尚未运行本片测试或实施生产；首红实跑见§5。

## 5. 首批真实前端红证

`7bbff35d` / tree `dc921f83` 三exact各1命中，12输入每轮前后同：两行为红、一对照绿。
来源16源码×双入口与退出10源码×双入口全部已具Range类型，但raw/complete为L0159、计划缺失；
不存在names/parse前置失败，尚未到ownership。Boolean/nullable三源码双入口精确L0159拒绝，
Array/List/MutableList/View四源码双入口保留Borrow计划并complete通过。
源码观测并不证明snapshot/运行次数/退出清理；完整六组矩阵仍须后继层次验收。
冻结前cargo check一次E0608为helper公开table索引误用、0测试执行；改get后预检通过，旧日志保留。
仓外red3收据SHA `7f5d09007961d35c0af974dcaa4b159b92db268d52fdfe6483e3b55a3ba5803c`。

## 6. FE / ownership 阶段

Numeric provider 按真实 IntRange/LongRange 身份交付 owned Int/Long，source 以 Read 形成
OwnedSnapshot；借用 provider 保持 BorrowedSource，数值计划不建立 source/element loan。
各退出继续复用局部清理和词法 element/provider 边界，但数值计划没有 EndSource。
本阶段后端对真实 numeric descriptor 明确 Unsupported；SSA/native 尚未交付。
公开 source 查询变为可选借用目标，旧消费者和对应断言仅同步 Some，原借用合同保持。
冻结前有限 all-targets check 暴露 Option API 迁移、测试 validate 消费及 LoweringError 字段误用，
均为编译预检失败、0 测试执行；修正后的同范围预检通过。行为验收须另按固定名单实跑。

`44e59d62` 六项普通 exact 各 1 通过，55 输入前后相同；阶段收据 SHA
`20b0ab5b6a6006c245687abf9b5a7caee4ed40e9fca544b65ca9d2e172deaf28`。
覆盖双入口 Value/source 身份、owned item/原变量重赋、外层容器 loan 保留及 lower 明确拒绝；
不等于精确 Resource/ASAP 清理或 native 快照已验。后续加强计划数量、endpoint、所有退出无
EndSource 及 for primary span，并追加旧 Borrow provider 的普通共享回归。

### 6.1 旧 p34 门槛后继责任

以下旧 exact 仍冻结 p34 的 L0159，阶段备份尚未更新，不据本阶段开完成 PR：

- frontend / `final_type_admission`：`frozen_probe_roots_preserve_raw_deferred_recovery`、
  `complete::frozen_roots_reject_with_exact_source_diagnostics`。
- CLI / `native_cli`：`final_type_admission::single_probe_roots_reject_before_codegen`、
  `final_type_admission::single_deferred_user_message_build_preserves_source_root`。
- CLI / `project_cli`：`final_type_admission::project_probe_roots_reject_before_codegen`、
  `final_type_admission::project_deferred_user_message_build_preserves_source_root`、
  `final_type_admission::project_const_fallback_keeps_type_admission`。

原 fixture 字节保留。本阶段应为 FE 成功、numeric for lower Unsupported；后端完成时将 p34
转为有界成功正例，并以独立非法 provider 保留拒绝控制。`call_to_deferred.ko` 的 L0165
文案合同继续独立保留。含 run 的旧聚合 helper 不在本地阶段名单，后续需先拆审精确调用。

`63e85f07` 严格 10 exact 各 1 通过，63 输入前后同，收据 SHA
`03b44dc3daf2b66122d7199428adfd82126a625583d5ebb89d298dfabee2368f`。
新增四项仅验证旧 typed/ownership 借用计划及清理事实，不替代 final admission 或 native。
同 head fmt/check 通过；clippy 在 single validator 的等价布尔表达式触发 nonminimal_bool，
build 因遇错停未运行。保留失败，机械化简后同名单与四工程重新验收。

`147d1ba3` / tree `945cf016` 同 10 exact 再次各 1 通过，63 输入每轮前后同；
最终阶段收据 SHA `850b35cd00697eaff93c40d685d8a160990f922d707c65e985cfcedab1d84af0`。
同 head fmt、workspace/all-targets check、frontend/codegen/cli clippy、CLI build 全部通过；
actual-main `3fa567aa` size 0 errors，docs 700 通过。synthetic 仅编译，未本地执行。
阶段 backup 基线为 local `ef756580` 对应真实 remote `2987218d`；仍 active，等待数值 CFG、
极值安全递增、精确退出清理与有界 native。旧 p34 七项后继责任见上节，未以局部绿宣称全 CI。

## 7. 数值 CFG 首批普通源码采证计划

后端首三 exact 复用真实完整前端/ownership 的分阶段 helper：16 个来源/极值退出源码、
6 个 source call/重赋/返回源码、12 个 Resource 四出口及内外借用嵌套源码，各跑双入口。
首红先收齐可达阶段再要求 SSA/verify；第三组仅请求内存 LLVM，不 emit/JIT/run 产物。
当前失败须区分 names/type/ownership 与 lower，不将前置拒绝冒充后端缺口。
初始 oracle 只定位可达性缺口；实现验收前再锁 source call 身份/次数、Value 快照运输、
terminal 支配 increment、四退出真实清理事实，native 数值/轨迹另行固定独审。

`c897d7a3` 首三后端 exact 各 1 真实行为红，69 输入前后相同：32/12/24 共 68 观察
全部通过 names/type/ownership，停在 numeric for lower 的 UnsupportedNode；未到 LLVM。
不存在编译/parse/harness 前置失败。仓外收据 SHA
`bfc40e56bfd952024796237c27cad23426cc5db6095d573a50b8691aaaa6e74e`，独审核同。

候选实现采用两入口独立 numeric Value 状态：source 一次求值并投影 first/last/empty；
空区间直接 exhaustion，正常/continue 先消费前端 cleanup，再比较 current 与 last，
只有非终值边递增。unit 复用 pending/carry/rebind 保存两端点，single 以 body 参数保存 cursor；
借用 provider 仍走原 loan 路径。此候选 codegen all-targets 编译预检通过，行为验收尚待同三项实跑。
