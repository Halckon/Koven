# Koven Specs

> **性质**：变更合同索引 · **状态**：current · **读取时机**：计划、实施或验收一项具体变更时 · **唯一真源**：各 Spec 正文；当前交付见本页，演进实施账本仅保留历史基线

Spec 定义一次可独立验证的 Goal；路线图或 proposal 不会自动批准 Spec。
文档归档只关闭各 Spec 的有界合同，不代表全部语言演进或候选能力完成。

## 当前入口

### Active

- [Active 合同](active/README.md)：当前 0 份；后继须从实际 main 独立冻结合同。
- [当前实现边界](../architecture/README.md)：Map 的代表场景、证据层级与未评估组合见 [Map 场景矩阵](../architecture/map-native-execution.md#编号场景与保留边界)。

### 近期交付

- [0310 single Map 条件 break](../archive/specs/0310-single-map-conditional-break.md)：[PR89 实现 CI](https://github.com/Halckon/Koven/actions/runs/38407747517)双宿主 13 项逐名验收后有界归档；保留 unit dead 与扩展退出/seed 边界，归档 head CI/merge/main 待验收。

- [0309 普通 Borrow value-class 字段](../archive/specs/0309-borrow-value-class-fields.md)：[PR88 新 base CI](https://github.com/Halckon/Koven/actions/runs/38397786167)双宿主有界验收归档；保留字段读取、同步 child loan 与相邻拒绝边界，归档 head CI/merge/main 待验收。

- [0308 普通泛型 String 回归](../archive/specs/0308-generic-string-consumer-regression.md)：[PR87 物理 CI](https://github.com/Halckon/Koven/actions/runs/38390525995) 双宿主验收后有界归档；生产零改，本地 stub 误执行纠正保留，归档 head CI/merge/main 待验收。

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
- [完成 Spec Archive](../archive/specs/README.md)：294 份 `done`/`superseded`，只在追溯时读取
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
