# SPEC-0276: Unit 普通泛型函数体的具体类型归一化

> **性质**：有界变更合同 · **状态**：in-progress · **读取时机**：补齐普通顶层泛型 body 的具体类型身份时 · **唯一真源**：本页

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-276` |
| 所属 Phase | Phase 2 发布具体类型身份；Phase 3 保持所有权事实；Phase 4 消费并执行 |
| 语言规范 | [Guide v0.40](../../guide/README.md)、[泛型](../../guide/03-types-generics.md)、[集合](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户持续实施里程碑、满足前置并行、按实际情况调整草稿的站立授权；有界cache及DAG/预算/recipe仲裁独立准备审阅已完成 |
| 前置 Spec | SPEC-0275 |
| 前置 ADR | ADR-0020 |
| 关联 ADR | [ADR-0028](../../adr/accepted/0028-frontend-generic-body-type-normalization.md)（accepted） |
| 阻塞项 | 无；G1–G7仍需fail-first及实际验收 |
| 影响范围 | unit body checker 私有类型归一化、共享实例上限、ordinary static callee storage角色、已有nullable handle的deinit可达性、直接 frontend/SSA/native 验收 |
| 语言语义变更 | 否 |

## 1. Goal 与启动证据

普通顶层泛型函数的 body 首次产生 `Array<T>`、`List<T>`、`MutableList<T>` 时，真实调用
可以获得 frontend 发布的具体类型身份，沿既有 owner/Borrow/size 合同执行到 native。
不依赖签名、其它未使用声明或 backend intern 补种类型；只复用已经提交的类型与调用事实。

基线为 PR53 合并后的 main `2be64066a2011bb07a31bd68f9ac7441ab5a4baf`，独立分支
`feature/spec-0276`。实现/最终归档/main CI、完整原始产物和 source 身份闭环见
[0275交付账本](../../development/evidence/generic-containers-0275-delivery.json)。
main CI37253329605已15/15终态成功；原main的6个未提交文件内容、状态和index已保全。
本页承接[M3A草稿](../../development/sequential-collections-spec-draft.md) C07，未宣称全部M3A完成。

## 2. 实际缺口与复用接口

0275已支持直接容器签名替换；普通 `probe<T>(own x:T)` 中 `val xs=listOf(x)`
仍只发布List<T>，实际probe(1)未产生List<Int>。既有0275测试明确核对canonical不存在、
backend保持arena长度及MissingFact实际construction Span；该历史红边界是本片要补齐的行为。

`BodyChecker::substitute_type`已按UnitSymbolId结构替换并在frontend私有arena intern，
`UnitCallDescriptor`已保存实际有序type_arguments和target；trial已完整回滚signatures/parts。
`materialize_runtime_field_layouts`消费新增concrete nominal identity。复用这些现有接口，
不重新运行AST body检查、overload选择、ownership或布局语义，不引入第三方依赖。

## 3. 支持范围与事实归属

1. 支持普通顶层source函数body及其普通顶层直接调用链，至少三层relay→middle→inner。
   实参有显式/推断、Int/String/具真实deinit的非泛型普通Resource；空与非空、多个body构造。
2. 所有body/trial结束且无signature/body错误后，使用已提交source call descriptor中实参
   全部闭合的普通顶层callable作为种子，包括非泛型deinit/member中的实际helper<Int>调用。
   未调用的probe<T>不自行猜Int；symbolic helper<T>不成为闭合种子。
3. 每个种子按source DeclarationId与完整有序实参建立确定性worklist；替换环境只取该
   callable的UnitSymbolId。无T的普通callee继续传播；visited防同key递归。
4. 根据source-qualified函数item Span关联已提交expression、symbol、TypeRef以及call/
   construction需求；物化其完整类型树，包含局部Nullable<T>。词法lambda归外层函数实例，
   不扩大既有closure ABI。模板facts保持原identity；仅canonical table追加归一化类型。
5. 归一化在已有field layout materialization之前、sealed typed发布之前完成；后端只find，
   不开放可变arena或公开“已完成body实例”capability。

## 4. 有界展开与诊断兼容

共享现有1024 specialized-instance上限，frontend每种子展开是有界类型cache；Phase4仍独立
选择实际entry可达图并在现有位置决定资源拒绝。不新增前端limit诊断或planner入口快速失败。
保持所有已有canonical ID；合法归一化前缀保留，不做全root rollback以免制造提前MissingFact。

当pop specialized且已达到上限时停止该实例的后继展开，对current及尚未planned的pending
直接call实参/receiver/return和body类型做一次有限物化，禁止传播新的实例key。
nongeneric key严格指完整实参为空、实际目标无类型参数的ordinary顶层函数，visited后只
展开一次且不增加specialized counter。counter到限本身不清空pending；仍按pop顺序处理
nongeneric，第一次pop尚未planned的specialized超限key才转有限frontier并结束该种子。
现有recipe collector后续有限symbolic固定点保持template fallback；不无界展开field recipes。
原planner key/计数/recipe优先级与完整kind/Span不变；已有later sibling非法recipe须仍优先于limit。
范围内如发现implicit runtime边使cache不足，先修正本合同并重审，不能吞MissingFact或加大预算求绿。

实际native红测另定位两项既有消费者缺口，纳入本片的最小适配：普通顶层call descriptor
已经选定的静态callee（含Group）不作为runtime function value登记storage，generic closure/
函数值原表示边界不变；已有ADR0017支持的Nullable<Resource>沿inner发布hidden deinit
可达性，复用现有NullableHandle/conditional drop，不引入inline nullable或新的清理语义。
这两项以原三层relay及Nullable<Resource>真实红测为直接证据，修改后需独立复审。

实例预算不是类型图工作量证明。新cache的闭合性与结构归一化、下游field-layout concreteness
按canonical UnitTypeId缓存已完成结果，区分active与completed；共享子图不按路径反复展开。
具体实参子图与深链使用迭代工作栈，不将1024层展开压入宿主递归栈；保留原结构替换/intern语义，
通过现有helper的最小适配复用，不增加第二套类型规则。测试用unique节点访问计数验证线性
共享DAG读取和深链完成，不用宽泛timeout代替。具体替换的memo作用域必须含该实例substitution，
禁止跨不同UnitSymbol实参环境复用错误结果。

## 5. 必需验收

| ID | 可验证完成标准 | 当前状态 |
|---|---|---|
| G1 | 三容器×Int/String/Resource，空/非空、显式/推断、多个body构造；native完整stdout和正常allocation/free逐指针/次数匹配，无隐式clone/retain | 本地frontend九组及native全矩阵通过；18个成功fixture真实object/link/run/逐指针计数 |
| G2 | 三层泛型调用；同名T跨文件身份隔离、重复实例去重、fresh反序输入稳定；原模板facts及sealed typed owner保持，backend前后arena不增 | 本地三层/跨文件身份/反序/重复calls通过；SSA body-only成功且arena不增；sealed view与编译契约8/8通过 |
| G3 | 非泛型Resource.deinit仅闭合调用helper<Int>，entry仅构造Resource；真实implicit清理和helper body-only容器成功 | 本地三容器真实implicit deinit→helper body通过，完整stdout/逐指针计数匹配 |
| G4 | body局部Nullable<T>归一化；Nullable<Resource>真实native，Nullable<Int>仅核canonical并保持既有inline representation拒绝；无具体调用不产生List<Int> | 本地三容器Nullable<Resource>真实native通过；generic Int canonical齐全且保持原拒绝/目标保全 |
| G5 | frontend私有小seam0/1/2核prefix/frontier与specialized计数；codegen沿原planner seam独立核pure limit及later sibling非法recipe完整kind/Span；production1024真实增长源码仅编译拒绝/目标保全，未选增长源码正常entry完成；共享DAG按unique节点访问数和深链验证 | 本地私有frontend/独立planner0/1/2与4096 DAG通过；production1024拒绝/两dest保全、无seed及未选闭合growth seed正常entry均通过；测试独立完整复审通过，零新增finding |
| G6 | no-match/ambiguous/lambda trial/最终expected-type错误不启动cache；恢复事实与既有诊断保持，不以错误unit arena总长不变替代隔离 | 本地6项isolation通过；multifile_type_checking整体130/130通过、0ignored |
| G7 | 后继支持不扩大0275 recursive-template/owner-recipe边界；相关frontend、unit plan/recipe/error-order/container/native及CLI/M1A/词频直接消费者通过；独立全审、Architecture、双宿主CI、最终归档CI、merge/main闭环 | 全部本地门禁及独立完整复审通过；CLI85/M1A12公共命令/词频37公共命令实际通过；双宿主与最终归档/merge/main在途 |

G1–G4必须使用真实body-only构造，源码中不得放入未使用的concrete容器声明补种。
先建立fail-first证据，再修改生产代码；只测find(List<Int>)不替代SSA/object/link/run。
frontend私有cfg(test) seam不从codegen依赖可见，两个0/1/2验收为分层证据；不伪称同一次
跨crate小预算流水线、不新增公开可变owner/test bridge。实际production1024验证跨阶段边界，
增长程序绝不执行native，不注入错误IR或内存故障。
现有历史body-only拒绝测试在本片变为真实成功合同，其其它缺substitution/递归模板保护保留。

## 6. 非目标

generic nominal owner/member/StaticSelf/inherited/delegated派生路线、任意递归模板lowering、
新集合API、runtime长度initializer、投影/Inout扩张、短路owner合流、single/unit全面合并、
M1B-b输入/分词、M4b真实故障校准、性能和公开Release不进入本片。语言允许而未实现的路线
继续登记独立缺口，不将后端拒绝提升为语言限制。

## 7. 实施与提交

1. 工程算法独立审阅、ADR前置与正式合同 → docs/inventory/DAG检查；有实质未决项保持draft。
2. 真实失败frontend/SSA/native矩阵 → root串行Cargo；测试可按独立文件委派，禁止争用target。
3. 最小私有归一化与共享上限 → G1–G6、共享/下游检查、独立完整审阅及修复再审。
4. 同步Architecture/真实账本并单一逻辑实现提交 → 精确head PR CI。
5. 完成验收后归档/inventory/DAG → 最终归档head全部必需CI成功才merge，再核actual main CI。

合同与实现分别提交，消息含SPEC-0276。新模块低于1000物理行，不提高旧尺寸baseline；
既有超限文件仅允许最小注册/hook，并按尺寸规则明确有限增长与审阅依据。
本地只选直接受影响frontend targets/filters，严格clippy/fmt/尺寸/docs及diff；不默认全量frontend。

## 8. 准备记录

2026-10-05：基线main CI及原始artifact已独立复核；正式0276 Rust矩阵未运行。
已否决全root canonical撤销和planner前early-limit候选，因其可能改变MissingFact/recipe优先级。
首次独立准备审阅指出field-layout concreteness只使用路径visiting、共享Function DAG可能
指数重复遍历；已纳入completed memo/迭代深链与确定性节点计数要求。候选源码尚未运行，
不得记作已复现。nongeneric边界与跨crate私有seam的证据层级已明确，修订窄复审无新增实质阻塞；
按用户站立授权批准0276与接受ADR0028，仅授权实施，不据此标记G1–G7通过。


## 9. 首轮frontend失败证据（2026-10-05）

合同/ADR与0275交付闭环已提交为`c0e5cbf`。生产代码仍未修改；新测试复用现有
`multifile_type_checking` helper并独立注册子模块（补身份断言后227行），没有另造harness。
实际命令：`cargo test --locked -p lang-frontend --test multifile_type_checking generic_body_types -- --nocapture`，
共享target串行、offline缓存。最终结果5项：1passed/4failed/0ignored/118filtered。
三容器分别实际完成Int/String/Resource类型检查与原template facts断言，再聚合报告
全部9组缺具体canonical；源码只有body内empty/full/again构造，无签名或unused seed。
三层relay→middle→inner在局部Nullable<Int> identity断言失败；未调用probe<T>不猜Int的
对照通过。临时原始日志为`/private/tmp/0276-frontend-body-red-reviewed.log`；
早期同选择在各loop第一Int处失败的日志另保留，不能将它误计为9组已命中。
上述是预期失败证据，G1–G7均未记完成；真实native矩阵仍在准备，未执行。


## 10. Native失败与测试审阅（2026-10-05）

复用既有native helper，以私有child注册354行模块；成功夹具要求真实object/link/run、
完整stdout、逐指针分配/释放及禁止隐式clone/retain。首轮缺NativeObjectError导入的E0425
已修复，编译失败不计行为红测。nongeneric Nullable<Int>控制实际1/1通过，冻结既有
InvalidModel、detail `frontend lowering failed with MissingFact`、provider真实null Span；
span从AST/source slice提取，不硬码偏移。实际链路为unsupported storage未进入type mapping，
null lowering查映射产生MissingFact；本片保持原分类，不扩inline Nullable ABI。

最终命令`cargo test --locked -p lang-codegen --lib unit_generic_body_native -- --nocapture`
为7项：1passed/6failed/0ignored/856filtered。五成功合同在各loop首fixture实际emit
MissingFact拒绝；新增generic Nullable<Int>在C<Int>缺canonical守卫处失败。
不能将设计中的18个成功夹具或循环后续元素称为实际native执行通过。临时原始日志
`/private/tmp/0276-native-body-red-frozen.log`；此前6项1pass5fail另存，不累计为覆盖。
控制项实际两种目标状态均目录全部names/bytes、arena和临时文件保全。

独立完整测试审阅发现frontend以calls长度保护模板事实的P2缺口；补relay/middle各自
UnitSymbolId与provider来源、consumer实际Int与来源后，5项重跑仍1pass4fail，
身份断言已实际越过，窄复审关闭P2。新增native全部7项完整/窄审无新增finding，
generic Int拒绝先要求canonical存在，避免拿新cache缺口冒充原inline表示拒绝。
生产归一化、预算/DAG私有矩阵、实际资源运行及G1–G7完成验收仍待后续实现。


## 11. 本地实现与定向绿测（2026-10-05）

私有generic_body_types按已提交descriptor收集真实种子，UnitSymbolId环境仅追加canonical；
新的type_graph复用原substitute入口，以迭代active/completed读取DAG，field查询与闭合实参
查询分别缓存。独立生产审阅发现EnumCase流量细化实参漏种P2，新实际source回归先失败，
再用仅闭合query沿root读取的最小修复通过；旧runtime field与substitute规则不扩张。

`cargo test --locked -p lang-frontend --lib --test multifile_type_checking generic_body_ -- --nocapture`：
lib4/4（202filtered）和integration14/14（116filtered），均0ignored；随后相关整个
`--test multifile_type_checking`130/130通过。私有0/1/2检查有限frontier而不声称跨crate共享seam；
4096层合法Function DAG的两边共享同一child，concreteness/substitution只完成4097个unique节点，
重复query不增visits，identity替换不增arena；Int/String不同环境各建memo，无递归宿主深栈。

native首轮生产实现为4passed/3failed，真实Span定位callee inner与Nullable<Resource>的null；
不是canonical缺失。合同§4按现有ABI扩大最小消费者适配并独立审阅：descriptor选中的普通
static callee不登记runtime function value；nullable Class沿inner规划hidden deinit，拒绝inline/
Rc/Box资源wrapper和generic名义recipe。旧body-only MissingFact/nullable Class缺口迁移正例，
原recursive-template、缺substitution、其它resource形状保留。

`cargo test --locked -p lang-codegen --lib unit_generic_body_ -- --nocapture`11/11通过，859filtered、
0ignored；其中9native+2planner。原7native实际跑完18个成功fixture的object/link/run与完整
stdout/逐指针分配释放核对；Nullable<Int>两个控制保持既有InvalidModel/MissingFact和null Span。
新production1024增长入口仅emit拒绝、两种目标全部names/bytes保全；从未link/run增长入口。
无实际seed、unused()中真实闭合grow<Int> seed两个控制均normal entry正常构建/run，证明cache
预算不提前让整个unit失败。planner私有0/1/2精确relay/middle/inner item Span；later sibling
recipe对0/1/2及正逆inputs始终UnsupportedNode/GrowB.next原name Span。

`unit_lower::type_plan`3/3（863filtered）验证static callee角色、保留generic FunctionValue拒绝、
Group walker只标callee链而不标实参；Group项是descriptor role gate后的AST链测试，未冒充
frontend将Group调用选择成Declaration。`ssa::unit_plan_tests`53/53（813filtered）证明原recipe/
error-order回归，执行时新预算tests尚未加入；后者另在上述11项实际运行。
`unit_resource_deinit`13/13（858filtered）覆盖新null/wrap/hidden body/LLVM正例，Payload/Rc/Box
资源nullable精确null拒绝及全部既有相关native；均0ignored。新增预算文件初次E0716已修复，
该编译错误不记行为红测。

临时原始日志分别`0276-frontend-budget-dag-first.log`、`0276-frontend-multifile-green.log`、
`0276-native-body-downstream.log`、`0276-codegen-body-budget-all.log`、`0276-codegen-callee-role.log`、
`0276-codegen-plan-regressions.log`、`0276-codegen-deinit-regressions-green.log`，均在`/private/tmp/`。
本地fmt/严格clippy/workspace、共享sealed/CLI/词频与M1A消费者、完整codegen及双宿主PR/main仍待；
当前结果不宣称整个SPEC完成。

实际红绿日志、命令、计数、内容SHA与复审Rust源码指纹已保存在
[本地证据账本](../../development/evidence/generic-body-0276/local.json)。历史红日志保持对应
checkpoint含义，不能拿最终源码指纹冒充旧测试版本；完整codegen结果属于最终复审Rust快照。


独立最终定向复审（2026-10-05）：新frontend生产/EnumCase查询修复、预算/DAG、static callee、
nullable Class reachability及旧保护迁移均由未写相应实现的审阅者核对；无新增production finding。
nullable guard验证缺口补真实Payload/Rc/Box nullable null的精确Unsupported/span后关闭。
审阅明确Group AST链私有测试的证据层级，不把它升格为任意Group static源码到native支持。
严格完整门禁仍待；完整codegen实际870passed/0failed/1既有Mac LLDB权限ignored（不计通过），
另2项native view compile-contract与4 doctests通过；原始日志`0276-codegen-full.log`。


## 12. 共享门禁进展（2026-10-05）

完整codegen为870passed/0failed/1既有Mac LLDB权限ignored（826.08s），native view公开
compile-contract2/2与doctests4/4；没有filtered。相关七个frontend消费者target分别为
multifile ownership105、capability4、member7、signature determinism2、signatures8、
owned view1、owned view compile-contract7，全部passed且0failed/ignored/filtered。
此前整个multifile_type_checking130项与私有预算/DAG4项的成功仍适用，不称frontend全量。

严格clippy（frontend+codegen all-targets、`-D warnings`）与`cargo check --locked --workspace --all-targets`
均通过。fmt通过；docs521页与37个checker测试通过；Rust尺寸为789手写文件、45旧超限，
基线及例外上限未提高。工具现场复核：rustc1.96.0、backend LLVM/Clang21.1.8，
不以rustc内置LLVM22.1.2代替backend版本。复审19个变更Rust文件指纹与完整codegen时一致。

CLI全套85/85通过，无failed/ignored/filtered，随后显式重建kovenc。公共M1A四项目12命令、
词频九项目37命令并行但各自目录隔离，均真实build/artifact/run；没有并发Cargo门禁。
从原始JSON独立读取完整exit/stdout/stderr及源码hash，词频还核byte argv与独立参考序列。
两个消费者记录相同实际compiler SHA且运行前后不变，49条命令与其源码合同均实际通过。
原始记录保存在本地账本关联JSON；preview与远端精确head在PR CI交付，不将其提前记通过。
