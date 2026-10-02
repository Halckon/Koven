# Guide 示例验证与审计更正

> **性质**：当前实现事实与审计更正账本 · **状态**：current · **读取时机**：核对规范示例、PR #6 审计与实际前端覆盖时 · **唯一真源**：当前 Guide、代码与直接提取示例的测试

## 快照不能混用

- PR #6 合入的 main `3be83b5` 使用 Guide v0.38。审计原文是历史意见，不是实施结果；
  其中“未动”“不存在”等结论不能不经核查就套用到合并后的代码或当前 Guide。
- 整合基线 `9b83ab2` 使用 Guide v0.40，继承 main 文档及已批准切片；真实 v0.39 历史
  已在较早的 `ed0727f` 冻结。后到的 PR #6 不回写该快照。
- 本页记录 SPEC-0238 的更正及 SPEC-0240/0241 的当前前端/native 边界，不修改 PR #6 审计原文、既有历史
  Guide 或冻结验收结果。PR 发布、CI 与 main 合并状态分别核对，不能由本地门禁推断。

## 审计结论更正

| 主题 | 当前可核验事实 | 权威或证据 |
|---|---|---|
| `deinit`、双轨析构“不存在” | main `3be83b5` 的 Guide08/10 已有合同；当前 v0.40 进一步明确 receiver 和清理顺序。合同已存在不等于资源 lifetime/drop/native 已完成 | [成员规则](../guide/08-class-family-members.md#deinit-成员语法与资源析构契约)、[所有权](ownership.md) |
| 所有权原语“未动” | Guide10/13 的 replace/swap 已有可信 typed facts；owned mutable root 的 ownership/SSA/native 由 SPEC-0244 定向证明，field/index/Inout 参数与 closure 来源运输仍未交付 | [原语边界](root-ownership-primitives.md) |
| `RawPtr` 字段必然自动 Copyable | 当前标准环境不绑定 `RawPtr`，测试在名称阶段得到 L0080；不能从 unresolved 类型推导 Copyable，更不能把假定的 double-free 当作已复现事实 | `guide_litmus::raw_pointer_name_is_unresolved_not_proven_copyable`；[类型事实](names-and-types.md) |
| const 位运算 | Guide05 的六个具名中缀操作已由共享值内核完成；单文件/unit const 正向测试覆盖八种整数。Litmus12 保持原始 const 源码，并纳入两条 native 门禁 | [const 合同](../guide/05-declarations-callables.md#363-封闭-const-expression-与求值失败)；`guide_litmus` |
| `Box<enum>` 与投影/拆箱 | 递归布局/构造/运输的已有证据与 `.value`/`unbox` 的 staged 合同是不同范围；前者不证明后者可执行 | [构造规则](../guide/11-copyability-layout-construction.md#内建-box-身份与实参边界)、[SSA 实现](ssa-codegen-runtime.md) |
| 文档内语法/命名 | Counter mutator 标明 `inout fun`；replace/swap 使用声明端模式且返回类型为 `T`；enum payload 不写字段 `val`；Result 脱糖用 `success`，与 prelude 一致 | [Guide09](../guide/09-nullability-errors.md)、[Guide10](../guide/10-ownership-borrowing-drop.md)、[Guide11](../guide/11-copyability-layout-construction.md)、[Guide13](../guide/13-program-runtime-standard-library.md)、[Litmus](../guide/15-conformance-and-staging.md#规范性-litmus-程序集) |

## 调用借用的现有边界

嵌套参数求值 `accept(read(local), &local)` 已通过单文件与 unit ownership：read 的 shared
loan 结束于 `read(local)`，outer exclusive loan 结束于 `accept(...)`。新增测试分别断言两个
loan 的 call identity、种类和 end span。

交付 callee 的 Borrow 不因参数求值结束而结束：`worker.update(worker)` 在 inout receiver
与 Borrow 实参重叠时仍为 L0135。[SPEC-0243](../archive/specs/0243-receiver-two-phase-borrows.md)
已将两条前端路径改为 receiver reservation、正常 CallEntry activation；规范允许的
`worker.update(worker.read())` 已转为正向断言，普通 `&` 实参仍立即独占。新的定向 suite
另验证 this、字段/index overlap、控制转移、phase facts 与 borrow temporary 清理。
unit SSA/native 已覆盖 class/value class receiver 与源码顺序；single instance receiver
native 和 unit 非根字段/index native 仍不在已完成范围。不引入 NLL 或借用返回类型。

## 直接提取 Guide 的门禁

`crates/lang-frontend/tests/guide_litmus.rs` 通过 `include_str!` 读取当前 Guide15 的 12 个
Kotlin fenced blocks，核对数量、唯一连续编号及每节恰好一段源码。没有第二份可悄悄漂移的
Litmus fixtures；源文档变化会触发重新编译。Guide10/13 API 签名及 Guide11 enum 例子也直接读取。

每个 Litmus 在标准环境下分别运行单文件及 compilation-unit 名称、类型、常量验证和所有权
入口。诊断成功路径要求诊断为空、常量/ownership materialization 能力已发布、ownership deferred
为空；同时遍历实际 AST expression/type-ref，逐样例逐入口锁定 Deferred/Error/缺失的
源码节点与种类，没有任意 Deferred 豁免。known-gap 在确切
失败阶段停止并锁定有序 code、UTF-8 起止范围与源码切片。诊断新增、改变或意外消失都会使
门禁失败，不能通过 ignore/skip 隐藏；真正实现后须同时升级测试与本页状态。

| Guide15 样例 | 当前前端结果（两入口） | 尚不能据此宣称 |
|---|---|---|
| 2、6、9、10 | 诊断/ownership 检查通过；实际节点没有 Deferred/Error/缺失 typed fact | SSA/native 执行通过 |
| 1、3、5 | 诊断/ownership 检查通过；single 的赋值表达式仍为 Deferred(Assignment)，unit 对应节点已定型 | single typed 已闭合 |
| 7、8 | 诊断/ownership 检查通过；Resource/Node 构造目标的非值 expression 是 single Error / unit 无值类型；8 的 single 赋值还为 Deferred(Assignment) | 构造目标占位等于用户源码错误，或全部 typed 产物已闭合 |
| 11 | 诊断/ownership 检查通过；两入口 listOf callee 为 Deferred(Call)，single 赋值为 Deferred(Assignment)，unit for-body 还有 LoopSource/ControlJoin/Assignment | for-body unit typed 已闭合或 for native 已支持 |
| 4 | 原始 `return when` 两入口诊断/ownership 通过；Shape 非值 qualifier 保留 single Error / unit 无值类型 | compilation-unit enum expression condition 或直接 case 调用实参已 native 支持 |
| 12 | const/ownership 检查通过；BitMasks 非值 namespace 仍为 single Error / unit 无值类型；两 native 入口执行原始规范源码 | 其他 Litmus 或一般 constructor 占位已经闭合 |

`bash scripts/check_guide_litmus.sh` 是独立可复用门禁，运行文档结构检查、23个Guide suite
测试、两个相关ownership suite，以及位运算、inv与两阶段借用定向前端suite。十二Litmus的两入口
诊断/ownership检查均通过，return-when与const位运算已转为正向验证；typed节点仍按
上表锁定精确快照。派生源码每次替换都要求恰好命中一次，避免模板漂移后静默失去覆盖。

本门禁同时执行SPEC-0241的3个单文件native与3个SSA定向回归，以及SPEC-0240的
`guide_litmus_12`两条native入口，并执行 SPEC-0243 的 `receiver_two_phase` 定向 SSA/native。它不枚举其他未选前端套件，也不替代其他语言能力的
native验证。SPEC-0242已修复三个旧call-argument失败，五个multifile type既有失败仍单独
记录。历史门禁见[SPEC-0238](../archive/specs/0238-guide-litmus-gate.md)，增量范围与验收见
[SPEC-0240](../archive/specs/0240-integer-bitwise-execution.md)、SPEC-0241/0242。

## Return 控制表达式与 Litmus4 的 native 边界

SPEC-0241 的 native 测试直接提取 Guide15 Litmus4，原 `Shape`/`area` 函数不改写；仅追加
显式 `Shape` 局部变量调用 Circle(2)、Rectangle(4,5)、Point，验证 stdout 为 12、20、0。
括号变体保留为等价 parser/frontend 回归，不再作为未修原例的替代。

单文件 lowering 只把已验证的无 payload Copyable enum case condition 降为 tag 比较；
精确检查 construction/ownership identity 与 enum layout，不构造临时比较值。测试另覆盖
不同 enum 的同名 case 及相反 variant 顺序。通用 enum equality、payload/grouped case
condition 和 MoveOnly root 不在此切片内。unit 原例仍在 `Shape.Point -> 0` 返回
UnsupportedNode；single 的直接 case 调用实参仍保留 MissingFact 精确范围回归。
这些边界不通过忽略测试、增加预算或改写 Guide 消除；实际验收见
[SPEC-0241](../archive/specs/0241-return-control-operands.md)。
