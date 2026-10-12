# SPEC-0300: single for 中 Map guarded owner 运输

> **性质**：已批准变更合同 · **状态**：in-progress · **读取时机**：准备单文件for外层Map owner运输时 · **唯一真源**：本页的有界候选范围；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0300` |
| 所属 Phase | Phase 3 cleanup事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[for生命周期](../../guide/12-collections-destructuring.md#373-source-loan退出清理与冲突) |
| 批准依据 | 2026-10-12在0297/PR78合并后开始本地实施；从actual main `003f1299f817f6c903f3f450c73ba32e222ce5ba`建立`fix/spec-0300` |
| 前置 Spec | [SPEC-0297](../../archive/specs/0297-map-key-return-cleanup.md)、[SPEC-0295](../../archive/specs/0295-map-argv-word-frequency.md)、[SPEC-0296](../../archive/specs/0296-owned-null-comparison-cleanup.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | single须证明既有IterationPhi/guarded drop与SSA owner carrier的精确对应 |
| 影响范围 | single for的外层具名Map owner携带及条件清理；unit作为回归对照 |
| 语言语义变更 | 否；不是开放Map迭代provider |

## 1. Goal 与重起草来源

普通单文件函数持有一个外层具名Map，在既有顺序容器for body中修改该Map，能够在零轮、
单轮和多轮正常耗尽路径运输唯一owner并按真实cleanup条件恰一次清理。Map不是该for的source。

2026-10-12原未发布草案丢失，本页按已定位事实重新起草，非逐字恢复。原普通源码边界与
审计见[历史摘要](../../archive/migrations/pr72-74-followup-audit.md)，不据旧检查数字声明本页已验收。

## 2. 已定位边界与范围

0296保留的普通源码测试`map_remove_single_for_guarded_owner_boundary_is_explicit`使用
`tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko`：外层`texts`为Named、
对应symbol的IterationPhi，非Resource、非本for source；现有constant-presence豁免不适用。
`lower_frontend/orchestrate.rs`的guarded-drop门以该fact.value_origin拒绝。测试明确没有
SaveOwnerSnapshot或closure capture/release动作；该拒绝也不是N1a borrow-binding gate。

首片消费上述已发布owner版本、进入/回边/耗尽输入及条件，建立可验证的SSA carrier；
不能按`texts`名称、Map调用文本或payload类型绕过全局门。未处理的snapshot、closure、
其他条件owner或有歧义来源继续明确拒绝，不放松verifier。

至少保留原两次remove同key的present/missing程序，并用正常Map操作对照区分owner运输与
0296判空清理；顺序容器source仍遵守现行provider合同。每个具体Map实例遵循既有
`is_resource_type`事实，payload的资源属性会参与递归分类，不能从Int/String例推出
Resource payload组合属于同一门或同一清理时机。首片仅覆盖已定位的非Resource
IterationPhi guarded owner；所借source与被修改的另一Map仍是不同root。

## 3. 非目标与后继退出

不做一般guarded cleanup、任意owner选择/snapshot、递归closure或循环全重构；不开放Map
作为for source、N1a更广CFG、ordinary Resource?工厂或新的nullable表示。不重做unit已支持路径。

首片以正常耗尽为有界Goal。`break`/`continue`及更广return组合登记为独立后继候选，
不是语言禁用；不能把耗尽成功当这些退出已闭合。后继须逐边证明最近loop身份、
零轮/多轮owner可用性、continue保留source/provider、break结束最近provider、return前缀交付
与外围owner清理，再用single/unit普通源码独立验收。未证明路径不得静默生成错误SSA。

## 4. 最小验收与交付

- [ ] 原普通源码先在single重现精确guarded fact/kind/Span；unit同源作为独立成功或边界对照记录。
- [ ] 零/一/多轮与重复删除missing路径的真实SSA/verifier、未改写LLVM及普通native输出；
  循环后仍合法使用的Map不提前清理，不存在零轮复活、回边丢owner或重复drop。
- [ ] 非Resource实例按既有String/MoveOnly存储支持建立最小正常例，清理责任与provider源分离；
  Resource payload组只核既有资源分类及保持边界，不据此扩大首片至资源owner运输。
- [ ] 0296具名/临时判空、已有Map操作和unit循环回归保持；未选snapshot/closure/退出边界不被广泛豁免。
- [ ] 从届时main独立激活本Spec，先红测再实现，并同步Architecture与实际验收；PR/CI另行闭环。

本地仅执行普通源码、真实SSA、未改写LLVM、普通native及静态边界验证；注入、校准、
sanitizer、reducer和聚合driver不在本地执行。stdout不冒充分配计数或完整安全结论。

## 5. 实施边界与唯一验收账本

复用现有linear binding carrier，核对statement/symbol、Header/Exit owner identity、
Entry/Fallthrough/Exhaustion输入及可用性条件；具名Map必须由typed事实证明非Resource且
不是provider source。只接受有唯一真实来源的静态presence证明，不建立一般条件owner表示。
所有guarded drop入口（orchestrate、普通drop、iteration Action::Drop）使用同一证明；
实际回边读取Map操作后的owner，耗尽先消费Header清理再按真实Exit可用性保留owner。
未证明的snapshot/closure、歧义来源及break/continue/return组合仍明确拒绝。

| 验收项 | 测试选择/输入 | 实际结果 |
|---|---|---|
| 激活基线 | main `003f1299f817f6c903f3f450c73ba32e222ce5ba`，tree `ded7b20b99879031df7fa1e1ae62734dbe8c7d4e` | 已核真实Git对象；main CI pending，另行跟踪 |
| 原fixture红测/同源unit | `cargo test --locked -p lang-codegen --lib map_loop_original -- --nocapture` | 1 single预期失败（UnsupportedNode，27..32），1同源unit SSA/LLVM通过；Header清理/Exit缺席facts已核 |
| 正常耗尽运输 | `cargo test --locked -p lang-codegen --lib map_loop_`；原fixture及`normal-exhaustion.ko` | 实现前红测4 single失败/4 unit通过；unit basic/constant native均使用未改写LLVM；包括0/1/3轮×后续使用/不再使用、remove及put/contains |
| 边界与回归 | snapshot/closure/退出、0296/0297、unit/provider | 待运行 |
| 静态质量/独立审阅 | fmt、codegen clippy、docs、尺寸、diff及另一审阅者 | 待运行 |
| 发布闭环 | PR/双宿主CI/归档/merge | 未执行；由授权发布流程另行处理 |
