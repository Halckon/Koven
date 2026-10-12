# SPEC-0300: single for 中 Map guarded owner 运输

> **性质**：已批准变更合同 · **状态**：done · **读取时机**：准备单文件for外层Map owner运输时 · **唯一真源**：本页的有界交付范围；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0300` |
| 所属 Phase | Phase 3 cleanup事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide v0.43](../../guide/README.md)、[for生命周期](../../guide/12-collections-destructuring.md#373-source-loan退出清理与冲突) |
| 批准依据 | 2026-10-12在0297/PR78合并后开始本地实施；从actual main `003f1299f817f6c903f3f450c73ba32e222ce5ba`建立`fix/spec-0300` |
| 前置 Spec | [SPEC-0297](../../archive/specs/0297-map-key-return-cleanup.md)、[SPEC-0295](../../archive/specs/0295-map-argv-word-frequency.md)、[SPEC-0296](../../archive/specs/0296-owned-null-comparison-cleanup.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 本地证明与普通路径已验收；发布、双宿主CI与归档仍待闭环 |
| 影响范围 | single for的外层具名Map owner携带及条件清理；unit作为回归对照 |
| 语言语义变更 | 否；不是开放Map迭代provider |

## 1. Goal 与重起草来源

普通单文件函数持有一个外层具名Map，在既有顺序容器for body中修改该Map，能够在零轮、
单轮和多轮正常耗尽路径运输唯一owner并按真实cleanup条件恰一次清理。Map不是该for的source。

2026-10-12原未发布草案丢失，本页按已定位事实重新起草，非逐字恢复。原普通源码边界与
审计见[历史摘要](../../archive/migrations/pr72-74-followup-audit.md)，不据旧检查数字声明本页已验收。

## 2. 已定位边界与范围

基线0296的普通源码测试`map_remove_single_for_guarded_owner_boundary_is_explicit`使用
`tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko`：外层`texts`为Named、
对应symbol的IterationPhi，非Resource、非本for source；现有constant-presence豁免不适用。
基线`lower_frontend/orchestrate.rs`的guarded-drop门以该fact.value_origin拒绝。测试明确没有
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

首片以Array/List/MutableList正常耗尽为有界Goal。`break`/`continue`及更广return组合登记为独立后继候选，
不是语言禁用；不能把耗尽成功当这些退出已闭合。后继须逐边证明最近loop身份、
零轮/多轮owner可用性、continue保留source/provider、break结束最近provider、return前缀交付
与外围owner清理，再用single/unit普通源码独立验收。未证明路径不得静默生成错误SSA。

具体后继还包括：error()/Abort幸存条件与phi presence的合取（body内或loop后）；
同symbol连续/嵌套for形成的IterationPhi seed；普通返回Map调用的Expression seed；
RangeView source的独立short-range/result-source loan退出责任，以及一般snapshot/closure。
首片只接受Map构造或own参数作为真实seed。上述均有明确拒绝或原有回归，不是语言禁止；
不为验收忽略新增condition，不将原有Resource constant-presence路径计为新资源owner支持。

## 4. 最小验收与交付

- [x] 原普通源码先在single重现精确guarded fact/kind/Span；unit同源作为独立成功或边界对照记录。
- [x] 零/一/多轮与重复删除missing路径的真实SSA/verifier、未改写LLVM及普通native输出；
  循环后仍合法使用的Map不提前清理，不存在零轮复活、回边丢owner或重复drop。
- [x] 非Resource实例按既有String/MoveOnly存储支持建立最小正常例，清理责任与provider源分离；
  Resource payload组只核既有资源分类及保持边界，不据此扩大首片至资源owner运输。
- [x] 0296具名/临时判空、已有Map操作和unit循环回归保持；未选snapshot/closure/退出边界不被广泛豁免。
- [x] 从届时main独立激活本Spec，先红测再实现，并同步Architecture与实际验收；PR/CI另行闭环。

本地仅执行普通源码、真实SSA、未改写LLVM、普通native及静态边界验证；注入、校准、
sanitizer、reducer和聚合driver不在本地执行。stdout不冒充分配计数或完整安全结论。

## 5. 实施边界与唯一验收账本

复用现有linear binding carrier，核对statement/symbol、Header/Exit owner identity、
Entry/Fallthrough/Exhaustion输入及可用性条件；具名Map必须由typed事实证明非Resource且
不是provider source。只接受Map构造或own参数的唯一真实来源与静态presence证明，不建立一般条件owner表示。
独立记录实际SSA slot/ValueId，逐前驱验证种子、MapPut/MapRemove owner结果及block parameter
lineage；同symbol不是owner证明，缺binding或carrier必须报错。provider附加loan/length/cursor
在carried前缀之后，不改变Header/body/exhaustion的Map槽位。
所有guarded drop入口（orchestrate、普通drop、iteration Action::Drop）使用同一证明；
实际回边读取Map操作后的owner，耗尽先消费Header清理再按真实Exit可用性保留owner。
未证明的snapshot/closure、歧义来源及break/continue/return组合仍明确拒绝。

| 验收项 | 测试选择/输入 | 实际结果 |
|---|---|---|
| 激活基线 | main `003f1299f817f6c903f3f450c73ba32e222ce5ba`，tree `ded7b20b99879031df7fa1e1ae62734dbe8c7d4e` | 已核真实Git对象及双亲；激活时main CI pending，另行跟踪 |
| 原fixture红测/同源unit | `cargo test --locked -p lang-codegen --lib map_loop_original -- --nocapture` | 1 single预期失败（UnsupportedNode，27..32），1同源unit SSA/LLVM通过；Header清理/Exit缺席facts已核 |
| 初次矩阵红测 | `cargo test --locked -p lang-codegen --lib map_loop_`，实现前8项 | 4 single失败/4 unit通过；初版含error()额外guard，原字节保留为`guarded-abort-boundary.ko`，不将其记为后续正例 |
| 正常耗尽及精确边界 | 最终 `cargo test --locked -p lang-codegen --lib map_loop_` | 19通过；0/1/3轮×后续使用/不再使用、重复remove、put/contains、String/MoveOnly、own参数/双Map；single及unit basic/constant分别运行；7项测试为普通native，其余为真实SSA或静态边界 |
| SSA责任 | 上述真实source及未改写LLVM | Header drop/Exit Never与live Exit分开断言；每个实际Map一个终端drop，实际entry/backedge/drop lineage和所有权verifier均通过；stdout不等价于分配/释放计数 |
| provider边界红绿 | `single_map_loop_range_provider_remains_explicit` | 收紧前1项红测（意外接纳）；要求Sequential后通过；没有改变旧constant-presence范围 |
| Map/0297 SSA回归 | `ssa::lower_frontend_tests::map_result_tests` | 48通过（扩展最后2个静态边界前运行，二者已由最终19项覆盖） |
| 0297普通native | `native::unit_tests::map_key_return_tests` | 18通过 |
| 0296具名/临时普通native | `native::unit_tests::owned_null_cleanup_tests`、`native::unit_tests::temporary_null_cleanup_tests` | 11/8通过 |
| unit Map/provider | `native::unit_tests::map_word_frequency_tests`、`native::unit_tests::sequential_iteration` | 4/8通过；后者含原生正常路径、源表达式defined Abort和真实SSA长度快照 |
| single provider回归 | `native_sequential_for_tests.rs`顶层11个普通测试，逐项选择（不运行其注入子模块） | Array/List/MutableList、临时source、解构、嵌套、break/continue/return与Unicode共11通过；不据此扩大新guarded Map出口 |
| snapshot/closure负例 | `rejects_produced_guarded_cleanup_before_lowering_bindings`、`rejects_conditional_capture_instance_before_narrow_closure_lowering` | 各1通过；未豁免 |
| 静态质量 | fmt、codegen all-targets clippy、docs、checker、尺寸、diff | 全部通过；651 Markdown、checker 41项；Rust size 1041文件/38历史超限，未增加cap |
| 独立审阅 | 另一审阅者核全部入口、owner/condition/carrier唯一性、Header/Exit时序及seed/provider范围 | 代码与合同静态终审无阻塞；按建议补Sequential与capture/instance边界，普通红绿验证通过；审阅者未运行测试 |
| 发布闭环 | PR/双宿主CI/归档/merge | 未执行；由授权发布流程另行处理 |

验收环境为Rust 1.96.0、LLVM 21.1.8及独占共享Cargo target；未运行全量workspace或任何本地
注入、计数重写、calibration、sanitizer、reducer、aggregate driver。只编译的clippy不执行这些测试。
原始日志、payload、bundle和原始commit对象在仓外双份保留，仓内仅保留真实fixture和简要账本。

尺寸审阅：`lower_frontend.rs` 1481行、`native/unit_tests.rs` 1937行，各增加2行（记录字段/
模块注册），另一审阅者已审查，均低于既有1525/2030上限；未提高cap或宣称旧尺寸债清零。
新生产helper分别283/354行，orchestrate 995行。

## 6. PR79 实现验收与有界归档

§1–5保留本地实施检查点原文，包括当时未发布、未运行和待闭环的状态；本节追加后续事实，
不将历史观察改写为重跑。2026-10-12，PR79实现CI成功后，仅按正常耗尽Goal有界归档。

| 项目 | 精确输入与实际结果 |
|---|---|
| PR / 实现head | [PR79](https://github.com/Halckon/Koven/pull/79)，`e4839e9b8eee79c902d82c5c74a7a1f613ffc6c4` |
| tree / base | tree `8ae8c40c5c6ba7755aa4b33deb9421e00a4f8f5e`；base `003f1299f817f6c903f3f450c73ba32e222ce5ba` |
| CI输入 | [run38252847134](https://github.com/Halckon/Koven/actions/runs/38252847134)，attempt1，pull_request；synthetic `ac2be0f3e24de1e8cbf8449ccb37bbe95e84dcb7`的父为base/实现head，tree与实现head相同 |
| 实现CI终态 | 2026-10-12 13:19:18 UTC completed/success；15实体job成功，唯一Tree-sitter因`editors=false`合法skip；`reuse=false`，代码与preview实际运行 |
| Linux新增选择 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38252847134/job/114824878258)：19个新增测试逐名各一次ok，7普通native、6源码SSA正例、6静态边界；旧测`map_remove_single_for_guarded_owner_is_carried`也ok |
| macOS新增选择 | [Targeted Tests](https://github.com/Halckon/Koven/actions/runs/38252847134/job/114824878418)：同19新+1旧逐名ok，无0300 ignored |
| 完整codegen目标 | Linux 1332通过/0失败/0ignored；macOS 1331通过/0失败/1旧ignored，唯一LLDB断点测试因CI缺debugserver task-port权限；不是0300跳过 |
| 其他门禁 | 直接依赖、变更检测、双宿主Check/严格Clippy、Docs/Fmt/尺寸均成功；[CI Passed](https://github.com/Halckon/Koven/actions/runs/38252847134/job/114830121283)及[Code CI Evidence](https://github.com/Halckon/Koven/actions/runs/38252847134/job/114830121295)独立确认必需jobs成功 |
| preview链 | 双宿主producer均针对上述实现head；Linux artifact `11748734876`与macOS `11749585824`分别由[Linux consumer](https://github.com/Halckon/Koven/actions/runs/38252847134/job/114829237685)/[macOS consumer](https://github.com/Halckon/Koven/actions/runs/38252847134/job/114829738195)实际下载、安装并通过 |

Code CI Evidence v1绑定上述PR/head/base/synthetic，代码输入摘要为
`12b0992971ffa9c8a3d2c2e8796311595d417d8ae3d2eaaf7f42af3b295bb3b1`。
macOS Targeted中的两条Linux-only sanitizer运行/保存步骤依平台条件合法跳过，
不构成macOS动态检测通过。普通stdout与SSA/verifier的唯一消费、drop时序责任分开；
远端通用检测通过也不自动成为0300专属分配计数或完整内存安全证明。

每宿主19项来自`check_integration.sh`→`check_core.sh`的完整lang-codegen目标，
本地99项定向回归存在SSA重叠，11项provider是另一次本地选择，不能相加为CI新增计数。
远端六提交采用实际发布metadata；本地终点`ae69eb409351fc3160465638a9db408b088a55dd`
与上述远端head的完整tree相同，本次归档直接续在真实远端head之后。
完整原始日志、发布映射、payload/bundle和CI收据保留仓外，不把原始JSON/log加入仓库。

§3全部未覆盖组合继续保留，Resource owner、一般snapshot/closure、额外Abort guard、
break/continue/return、连续/嵌套seed、返回Map调用seed、RangeView均不因归档扩大支持，
也不被改写为Guide禁令；0298/0299保持draft。
本次归档只移动合同、更新两处状态、索引/inventory和生成DAG，并追加本节；生产、测试、
fixture、Guide与原有实施正文保持原字节。本地归档只运行静态检查，不重跑Cargo或动态验证。
归档静态验收通过：651页文档、docs/DAG 42项、DAG重复生成稳定与暂存diff检查；
独立审阅核验CI身份、逐名命中、合法skip和原文保全，无阻塞。
最终归档head CI、merge与actual-main CI仍待另验，本收据不提前声称它们已通过。
