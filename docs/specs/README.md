# Koven Specs

> **性质**：变更合同索引 · **状态**：current · **读取时机**：计划、实施或验收一项具体变更时 · **唯一真源**：各 Spec 正文；本轮收尾清单与当前交付见本页，演进实施账本仅保留历史基线

Spec 定义一次可独立验证的 Goal；路线图或 proposal 不会自动批准 Spec。
文档归档只关闭各 Spec 的有界合同，不代表全部语言演进或候选能力完成。

## 当前入口

### Active

- [Active 合同](active/README.md)：当前 0 份。
- owned source单条件Continue与Return已分别由0312/0314有界验收；更宽控制流保持独立边界。
- [当前实现边界](../architecture/README.md)：Map 的代表场景、证据层级与未评估组合见 [Map 场景矩阵](../architecture/map-native-execution.md#编号场景与保留边界)。

### 本轮 M2B 实用化收尾

2026-10-14 从 actual-main `16b4464f4d012eb4813e264b480de4198520aeee`
（tree `45ab7de3c9dee1996a108ff9d3d1b748e7c84621`）冻结以下有界清单。
这是收尾范围与完成条件，不表示 M2B 全部能力完成，也不自动批准新语义或未建立的 Spec。

- [ ] single 普通 Borrow 非泛型 value-class 的只读方法：[0316](../archive/specs/0316-single-readonly-value-methods.md)已完成独立真实红测、有界修复及双宿主实现head CI，已按有界合同归档；归档head CI、合并与actual-main待核；首片限 owned Int/Boolean 结果，不扩通用 receiver 借用返回。unit 直接字段已由0309接通，不能再列为同一实现缺口。
- [ ] 范围绑定函数内的 `&&`/`||`：独立定位并冻结最小合同，核求值一次、未选后缀不执行与 root/loan/cleanup。最终收尾前必须落实独立 Spec 或本节的明确延期条目及理由；当前待选，不计实现。
- [ ] return operand 内更新 Map：与前项分别定位、冻结当前 owner 和终止路径清理合同；最终收尾前同样必须落实独立 Spec 或明确延期条目及理由。不能从0313/0314的条件 return 成功推定该组合已支持。
- [ ] 最终联合验收：在选定切片整合后的精确 SHA 上，覆盖 single、unit（含适用的 basic/constant 及 provider 输入顺序）、公开 CLI 的代表源码；逐项记录输入 SHA、入口、真实 fixture/测试名、正例与精确诊断负例、命令/CI链接及实际执行状态。分别核 owner/loan 身份、清理次序与正常执行，不把 stdout 当动态释放计数；CLI 不继承 codegen 入口能力。
- [ ] 边界逐项有去向：以[Map矩阵](../architecture/map-native-execution.md#编号场景与保留边界)、[范围能力门](../architecture/range-local-control-flow.md)及[普通借用后继](../architecture/synchronous-borrow-result-arguments.md)为入口，将已知边界逐项标明本轮关闭证据或独立后继/延期理由。未评估、未运行、失败、filtered、ignored 不计通过；代表场景不是全部笛卡尔积。

完成本轮须上述每项满足自己的完成条件；“已决定延期”只关闭范围决定，不表示功能已实现。
需要 Guide 变化的候选必须另行形成版本并取得明确启用确认，不在实现中补写语义。
广义 CFG/NLL、Inout 来源、N1b、Option、consume、通用 Clone、条件/多来源及借用存储/逃逸不捆入本轮。
List/Map 尚未选定的高层基础 API、Map 遍历、forEach/map/filter 与 file IO 放到后续 v1 独立里程碑；
已有顺序迭代及 take/drop/dropLast 不重新列待做，候选继续遵守现行 Guide 与各自前置。

### 近期交付

- [0316 single value-class 只读方法](../archive/specs/0316-single-readonly-value-methods.md)：[实现CI](https://github.com/Halckon/Koven/actions/runs/38528485581)双宿主逐名核验本片52、共享70、CLI4与frontend22项通过；[PR96](https://github.com/Halckon/Koven/pull/96)按有界合同归档，原失败/receiver来源边界保留；归档head CI、合并与actual-main待核。

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
- [完成 Spec Archive](../archive/specs/README.md)：300 份 `done`/`superseded`，只在追溯时读取
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
