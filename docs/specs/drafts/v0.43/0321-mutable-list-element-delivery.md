# SPEC-0321: single MutableList.add 元素所有权交付记账

> **性质**：有界修复草案 · **状态**：draft · **读取时机**：审查 add 元素交付与后继 CFG owner 运输时 · **唯一真源**：本合同；语言规则仍以现行 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0321` |
| 所属 Phase | Phase 4 消费既有 Phase 3 Value delivery；Phase 6 两入口对照 |
| 语言规范 | [集合元素交付](../../../guide/12-collections-destructuring.md)、[String owner](../../../guide/13-program-runtime-standard-library.md#string)、[所有权](../../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 原A→B→C顺序中的C“List MoveOnly追踪”；2026-10-14批准独立静态调查、预留0321及草案/测试候选；未批准生产实现或测试执行 |
| 前置 Spec | [SPEC-0320 Enum case Borrow分类](../../active/0320-enum-case-borrow-categories.md)，当前未合main；本片不得混入其PR102 |
| 分支基线 | `fix/spec-0321`依赖0320本地最终`da03e0f84f6f3fdd2d4a3572117a511dfa8ce608` / tree`f0abc5e03035f9911a26fa44a026bfc281c25f92`；远端0320`7a305bd7bcead62921827ca8a24eba5fed8504ef`同tree，尚未主干交付 |
| 官方编号核验 | 2026-10-14官方main`a0f5f8ac`全树最大0319；上述0320完整树最大0320；93分支及2个开放PR未见更新编号，保留0321；实施前重核并正常同步实际main |
| 语言语义变更 | 否；add已取得Value元素，Copyable复制、MoveOnly转移；不能放宽verifier或新增循环能力 |
| 阻塞项 | 设计独审、0320交付/实际main同步、tests-only与helper独审、逐名运行授权及真实红测均待；本片仅静态草案 |

## 1. 单一 Goal

single lowering提交`MutableList.add`后，不再把已经交给容器的MoveOnly元素owner留作caller的
binding或temporary并运入后继CFG；只移除真实已交付owner的记录，未交付和Copyable来源保持。
消费现有P3的精确call/argument/source事实，不凭名字、语法拼写、同类型或循环特判猜交付。
不新增drop来替代转移，不改运行时布局、扩容算法、Guide或SSA verifier所有权合同。

## 2. 原probe与证据边界

原`p38_list_of_strings.ko` SHA256为
`0a24d6b51b981f082fe2a1f7555202dcb722b809646787f196c5e8c50a550518`：

```koven
fun main(): Unit {
    var xs = mutableListOf<String>()
    xs.add("x"); xs.add("y")
    for (s in xs) { println(s) }
}
```

原review的`614a7a7f`及`b43a475b`矩阵均记录single `BE:InvalidSsa`、project `OK`。
原字节与矩阵保留仓外`review-input/latest/inspected/review-2026-10-13/`；这不是0320基线上的新运行。
本片尚未执行probe、测试或构建；具体当前首失败阶段、verifier错误及Span仍须真实基线确认。
“String元素追加”指`MutableList<String>.add`，不是String拼接或字符串append API。

## 3. 静态根因候选

- [single lower](../../../../crates/lang-codegen/src/ssa/lower_frontend.rs)按Temporary+MoveOnly登记`temporaries`；Group可为相同ValueId保留多个表达式key。
- [single append](../../../../crates/lang-codegen/src/ssa/lower_frontend/container/access.rs)仅取receiver/element、发`ContainerAppend`、更新list binding及消费drop；没有移除已交付element的binding/temporary。
- [verifier](../../../../crates/lang-codegen/src/ssa/verify_ownership.rs)的`ContainerAppend`消费元素；[carried slots](../../../../crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs)却遍历所有MoveOnly binding及temporary，[for入口](../../../../crates/lang-codegen/src/ssa/lower_frontend/loop_control/iteration.rs)据此构造header实参。
- 因而已消费元素仍可能跨for边运输；这是源码支持的强根因候选，不能代替真实baseline。
- [unit append](../../../../crates/lang-codegen/src/ssa/unit_lower/container.rs)已有`consume_container_delivery`：核唯一call/argument、category/copyability及真实root/Temporary source，再移除owner记录；本片以此事实合同对齐single，不能照抄source-qualified ID或删除整个temporary表。
- single的`forget_delivered_owners`已有按真实ValueId移除binding及temporary别名的窄机制；是否直接复用须在核对P3事实后决定。不得遗漏named root或只删除Group外层key。

## 4. 支持、待核与非目标

| 形状 | 当前证据与本片处理 |
|---|---|
| 原p38 String两次add后正常for | 历史single红/project绿；本片必保留同字节两入口，当前基线待跑 |
| 空列表、零/单/多次追加 | 既有容器语义；空列表与Copyable作为不误伤控制，不宣称已有single组合验收 |
| Copyable named元素 | 必须保持交付后原binding可用；旧single SSA测试仅Int追加无for |
| String literal/Group、MoveOnly named root | 同一已交付owner的temporary/绑定两种来源，纳入精确身份验证；当前结果待核 |
| Resource构造值与named root | 原unit native已验证追加后逆序deinit，无后继for；本片仅用已有合法class/deinit形状比较single/unit，普通stdout验证恰一次，不引入仪器 |
| 正常耗尽 | 原p38核心；容器按既有ASAP点清理，元素由容器拥有 |
| 既有无条件break/continue/return | 候选有界回归，先独立核源码前置与既有支持，再锁具体fixture；不能据此启用任意出口组合 |
| 条件/嵌套出口、复杂元素实参控制流 | 非目标；若需扩source-presence、CFG或提前退出合同，停止并独立报告 |
| insertAt及其他容器/Map操作 | 静态相邻疑点独立保留，不夹带修复，不把add验收外推 |

空列表不能制造元素drop；String literal虽可引用静态字节，其MoveOnly SSA义务仍成立。
Resource只核普通源码可观察的访问与deinit顺序，不以allocator计数或改写IR证明。
当前未确证的拒绝边界须按真实前置失败单列，不将它们改成新的预期成功，也不把旧拒例重命名成通过。

## 5. 最小普通源码测试候选

本节仅列候选；实际target、helper、exact名字、源码SHA与argv固定并独审后再逐名请求运行。
不得执行聚合测试、故障注入、校准、计数IR或自行扩大矩阵。

1. 原p38：分采parse/type/final/ownership/SSA/LLVM，再通过公开CLI single与project basic/constant
   build/run同源码；成功输出应为`x\ny\n`。原历史结果、当前真实红和unit已绿对照分别记账。
2. 同一简单for分别使用`xs.add(("x"))`及`val x = "x"; xs.add(x)`，锁Group别名和named-root。
   独立未交付String与Copyable Int在loop后仍可读，防按类型清空或误删其他owner。
3. 空String列表的for不得打印body标记；正常输出`empty-ok`。Copyable named元素add后仍可读。
   不通过统计总SSA指令数或内部计数替代合法性/行为断言。
4. 沿已有unit Resource源码形状使用两个元素，普通输出区分构造/访问/逆序deinit/函数末；
   加入后继正常for及有限既有出口对照。若先于append受原功能限制，单列且收窄，不修其他门禁。
5. 复用`ownership_containers::mutable_list_add_transfers_move_only_elements`的L0131及
   `mutable_list_add_conflicts_with_active_element_borrow`的L0135原oracle；已绿保护不计新增红。

SSA只消费正常源码产物与标准verifier，可断言真实owner/delivery/edge身份，不修改产物做反例。
若要新增unit constant或CLI覆盖，必须明示它是对照，不以unit绿替代single修复。

## 6. 验收与停止条件

- [x] 官方库存/编号、原p38字节与历史矩阵、single/unit静态链核对。
- [ ] 草案独审、冻结有界支持矩阵，确认正式实施授权。
- [ ] 正常同步0320实际main；tests-only及helper独审、逐名运行授权。
- [ ] 当前输入真实expected-positive红与已有绿/拒绝边界分采。
- [ ] 只对精确已交付owner作最小修复，普通源码回归及工程门禁。
- [ ] 独立PR双宿主物理CI、Architecture同步、验收归档及main交付。

0320依赖未合前不宣称main基线已固定；生产、构建、测试和远端CI均未执行。
若必须改Guide、放宽verifier、补通用循环能力或改无关容器操作，停止并请求独立决定。
