# SPEC-0314: owned Sequential source 条件 return 证明

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 owned source 条件 return 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0314` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-14 用户对 owned-source return 下一里程碑明确继续实施 |
| 前置 Spec | [SPEC-0312](../../archive/specs/0312-owned-source-conditional-continue.md)、[SPEC-0313](../../archive/specs/0313-single-map-conditional-return.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 本地有界实现及验收通过；文档终审、PR完整CI及合并闭环待验证 |
| 影响范围 | 非 Resource owned Array/List/MutableList source 的单条件 return |
| 语言语义变更 | 否 |

## 1. 输入与目标

从官方远端读取 actual-main `934b65908a7e8a42b3b7d969130aa2c95b191703`、tree
`815f8621f02863278adbeb1be84e7a76c4f8049b`，核本地文件/refs及远端0314未占用，
新建独立 `feature/spec-0314`。旧仓外计划在本执行器缺失，本合同依据真实源码与本次授权
重新建立；不宣称恢复旧计划字节或历史本机日志。

支持三Sequential容器、本地声明或own参数，正常耗尽后source live/dead；先独立无Map，
再组合已合入0313的外部Map。unit basic/constant单独分类，不从single结果推断。

## 2. 有界证明

保留旧constant presence及Resource路径/global implies。新证明只消费完整frontend事实：
source root、非Resource、owned身份、唯一真实声明initializer或同symbol own参数Entry；
透明Group只用既有归一，禁止替换、alias、跨loop或猜同型seed。

Header使用真实Always Entry建基；唯一真实Fallthrough回边仅假设该Header presence，
保留其他selector。一个条件Return、一个Fallthrough、一个Exhaustion；Return没有Header/Exit
phi incoming。独立证明Return path与Header presence共同选择精确Drop，并核DropFact owner、
point、condition及frontend完整actions。清理沿已发布actions，不自行重排资源。
正常耗尽live Exit必须从已证Header运输；dead source在EndSource后Drop Header且Exit absent。
不删除continue gate硬放Return，不把Exit自证作为Header证明。

精确SSA验证实际seed/source loan/length/cursor/element cleanup、Return独立终结、最新Map owner
和source权限恢复；动态stdout只证明普通执行，不代替唯一消费或free count。

## 3. 验收与非目标

先合同及tests-only独立提交，再冻结旧生产红测；single三容器×local/own×live/dead，
无Map12格与Map12格分别报告。basic/constant×三容器×local/own×live/dead24格独立分类。
扩展覆盖0轮、never/first/last、Group、同型旁观owner、独立fresh-inner及拒绝边界。
共享调用方按静审后的普通源码目标定向回归；fmt/clippy/docs/尺寸、独立审查、真实CI闭环。

不扩Guide、frontend、SSA model/verifier、LLVM、一般Resource/global implies；多return、
break/continue混合、无fallthrough、跨loop/capture、RangeView等保持原能力。缺阶段事实或
语义未知立即停报。云不运行注入、校准、sanitizer runtime、bad IR、LLVM计数插桩、aggregate
或mock aggregate/full discover。baseline/current绝对独立Cargo target串行；日志/完整JSON
及source hash冻结留仓外，Spec只保留简短账本。

## 4. 唯一验收账本

| 验收项 | 输入与选择 | 实际结果 |
|---|---|---|
| 基线/合同 | §1 actual-main | 远端commit/tree与编号空闲已核；生产未改 |
| single无Map/Map矩阵 | tests-only `246e263b`，`owned_source_return_tests::` | 24项均红，frontend通过、精确UnsupportedNode keys；source前后hash相等 |
| unit basic/constant | 同tests-only输入/选择 | 24/24通过SSA/verifier/原LLVM，0 ignored；不是新增unit能力 |
| 独立最小实现 | `3d491f7b` → `4cea1ca6`；同48格 | 首次编译E0425（StatementId命名空间）失败并独立修复；4cea后48/48绿，生产此后未改 |
| 最终新增选择 | `3dad218de7847462aa1af23cff8198f31d6f9f8c`，`owned_source_return_` | 117/117通过，0 ignored；精确末尾下划线过滤器不包含旧controls；前后source hash相同 |
| 新增分解 | 同上 | single24、unit SSA24、single native矩阵24、边界/前端/inner结构12、共享Map结构新增3、载荷/权限native6、unit native24；不按复跑累计 |
| 精确SSA | single24及Group/同型decoy、fresh inner live/dead | 声明initializer或首个own参数seed，精确source loan、单快照、零cursor、唯一Fallthrough+1；Return无回边/Exit incoming，elementEnd→sourceEnd→精确source Drop；Map返回清理关联最新put而非seed/旁观owner |
| ordinary single native | 24独立可执行文件，每个9次work调用 | 三容器×local/own×live/dead×无Map/Map×0/1/3元素×never/first/last，共216条普通动态调用；factory每次一次；不证明free count |
| 返回与清理 | 三份owned-source fixture及各自SSA/native | 两body Resource逆声明顺序，borrowed String operand先求值，再element/source/Map清理；Copyable element、独立String/MoveOnly value-class含Group返回，返回值无额外Drop；双同型Map含remove与own source保持各自最新owner |
| 权限/元素/inner | 3dad最终选择 | 正常耗尽后source own交付、MutableList add/remove恢复；body move/替换/增删保持L0135，MoveOnly解构/discard普通native；fresh inner两外层值及early/never、source live/dead真实动态与精确结构通过 |
| 独立unit native | basic/constant×三容器×local/own×live/dead24项 | 24个可执行文件、各early/never两次调用；每项两种真实source-input顺序各发object并核稳定IR/object。unit生产零改 |
| seed与控制边界差分 | baseline `2496eb7b` 对current7c/最终3dad；10项11份源码 | 基线全UnsupportedNode keys；仅Group+decoy两份和fresh inner转绿。multiple return、混合continue、replacement、alias、Resource、outer含inner、same-source nested/连续loop保留精确keys拒绝 |
| 0312共享source/Resource | `88042123`，`owned_source_continue_` / `ssa::lower_frontend_tests::resource_deinit_tests::` | 32/15通过；旧Return对照按本片新增成功更新，其余原能力保持 |
| Map共享与返回operand | 同输入，`ssa::lower_frontend_tests::map_result_tests::` / `map_put_control_exit` | 94/20通过；94含原0313 owned-source8格转正，及新增3项结构；各选择有重叠，不相加声称唯一目标数 |
| 共享普通native | 同输入，`native::unit_tests::{map_loop_owner_tests,map_key_return_tests,owned_null_cleanup_tests,temporary_null_cleanup_tests}`分别执行 | 52/18/11/8通过；52包含本片24unit native。原版LLVM/object/link/run，无计数插桩；source前后hash相等 |
| 静态门禁 | 3dad；fmt、`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`、sizes、size-policy | 全通过；Python指定size-policy模块47通过。38项历史尺寸债保留；测试入口仅3行注册2007→2010有限例外获独立审阅，生产尺寸额度不变 |
| 独立代码审查 | 3dad及真实日志 | Return独立proof、Continue原body、117新增/共享实际计数及source hash无阻断；文档终审待完成 |
| 文档/远端门槛 | 本地最终账本与分支checkpoint | docs/diff待最终冻结复验；PR完整CI、归档、merge及actual-main CI尚未执行，不以分支备份替代 |

首轮offline在编译前缺inkwell缓存；随后`cargo fetch --locked`补齐原锁定依赖，
再于独立baseline工作树与绝对target执行48项。日志与完整source hash留仓外。


Cargo test共同前缀为 `cargo test --locked --offline -p lang-codegen --lib`。旧Return控制
在共享Map94项中逐格复跑，不属于117新增筛选。精确冻结提交、CWD、工具chain与绝对
baseline/current target身份、前后全source SHA、stdout/stderr均留仓外恢复包；Git仅保留
持续测试实际消费的fixture。Rust/Cargo1.96.0、LLVM/Clang21.1.8、Linux x86_64。

### 保留限制

- 旧仓外计划与前轮日志未在本执行器恢复；只使用官方Git对象和本轮新证据。
- tests-only依赖缓存失败、首生产E0425及独立修复历史均保留，不改写成首次即绿。
- 新helper只显式核FinishProvider→EndSource→Drop；完整EndElement及body/外围动作合同
  来自frontend已验证事实，执行仍沿完整actions。普通fixture另有精确SSA与native顺序证据。
- mixed break、无Fallthrough、field/Inout/RangeView等由原0312/0313边界回归与未改变的
  入口约束保留；不冒称本片已为每一种Return组合做新的逐形状实证。
- 不运行workspace/frontend全量、故障注入/校准/sanitizer runtime/bad IR/计数插桩或full
  discover。macOS未在本地运行；跨平台与合入门槛由后续真实PR/main CI验证。
