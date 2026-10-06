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
| 阻塞项 | expected move 冲突经用户明确启用 v0.41 已解决；callable 来源 API 已冻结并有实际空表失败测试；来源图和 source/helper ABI 仍在实施 |
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
| E1 | 两前端合法 expected move literal 的失败测试；capture/loan/drop facts；两 source runtime lowering 成功目标的真实红测 | 前端已通过；两 source runtime lowering 红测保留，成功目标待实现 |
| E2 | Array/List × single/unit × pointer/shared/owned 12 格源码 SSA/LLVM/native；0/1/3、named/temporary | 待 |
| E3 | size 第一 Borrow 在后续 operand/nested call 内保持，initializer Inout size 冲突 L0135；size→拒负→initializer expression→allocation→callback；overflow/allocation fail 保留 expression 副作用且无 callback；零长度及升序一次，LLVM preheader 固定 storage | 待 |
| E4 | 命名 callback 重用、capture source 冲突与最终 source 复用、temporary ASAP、逆索引 Resource drop；逐指针 allocation/free | 待 |
| E5 | single 同文件与 unit 跨文件 generic/helper 各三种环境、显式/合法推断 T、body-only demands、同签名布局隔离、重复去重、正逆 inputs、arena 不增长及预算边界 | 待 |
| E6 | malformed SSA callback/loan/type/mode 正反例；0278 既有控制、旧 static generator 与 Unit/ZST ABI 回归 | raw SSA/LLVM 切片已验收；源码/native 仍待 |
| E7 | 两公共 CLI 入口实际 build/run；原 object 失败保全、无 temporary 残留、required CI 实际选择新测试 | 待 |
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
