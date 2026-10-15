# SPEC-0321: single MutableList.add 元素所有权交付记账

> **性质**：有界修复合同 · **状态**：in-progress · **读取时机**：审查 add 元素交付与后继 CFG owner 运输时 · **唯一真源**：本合同；语言规则仍以现行 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0321` |
| 所属 Phase | Phase 4 消费既有 Phase 3 Value delivery；Phase 6 两入口对照 |
| 语言规范 | [集合元素交付](../../guide/12-collections-destructuring.md)、[String owner](../../guide/13-program-runtime-standard-library.md#string)、[所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 原A→B→C顺序中的C“List MoveOnly追踪”；2026-10-14草案独审通过，批准迁active及tests-only准备；后续已批准正常main同步及独审后的9 exact基线执行；生产实现仍待独审放行 |
| 前置 Spec | [SPEC-0320 Enum case Borrow分类](../../archive/specs/0320-enum-case-borrow-categories.md)，已由PR102合入actual main `7c348e704ecd3595fa17ce4b08a2734d7d966e80`；本片独立交付 |
| 分支基线 | `fix/spec-0321`依赖0320本地最终`da03e0f84f6f3fdd2d4a3572117a511dfa8ce608` / tree`f0abc5e03035f9911a26fa44a026bfc281c25f92`；远端0320`7a305bd7bcead62921827ca8a24eba5fed8504ef`同tree；现正常merge实际main `7c348e704ecd3595fa17ce4b08a2734d7d966e80` / tree `910164ff461374f2c133a7f5ac4d938486c32de4`，保留真实父链 |
| 官方编号核验 | 2026-10-14官方main`a0f5f8ac`全树最大0319；上述0320完整树最大0320；93分支及2个开放PR未见更新编号，保留0321；实施前重核并正常同步实际main |
| 语言语义变更 | 否；add已取得Value元素，Copyable复制、MoveOnly转移；不能放宽verifier或新增循环能力 |
| 阻塞项 | 设计独审已通过；0320已交付并正常同步实际main；9 exact真实基线已完成，生产最小修复及独立live owner候选仍待独审 |

## 1. 单一 Goal

single lowering提交`MutableList.add`后，不再把已经交给容器的MoveOnly元素owner留作caller的
binding或temporary并运入后继CFG；只移除真实已交付owner的记录，未交付和Copyable来源保持。
依据既有typed call/argument/source身份及P3验证，unit以公开delivery事实对照；single不虚构同名表。
不凭名字、语法拼写、同类型或循环特判猜交付。
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

- [single lower](../../../crates/lang-codegen/src/ssa/lower_frontend.rs)按Temporary+MoveOnly登记`temporaries`；Group可为相同ValueId保留多个表达式key。
- [single append](../../../crates/lang-codegen/src/ssa/lower_frontend/container/access.rs)仅取receiver/element、发`ContainerAppend`、更新list binding及消费drop；没有移除已交付element的binding/temporary。
- [verifier](../../../crates/lang-codegen/src/ssa/verify_ownership.rs)的`ContainerAppend`消费元素；[carried slots](../../../crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs)却遍历所有MoveOnly binding及temporary，[for入口](../../../crates/lang-codegen/src/ssa/lower_frontend/loop_control/iteration.rs)据此构造header实参。
- 因而已消费元素仍可能跨for边运输；这是源码支持的强根因候选，不能代替真实baseline。
- [unit append](../../../crates/lang-codegen/src/ssa/unit_lower/container.rs)已有`consume_container_delivery`：核唯一call/argument、category/copyability及真实root/Temporary source，再移除owner记录；本片以此事实合同对齐single，不能照抄source-qualified ID或删除整个temporary表。
- single没有unit式公开`ValueDeliveryFact`表；须核typed append/operand身份、类别与已验证ownership，不能假称已有该表。已有`lower_value_argument`/`take_pending_value_argument`与`forget_delivered_owners`分别管理调用期交付及真实ValueId别名；是否复用须独审，不按语法猜所有权。

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
Resource和出口未进入首批tests-only；当前未确证的拒绝边界须按真实前置失败单列，不将它们改成新的预期成功，也不把旧拒例重命名成通过。

## 5. 最小普通源码测试候选

本节仅列候选；实际target、helper、exact名字、源码SHA与argv固定并独审后再逐名请求运行。
不得执行聚合测试、故障注入、校准、计数IR或自行扩大矩阵。

1. 原p38：分采parse/type/final/ownership/SSA/LLVM，再通过公开CLI single与project basic/constant
   build/run同源码；成功输出应为`x\ny\n`。原历史结果、当前真实红和unit已绿对照分别记账。
2. 同一简单for分别使用`xs.add(("x"))`及`val x = "x"; xs.add(x)`，锁Group别名和named-root。
   独立未交付String在append后、loop前仍可读，Copyable Int在loop后仍可读，防按类型清空或误删其他owner；String跨loop原形状保留独立拒绝边界。
3. 空String列表的for不得打印body标记；正常输出`empty-ok`。Copyable named元素add后仍可读。
   不通过统计总SSA指令数或内部计数替代合法性/行为断言。
4. 沿已有unit Resource源码形状使用两个元素，普通输出区分构造/访问/逆序deinit/函数末；
   加入后继正常for及有限既有出口对照。若先于append受原功能限制，单列且收窄，不修其他门禁。
5. 复用`ownership_containers::mutable_list_add_transfers_move_only_elements`的L0131及
   `mutable_list_add_conflicts_with_active_element_borrow`的L0135原oracle；已绿保护不计新增红。

SSA只消费正常源码产物与标准verifier，可断言真实owner/delivery/edge身份，不修改产物做反例。
若要新增unit constant或CLI覆盖，必须明示它是对照，不以unit绿替代single修复。

## 6. 首批 tests-only 准备

首批6份普通源码fixture在`crates/lang-codegen/tests/fixtures/mutable-list-delivery-0321/`；
原p38逐字保留，另有Group、named-root、空列表、Copyable保留与独立live owner。没有Resource/出口扩展。
`ssa::mutable_list_delivery_tests`的每个目标先收齐single/basic/constant三入口结果再断言成功，
单入口前置或SSA失败均带阶段记录；unit锁真实call/operand delivery、源级Value引用Symbol与place.root同一及Copyability；single锁typed身份与ownership合法。
CLI三个目标独立记录原p38的build/run/普通artifact，constant marker另存文件，不改主源码字节。

| package / target | exact候选（均未执行） |
|---|---|
| lang-codegen / lib；前缀`ssa::mutable_list_delivery_tests::` | `p38_original_append_then_for_has_verified_delivery`、`grouped_append_operand_has_verified_delivery`、`named_root_append_operand_has_verified_delivery`、`empty_list_iteration_remains_valid`、`copyable_append_preserves_original_binding`、`append_preserves_unrelated_live_owner` |
| lang-cli / native_cli；前缀`mutable_list_delivery::` | `single_p38_string_add_then_for_builds_and_runs` |
| lang-cli / project_cli；同前缀 | `project_basic_p38_string_add_then_for_builds_and_runs`、`project_constant_p38_string_add_then_for_builds_and_runs` |

18个codegen入口只验证分析/SSA/LLVM；只有原p38的3个CLI入口另有stdout行为oracle，二者分账。
上述名单的首轮真实结果见下节，初始tests-only准备时未编译、未运行。0320实际main正常同步后已重新固定
源码/argv与基线，并经helper/runner独审及root逐名授权完成首轮；原L0131/L0135测试与oracle未改。

## 7. 真实基线账本

2026-10-15固定HEAD `1a0ecab44dbd99b65891b8f87211a20ed284f674` / tree
`ff861004984e4571ff481e76eb0111c7ded823d0`；正常双父为tests-only `ad53006d`与actual main
`7c348e70`。Linux Rust1.96.0 / LLVM21.1.8，9个上述exact各实际命中1条，串行使用
`cargo test --locked --offline -p PACKAGE TARGET EXACT -- --exact --nocapture --test-threads=1`。

| 验收项 | 实际结果 |
|---|---|
| p38、Group、named-root，各single/basic/constant | frontend与精确operand/delivery身份均通过；single SSA `InvalidSsa`且Span为None；两unit入口SSA/LLVM通过 |
| 空列表、Copyable，各三入口 | 6个分析/SSA/LLVM入口通过，不据此声称stdout验证 |
| 独立live String跨for，各三入口 | single SSA前置`UnsupportedNode`，SourceId0字节27..35为retained声明；两unit入口通过；这是独立既有边界，不算append根因红，不扩循环能力 |
| 原p38公开CLI single | build/run均exit2，报告InvalidModel/InvalidSsa；stdout空，artifact未运行 |
| 原p38公开CLI project basic/constant | build/run/artifact均exit0；build输出空，run/artifact stdout精确`x\ny\n`，stderr空；源码不变和临时产物清理oracle通过 |

合计18分析入口14绿、3个single InvalidSsa红、1个独立UnsupportedNode；9 exact为4绿5失败断言。
首次offline因缺inkwell在编译前exit101、零执行；另获准正常`cargo fetch --locked`成功后重跑，
原失败未覆盖。所有13个fixture/helper输入SHA前后不变，生产零改动。原始日志与清单仓外保留，
稳定输入可由上述固定Git提交及exact复现；该基线不是完整套件或macOS验收，也没有具体verifier错误/Span。

原`retained-owner.ko`基线字节已原样保留为`retained-owner-across-for.ko`；新候选
`retained-owner.ko`仅将`println(retained)`移到append后、for前，验证同类型未交付owner在append
时live且交付后仍可读，不宣称跨循环能力。该候选在`82ade427`经独审/授权单exact运行，single仍在retained声明27..35报
`UnsupportedNode`，两unit绿；仅提前last-use并未避开已有门，原字节另存为
`retained-owner-before-for.ko`。第二tests-only候选在xs声明之后使用普通词法块包住
retained声明、append和读取，块结束再for；parser已有Block statement、single已有
AfterStatement drop消费入口，不增加功能。该候选仍待独审/授权运行，不改写两版首失败。

## 8. 验收与停止条件

- [x] 官方库存/编号、原p38字节与历史矩阵、single/unit静态链核对。
- [x] 初始草案独审通过并批准迁active及tests-only；当时未授权生产与测试，后续基线授权/执行见上文。
- [x] 正常同步0320实际main；tests-only及helper/runner独审、逐名运行授权。
- [x] 当前输入真实expected-positive红与已有绿/独立拒绝边界分采。
- [ ] 只对精确已交付owner作最小修复，普通源码回归及工程门禁。
- [ ] 独立PR双宿主物理CI、Architecture同步、验收归档及main交付。

0320已合实际main并正常同步；0321固定基线及runner已独审并运行上述9 exact。生产修复与本片远端CI未执行。
若必须改Guide、放宽verifier、补通用循环能力或改无关容器操作，停止并请求独立决定。
