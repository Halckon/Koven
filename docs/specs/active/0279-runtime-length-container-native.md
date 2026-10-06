# SPEC-0279: Array/List 运行时长度源码构造到 native

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收运行时长度顺序容器构造时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0279` |
| 所属 Phase | Phase 2/3 既有合同对齐；Phase 4 SSA/LLVM/native |
| 语言规范 | 已启用 Guide v0.41；[集合](../../guide/12-collections-destructuring.md)、[closure](../../guide/07-calls-lambdas-closures.md) |
| 批准依据 | 用户持续实施里程碑、满足前置并行及根据实现调整草稿的站立授权；除用户明确启用的 expected move 澄清外，只落实已有语义 |
| 前置 Spec | SPEC-0275、SPEC-0276、SPEC-0278 已 done；0278 final/merge/actual main CI 已闭环 |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0009](../../adr/accepted/0009-concrete-closure-internal-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) accepted |
| 关联 ADR | 不新增 erased callable、heap environment 或容器 ABI |
| 阻塞项 | expected move 冲突经用户明确启用 v0.41 已解决；callable 来源 API、来源图及 source/helper ABI 已完成本地验收；远端交付仍待 |
| 影响范围 | lang-frontend lambda expected 合同；lang-codegen callable planning、source lowering、SSA、LLVM 与 native；验收及 Architecture |
| 语言语义变更 | 用户明确启用 v0.41 的 expected move literal 澄清；其余工程范围不新增语义 |

## 1. Goal 与基线

从 actual main `6575e9069506fbdddcbf7e8cd5e873de91eea5b7` 创建
`feature/spec-0279`；前置精确 final/main 双宿主证据见
[0278 交付账本](../../development/evidence/closure-escape-0278/delivery.json)。

交付 `Array<T>(size, initializer)`、`List<T>(size, initializer)` 在单文件与普通
compilation-unit 两入口的完整源码→类型/所有权→SSA verifier→LLVM verifier→object/link/run。
三种 initializer 均须真实源码成功：无捕获 function pointer、直接 shared-capture closure、
owned move closure。命名 initializer 不被构造消费，临时 initializer 在 CallReturn 的 ASAP 点清理。
只通过 raw SSA 原语、只关闭共享 verifier 前置或只支持无捕获 callback 均不能算本片完成。

基线公共 build 已用 PR56 实现 CI 的已验证 macOS preview 实际执行；编译器 source 是
`f08feb5af4ce934289f1660780aefb9a64d332bb`，不可改写为 final/main 身份。
两入口 pointer/shared/helper/resource 等 corrected 用例在 lowering 返回 UnsupportedNode；
普通 Fn expected 的 move literal 在两前端实际 L0084。初次夹具错误及 shared oracle 修正
单独保全，见[公共 build 基线收据](../../development/evidence/runtime-constructor-0279/public-baseline/receipt.json)；
这些 build 失败不替代正式分层红测或 native 成功证据。

## 2. 复用与非目标

复用 frontend runtime construction descriptors、两个 Borrow 参数、capture/loan/drop facts；
现有 source-qualified canonical 类型、有限实例预算、concrete closure model/thunk、
checked buffer allocator、逆序元素 drop 与 allocation accounting harness。
已有 LLVM `closure::invoke` 每次为 environment 创建 alloca，不能直接放进 generation 循环。
unit planner 当前拒绝 lambda 内 resource demand，必须消费实际 concrete lambda 的 deinit 需求。

不新增 MutableList 增长/删除、Map、动态分发、erased callable、跨线程或借用返回语义。
不支持不同 callable layouts 的任意 runtime join；精确拒绝缺少 provenance 的边界。
不启动 daybreak、P2 重采样、本地安全验证或故障注入/校准；这类验证仅记录未执行，
主干既有远端 CI 按用户确认允许。
嵌套 capture LoanId 全局生命周期不因本片自动关闭；仅实际新失败测试证明必要时做最小修复。

## 3. 冻结的实现合同

### 3.1 expected lambda 与类型身份（v0.41 已启用）

用户明确接受候选并启用 v0.41；前版冲突与候选保全在
[迁移账本](../../archive/migrations/v0.41-enablement.md)。当前规则已写入 Guide03 正文，
不再以 Guide07 例子覆盖前版正文。允许普通 `(Int) -> T` expected 上下文接收 `move { ... }` literal，沿用该 expected Function
canonical identity；capture 仍由语法及 frontend ownership facts 发布为 Owned。
前版实际 L0084 保留为历史基线；在已启用 v0.41 下，相同候选成功测试成为实现红测。含 `move` 的 Function 静态身份继续按 Guide03 区分；不放宽已经定型的 named
`move Fn` 到普通 Fn 的 assignability，也不推导 lambda body 中的泛型 T。
必测有效参数模式、Owned/Move capture 与 ASAP drop；反例保留 arity、参数 mode、普通
borrowed closure 到强约束 expected、named Function 身份不匹配及 L0137/L0138。

### 3.2 同步 borrowed generator 与求值顺序

保留旧 `ContainerGenerate` 静态 Value Int primitive 的合同；新增显式 borrowed generation
操作，字段为 container type、Int length Value、shared initializer Loan。
其 signature 为一个 Shared Loan(Int) 参数；普通 T 返回一个完整 T，Unit 元素允许
既有 void callable return 并形成逻辑 Unit。结果是一个完整 owning container。
operands、render、类型 verifier、dominance/active loan 与 ownership effects 必须同时登记。
错误/失效/exclusive loan、错误参数模式/返回元素、提前 drop callback 均须结构化拒绝。
generation 只读 callable，不 consume、Copy、clone 或 retain callback；0278 逃逸防线继续有效。

source lowering 先对 size 求值一次，按第一参数事实建立 Shared Loan 并读取 Int 快照；
该 loan 在 initializer expression 及其 nested calls 前已活跃，并保持至 constructor CallReturn。
随后通过显式比较/Abort CFG 拒绝负值，
再求 initializer expression 一次并建立/复用其 Shared Loan；不把“负值检查只在 LLVM
generation 内”当作 source 顺序证明。raw generation 仍受既有非负/目标宽度检查保护。
受检 bytes 与 buffer allocation 在 initializer expression 后、任何 callback 调用前发生。
零长度仍求 initializer expression 一次、调用零次；正常按索引升序各一次、元素直接进槽。
受检 bytes overflow 与 allocation fail 均必须不调用 callback，且保留已经发生的 initializer
expression 副作用。按照用户最新范围，本地不执行确定性 test-only 分配失败及其他
故障注入验证；这些验收登记为未执行，不能用正常路径或远端既有 CI 代替。
Abort 不展开清理；正常逆索引 drop 元素，最后释放 buffer。

LLVM 复用 checked allocation/loop，并把 callable function/environment 的准备与 Borrow Int
storage 放在 loop preheader 一次完成；loop body 只更新 index、调用 prepared callable、存槽。
function pointer 不造 environment；concrete closure 内联环境，不发生隐式 heap allocation。
Unit void bridge 仍逐逻辑索引调用，不能因 stride 为零跳过 callback。

### 3.3 concrete helper 身份

每个函数实例按 Function 参数位置有序记录 concrete callable provenance。lambda 使用
source-qualified expression identity 与所属具体实例；已知无捕获具名函数使用 source-qualified
callable target 与具体替换。环境运行期值不进入 key；capture types 从 frontend facts 具体替换。
helper 参数直接转发保持原 provenance；同签名不同 lambda/layout 不能合并，重复身份须去重。
不建立全局 Function TypeId→某个 closure layout 的映射，不在 backend intern frontend arena。

API 链必须一致：frontend canonical Function 替换→unit plan 有序参数 provenance→具体
callee signature/parameter storage→callsite Borrow 路由。含 T 的 Function 已由 frontend
generic body 发布时只读查找；缺少事实结构化失败。新增 specialization 计入既有
1024 实例预算；重复实例不重复计数，边界、递归转发和输入顺序必须有确定性测试。
具体 Rust 类型与 API 定位经独立研究/复核后记录到实施账本，允许在上述合同内调整。

### 3.4 范围与资源

本片选定 Int、String、普通 Resource class、Unit、以及已有可存储名义/容器实例的 T；
复用布局/drop 边界，不把 raw synthetic MoveOnly ZST 当作已实现的源码类型。
Unit 零 stride 的逻辑调用必须真实源码 native 验证；底层非平凡 ZST drop 保留既有 synthetic
测试作为共享 ABI 回归，不能声称新增 source MoveOnly ZST surface。
含 resource lambda 须沿实际 frontend ownership/deinit facts 放开，不能删除 blanket refusal
后猜测 cleanup。命名 shared callback 的 capture loan 持续到 closure 自己 ASAP drop；
constructor CallReturn 只结束参数 loan。临时 owned environment 按 capture 逆序 drop；
每个 element、environment descendant、buffer 只有一个 owner，无提前 free 或隐式 clone/retain。

## 4. 验收矩阵与实施次序

| Gate | 必需证据 | 当前状态 |
|---|---|---|
| E1 | 两前端合法 expected move literal 的失败测试；capture/loan/drop facts；两 source runtime lowering 成功目标的真实红测 | 正式红测保全；frontend及两source成功目标已通过，结构化缺来源反例保留 |
| E2 | Array/List × single/unit × pointer/shared/owned 12 格源码 SSA/LLVM/native；0/1/3、named/temporary | 本地两入口各144源码与normal native组合及最新完整复核通过；远端宿主待 |
| E3 | size 第一 Borrow 在后续 operand/nested call 内保持，initializer Inout size 冲突 L0135；size→拒负→initializer expression→allocation→callback；overflow/allocation fail 保留 expression 副作用且无 callback；零长度及升序一次，LLVM preheader 固定 storage | 普通源码/SSA/LLVM与两入口各6native求值trace通过；故障/校准及allocation观测按用户范围仅登记未执行 |
| E4 | 命名 callback 重用、capture source 冲突与最终 source 复用、temporary ASAP、逆索引 Resource drop；逐指针 allocation/free | 源码生命周期/公共CLI重用与各6Resource native trace通过；逐指针allocation/free未执行 |
| E5 | single 同文件与 unit 跨文件 generic/helper 各三种环境、显式/合法推断 T、body-only demands、同签名布局隔离、重复去重、正逆 inputs、arena 不增长及预算边界 | planner/源码完整回归与两入口各48基础helper及48嵌套/名义元素helper native通过；不外推全部可存储T |
| E6 | malformed SSA callback/loan/type/mode 正反例；0278 既有控制、旧 static generator 与 Unit/ZST ABI 回归 | raw SSA/LLVM 切片已验收；源码/native 仍待 |
| E7 | 两公共 CLI 入口实际 build/run；原 object 失败保全、无 temporary 残留、required CI 实际选择新测试 | 本地两入口各6公共build/run与最新生产复核通过；required CI实际选择待闭环 |
| E8 | 独立 fresh-context 全审、Architecture/验收账本、归档 inventory、精确 final PR 与 actual main CI | 待 |

顺序：合同及 E1 红测→frontend literal 合同→SSA/LLVM borrowed bridge→两 source 桥→
helper identity 与 canonical body demands→资源/public native 矩阵→全审及必要共享消费者→远端交付。
独立作者仅在 API 一致且文件所有权明确时并行；root 串行运行本地 Cargo，不争用 target。
每项记录命令、精确 head、宿主及 passed/failed/ignored/filtered；未跑不得记为通过。
本地只执行普通编译器行为验证；安全验证、故障注入及校准仅登记未执行。
远端既有校准成功也不替代本片 intent tests；E3 的相关未执行项必须保留到交付账本。

## 5. 交付账本

2026-10-05：0278 前置证据独立复核通过并在本分支逻辑独立提交；本片尚未修改生产源码。
草稿中 public shared oracle `scale=7, index=2` 已经实际更正为 9 并重跑失败；
negative factory 使用分组 return lambda，避免 parser 夹具错误。正式 E1 必须采用修正源码。
实施前独立 helper API 研究正在进行，不把静态研究算作行为验收。
独立合同复审补回第一 size Borrow 的跨 operand 生命周期、overflow/分配失败对照，
并明确 single 同文件及 unit 跨文件 helper 各三种环境；窄复核通过；110 份公共基线 raw 保全亦独立核验，不记作 native 验收通过。

尺寸旧欠账：`lower_frontend.rs` 1480、`unit_lower.rs` 1266、`verify_ownership.rs` 1658 物理行。
本片新增领域逻辑放单职责子模块；超线增长例外必须显式审阅，不机械压行或切片。

2026-10-05 补充：四项 expected move 候选测试实际因 L0084 失败，生产未改。重新核对
Guide03 正文后登记语言前置冲突；此前“无语义阻塞”的判断不足，§3.1 暂停等待决定。
两项 runtime source 测试已实际失败，每项保全 Array/List × pointer/shared 四个 UnsupportedNode，
其 parsed/name/type/ownership 前置均成功；剩余成功目标仍未实现。

2026-10-05：用户明确采用候选并启用 v0.41，语言前置已解决；规范迁移正在独立复核，
生产 checker 仍未修改。此前冲突登记保留为当时记录。

规范迁移独立完整复核通过：16 页完整快照、32 SHA、机械链接及唯一语义变化符合用户决定；
结构门禁 545 页及 37 项 checker 测试实际通过，原始输出见
[v0.41 验证收据](../../development/evidence/runtime-constructor-0279/v041/receipt.json)。
规范改动单独提交后继续 E1 实现，不据此关闭 source/native 或 E8。

前端切片：两 checker gate 已对齐已启用 v0.41，expected canonical 与 AST Owned/Move capture
保持分离；named identity、strong move、mode/arity、L0137/L0138/L0131 的反例全部通过。
unit 临时 owned closure 的新 intent test 实际暴露 Captured drop 遗漏；最小修复复用 named
路径的逆序 capture facts，正常 CallReturn 与 ControlTransfer 都在 Temporary 前释放 Owned+Move，
Copy 不清理、Abort 不展开。两 String/Copy Int 与 iteration ordered/flat 一致性补测实际通过。
独立复核了生产 gate、ownership facts、root 修复及 Shared 顺序；额外测试复核后才提交。
六个完整相关 suite 合计 482 passed、0 failed、0 ignored、0 filtered；
源码起始 SHA、实际失败/夹具失败、最终 raw 和未关闭范围见
[前端验收收据](../../development/evidence/runtime-constructor-0279/frontend/receipt.json)。
`expression.rs` 旧欠账1206→1207，仅 gate 注释增加1行，精确例外登记并独立审阅。
源码生成、helper、LLVM/native、E8 仍在实施，不以本次前端验收关闭它们。

SSA/LLVM 工程切片：新增 synchronous borrowed generator，prepared callable/index storage
在 preheader；Unit void 仍调用。root review 实际发现根内容覆盖后旧捕获依赖失真，补精确覆盖、
顺序覆盖、Take、CFG 同时重绑定与投影正反例；有限 current-content proof 复用 0278 worklist。
ConcreteClosure RootReplace/RootSwap 的 OperationContract 拒绝是旧边界，不算新行为红测。
独立审阅又发现非法 type graph 可使新证明越界 panic，root 真实复现并在 module type/deinit
门禁后才进入 ownership/content，修后25项新测试通过；旧相关消费者实际通过并补门禁回归。
尺寸例外锁定 model1349、verify_operation1169、verify_ownership1681、adapter1607、runtime1188，
新增证明与测试单职责模块均小于1000行。完整 raw、夹具修正、各次选择数量与独立复审见
[borrowed generator 收据](../../development/evidence/runtime-constructor-0279/borrowed-generator/receipt.json)。
这是 E6 的工程子集；两 source/helper/factory 成功目标仍实际 UnsupportedNode，
E2/E3/E4/E5/E7/E8 尚未关闭，不据此归档本 Spec。

只读 canonical 切片：两入口 Function 参数及返回类型替换保留 move/mode，缺失目标为
MissingFact；单文件复用已有 canonical find 核心，unit 参数结构构造仅供查询，不产生类型。
4项直接测试涵盖6个正形与2个缺事实对照，实际先失败后通过；unit plan57、single lowering70
相关回归实际通过，0failed/0ignored。独立未参与作者复核了类型身份、validated边界及旧
recipe/预算/nested nominal拒绝；原始来源SHA与隔离范围见
[canonical验收收据](../../development/evidence/runtime-constructor-0279/canonical-callables/receipt.json)。
helper concrete ABI和source/native仍未完成；E5保持待验收。

共享引用读取切片：SharedReferenceFollow严格将active Shared Loan(SharedReference<T>)映射为
Shared Loan(T)，LLVM只读取slot pointer。父/祖先、CFG与provider依赖复用既有合同；两个
内容证明不从引用槽的空capture推断target闭包内容。7项正式有效红测修后通过，32项borrowed
相关回归通过（包含这7项），0failed/0ignored；独立非作者复核通过，
[原始收据](../../development/evidence/runtime-constructor-0279/shared-reference-follow/receipt.json)
保留API编译缺失与两处夹具错误，均不冒充行为红。source/shared/native与一般nested LoanId仍待。

只读 callable 来源切片：两既有 sealed ownership 产物提供 Lambda/真实 Fn 参数/已选定函数/
FactoryResult 来源及唯一正常无捕获返回摘要；Deferred 不补来源，P2/P3 错误清空两表。
有限图增量求解复用实际 CFG 状态，不替代 capture/loan/drop 或重新检查 P2/P3。独立复审
实际发现普通 Value 交付误计返回及 nested actual Return 被 depth 压掉，两类正式红测修后通过。
31项正式合同与8项审阅边界均包含在374项完整相关回归中，不相加；0failed/0ignored/0filtered。
专用 target 严格 clippy、全仓格式、尺寸及五crate全target编译检查通过；独立非作者完整复审
和机械 lint 修正窄复审无未决 finding。共享 target 的缓存编译失败及0.02s缓存 lint不算行为
或严格门禁通过，原始输出保留在[收据](../../development/evidence/runtime-constructor-0279/callable-provenance/receipt.json)。
三项精确旧欠账例外锁checker1413、unit1149、dataflow1247，baseline未抬高；实现事实见
[来源专页](../../architecture/callable-provenance.md)。本片不关闭helper ABI/source/native或PR/main；
本地故障注入/校准按用户范围只登记未运行。

helper planner 首批正式旧 API 行为红：single13项5pass8fail、unit8项1pass7fail，
全部最终夹具先通过源码/类型/所有权前置。首次测试Ord编译错误及两处MoveOnly alias
移动后读取的非法夹具单独登记，不能冒充planner行为红；修正后共15处有效合同失败。
shared arena使用双key-map与单append-only record Vec，source/callback token严格指向
先前已reserve记录；3项普通合同通过，独立非作者窄审无finding，不增加预算或runtime值身份。
两入口完整key/route正在实施，source ABI/native未关闭；原始输出见
[planner收据](../../development/evidence/runtime-constructor-0279/helper-planner/receipt.json)。

helper planner 切片：两入口完整 callback 槽、owner-qualified 来源、Parameter 转发、pointer
factory memo 与只读 call 路由已实现；unit driver/static call/thunk 保留并消费同一个 source plan。
原21项有效用例全部转绿，新增查询与复审边界后，single19项、unit13项及arena3项共35项
新合同包含在308项相关通过中，不重复计数；0failed/0ignored，选择及过滤数见raw。
独立复审实际发现普通 Nullable<T> 参数过早具体化、unit Fn capture 缺少明确拒绝，以及
named Unknown 诊断包含名称前缀；各有合法前置真实红测，最小修复后独立窄复审通过。
15个源码文件SHA与最终检查一致，首批80份压缩raw逐份解压SHA核验；严格codegen all-targets
clippy、workspace all-targets编译、格式及尺寸门禁通过。Architecture索引201行门禁失败
保留，修正文档路由句后通过；实现事实见[实例规划](../../architecture/callable-instance-plans.md)。
unit_lower旧欠账仅增长6行至1272，精确例外经非作者复核、baseline1252未抬高。
unit lowering回归显式排除3项待实现source目标；6项两入口runtime/helper/factory成功目标
另实际运行，仍0pass6fail UnsupportedNode，未用过滤结果声称source ABI/native通过。
本片不关闭E2/E3/E4/E5/E7/E8，不归档、不提前PR；本地故障注入/校准仍只登记未运行。

2026-10-06 source ABI 实施检查点：unit 每个 reachable source 冻结已有无捕获返回摘要，
直接 constructor factory 查询及 returned named body 可达性两项合法红测转绿；完整 unit planner
72项通过（包含两项，不相加）。源码矩阵另以精确 `1aef99b` 生产加测试覆盖的隔离快照
运行，两入口各144个组合均先通过前端，实际288个 UnsupportedNode；不回滚正在实施的生产。
当前 single 经具体签名、thunk、borrowed generation 和 descriptor-driven size/index 接入后，
96个 pointer/Owned 组合已通过源码SSA/LLVM；48个Shared仍InvalidSsa，named CFG intent
进一步实际定位LoanInactive。guard次序intent已通过，temporary Shared iteration intent仍失败。
编译可见性错误、修正后all-targets编译成功与各实际失败原始输出均保留在
[source ABI 收据](../../development/evidence/runtime-constructor-0279/source-abi/receipt.json)。
这仅是源码部分进展；unit D、Shared生命周期、真实native/public CLI及E8仍待，不归档、不提前PR。

后续实际 checkpoint：独立冻结 single 快照的4项runtime均通过，其中144个源码组合SSA/LLVM
全部成功；144个normal native组合在133.57秒内全部object/link/run，校实际length、非零
非Unit末值、callback次数（含Unit零stride）及Resource析构总数。总数不证明逐owner身份、
升序callback、逆序析构或allocation accounting。live unit D首次all-targets编译通过；
3项原direct/helper/factory成功，矩阵108个Int/String/Unit通过SSA/LLVM，36个Leaf仍被
既有lambda resource demand gate拒绝。single直接bare具名函数新intent仍MissingFact；
独立审阅另登记outer synthetic slot在内部loop控制转移时的潜在scope问题，待真实红测。
native和两public CLI targets的原生产失败已保全，各循环首case失败不冒称全部执行；
当前unit/native/public combined验收仍未关闭。细节及宿主/源码SHA见上述source ABI收据。


实际事实修正及后续验收：single bare named initializer/helper 的 frontend 类型仍为
Deferred(OverloadSelection/Call)，sealed callable 来源为空且 initializer 有 Temporary LoanFact；
此前 synthetic no-fact KnownFunction 桥的假设不成立，已全部撤回。三个正式反例核对真实事实
并要求精确 operand Span 的 UnsupportedNode，不能把它们记作成功 initializer 支持。
真实 pointer 正控继续使用无捕获 lambda/已验证 pointer factory；unit 发布的 selected named
事实不同，已有真实裸具名 initializer 正控。此前 innerloop slot 假说未到达 slot 分配，
不是生命周期行为红；unit 两项 innerloop fixture 也因已有 frontend LoanFact 未满足 no-fact
前置，原输出保全为夹具错误，后续按真实路径修正而不删除事实。

single 公共 CLI build 与独立 run 六个组合实际通过；普通 Resource trace 六个组合实际
证明 callback 升序、元素逆序以及 temporary Owned environment 在 CallReturn 清理。
最初 Resource ASAP oracle 与 Guide10 lexical 规则不符，保全为 oracle 错误；修正后通过。
single 泛型 nominal 命名曾使六个 Resource helper 正控实际失败，最小修复只对现有 mapper
已接受的 canonical 类型使用 TypeId 后缀，旧 builtin 名称保持；正式正控及完整7项
single_runtime_ intent family 已通过，arena 不增长。

unit 选定 runtime initializer/resource demand 与具名指针 Borrow 地址接入后，两个正式
intent 和144个源码组合均通过 SSA/LLVM。其 native 矩阵只执行完36个 Int、36个 String
及 Leaf 首个零长度 case，后续 Borrow Resource 参数的 Copyable 字段读被拒绝；Unit native
cases 尚未到达。Resource order pointer case 已执行，Shared 在 scale.name 的 String
字段 Borrow 被拒绝。两个合法前置的字段 intent 均实际失败，当前最小字段路由修复实施中。
这些 native 失败不能据 source 全绿宣布 E2/E4 完成；逐指针 allocation/free 仍未执行。
本地安全验证、故障注入和校准继续仅登记，不运行。


字段及 normal native 后续检查点：两个字段 intent 已转绿，unit144个组合及6个 Resource
顺序 trace 均实际 object/link/run；两入口 generic helper 扩至各48个显式/推断 T 组合
（Int/String/Resource/Unit × Array/List × 三环境 × 两种类型实参），全部运行通过。
E3 另各6个 ordinary native trace 明确验证负值只求 size、零值仍求 factory 一次但不调用
callback、正3值按0/1/2升序一次；source第一Borrow跨factory与Inout冲突由已有intent负责。
首次两个新测试的 harness调用/模块路径编译错误已单列，不计行为红。

新 fresh-context 完整审阅实际发现 single普通when漏 unmatched capture map，三个合法
variant均真实InvalidSsa；3行新增与1处替换的最小修复后均通过，第四个同entry多条件
matched合流正控随后补入。unit字段/完整ABI独立复审无生产finding；unit普通while的既有
control-prefix拒绝不放开，两个innerloop intent改用真实for descriptor的已有能力切片，
保留selectedidentity和真实TemporaryLoanFact，实际2项通过，不冒称synthetic ABI slot证明。
五项尺寸增长经非作者复核，baseline未抬高，精确锁single driver1502/control1507、
unit driver1325/control1079、native parent1300；新领域算法子页均小于1000行。
严格门禁、完整相关共享消费者、第四when正控、公共两CLI与最终source SHA闭环正在继续。
本地安全验证、故障注入、校准、逐指针allocation/free仍只记录未执行，E8/归档/PR未关闭。


完整消费者检查点：single原94pass/6fail中三项旧正控确有whole-typed-span预登记回归，
已改为现有structural construction descriptor与P3实际计划/loan demand；类型元信息及
return后无实际Lambda origin的表达式不注册runtime布局/thunk。source member精确
UnsupportedNode恢复；Int index/inv、owned Resource body、shared closure及canonical
Function只读查询的旧负测改为实际契约正控，其他边界保留。两个非作者互换复审通过。
当前Single102、Unit160、unit planner72、runtime source8均0fail0ignored；runtime source
与Unit suite有重叠，不能相加为独立用例总数。第四when alternatives正控包含在102中。
single driver最终1505（原source接线22＋member诊断3），policy已锁最终值，baseline未改。

静态ListForm/EmptyMutableList缺少通用P3已求值容器事实的旧预登记边界另记录：原1aef99b
同样对所有typed container descriptor预登记。本片没有借drop/loan缺失猜测其可达性，
也未宣称所有不可达unsupported静态storage均闭环。严格clippy codegen/CLI all-targets
实际通过；全workspace编译、CLI重新build及最新native/公共路径复核仍将按记录闭环。


source ABI 逻辑切片最终本地检查点：冻结最新Rust后，8个normal native entry覆盖408个
实际object/link/run组合全部通过（两入口各144 core＋48显式/推断helper＋6resource trace＋
6evaluation trace），0failed/0ignored。两个公开CLI target重新各6组build、artifact run及
独立run通过；CLI已重新build。codegen/CLI all-targets严格clippy、workspace五crate
all-targets check、格式、精确尺寸及47项政策测试通过。fresh全审发现和完整消费者回归的
实际失败都已保全，修复后非作者交叉复审无未决finding；机器检查改动Rust SHA与最终
门禁输入一致，并解压核对raw SHA。详细边界见[source ABI收据](../../development/evidence/runtime-constructor-0279/source-abi/receipt.json)
及[独立审阅记录](../../development/evidence/runtime-constructor-0279/source-abi/review.json)。

本片可作为独立实现提交，Spec仍in-progress：E5已有其他可存储名义/嵌套容器T的更广
源码native覆盖、最终跨宿主CI/归档/PR/main交付仍待；用户排除的本地安全/注入/校准与
allocation观测只登记未执行。不能以本地Slice全绿代替完整Spec结束。

2026-10-06 E5 扩展检查点：在 source ABI 提交 `03c0ba9` 上仅新增测试，生产实现未改。
single 同文件和 unit 真实跨文件 `import p.generate` 各48个 helper 用例覆盖
`Array<Int>`、`List<Int>`、Int 字段 value class 与普通泛型 `Holder<Int>`，分别组合
Array/List、pointer/Shared/Owned 与显式/推断 T。两个测试实际全部通过，0failed、0ignored、
1030filtered；真实前端验证、对象发射、链接及96次进程执行均由既有 harness 执行。
借用 verifier 核内层长度1、末元素2/9、外层长度3及callback恰3次；不宣称全部元素内容
和顺序，也不把无 `leaf` 输出当成零SSA Drop或allocation/free证明。
独立非作者复审无阻断finding；codegen all-targets严格clippy、格式和精确尺寸检查通过。
原 source ABI 收据保持冻结，新证据见[E5元素扩展收据](../../development/evidence/runtime-constructor-0279/storable-helpers/receipt.json)。
E6普通回归核对与E7/E8远端交付仍待；用户排除的本地检查继续仅登记未执行。
