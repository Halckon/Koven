# Koven Specs

> **性质**：变更合同索引 · **状态**：current · **读取时机**：计划、实施或验收一项具体变更时 · **唯一真源**：各 Spec 正文；本轮收尾清单与当前交付见本页，演进实施账本仅保留历史基线

Spec 定义一次可独立验证的 Goal；路线图或 proposal 不会自动批准 Spec。
文档归档只关闭各 Spec 的有界合同，不代表全部语言演进或候选能力完成。

## 当前入口

### Active

- [Active 合同](active/README.md)：当前 1 份。
- owned source单条件Continue与Return已分别由0312/0314有界验收；更宽控制流保持独立边界。
- [当前实现边界](../architecture/README.md)：Map 的代表场景、证据层级与未评估组合见 [Map 场景矩阵](../architecture/map-native-execution.md#编号场景与保留边界)。

### 本轮 M2B 实用化收尾

2026-10-14 从 actual-main `16b4464f4d012eb4813e264b480de4198520aeee`
（tree `45ab7de3c9dee1996a108ff9d3d1b748e7c84621`）冻结的三片现已完成有界交付。
最终整合输入为 actual-main `b43a475b46c677295254f77d640051dd8edae828`
（tree `57a237d13b1deeab0d5d82ff7bf84800828e2a29`）。以下只关闭本轮有限清单，
不表示广义 M2B、v1 标准库或全部 Guide 能力完成，也不自动批准后继语义或 Spec。

- [x] single 普通 Borrow 非泛型 value-class 只读方法：[0316](../archive/specs/0316-single-readonly-value-methods.md)已完成真实红测、有界修复、归档及 main 交付。首片限 owned Int/Boolean 结果；unit 直接字段由0309既有支持，不算新增能力。
- [x] 范围绑定函数内的 `&&`/`||`：[0317](../archive/specs/0317-range-normal-short-circuit.md)已完成同根词法 scope、已建立描述符后的单层正常短路，并与0316只读 receiver 联合验收；归档及 main 交付完成。
- [x] return operand 内 Map 最后使用：[0318](../archive/specs/0318-map-return-operand-last-use.md)已完成 temporary Array<Int>、单 Map、单条件 Return 与互补 Fallthrough、owned Int 的有界提前清理，保留原 CallReturn ASAP 点；归档及 main 交付完成。
- [x] 最终代表联合验收：上述精确 main 的 [CI38547506105](https://github.com/Halckon/Koven/actions/runs/38547506105) attempt 1 完整 success，reuse=false，15 jobs 实际成功、1 个 PR 专属 marker 合法 skipped；双宿主逐名555 codegen、50 CLI、22 frontend通过，双 preview 包摘要/manifest/成员与每宿主12条独立安装命令已核验。各入口及正反例范围仍以对应合同和下表为限。
- [x] 已知边界有明确去向：下表将当前拒绝与本轮延期分开记录；“已决定延期”只关闭范围决定，不表示功能已实现。未评估、未运行、失败、filtered、ignored 不计通过，代表场景不等于全部笛卡尔积。

联合验收包含 single、unit basic/constant 及0317适用的 provider 正逆序。0318的
[cardinality fixture](../../crates/lang-codegen/tests/fixtures/map-return-last-use-0318/cardinality.ko)
仅由[三个 native 测试入口](../../crates/lang-codegen/src/native/unit_map_operand_last_use_tests.rs)分别执行；
没有同 fixture 的公开 CLI 验收。50 CLI 是范围/只读方法代表，不向 Map fixture 或所有组合外推。
独立 SSA oracle 核 owner/loan 身份、清理次序和正常回边；stdout 不当动态释放计数。
0317三轮历史云验收误跑 counted 的[更正](../archive/specs/0317-range-normal-short-circuit.md#6-云验收范围更正2026-10-14)
已由[PR99](https://github.com/Halckon/Koven/pull/99)合入；原日志和原计数保留，不伪造扣减后验收。
0318云端555名单精确排除该1项，远端 CI 额外执行的 counted 单列，不混入555；macOS既有 LLDB ignored 不计通过。

#### 保留边界与去向

下列拒例均来自正常源码，测试锁定真实诊断及对应 Span；只描述列出的入口与组合。
这些实现扩展尚无新编号合同，须另立有界 Spec 后实施，不因本清单取得实施授权。

| 保留范围 | 当前拒绝点与测试 | 去向/理由 |
|---|---|---|
| 0316 single 的 own/Inout、泛型或普通 class receiver、非 Int/Boolean owned 结果 | [readonly_receiver.rs](../../crates/lang-codegen/src/ssa/lower_frontend/readonly_receiver.rs) `admitted`；[边界测试](../../crates/lang-codegen/src/ssa/single_readonly_method_boundaries.rs) `readonly_keeps_receiver_and_result_boundaries` 锁调用处 `UnsupportedNode` | 延期；不同 receiver/返回 ABI 不从本片继承 |
| 0316 single 的借用返回别名、字段/nullable/iteration receiver 与 this.String 字段操作 | 同一 receiver/view gate；同份边界测试的 borrow-result、grouped-field、nullable、iteration、String-field 各例锁调用或 `this.text` | 延期；需独立派生 loan/CFG 与字段合同，不能按同类型猜来源 |
| 0317 嵌套/hidden CFG、binding 前后、多根、新 binding、循环/lambda/control exit | [short_circuit.rs](../../crates/lang-codegen/src/ssa/borrow_result_support/short_circuit.rs) `allows`/`normal_tree`；[边界测试](../../crates/lang-codegen/src/ssa/range_lowering_tests/local_short_circuit_boundary.rs) `nested_and_hidden_cfg`、`scope_and_roots`、`loop_lambda_and_control_exit` 对应 `range_short_circuit_boundary_*` | 延期；当前仅既有同根词法 scope 内的单层正常表达式，不泛化 NLL/CFG |
| 0317 own MoveOnly/Inout 调用、receiver 效果、外层实参或 RHS 新 range metadata | 同份短路边界测试 `owning_and_inout_calls`、`receiver_effects`、`range_creation_and_aliases`、`outer_call_effects_and_new_metadata_prefix`；`call_allowed`/整树准入保持 `UnsupportedNode` | 延期；新条件交付/metadata 需单独证明。Nothing/借用结果非法例另锁 frontend `L0085`/`L0164`，不冒充 backend gate |
| 0318 新 ASAP 分支的 List、owned/Borrow Array、两个 Map 或 Boolean 返回 | [return_operand.rs](../../crates/lang-codegen/src/ssa/lower_frontend/map_loop_owner/return_operand.rs) 的 provider/source/type/phi 候选门；[边界测试](../../crates/lang-codegen/src/ssa/lower_frontend_map_operand_last_use_tests.rs) `extended_provider_and_result_shapes_are_rejected`、`two_maps_and_borrowed_array_are_rejected`（均带 `map_operand_last_use_` 前缀）锁 `UnsupportedNode(cache)` | 延期；仅限制新提前清理分支，不撤回旧普通 Return 已支持的 provider/多 Map 场景 |
| 0318 相同 guard 但 call 在 operand 外、operand 内 checked 算术 Abort | [carrier.rs](../../crates/lang-codegen/src/ssa/lower_frontend/map_loop_owner/carrier.rs) 活动 Return 身份及完整 start→drop→Return CFG；同份测试 `same_guard_outside_return_is_rejected` / `checked_operand_abort_path_is_rejected` 分别锁 `UnsupportedNode(cache)` / `MissingFact(cache)` | 延期；前者真实 P3 候选不能替代活动身份，后者 Abort 路径不满足全部后继到达 Return；不延迟 ASAP 或放松 gate |
| 直接 owned nullable remove 返回、独立 nullable factory、外围 Resource、多/混合退出 | [Return controls](../../crates/lang-codegen/src/ssa/lower_frontend_map_return_controls_tests.rs) 与上述 operand tests 的 `map_operand_last_use_no_map_nullable_control` 保留独立拒例；分别锁 cache、factory 声明或 outer 的原诊断 | 延期；前两者非一般 nullable 工厂/返回能力，Resource 去掉 Map 仍拒；各自需独立 owner/退出证明 |

更广处置仍从[Map矩阵](../architecture/map-native-execution.md#编号场景与保留边界)、
[范围能力门](../architecture/range-local-control-flow.md)与[普通借用后继](../architecture/synchronous-borrow-result-arguments.md)查询。
广义 CFG/NLL、Inout 来源、N1b、Option、consume、通用 Clone、条件/多来源及借用存储/逃逸不捆入本轮；
它们不是上述代表拒例的全域推论。需要 Guide 变化的候选须另形成版本并取得明确启用确认。
List/Map 尚未选定的高层基础 API、Map 遍历、forEach/map/filter 与 file IO 留后续 v1 独立里程碑，
尚未据此建立新 Spec；已有顺序迭代及 take/drop/dropLast 不重新列待做，原有 draft/candidate 状态不变。
后继A的[0319 最终类型消费准入](../archive/specs/0319-final-type-admission-diagnostics.md)已按实现head双宿主物理CI有界验收归档，并经归档head CI后合入main；
随后B已登记[0320 Enum case Borrow实参分类](active/0320-enum-case-borrow-categories.md)，已在实际0319 main上固定tests-only，尚待真实红测；C List MoveOnly转移追踪仍独立、未编号。single/project、basic/constant同源码差异矩阵仍需逐项建立，
不以[现存入口差异](../architecture/lowering-support.md#现存入口差异)或本轮有限收尾宣称这些修复已完成。

### 近期交付

- [0319 最终类型消费准入](../archive/specs/0319-final-type-admission-diagnostics.md)：[PR101实现CI](https://github.com/Halckon/Koven/actions/runs/38588845331/attempts/1)在实际head `5f12af1d` / base `18865460` 16/16 jobs成功、reuse=false；双宿主新final target各13/13，check/clippy/组合及双preview通过后有界归档。保留[首次CI](https://github.com/Halckon/Koven/actions/runs/38587442021)的103/104选择数失败及targeted skipped、本地诊断/常量回归/格式失败；[归档head CI](https://github.com/Halckon/Koven/actions/runs/38592462711)合法复用该实现证据后通过，PR101已合入实际main `a0f5f8ac`；[main CI](https://github.com/Halckon/Koven/actions/runs/38592817579)待终核。

- [0318 Map return operand 最后使用](../archive/specs/0318-map-return-operand-last-use.md)：[实现CI a2](https://github.com/Halckon/Koven/actions/runs/38541560041/attempts/2)在head `f89d466c` / base `878801a3` 双宿主555 codegen、50 CLI、22 frontend与preview验收通过；7前置继承a1、8项a2实际执行，a1基础设施取消保留；[归档CI](https://github.com/Halckon/Koven/actions/runs/38545555262)成功，[PR98](https://github.com/Halckon/Koven/pull/98)合为main `10e41fb6`。其[main CI38546079533](https://github.com/Halckon/Koven/actions/runs/38546079533)仍为 cancelled：15个实际 jobs 成功，仅 PR marker 占位被 main 并发取消；后续 `b43a475b` 的完整 success 见上方联合验收，不倒写前一 run。

- [0317 同根范围正常短路](../archive/specs/0317-range-normal-short-circuit.md)：[新base物理CI](https://github.com/Halckon/Koven/actions/runs/38535052886)在base `3661d682` / head `5445f38b` 双宿主逐名429 codegen、50 CLI、22 frontend及preview独立安装通过，已按有界合同归档；[PR97](https://github.com/Halckon/Koven/pull/97)归档head已验收并合为main `878801a3`，[main CI](https://github.com/Halckon/Koven/actions/runs/38539127724)成功。旧baseCI38531927614仅保留历史证据。云端三轮 counted 误执行与[PR99更正](https://github.com/Halckon/Koven/pull/99)已随main `b43a475b`交付，不将原302/429整批称为合规普通云验收。

- [0316 single value-class 只读方法](../archive/specs/0316-single-readonly-value-methods.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38528485581)双宿主逐名核验本片52、共享70、CLI4与frontend22项通过；[PR96](https://github.com/Halckon/Koven/pull/96)按有界合同归档，原失败/receiver来源边界保留；已合为main `3661d682`，[main CI38532630912](https://github.com/Halckon/Koven/actions/runs/38532630912)成功。

以下0308–0315于2026-10-14按 GitHub 的 merged 状态、精确归档 head 与各自 merge SHA 的 run 重新核对。
归档 CI 与 actual-main push CI 均已成功；实现物理测试、归档复用和 main 执行仍是不同证据层。
这里只同步当前交付，不改写历史 Spec 中当时的失败、待验说明或未覆盖范围。

- [0315 范围绑定 Int 比较](../archive/specs/0315-range-int-comparisons.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38506286229)双宿主562项逐名通过，CLI漏选修正与首轮失败保留；[归档CI](https://github.com/Halckon/Koven/actions/runs/38508400029)成功，[PR94](https://github.com/Halckon/Koven/pull/94)合为main `16b4464f`，[main CI](https://github.com/Halckon/Koven/actions/runs/38513548543)成功。

- [0314 owned Sequential source 条件 return](../archive/specs/0314-owned-source-conditional-return.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38501870863)双宿主117新增与241共享去重目标逐名通过（交集27），独立preview安装链通过后有界归档；[归档CI](https://github.com/Halckon/Koven/actions/runs/38503977044)成功，[PR93](https://github.com/Halckon/Koven/pull/93)合为main `cedbe73e`，[main CI](https://github.com/Halckon/Koven/actions/runs/38505478388)成功。

- [0313 single Map 条件 return](../archive/specs/0313-single-map-conditional-return.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38483423039)双宿主230联合目标逐名通过后有界归档；Return无phi incoming，owned-source Return旧拒由0314独立修复；[归档CI](https://github.com/Halckon/Koven/actions/runs/38485667518)成功，[PR92](https://github.com/Halckon/Koven/pull/92)合为main `934b6590`，[main CI](https://github.com/Halckon/Koven/actions/runs/38487002306)成功。

- [0312 owned Sequential source 条件 continue](../archive/specs/0312-owned-source-conditional-continue.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38433956651)双宿主32项新增及3项改名旧回归有界验收，保留单owner单loop、fresh独立实例及原失败边界；[归档CI](https://github.com/Halckon/Koven/actions/runs/38437528988)成功，[PR91](https://github.com/Halckon/Koven/pull/91)合为main `51f30090`，[main CI](https://github.com/Halckon/Koven/actions/runs/38482002922)成功。

- [0311 single Map 条件 continue](../archive/specs/0311-single-map-conditional-continue.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38420920418)双宿主24项逐名有界验收，保留source范围修订、原红测与隔离纠正；[归档CI](https://github.com/Halckon/Koven/actions/runs/38425132312)成功，[PR90](https://github.com/Halckon/Koven/pull/90)合为main `8e0ce49d`，[main CI](https://github.com/Halckon/Koven/actions/runs/38426712468)成功。

- [0310 single Map 条件 break](../archive/specs/0310-single-map-conditional-break.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38407747517)双宿主13项逐名有界验收，保留unit dead与扩展退出/seed边界；[归档CI](https://github.com/Halckon/Koven/actions/runs/38411919960)成功，[PR89](https://github.com/Halckon/Koven/pull/89)合为main `614a7a7f`，[main CI](https://github.com/Halckon/Koven/actions/runs/38412890083)成功。

- [0309 普通 Borrow value-class 字段](../archive/specs/0309-borrow-value-class-fields.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38397786167)双宿主有界验收，保留字段读取、同步child loan与相邻拒绝边界；[归档CI](https://github.com/Halckon/Koven/actions/runs/38401678053)成功，[PR88](https://github.com/Halckon/Koven/pull/88)合为main `162e8e5d`，[main CI](https://github.com/Halckon/Koven/actions/runs/38402505848)成功。

- [0308 普通泛型 String 回归](../archive/specs/0308-generic-string-consumer-regression.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38390525995)双宿主有界验收，生产零改、本地stub误执行纠正保留；[归档CI](https://github.com/Halckon/Koven/actions/runs/38394071178)成功，[PR87](https://github.com/Halckon/Koven/pull/87)合为main `cf7f8ede`，[main CI](https://github.com/Halckon/Koven/actions/runs/38396607970)成功。

- [0307 同步 Borrow 实参消费](../archive/specs/0307-synchronous-borrow-result-arguments.md)：[PR85](https://github.com/Halckon/Koven/pull/85) 已有界归档并合入 main `557e3684`；[actual-main CI](https://github.com/Halckon/Koven/actions/runs/38382922981) 成功，首轮失败保留。
- [0306 混合 Value 借用返回](../archive/specs/0306-mixed-value-borrow-result.md)：[PR84](https://github.com/Halckon/Koven/pull/84) 已合入 main `967bf98e`，[actual-main CI](https://github.com/Halckon/Koven/actions/runs/38368833991) 成功；首轮失败保留，M2B 后继开放。
- [0305 多 Borrow 参数](../archive/specs/0305-multi-parameter-borrow-result.md)、[0304 库存声明](../archive/specs/0304-spec-archive-inventory.md)：已完成各自有界交付；历史追溯从 [Archive](../archive/README.md) 进入。
- 0297/0300/0299/0298 首片均已合入并通过各自 actual-main CI：[PR78](https://github.com/Halckon/Koven/pull/78) / [38228159089](https://github.com/Halckon/Koven/actions/runs/38228159089)、[PR79](https://github.com/Halckon/Koven/pull/79) / [38259510177](https://github.com/Halckon/Koven/actions/runs/38259510177)、[PR80](https://github.com/Halckon/Koven/pull/80) / [38275520368](https://github.com/Halckon/Koven/actions/runs/38275520368)、[PR81](https://github.com/Halckon/Koven/pull/81) / [38327500616](https://github.com/Halckon/Koven/actions/runs/38327500616)。广义范围未随首片关闭。

### 候选与导航

- [v0.43 广义后继](drafts/v0.43/README.md#0298首片之外的开放后继)：0298 更广表达式/控制流/循环仍为候选；0292 consume 保持原 draft。
- [v0.44 候选](drafts/v0.44/README.md)：0291 只读 Map 消费式转换保持原 draft；[Proposals](../proposals/README.md) 不自动批准实施，P2 预算继续延期。
- [演进历史基线](evolution-status.md) / [工程方法](../development/README.md)：前者保留历史检查点，当前交付只在本节维护。
- [冻结库存](inventory.md)：active/archive 的独立声明与漏项检测，不由目录扫描自动生成。
- [依赖图](dependency-graph.md) / [SVG](dependency-graph.svg)：由 `scripts/gen_spec_dag.py` 生成拓扑，不承载验收状态。

## 版本与历史导航

- [v0.43 首片与后继合同](drafts/v0.43/README.md)
- [v0.44 候选合同](drafts/v0.44/README.md)

- [v0.40](drafts/v0.40/README.md)：v0.40 已继承的三项规则实施入口；v0.41引入并由现行版本继承的 move literal 澄清由 SPEC-0279 验收
- [v0.36](drafts/v0.36/README.md)：常量 Phase 2/3/4 已完成
- [v0.37](drafts/v0.37/README.md)：0179/0211/0212 已归档；0182本机补强证据按需从archive追溯
- [完成 Spec Archive](../archive/specs/README.md)：303 份 `done`/`superseded`，只在追溯时读取
- [Proposals](../proposals/README.md)：尚未启用的候选，不因本批归档取得规范地位

## 生命周期

```text
draft → approved → in-progress → done
  │                                  └→ archive/specs/
  └─ blocked 时保持 draft
```

- 编号在 active、drafts 和 archive 间全局唯一且不复用
- 前置 Spec 必须 `done`、前置 ADR 必须 `accepted`、语义 Guide 必须已启用，才能进入 active
- `done` 前逐条记录验收及实际命令；未执行、失败、filtered、ignored 与后继修复分别留证
- 状态迁移、路径迁移、索引与冻结 inventory 同批更新；原验收历史不重写
- 一个 Spec 只定义一个 Goal；长期架构理由写 ADR，完整语义链接 Guide

新建草案使用 [TEMPLATE.md](TEMPLATE.md)，放入 `drafts/` 或对应未启用版本子目录。
