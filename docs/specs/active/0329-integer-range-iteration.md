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
