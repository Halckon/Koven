# SPEC-0321: single MutableList.add 元素所有权交付记账

> **性质**：有界修复合同 · **状态**：done · **读取时机**：审查 add 元素交付与后继 CFG owner 运输时 · **唯一真源**：本合同；语言规则仍以现行 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0321` |
| 所属 Phase | Phase 4 消费既有 Phase 3 Value delivery；Phase 6 两入口对照 |
| 语言规范 | [集合元素交付](../../guide/12-collections-destructuring.md)、[String owner](../../guide/13-program-runtime-standard-library.md#string)、[所有权](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 原A→B→C顺序中的C“List MoveOnly追踪”；2026-10-14草案独审通过，批准迁active及tests-only准备；后续已批准正常main同步及独审后的9 exact基线执行；后续最小生产已批准并经独审、固定9 exact验证 |
| 前置 Spec | [SPEC-0320 Enum case Borrow分类](../../archive/specs/0320-enum-case-borrow-categories.md)，已由PR102合入actual main `7c348e704ecd3595fa17ce4b08a2734d7d966e80`；本片独立交付 |
| 分支基线 | `fix/spec-0321`依赖0320本地最终`da03e0f84f6f3fdd2d4a3572117a511dfa8ce608` / tree`f0abc5e03035f9911a26fa44a026bfc281c25f92`；远端0320`7a305bd7bcead62921827ca8a24eba5fed8504ef`同tree；现正常merge实际main `7c348e704ecd3595fa17ce4b08a2734d7d966e80` / tree `910164ff461374f2c133a7f5ac4d938486c32de4`，保留真实父链 |
| 官方编号核验 | 2026-10-14官方main`a0f5f8ac`全树最大0319；上述0320完整树最大0320；93分支及2个开放PR未见更新编号，保留0321；实施前重核并正常同步实际main |
| 语言语义变更 | 否；add已取得Value元素，Copyable复制、MoveOnly转移；不能放宽verifier或新增循环能力 |
| 待交付 | 本片有限实现已按§11双宿主物理CI验收归档；归档head CI、PR105合并授权、实际main交付及main CI仍待 |

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
初始设计时本片尚未执行probe、测试或构建；后续真实首失败与修复结果见§7–9，未获得的具体verifier错误及Span不作推断。
“String元素追加”指`MutableList<String>.add`，不是String拼接或字符串append API。

## 3. 修复前基线的静态根因候选

- [single lower](../../../crates/lang-codegen/src/ssa/lower_frontend.rs)按Temporary+MoveOnly登记`temporaries`；Group可为相同ValueId保留多个表达式key。
- [single append](../../../crates/lang-codegen/src/ssa/lower_frontend/container/access.rs)仅取receiver/element、发`ContainerAppend`、更新list binding及消费drop；没有移除已交付element的binding/temporary。
- [verifier](../../../crates/lang-codegen/src/ssa/verify_ownership.rs)的`ContainerAppend`消费元素；[carried slots](../../../crates/lang-codegen/src/ssa/lower_frontend/control/carried.rs)却遍历所有MoveOnly binding及temporary，[for入口](../../../crates/lang-codegen/src/ssa/lower_frontend/loop_control/iteration.rs)据此构造header实参。
- 因而已消费元素仍可能跨for边运输；这是源码支持的强根因候选，不能代替真实baseline。
- [unit append](../../../crates/lang-codegen/src/ssa/unit_lower/container.rs)已有`consume_container_delivery`：核唯一call/argument、category/copyability及真实root/Temporary source，再移除owner记录；本片以此事实合同对齐single，不能照抄source-qualified ID或删除整个temporary表。
- single没有unit式公开`ValueDeliveryFact`表；须核typed append/operand身份、类别与已验证ownership，不能假称已有该表。已有`lower_value_argument`/`take_pending_value_argument`与`forget_delivered_owners`分别管理调用期交付及真实ValueId别名；是否复用须独审，不按语法猜所有权。

## 4. 支持、待核与非目标

| 形状 | 当前证据与本片处理 |
|---|---|
| 原p38 String两次add后正常for | 历史single红/project绿；本片必保留同字节两入口，真实基线和修复结果见§7–9 |
| 空列表、零/单/多次追加 | 既有容器语义；空列表与Copyable作为不误伤控制，不宣称已有single组合验收 |
| Copyable named元素 | 必须保持交付后原binding可用；旧single SSA测试仅Int追加无for |
| String literal/Group、MoveOnly named root | 同一已交付owner的temporary/绑定两种来源，纳入精确身份验证；真实三入口结果见§7–9 |
| Resource构造值与named root | 原unit native已验证追加后逆序deinit，无后继for；仅作为已有合法class/deinit普通stdout对照候选；未纳入最终矩阵，不宣称本片已验证deinit |
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

| package / target | 初始exact候选（后续结果见§7–9） |
|---|---|
| lang-codegen / lib；前缀`ssa::mutable_list_delivery_tests::` | `p38_original_append_then_for_has_verified_delivery`、`grouped_append_operand_has_verified_delivery`、`named_root_append_operand_has_verified_delivery`、`empty_list_iteration_remains_valid`、`copyable_append_preserves_original_binding`、`append_preserves_unrelated_live_owner` |
| lang-cli / native_cli；前缀`mutable_list_delivery::` | `single_p38_string_add_then_for_builds_and_runs` |
| lang-cli / project_cli；同前缀 | `project_basic_p38_string_add_then_for_builds_and_runs`、`project_constant_p38_string_add_then_for_builds_and_runs` |

18个codegen入口只验证分析/SSA/LLVM；只有原p38的3个CLI入口另有stdout行为oracle，二者分账。
上述名单的首轮真实结果见下节，初始tests-only准备时未编译、未运行。0320实际main正常同步后已重新固定
源码/argv与基线，并经helper/runner独审及root逐名授权完成首轮；原L0131/L0135测试与oracle未改。

## 7. 真实基线账本

2026-10-14固定HEAD `1a0ecab44dbd99b65891b8f87211a20ed284f674` / tree
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
AfterStatement drop消费入口，不增加功能。该词法块候选在`6753d2ad`经独审/逐名批准执行，single前端与operand身份通过后报
`InvalidSsa`、Span None，两unit SSA/LLVM通过，exact实际1命中。它避开独立Unsupported门，
保留预期成功红；不改写前两版结果。该检查点随后批准最小append交付登记生产修复；后续验证见§8–9。

## 8. 最小修复与固定回归

生产仅改`lower_container_append`：核typed call/element类型，resolve_type取得具体copyability；
成功ContainerAppend后仅对MoveOnly实际element ValueId移除binding/temporary别名。Copyable不清，
缺事实及Unknown/Error返回MissingFact；不改loan表、unit、verifier、CFG、insertAt或drop。
`daea8367`首次编译因Copyability枚举分支不完整报E0004，零测试执行；独立`dbb7c4a5`补齐
Unknown/Error fail-loud，保留首次失败，不作行为红。

2026-10-15固定HEAD `dbb7c4a54eaff91b64292e6a1d7a79922e7bf05b` / tree
`a81b2fc957a8be26a26ba5a2e51091abb867b638`经独审/批准按原9 exact串行运行，retained使用
词法块版。9项各实际1命中全绿；18个single/basic/constant分析、身份、SSA/LLVM入口全通过。
原p38三公开CLI的build/run/artifact均exit0；build输出空，run/artifact stdout精确`x\ny\n`、
stderr空，源码不变和临时产物清理oracle通过。执行前后14个输入SHA一致且工作树干净。
这不是全套或macOS验收；原两版retained跨loop Unsupported边界、依赖零执行与E0004均保留。

新增codegen模块由`ssa/mod.rs`的cfg(test)注册；两CLI模块由native_cli/project_cli注册。
现有CI双宿主test经`check_integration.sh`→`check_core.sh`完整选择lang-codegen及lang-cli，
会纳入新增目标；这里只确认接线，尚未执行本片远端CI。本地不运行该聚合脚本。

## 9. 最终本地有界验收

2026-10-15固定HEAD `048e127c1ce430e20377c2b18fedd9a53cfda5ee` / tree
`7f3ade699c930c4ed3c59a6c735b6867e5997eb4`逐名重跑原9加共享5个exact，14个唯一目标
均实际1命中通过；原9仍分别核18分析入口与3CLI行为，不把工程检查计入行为测试数。
共享5项为`ownership_containers`的`mutable_list_add_valid_ownership_succeeds`、
`mutable_list_add_transfers_move_only_elements`（L0131）、`mutable_list_add_conflicts_with_active_element_borrow`（L0135），
以及codegen `ssa::lower_frontend_tests::call_lifetimes_tests::grouped_temporary_has_one_owner_at_argument_merge`
和`ssa::lower_frontend_tests::map_result_tests::map_operands_follow_argument_control_flow`。
它们只执行普通源码/既有分析器；旧append指令计数目标不在本地名单。

同一head串行工程门禁全部exit0：
- `cargo fmt --all -- --check`
- `cargo check --locked --offline --workspace --all-targets`
- `cargo clippy --locked --offline -p lang-codegen -p lang-cli --all-targets -- -D warnings`
- `cargo build --locked --offline -p lang-cli`

此前`66441633`的fmt因两个cfg(test)模块排序失败，独立机械提交修正；原失败保留。
最终源码/fixture/helper SHA前后相同，完整stdout/stderr、argv、退出码与hash收据仓外保留。
当前文档提交不改上述已验证代码输入；docs checker与diff check另行执行，不冒用旧head执行记录。
Resource/deinit与break/continue/return新增矩阵未纳入本片，不能从String正常for外推；无关String
跨for仍保留原Unsupported边界。未运行本地聚合、故障注入或IR计数；macOS与双宿主完整CI待PR。

## 10. 同步0322实际main后的复核

2026-10-15从真实PR105 head `906803c5befc8fe0cc74c28b314c3655621b5437`正常merge
actual main `296579948e46d4a596eb6e72384a6489f82dbe08`，固定merge
`0aa77b30719abaadd55b4e143a8e0b58ea6c9533` / tree `d52c48d096bf7c9e5db1e461509eb6f92a26a3ce`，
两真实父链保留；未将旧本地等内容映射链重复merge。仅5份索引/DAG冲突按0321 active、0322 archive
机械解决；Rust生产、测试、helper与fixture相对PR105原head零变化，Cargo与preview脚本/CI来自main。

新lock正常`cargo fetch --locked`取得toml 1.1.8及其锁定依赖；固定上述merge重新逐名串行执行
同14 exact，全部各实际1命中通过。18个分析入口与3个原p38 CLI build/run/artifact仍分别全绿，
运行stdout精确`x\ny\n`、stderr空；§9同四条工程argv亦全exit0。执行前后登记源码/helper/fixture
及Cargo输入SHA相同、工作树干净，raw/argv/hash收据仓外保留。没有新增测试目标，也没有在云端
执行0322故障注入、校准或聚合脚本；不能把本地14绿代替新base双宿主完整CI。

PR105原[CI 38659703874](https://github.com/Halckon/Koven/actions/runs/38659703874/attempts/1)的
Ubuntu preview consumer环境闭包拒绝保持失败历史；不靠本次正常merge覆写旧结果。
含0322的新base远端CI等待本PR更新触发，Spec仍保持active。

## 11. 实现head双宿主CI与有限归档

2026-10-15 [PR105 CI 38692676541 / attempt 1](https://github.com/Halckon/Koven/actions/runs/38692676541/attempts/1)
终态success：15个job成功，Tree-sitter CLI Corpus按变更范围跳过；`reuse=false`，不将范围skip
计作执行通过。真实head为`895b9f508b5c29019d4301072dce258fea885cc0`，tree为
`64ffcd591ed20785cbf02bca1e7864b3a8a5936b`；实际synthetic merge为
`e0f86f5ab684337cbf760ed885cfe661505d74f4`，ordered parents为main `29657994`、head `895b9f50`，
tree与head相同。本归档提交直接继承真实head，不合并旧本地等内容映射链。

- Ubuntu与macOS物理测试日志逐名确认§6新增6个codegen、3个CLI目标均`ok`；每宿主18个
  single/basic/constant分析/身份/SSA/LLVM入口与3个原p38 CLI行为oracle分别通过。
  CLI仍核build/run/artifact及精确`x\ny\n`输出，不把codegen入口当作stdout验收。
- 两宿主独立preview consumer各执行12条真实命令，全部exit0、输出字节校验成功，
  cleanup.removed与sentinel.unchanged均true；manifest绑定上述head/tree，没有命令超时。
- Linux实际producer libxml2 `2.9.14+dfsg-1.3ubuntu3.10`到consumer `.9`差hash路径通过；
  两侧各61 probes成功、errors为空，无timeout/output limit。沿用0322固定两修订合同，
  不扩展ABI或来源认证；source_uri_status与installed_origin仍为unproven。
  不把本次结果宣称为反方向实跑，相关协议测试仅在CI执行。
- §7–10的真实红、独立Unsupported边界、编译零执行及旧CI环境失败全部保留。
  本片没有运行云端聚合、故障注入或校准，亦未新增Resource/deinit或出口矩阵。

下载ZIP与GitHub artifact digest核同的证据如下；原始日志/JSON仓外保留，不复制入库：

| Artifact ID | 内容 | SHA256 |
|---|---|---|
| 11953705463 | Linux producer evidence | `393080cb97501181616eead6781ff7380d40909441b1c8631fd0dec6942b3be4` |
| 11953705576 | Linux consumer evidence | `7bce89f8d280d3651dd99c7760cccadfdd75f548eb831626ac6180b035698045` |
| 11953002764 | macOS consumer evidence | `9eb1de5525a5efe61c3f3a4ccaea1cc6415d5d766604538c791959cb67949c33` |

GitHub记录artifact于2027-01-13过期；保留固定输入、摘要、digest与真实fixture，
不承诺过期后完整远端日志仍在线。Architecture已同步实际ValueId清理及Copyable/无关owner保留。
归档done仅表示本片有限实现验收完成；A最终类型准入、B Enum case Borrow分类、C本片三个核心
缺陷的有界闭合不表示整份review或v1全部完成。归档head CI、PR105合并授权、实际main交付及
main CI仍待，不将上述实现head记录冒用为这些后继步骤已通过。

## 12. 验收与停止条件

- [x] 官方库存/编号、原p38字节与历史矩阵、single/unit静态链核对。
- [x] 初始草案独审通过并批准迁active及tests-only；当时未授权生产与测试，后续基线授权/执行见上文。
- [x] 正常同步0320实际main；tests-only及helper/runner独审、逐名运行授权。
- [x] 当前输入真实expected-positive红与已有绿/独立拒绝边界分采。
- [x] 精确已交付owner最小修复、14唯一普通源码目标及工程四门禁通过。
- [x] 实现head独立PR双宿主物理CI、Architecture同步及有限验收归档。
- [ ] 归档head CI、PR105合并授权、实际main交付及main CI。

0320已合实际main并正常同步；0321固定基线及runner已独审并运行上述9 exact。最小生产、固定14 exact、工程四门禁与实现head双宿主CI已完成；本片有限归档，后继交付待项见上文。
若必须改Guide、放宽verifier、补通用循环能力或改无关容器操作，停止并请求独立决定。
