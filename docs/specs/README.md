# Koven Spec 路线图

本目录依据现行 [v0.34 语言规范](../guide/00-index.md) 维护可独立验证、可独立
提交的 Goal；已完成 Spec 保留其实施时适用的 guide 引用。路线图负责排序，Spec 文件负责
定义一次交付；路线图条目本身不等于已批准的 Spec，也不授权实现。

[v0.32 §32](../guide/01-design-decisions.md#32-packageimport-绑定跨文件可见性与-compilation-unitv032)
已于 2026-08-26 由用户明确启用并取代 v0.31；ADR-0020 已接受，
SPEC-0025/0197 已完成多文件名称与 compilation-unit 类型检查；0197 覆盖 signature 与
callable/local/operator/if/type-test smart-cast/assignment/destructuring/when/loop-jump/lambda/
overload-lambda trial/generic TypeRef/generic source call/external/function-value call，以及 source
nominal/enum、intrinsic Box/Rc construction、source member body/call/projection、intrinsic Rc
member operation、core container construction、container place/assignment、contextual null literal 与
null-comparison/non-null-use flow、String interpolation、top-level variable/const initializer 与现行
expression-tail typed traversal，并通过完成审计；
SPEC-0198 已完成；SPEC-0215 已补齐受支持 lambda body 的独立 liveness 与隐式结果 Consume，
SPEC-0216 已统一 MoveOnly `if` / `when` control tail 的 checker/drop usage facts，SPEC-0199 已消费
这些 facts 并闭合完整现行 unit SSA/LLVM/multi-source DWARF/single-object native 路径，
SPEC-0199 与 SPEC-0187 均已完成；完整 `for` 仍由 v0.37 候选链 0179/0211/0212/0182 独立承接。
已接受 ADR-0022，SPEC-0052 已完成
后继本地 manifest/source provider，不把项目 IO 反向塞入 frontend 或 LSP。

[v0.33](../guide/00-index.md) 已于 2026-08-30 明确启用并取代 v0.32：grammar §9/SPEC-0213/0214 增加
规范化为普通 `CallArgument` 的尾 lambda 调用糖及 headerless lambda 隐式 `it`，
[§33](../guide/01-design-decisions.md#33-本地-project-process-entry-与公开-buildrunv033)
另为无依赖本地 project 定义显式 package-qualified entry 与公开 build/run；它不改变单文件
main，也不引入 manifest target/default 或 dependency build。SPEC-0213/0214/0054 均已完成。

[v0.34 §34](../guide/01-design-decisions.md#34-显式-instance-receiver-契约与静态分发调用v034)
已于 2026-08-31 显式重基到完整 v0.33 并启用：它保留 v0.33 grammar §9/§33，新增 member
声明的缺省 Borrow/显式 Borrow/Inout/Value、receiver 所有权交付与静态分发，并把窄化 `by`
委托限制为 Borrow receiver；不定义 iterator、具体标准库 API 或动态分发。

[v0.35 §35](../guide/01-design-decisions.md#35-nullable-when-剩余域与-所有权v035-候选未启用)
是同样以 v0.32 为基线、且不包含 v0.33/v0.34 的独立 nullable 控制流候选：它封闭 nullable
`when` 的剩余域/view/extraction 与 `!!` 的 Copy/Consume，并拆为 0202–0207 的
typed→ownership→pointer-like native 两条链；inline nullable、Elvis 与 safe call 仍延后。

[v0.36 §36](../guide/01-design-decisions.md#36-无运行时存储的关联常量与封闭求值v036-候选未启用)
是直接以 v0.32 为基线、且不包含 v0.33–v0.35 的独立 const/object 候选：它封闭无存储关联常量、
有限编译期求值和每次 use 重新物化，单文件链为 0026→0208→0209，跨文件 typed 集成由
0210 等待 0025/0197 后承接；一般 CTFE、global init 与 associated function 仍延后。

[v0.37 §37](../guide/01-design-decisions.md#37-借用式顺序容器迭代-providerv037-候选未启用)
同样直接以 v0.32 为基线、且不包含 v0.33–v0.36：它只为 intrinsic Array/List/MutableList 定义
无分配 borrowed provider、loop-scoped Borrow binding 与完整退出清理，实施链为
`{0179→0211, 0212}→0182`；ADR-0023 仍为 proposed，其他 provider 与 consuming iteration 延后。

[v0.26](../guide/01-design-decisions.md#26-调用期借用与-asap-析构点v026) 已由用户明确启用并
取代 v0.25；它把 callable 声明的无 marker 参数改为 `Borrow`、以声明侧显式 `own` 表达内部
`Value` owned binding，同时封闭同步调用期 loan 与 ASAP 析构点。源码/typed/所有权参数
契约迁移已由 SPEC-0176 完成；SPEC-0029 的 loan 与 drop facts 实现门禁已解除。

[v0.27](../guide/01-design-decisions.md#27-简化-closure-capture-与跨线程转移v027) 已由用户
明确启用并取代 v0.26；它封闭默认 shared capture、显式 `move` owned capture、borrowed
closure 逃逸、结构化 `Transferable` 与 compiler-bound 跨线程 callable effect。

[v0.28](../guide/01-design-decisions.md#28-泛型-callable-实例化与-overload-lambda-隔离v028)
已由用户明确启用并取代 v0.27；它封闭泛型 callable 实例化与 overload-lambda candidate
isolation，SPEC-0177 / SPEC-0174 均已完成实施与验收。

[v0.29 constructor 契约](../guide/01-design-decisions.md#29-名义enum-case-与-intrinsic-box-构造v029)
已于 2026-08-25 明确启用并取代 v0.28；SPEC-0183 / 0188 / 0184 已完成。

[v0.30 入口与共享所有权契约](../guide/01-design-decisions.md#30-约定程序入口与显式共享所有权v030)
已于 2026-08-26 明确启用并取代 v0.29；零参数 conventional main 由 SPEC-0193 实施，
参数化 main 当时必须等待一般 String/argv Array runtime，现已由 SPEC-0194 实施；SPEC-0045 的非 nullable Rc core、SPEC-0195
的通用 Borrow lowering 与 SPEC-0196 的 pointer-like nullable lowering 均已完成。Arena/Arc/Weak
没有因本版本获得实现授权。

[v0.31 一般 String 契约](../guide/01-design-decisions.md#31-一般-utf-8-string-owner-与最小运行时表面v031)
已于 2026-08-26 由用户明确启用并取代 v0.30；ADR-0018/0019 已接受，SPEC-0192 已完成。
其直接后继 SPEC-0194 已完成参数化 main/argv bridge。

[v0.25](../guide/01-design-decisions.md#25-条件-copyable内联递归与结构化解构v025) 已由用户
明确启用并取代 v0.24；它封闭条件 `Copyable`、有限内联布局、intrinsic `Box` 与结构化
解构契约，SPEC-0022 已完成实施与验收。

[v0.24](../guide/00-index.md) 已由用户明确启用并取代 v0.23；它封闭 enum case type、`when`
穷尽性与 smart cast，SPEC-0021 已完成实施与验收。

[v0.23](../guide/00-index.md) 已由用户明确启用并取代 v0.22；它封闭 nominal/generic/interface/
委托契约，SPEC-0020 已完成实施。

[v0.22](../guide/00-index.md) 已由用户明确启用并取代 v0.21；它正式封闭最小数值后缀、默认
数值类型与基础类型检查契约，并把词法 / AST 增量交给 SPEC-0066、类型阶段交给 SPEC-0019。
v0.21 已封闭单文件双命名空间、作用域、预声明环境与 L0079–L0081 名称诊断，并把首个
Phase 2 实现边界交给 SPEC-0018。
v0.20 已封闭 class-family、类型级 companion、匿名内部类边界与窄化接口委托，并把委托
Parser 拆为 SPEC-0064；
v0.12、v0.13 内容已合入 v0.14。
此外，SPEC-0056 已完成单文档语义跳转定义。
SPEC-0010、SPEC-0011、SPEC-0012、SPEC-0013、SPEC-0014、SPEC-0015、SPEC-0016、SPEC-0017、SPEC-0018、SPEC-0019、SPEC-0020、SPEC-0021、SPEC-0022、SPEC-0023、SPEC-0027、SPEC-0028、SPEC-0029、SPEC-0030、SPEC-0032、SPEC-0033、SPEC-0034、SPEC-0035、SPEC-0036、SPEC-0043、SPEC-0055、SPEC-0057、SPEC-0058、SPEC-0059、SPEC-0060、SPEC-0062、SPEC-0063、SPEC-0064、SPEC-0065、SPEC-0066、SPEC-0067、SPEC-0068、SPEC-0069、SPEC-0070、SPEC-0071、SPEC-0072、SPEC-0073、SPEC-0074、SPEC-0075、SPEC-0076、SPEC-0077、SPEC-0078、SPEC-0079、SPEC-0080、SPEC-0081、SPEC-0082、SPEC-0083、SPEC-0084、SPEC-0085、SPEC-0086、SPEC-0087、SPEC-0088、SPEC-0089、SPEC-0090、SPEC-0091、SPEC-0092、SPEC-0093、SPEC-0094、SPEC-0095、SPEC-0096、SPEC-0097、SPEC-0098、SPEC-0099、SPEC-0100、SPEC-0101、SPEC-0102、SPEC-0103、SPEC-0104、SPEC-0105、SPEC-0106、SPEC-0107、SPEC-0108、SPEC-0109、SPEC-0110、SPEC-0111、SPEC-0112、SPEC-0113、SPEC-0114、SPEC-0115、SPEC-0116、SPEC-0117、SPEC-0118、SPEC-0119、SPEC-0120、SPEC-0121、SPEC-0122、SPEC-0123、SPEC-0124、SPEC-0125、SPEC-0126、SPEC-0127、SPEC-0128、SPEC-0129、SPEC-0130、SPEC-0131、SPEC-0132、SPEC-0133、SPEC-0134、SPEC-0135、SPEC-0136、SPEC-0137、SPEC-0138、SPEC-0139、SPEC-0140、SPEC-0141、SPEC-0142、SPEC-0143、SPEC-0144、SPEC-0145、SPEC-0146、SPEC-0147、SPEC-0148、SPEC-0149、SPEC-0150、SPEC-0151、SPEC-0152、SPEC-0153、SPEC-0154、SPEC-0155、SPEC-0156、SPEC-0157、SPEC-0158、SPEC-0159、SPEC-0160、SPEC-0161、SPEC-0162、SPEC-0163、SPEC-0164、SPEC-0165、SPEC-0166、SPEC-0167、SPEC-0168、SPEC-0169、SPEC-0170、SPEC-0171、SPEC-0172、SPEC-0173、SPEC-0174、SPEC-0175、SPEC-0176、SPEC-0177、SPEC-0178、SPEC-0183、SPEC-0184、SPEC-0185、SPEC-0186 已完成；尚未物化的条目仍只是候选 Goal，不因编号预留而
自动获得实现授权；SPEC-0044、SPEC-0045、SPEC-0189、SPEC-0190 也已完成。

## Goal 与提交工作流

Spec 进入 `in-progress` 前必须已有单份明确确认或有效站立授权作为批准依据，并满足所有前置
Spec `done`、前置 ADR `accepted` 和阻塞项解除。存在有效站立授权时，可以在同一工作流中
按逻辑顺序完成 `approved → in-progress`，不要求中间状态形成独立提交。

后续使用 Goal 时固定按以下顺序推进：

1. 以 `完成 SPEC-NNNN：〈Goal〉；按本 Spec 验收并创建独立提交` 作为 Goal objective。
2. 把 Spec 标为 `in-progress`，按“实施计划”逐项执行；每一步先跑窄检查。
3. 完成测试后运行 Spec 选定的 Layer 2 静态门禁；只有满足升级条件时才运行 Layer 3 workspace
   全量基线，并同步 Architecture 当前事实。
4. 填写验证记录、逐项勾选验收，把 Spec 标为 `done`。
5. 只暂存该 Spec 范围，检查 staged diff 后提交；提交信息必须包含 `(SPEC-NNNN)`。
6. 提交成功后再把 Goal 标记为完成。未提交、验收缺失或检查未运行时不得完成 Goal。

一个提交不得混合两个 Spec。一个 Spec 可以有多个提交；含 Rust / 目标语言实现的提交必须
可构建、可测试，workspace 建立前的纯文档提交执行适用的链接、术语和 diff 检查。所有提交
都引用同一 Spec 编号；最终实现提交同时包含验收记录、Architecture 更新和 `done` 状态。
若一个 Spec 无法形成清晰的独立提交边界，应在批准前继续拆分。

Spec 草案、批准和 `in-progress` 状态不要求分别提交；最终实现提交仍必须独占一个 Spec，
并包含 `done` 状态、实际验收记录和 Architecture 更新。新 ADR 的决策正文仍应形成独立文档
提交，但可在首次提交时直接为 `accepted`，不得把 ADR 与依赖它的实现混入同一提交。
简化的是人工确认和重复状态文书，不是行为验收；任何检查只有实际成功后才能记录为通过。

默认验收按风险逐层升级，不把重型 workspace 命令机械绑定到每个切片：

1. **Layer 1 行为门禁**：合并运行直接受影响 test target 的相关 case，并依赖 Rust test harness
   自身并行；失败先判断本次回归还是已记录的无关基线漂移。
2. **Layer 2 静态门禁**：运行受影响 crate 的 Clippy/check；修改跨 crate 公共 API 时至少运行
   `cargo clippy --workspace --lib -- -D warnings` 与 `cargo check --workspace --lib`，再做格式与 diff
   检查。CLI/bin、grammar 或 native 边界由对应 Spec 额外列出定向 build/test，不由 `--lib` 冒充。
3. **Layer 3 升级门禁**：仅在 Spec 涉及广泛共享不变量、manifest/feature/依赖、发布基线，
   Layer 1/2 暴露跨域风险，窄测无法覆盖，或用户明确要求时，才运行受影响 crate 全量、workspace
   `--all-targets`/`--all-features` 或 native runtime matrix。

`lang-frontend` 全量测试耗时约一小时，不再作为每个切片的默认门禁。互不写源码且不争用同一
Cargo target lock 的检查与独立复审可以并行；会争用 build directory 的 Cargo 命令顺序执行，
避免“并行”退化为锁等待。每份 Spec 必须在验收标准中记录实际选择的层级、命令、升级条件和
任何未解决的基线漂移；只有实际成功的检查才能记录为通过。

## 2026-08-26—27 roadmap 依赖审计

本次按“全部前置 Spec 已 `done` → guide 已封闭 → 前置 ADR 已 `accepted`”重新核对候选节点：

| 队列 | 候选 | 审计结论 |
|---|---|---|
| 已完成 | SPEC-0193 零参数 conventional main | v0.30 首个实施节点已完成，成为后续 SPEC-0194 前置 |
| 已完成 | SPEC-0045 Rc shared owner core | 非 nullable construction/share/Copyable read、retain/release 与 native 主线完成；跨切面能力已迁移到后继 Spec |
| 已完成 | SPEC-0195 跨 callable Borrow lowering | DirectCall/CallableInvoke、frontend LoanFact、LLVM pointer ABI 与 Rc/class/Box native 验收完成 |
| 已完成 | SPEC-0196 nullable handle lowering | pointer-like nullable 的 frontend `if` proof、独立 SSA/verifier、LLVM null niche/conditional drop 与 class/Box/Rc native 主线完成 |
| 已完成 | SPEC-0192 | 一般 String owner、操作、drop、复合 owner/容器/closure native 闭环完成 |
| 已完成 | SPEC-0194 | 参数化 main、两阶段 argv owner bridge、Borrow Array 索引与 CLI 原始参数转交完成 |
| 现行 receiver 实施链 | SPEC-0201→0180→0181→0191 | v0.34 §34 已重基并启用；0201/0180/0181 done，0191 已按持续 Goal 的站立授权进入 native 实施，复用 ADR-0016 |
| 已物化 nullable 候选 | SPEC-0202→0203→0204；0205→0206→0207 | v0.35 §35 已起草；remaining-domain/`!!` 按 typed、ownership、pointer-like native 分层，复用 ADR-0017；未启用，全部保持 draft |
| 已物化 const/object 候选 | SPEC-0026→0208→0209；0210 | v0.36 §36 已起草；单文件 typed/eval→materialization ownership→native，unit typed integration 等待 0025/0197；未启用，全部保持 draft |
| 已物化 iteration 候选 | `{SPEC-0179→0211, SPEC-0212}→SPEC-0182` | v0.37 §37 与 proposed ADR-0023 已起草；typed/lifecycle 与 provider primitive 汇合到 native，不依赖 receiver/0046；未启用，全部保持 draft |
| 已完成 v0.33 lambda 实施链 | SPEC-0213→0214 | 0213 已把尾 lambda 规范化为普通 CallArgument；0214 已为全部 headerless lambda 纵向接通 contextual `it` 的 name/type/mode/ownership/drop/lowering anchor |
| 已完成 v0.33 project build | SPEC-0054 | 显式 package-qualified entry、validated unit frontend、single-object link、no-replace publish 与 argv run 已完成 |
| 现行多文件实施链 | SPEC-0025、0197、0198、0199、0187 | v0.32 §32 已启用，ADR-0020 已接受；0025/0197/0198 已完成 compilation-unit 名称、类型与 validated ownership 产物，0199 已完成 unit native lowering；0187 已完成 source-set v1、base/overlay unit diagnostics 与跨文件 definition |
| 仍有 Map 门禁 | SPEC-0024、0031、0037、0047 | 缺 Hashable/receiver/ownership/storage ADR 与完整公共 API；不能从顺序容器反推语义 |
| 已完成项目 source provider | SPEC-0052 | 消费 SPEC-0025 Stage 1 输入契约；ADR-0022 accepted，只产出本地 base snapshot，不等于已接入多文件 frontend 或项目构建 |
| 项目构建后继 | SPEC-0053/0054、0200 | 0054 已完成无依赖 build/run；0053/0200 独立承接依赖 lock/build |

审计据解锁价值选择 package/import，并把原先从 SPEC-0025 直接跳向 LSP/项目构建的缺口补成
名称→类型→所有权，再分叉到 native 与 LSP。v0.32 与 ADR-0020 已解除 compilation-unit
门禁，SPEC-0025 已完成名称层；后继分支的 ADR-0021/0022 均已接受，仍须完成各自前置 Spec。
公开 project build 已由 v0.33 完成；receiver 主线已由现行 v0.34 解锁并从 SPEC-0201 开始。
nullable `when`/`!!`、const/object、iteration 已分别物化为 v0.35/v0.36/v0.37 候选；
Map 继续等待 guide/ADR 门禁。

## Phase 0 Spec 队列

Phase 0 已细化为以下实际 Spec。状态以各 Spec 文件为准，并按前置关系依次推进：

| 顺序 | Spec | Goal | 前置条件 |
|---|---|---|---|
| 1 | [SPEC-0001](./0001-bootstrap-cargo-workspace.md) | 建立可检查的五 member Cargo workspace | [ADR-0002](../adr/0002-bootstrap-workspace-layout.md) accepted |
| 2 | [SPEC-0002](./0002-source-span-foundation.md) | 建立统一 source / `Span` 基础设施 | SPEC-0001 done；[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 3 | [SPEC-0003](./0003-structured-diagnostics.md) | 建立稳定、确定性的结构化诊断核心 | SPEC-0002 done；[ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 4 | [SPEC-0004](./0004-indexed-ast-foundation.md) | 建立保留 `Span` 的索引式 AST 基础 | SPEC-0001、0002 done |
| 5 | [SPEC-0005](./0005-language-fixture-harness.md) | 建立真实枚举 `.ko` 文件且零用例失败的 harness | SPEC-0001、0002、0003、0004 done |

Phase 0 的建议依赖关系：

```text
SPEC-0001
    └── SPEC-0002
            ├── SPEC-0003 ──┐
            └── SPEC-0004 ──┴── SPEC-0005
```

## 后续 Spec 候选

下表预留编号、Phase、单一 Goal 和依赖门槛。没有文件链接的候选项尚不是 Spec；只在前置
Phase 接近完成、适用 guide 已明确且必要 ADR 已接受时，才从模板创建文件并补齐可执行
计划。已有链接但尚未批准或仍带阻塞项的文件继续保持 `draft`，不得据此实施。这能避免
长期空壳 Spec 与实现事实漂移。

下列 Phase 队列中所有以 v0.35–v0.37 为语言依据的 `draft` 条目，共同前置都是：先明确该
候选对现行 v0.34 的重基与取代关系，再启用对应 guide 版本。各表中的“待启用”是这一完整
门禁的缩写，不能只凭启用动作跳过重基；v0.34 已完成该门禁，不再属于此集合。

### Phase 1：Lexer / Parser

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0006](./0006-deterministic-lexer.md) | Lexer 覆盖字面量、标识符、关键字和 trivia（`done`） | 0002、0003、0005 `done`；v0.5 已生效 |
| [0007](./0007-pratt-expression-parser.md) | Pratt parser 覆盖完整表达式优先级（`done`） | 0004、0006 `done`；v0.6 已生效；站立授权已记录 |
| [0008](./0008-declaration-parser.md) | 解析 `val` / `var` / `const val`、函数、泛型与调用点类型实参（`done`） | 0007 `done`；v0.7 已生效；站立授权已记录 |
| [0009](./0009-block-statement-parser.md) | 解析 block / statement 序列与函数 block body（`done`） | 0008 `done`；v0.8 已生效；站立授权已记录 |
| [0010](./0010-lambda-literal-parser.md) | 解析 lambda literal（`done`） | 0009 `done`；v0.9 已生效；站立授权已记录 |
| [0011](./0011-implicit-unit-return.md) | 解析具名函数省略返回标注时的隐式 `Unit`（`done`） | 0009 `done`；v0.9 已生效；站立授权已记录 |
| [0012](./0012-callable-parameter-and-call-argument-parser.md) | 解析统一 callable 参数 marker、typed call argument、命名实参与调用点 `borrow` / `&` 模式（`done`） | 0010、0011 `done`；v0.14 已生效；站立授权已记录 |
| [0013](./0013-local-val-destructuring-parser.md) | 解析 block / lambda body 内局部 `val` 解构（`done`） | 0012 `done`；适用 guide 已启用；站立授权已记录 |
| [0014](./0014-complete-file-parser.md) | 组合 0007–0013 已有节点为完整文件并实现声明分隔、跨声明恢复与级联抑制（`done`） | 0011、0013 `done`；v0.15 已封闭完整文件恢复契约；站立授权已记录 |
| [0062](./0062-top-level-declaration-separators.md) | 按 v0.16 修正顶层声明换行 / 分号分隔（`done`） | 0014 `done`；v0.16 已生效；当前持续 Goal 的站立授权 |
| [0015](./0015-package-import-parser.md) | 解析 `package` / Kotlin 风格 `import`（`done`） | 0014、0062 `done`；v0.17 已生效；当前持续 Goal 的站立授权 |
| [0016](./0016-control-flow-parser.md) | 解析 `if` / `when` / `super`、loop-family 与 jump 控制流（`done`） | 0009、0014 `done`；v0.18 已生效；当前持续 Goal 的站立授权 |
| [0063](./0063-postfix-error-propagation-parser.md) | 解析 postfix 错误传播 `?`（`done`） | 0016 `done`；v0.19 已生效；当前持续 Goal 的站立授权 |
| [0017](./0017-class-family-parser.md) | 解析 `value class` / `class` / `interface` / `enum class` / 具名 `object` / `companion object`（`done`） | 0014 `done`；v0.20 已生效；当前持续 Goal 的站立授权；不含接口委托 |
| [0064](./0064-interface-delegation-parser.md) | 增量解析 `Interface by valField` 接口实现委托（`done`） | 0017 `done`；v0.20 已生效；当前持续 Goal 的站立授权 |
| [0065](./0065-parser-module-decomposition.md) | 按职责拆分 Parser 内部模块并保持现有行为（`done`） | 0017、0062–0064 `done`；用户明确要求拆分 Parser |
| [0066](./0066-numeric-literal-suffixes.md) | 识别 `L` / `u` / `f` 数值后缀并在 Parser AST 保留规范化身份（`done`） | 0006、0007 `done`；v0.22 已生效；用户明确批准实施 |
| [0068](./0068-frontend-adversarial-matrix.md) | 增加 Lexer / 完整文件 Parser 对抗组合矩阵（`done`） | 0006、0014 `done`；当前持续 Goal 的站立授权 |
| [0069](./0069-parser-entry-adversarial-matrix.md) | 覆盖独立 expression / declaration / block Parser 对抗矩阵（`done`） | 0007–0009、0068 `done`；当前持续 Goal 的站立授权 |
| [0072](./0072-pratt-operator-matrix.md) | 锁定 Pratt 全优先级、结合性与不结合组矩阵（`done`） | 0007、0069 `done`；当前持续 Goal 的站立授权 |
| [0073](./0073-lexer-boundary-matrix.md) | 锁定固定词边界、符号最长匹配与注释优先级矩阵（`done`） | 0006、0068 `done`；当前持续 Goal 的站立授权 |
| [0074](./0074-parser-token-inventory-matrix.md) | 以完整词法片段库存覆盖四个公开 Parser 入口（`done`） | 0006、0014、0069、0073 `done`；当前持续 Goal 的站立授权 |
| [0075](./0075-parser-lexical-owner-placement-matrix.md) | 覆盖 lexical owner 在代表性语法位置的恢复矩阵（`done`） | 0006、0014、0069、0074 `done`；当前持续 Goal 的站立授权 |
| [0076](./0076-parser-diagnostic-witness-matrix.md) | 建立已发布 Parser 诊断的公开入口 witness 矩阵（`done`） | 0003、0014、0069、0075 `done`；当前持续 Goal 的站立授权 |
| [0077](./0077-parser-trivia-invariance-matrix.md) | 建立完整语法组合的非换行 trivia 等价矩阵（`done`） | 0006、0014、0073、0076 `done`；当前持续 Goal 的站立授权 |
| [0078](./0078-parser-line-break-boundary-matrix.md) | 建立 LF / CRLF 与 comment 换行的结构边界矩阵（`done`） | 0014、0016、0017、0062、0077 `done`；当前持续 Goal 的站立授权 |
| [0079](./0079-parser-prefix-truncation-matrix.md) | 建立合法完整语法逐 UTF-8 前缀的 EOF 恢复矩阵（`done`） | 0006、0014、0068、0074、0078 `done`；当前持续 Goal 的站立授权 |
| [0080](./0080-parser-token-omission-matrix.md) | 建立合法完整语法逐显著 token 的缺失恢复矩阵（`done`） | 0006、0014、0069、0075、0079 `done`；当前持续 Goal 的站立授权 |
| [0081](./0081-parser-lexical-poison-replacement-matrix.md) | 建立合法完整语法逐显著 token 的词法 poison 替换矩阵（`done`） | 0006、0014、0074、0075、0080 `done`；当前持续 Goal 的站立授权 |
| [0082](./0082-parser-token-duplication-matrix.md) | 建立合法完整语法逐显著 token 的重复恢复矩阵（`done`） | 0006、0014、0073、0080、0081 `done`；当前持续 Goal 的站立授权 |
| [0083](./0083-parser-lexical-poison-insertion-matrix.md) | 建立合法完整语法 token gap 的词法 poison 插入矩阵（`done`） | 0006、0014、0077、0081、0082 `done`；当前持续 Goal 的站立授权 |
| [0084](./0084-parser-adjacent-token-transposition-matrix.md) | 建立合法完整语法相邻 token 的交换恢复矩阵（`done`） | 0006、0014、0073、0080、0082、0083 `done`；当前持续 Goal 的站立授权 |
| [0085](./0085-parser-entry-prefix-truncation-matrix.md) | 建立 expression / declaration / block 独立入口逐 UTF-8 前缀恢复矩阵（`done`） | 0006–0009、0069、0079 `done`；当前持续 Goal 的站立授权 |
| [0086](./0086-parser-entry-token-omission-matrix.md) | 建立 expression / declaration / block 独立入口逐显著 token 缺失恢复矩阵（`done`） | 0006–0009、0069、0080、0085 `done`；当前持续 Goal 的站立授权 |
| [0087](./0087-parser-entry-token-duplication-matrix.md) | 建立 expression / declaration / block 独立入口逐显著 token 重复恢复矩阵（`done`） | 0006–0009、0069、0082、0085、0086 `done`；当前持续 Goal 的站立授权 |
| [0088](./0088-parser-entry-lexical-poison-replacement-matrix.md) | 建立 expression / declaration / block 独立入口逐显著 token 词法 poison 替换矩阵（`done`） | 0006–0009、0069、0081、0085–0087 `done`；当前持续 Goal 的站立授权 |
| [0089](./0089-parser-entry-lexical-poison-insertion-matrix.md) | 建立 expression / declaration / block 独立入口逐 token gap 词法 poison 插入矩阵（`done`） | 0006–0009、0069、0083、0085–0088 `done`；当前持续 Goal 的站立授权 |
| [0090](./0090-parser-entry-adjacent-token-transposition-matrix.md) | 建立 expression / declaration / block 独立入口相邻 token 交换恢复矩阵（`done`） | 0006–0009、0069、0084、0085–0089 `done`；当前持续 Goal 的站立授权 |
| [0091](./0091-parser-entry-trivia-invariance-matrix.md) | 建立 expression / declaration / block 独立入口非换行 trivia 等价矩阵（`done`） | 0006–0009、0069、0077、0085–0090 `done`；当前持续 Goal 的站立授权 |
| [0092](./0092-parser-entry-line-break-boundary-matrix.md) | 建立 expression / declaration / block 独立入口结构性换行边界矩阵（`done`） | 0006–0009、0069、0078、0085–0091 `done`；当前持续 Goal 的站立授权 |
| [0093](./0093-parser-entry-adversarial-output-invariants.md) | 强化 expression / declaration / block 对抗矩阵的 Lexer / AST / diagnostic / root 不变量（`done`） | 0006–0009、0068、0069、0085–0092 `done`；当前持续 Goal 的站立授权 |
| [0094](./0094-parser-token-inventory-output-invariants.md) | 强化 120-item token inventory 的四入口 Lexer / AST / diagnostic / root / directive 不变量（`done`） | 0006–0009、0014、0073、0074、0093 `done`；当前持续 Goal 的站立授权 |
| [0095](./0095-parser-lexical-owner-output-invariants.md) | 强化 lexical-owner 矩阵的 Lexer / AST / diagnostic / root / sentinel 不变量（`done`） | 0006、0014、0075、0093、0094 `done`；当前持续 Goal 的站立授权 |
| [0096](./0096-parser-diagnostic-witness-output-invariants.md) | 强化 Parser diagnostic witness 的 Lexer / AST / diagnostic / root / directive 不变量（`done`） | 0003、0014、0076、0093–0095 `done`；当前持续 Goal 的站立授权 |
| [0097](./0097-parser-trivia-output-invariants.md) | 强化 Parser trivia 等价矩阵的 Lexer / AST / diagnostic / root / directive 不变量（`done`） | 0006、0014、0077、0093–0096 `done`；当前持续 Goal 的站立授权 |
| [0098](./0098-parser-line-break-output-invariants.md) | 强化 line-break carrier 分段及 Parser AST / diagnostic / root / directive 不变量（`done`） | 0006、0014、0078、0093–0097 `done`；当前持续 Goal 的站立授权 |
| [0099](./0099-parser-file-mutation-output-invariants.md) | 强化六个完整文件恢复矩阵的共享 AST / diagnostic / root / directive 不变量（`done`） | 0006、0014、0079–0084、0093–0098 `done`；当前持续 Goal 的站立授权 |
| [0100](./0100-parser-entry-line-break-output-invariants.md) | 统一 line-break carrier 真源并强化独立入口 AST / diagnostic / typed-root 不变量（`done`） | 0006–0009、0078、0092、0093–0099 `done`；当前持续 Goal 的站立授权 |
| [0101](./0101-parser-entry-trivia-output-invariants.md) | 强化独立入口 trivia 精确分段并复用双解析 shape（`done`） | 0006–0009、0077、0091、0093–0100 `done`；当前持续 Goal 的站立授权 |
| [0102](./0102-frontend-adversarial-output-invariants.md) | 强化完整文件对抗矩阵的双 Lexer / 双 Parser 公开产物不变量（`done`） | 0006、0014、0068、0093–0101 `done`；当前持续 Goal 的站立授权 |
| [0103](./0103-lexer-boundary-output-invariants.md) | 强化固定词 / 符号边界矩阵的双 Lexer 公开产物不变量（`done`） | 0006、0073、0093–0102 `done`；当前持续 Goal 的站立授权 |
| [0104](./0104-pratt-operator-output-invariants.md) | 强化 Pratt 运算符矩阵的双 Lexer / 双 Parser 与完整诊断不变量（`done`） | 0006、0007、0072、0093–0103 `done`；当前持续 Goal 的站立授权 |
| [0105](./0105-parser-entry-adversarial-lexer-invariants.md) | 强化独立入口对抗矩阵的双 Lexer 确定性不变量（`done`） | 0006–0009、0069、0093、0103–0104 `done`；当前持续 Goal 的站立授权 |
| [0106](./0106-parser-token-inventory-lexer-invariants.md) | 强化 token inventory 分类、四入口与定向回归的双 Lexer 不变量（`done`） | 0006–0009、0014、0074、0094、0103–0105 `done`；当前持续 Goal 的站立授权 |
| [0107](./0107-parser-lexical-owner-lexer-invariants.md) | 强化 lexical-owner 矩阵的双 Lexer 确定性不变量（`done`） | 0006、0014、0075、0095、0103–0106 `done`；当前持续 Goal 的站立授权 |
| [0108](./0108-parser-diagnostic-witness-lexer-invariants.md) | 强化 diagnostic-witness 矩阵的双 Lexer 确定性不变量（`done`） | 0003、0006–0009、0014、0076、0096、0103–0107 `done`；当前持续 Goal 的站立授权 |
| [0109](./0109-parser-trivia-lexer-invariants.md) | 强化 trivia 等价矩阵的双 Lexer 确定性不变量（`done`） | 0006、0014、0077、0097、0103–0108 `done`；当前持续 Goal 的站立授权 |
| [0110](./0110-parser-line-break-lexer-invariants.md) | 强化完整文件 line-break 边界矩阵的双 Lexer 确定性不变量（`done`） | 0006、0014、0078、0098、0100、0103–0109 `done`；当前持续 Goal 的站立授权 |
| [0111](./0111-parser-file-mutation-lexer-invariants.md) | 强化六个完整文件 mutation 矩阵的共享双 Lexer 确定性不变量（`done`） | 0006、0014、0079–0084、0099、0103–0110 `done`；当前持续 Goal 的站立授权 |
| [0112](./0112-parser-entry-line-break-lexer-invariants.md) | 强化独立入口 line-break 边界矩阵的双 Lexer 确定性不变量（`done`） | 0006–0009、0078、0092、0100、0103–0111 `done`；当前持续 Goal 的站立授权 |
| [0113](./0113-parser-entry-trivia-lexer-invariants.md) | 强化独立入口 trivia 等价矩阵的双 Lexer 确定性不变量（`done`） | 0006–0009、0077、0091、0101、0103–0112 `done`；当前持续 Goal 的站立授权 |
| [0114](./0114-parser-entry-mutation-lexer-invariants.md) | 强化六个独立入口 mutation 矩阵的共享双 Lexer 确定性不变量（`done`） | 0006–0009、0085–0090、0093、0103–0105、0111–0113 `done`；当前持续 Goal 的站立授权 |
| [0115](./0115-fixture-frontend-output-invariants.md) | 强化 pass / fail fixture 的双 Lexer / 双 Parser 公开产物不变量（`done`） | 0005–0017、0062–0066、0103–0114 `done`；当前持续 Goal 的站立授权 |
| [0175](./0175-call-argument-lambda-boundary.md) | 修复 block 内 call argument lambda 被 outer block stop 误判（`done`） | 0010、0012 `done`；实施时适用 v0.25；当前持续 Goal 的站立授权 |
| [0213](./0213-trailing-lambda-call-parser.md) | 把同行尾 lambda 规范化为最后一个普通 CallArgument（`done`） | 0010、0012、0014、0175 `done`；v0.33 已启用 |
| [0214](./0214-implicit-it-lambda-parameter.md) | 为 headerless lambda 建立 contextual 隐式 `it` 的 AST/name/type/ownership 纵向事实（`done`） | 0010、0018、0019、0032、0067、0173、0197、0198、0213 `done` |
| [0201](./0201-instance-receiver-mode-parser.md) | 解析 instance member 的缺省/显式 Borrow、Inout、Value receiver marker（`done`） | 0017、0064、0176 `done`；v0.34 已启用 |

### Phase 2：名称与类型检查

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0018](./0018-single-file-name-resolution.md) | 完成单文件声明收集、作用域和名称诊断（`done`） | 0014 `done`；v0.21 已生效；当前持续 Goal 的站立授权 |
| [0019](./0019-basic-type-checking.md) | 检查基础类型、局部推导、隐式 `Unit` / 显式返回类型与 `Nothing`（`done`） | 0018、0066 `done`；v0.22 已生效 |
| [0020](./0020-nominal-generic-interface-types.md) | 检查泛型及 class / interface / enum / value class 名义类型与窄化接口委托（`done`） | 0019、0017、0064 `done`；v0.23 已明确启用 |
| [0021](./0021-when-exhaustiveness-smart-cast.md) | 实现 `when` 穷尽性与 smart cast（`done`） | 0020、0016 `done`；v0.24 已明确启用 |
| [0022](./0022-copyable-structural-destructuring.md) | 推导条件 `Copyable`、检查有限内联布局与结构化解构类型（`done`） | 0019、0020 `done`；v0.25 已明确启用 |
| [0067](./0067-callable-type-checking.md) | 检查 callable 选择、实参映射、参数模式与 place/temporary 类别（`done`） | 0019、0020、0022 `done`；实施时适用 v0.25 callable 契约；当前持续 Goal 的站立授权 |
| [0023](./0023-sequential-container-types.md) | 检查顺序容器的名义类型、元素可存储性、核心构造和索引 place 类型（`done`） | 0020、0022、0067 `done`；v0.6 已生效 |
| [0202](./0202-nullable-when-flow-facts.md) | 发布 nullable `when` 的剩余域、alternative 交集与 body non-null typed facts（`draft`） | 0019、0021 `done`；v0.35 待启用 |
| [0205](./0205-non-null-assertion-facts.md) | 发布 `!!` 的 operand/category 与 Copy/Consume extraction typed descriptor（`draft`） | 0019、0022、0067 `done`；v0.35 待启用 |
| [0026](./0026-associated-constant-evaluation.md) | 选择单文件 object/companion const 并发布 typed ConstValue/dependency/use facts（`draft`） | 0017–0020 `done`；v0.36 待启用 |
| [0210](./0210-multifile-associated-constants.md) | 复用 0026 evaluator 集成跨文件 associated const/dependency/cycle（`draft`） | 0026；0025、0197；v0.36 待启用；ADR-0020 `accepted` |
| [0130](./0130-name-resolution-frontend-input-invariants.md) | 强化名称解析 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0018、0093、0115、0128、0129 `done`；当前持续 Goal 的站立授权 |
| [0131](./0131-type-checking-frontend-input-invariants.md) | 强化类型检查核心 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0019–0023、0067、0130 `done`；当前持续 Goal 的站立授权 |
| [0132](./0132-callable-type-frontend-input-invariants.md) | 强化 callable 类型 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0067、0130、0131 `done`；当前持续 Goal 的站立授权 |
| [0133](./0133-container-type-frontend-input-invariants.md) | 强化顺序容器类型 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0023、0130–0132 `done`；当前持续 Goal 的站立授权 |
| [0134](./0134-copyability-type-frontend-input-invariants.md) | 强化 copyability 类型 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0022、0130–0133 `done`；当前持续 Goal 的站立授权 |
| [0173](./0173-lambda-parameter-contract-facts.md) | 让唯一期望函数类型的 lambda 采用并保存 Value/Borrow/Inout 参数契约（`done`） | 0019、0067 `done`；实施时适用 v0.25；当前持续 Goal 的站立授权 |
| [0177](./0177-generic-callable-instantiation.md) | 泛型 callable 显式/实参推导实例化与稳定实例 identity（`done`） | 0020、0022、0032、0067 `done`；v0.28 已生效 |
| [0174](./0174-overload-lambda-candidate-isolation.md) | 对多 overload 候选逐一隔离检查 lambda expected contract/body（`done`） | 0067、0173、0177 `done` |
| [0178](./0178-jump-target-checking.md) | 检查 break/continue 最近词法 loop 与 callable boundary（`done`） | 0016、0019 `done`；现行 v0.18/v0.28 语义已封闭；当前持续 Goal 的站立授权 |
| [0179](./0179-sequential-iteration-typed-plan.md) | 为三种 intrinsic 顺序容器发布 provider、元素与名称/discard/value-class Borrow binding typed plan（`draft`） | 0016、0018–0020、0022、0023、0178 `done`；v0.37 待启用；ADR-0023 `proposed` |
| [0180](./0180-instance-receiver-typed-facts.md) | 发布 instance member、`this` 与 Borrow-only 委托的 receiver typed facts（`done`） | 0020、0067、0176、0177、0201 `done`；v0.34 已启用 |
| [0183](./0183-constructor-typed-facts.md) | 发布普通/泛型 nominal、enum case 与 intrinsic Box constructor 的 target、实例类型、Value 参数映射和字段/case 顺序 typed fact（`done`） | 0020、0022、0067、0177 `done`；v0.29 已生效 |
| 0024 | 检查 `Map` / `MutableMap` 的 key 契约、value 所有权约束和查询结果类型 | 0020；新 guide 明确 key 等价关系、返回所有权与修改 API |
| [0025](./0025-multifile-package-import-name-resolution.md) | 建立 compilation-unit package/import 名称解析（`done`） | 0015、0018 `done`；v0.32 已启用；ADR-0005/0020 `accepted` |
| [0197](./0197-multifile-type-checking.md) | 在统一声明身份上完成跨文件签名/body 类型检查（`done`） | 0025 `done`；v0.32 已启用；ADR-0020 `accepted` |
| [0218](./0218-compilation-unit-assignment-facts.md) | 发布普通 `=` 的 target/value/operator/storage-type/control descriptor，移除合法路径的 `Deferred(Assignment)`（`done`） | 0019、0020、0197 `done`；现行 v0.34；当前持续 Goal 的站立授权 |
| [0219](./0219-compilation-unit-runtime-field-layout-facts.md) | 发布 ordinary-class owner-instance-qualified concrete runtime field layout（`done`） | 0020、0177、0197 `done`；已解除 0191 nested generic field recipe 的 frontend 前置；当前持续 Goal 的站立授权 |

Phase 2 roadmap 中的“泛型单态化类型层面准备”已物化为 SPEC-0177，随后由 SPEC-0174
完成 overload-lambda 候选隔离。两项均实施现行 v0.28 语义；它们不是
SPEC-0027–0032 的前置，但进入依赖具体实例的 SSA / codegen Goal 前必须完成。

### Phase 3：所有权与借用

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0027](./0027-variable-ownership-use-after-move.md) | 建立变量所有权状态并检测 use-after-move（`done`） | 0019、0020、0022、0067 `done`；当前持续 Goal 的站立授权 |
| [0028](./0028-conditional-copy-structural-move.md) | 实现条件复制、移动与消费式解构检查（`done`） | 0022、0027 `done`；当前持续 Goal 的站立授权 |
| [0176](./0176-borrow-default-parameter-contracts.md) | 迁移无 marker `Borrow`、声明侧 `own` 与既有三态参数事实（`done`） | 0012、0067、0173、0028 `done`；v0.26 已生效；当前持续 Goal 的站立授权 |
| [0029](./0029-call-loans-drop-points.md) | 检查 `Value` / `Borrow` / `Inout` 调用效果、调用点 `borrow` / `&` 冲突并确定 ASAP 析构点（`done`） | 0176 `done`；v0.26 已生效；当前持续 Goal 的站立授权 |
| [0203](./0203-nullable-when-ownership.md) | 检查 nullable `when` proof view、Copy/Consume extraction 与 branch drop（`draft`） | 0028、0029 `done`；0202；v0.35 待启用 |
| [0206](./0206-non-null-assertion-ownership.md) | 检查 `!!` whole-root consumption、abort edge 与 move/loan/drop facts（`draft`） | 0028、0029 `done`；0205；v0.35 待启用 |
| [0208](./0208-constant-materialization-ownership.md) | 把 const use 解释为 scalar inline 或独立 String temporary owner（`draft`） | 0028、0029 `done`；0026；v0.36 待启用 |
| [0211](./0211-sequential-iteration-ownership.md) | 发布 whole-loop source loan、逐轮 Borrow binding 与全部退出 cleanup facts（`draft`） | 0029、0030、0032 `done`；0179；v0.37 待启用；ADR-0023 `proposed` |
| [0030](./0030-sequential-container-element-ownership.md) | 检查顺序容器元素 place 的读取、借用、替换与析构所有权规则（`done`） | 0023、0029 `done`；v0.26 生效；当前持续 Goal 的站立授权 |
| [0181](./0181-instance-receiver-ownership.md) | 检查 instance receiver/`this` 的字段访问、loan、移动、capture 与 drop（`done`） | 0029、0032、0180 `done`；含 Value `StaticSelf` conditional receiver-drop fact，Phase 4 消费由 0191 承接 |
| 0031 | 检查 `Map` / `MutableMap` 查询和修改的 key / value 所有权规则 | 0024、0029；新 guide 明确完整 Map 契约 |
| [0032](./0032-move-closure-transferable.md) | 检查 move closure 与 `Transferable`（`done`） | 0020、0029 `done`；v0.27 已生效；当前持续 Goal 的站立授权 |
| [0188](./0188-constructor-ownership-effects.md) | 检查 constructor ordered Value delivery、construction root owner 与 drop obligation（`done`） | 0183、0029 `done`；v0.29 已生效；当前持续 Goal 的站立授权 |
| [0198](./0198-multifile-ownership-checking.md) | 发布跨文件 call/constructor 的 loan、move、drop 与 capture facts（`done`） | 0197 `done`；v0.32 已启用；ADR-0020 `accepted` |
| [0215](./0215-lambda-body-result-drop-facts.md) | 发布 lambda body 隐式 MoveOnly 结果与内部 owner drop facts（`done`） | 0029、0032、0197、0198 `done`；当前持续 Goal 的站立授权 |
| [0216](./0216-control-result-drop-facts.md) | 传播 MoveOnly `if` / `when` Consume usage 并发布精确 branch result/drop facts（`done`） | 0029、0197、0198、0215 `done`；当前持续 Goal 的站立授权 |
| [0217](./0217-lambda-value-parameter-drop-facts.md) | 发布 MoveOnly Value lambda 参数 entry/last-use/exit drop facts（`done`） | 0029、0032、0197、0198、0215、0216 `done`；当前持续 Goal 的站立授权 |

### Phase 4：SSA、LLVM 与原生 AOT

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0033](./0033-typed-ssa-ir-verifier.md) | 实现最小 typed SSA IR 与 verifier（`done`） | 0021、0029、0177、0174 `done`；[ADR-0006](../adr/0006-typed-ssa-block-parameters.md) `accepted` |
| [0034](./0034-scalar-control-flow-llvm-lowering.md) | 把标量表达式和控制流经 verified SSA lower 到 verified LLVM IR（`done`） | 0033 `done`；[ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md) `accepted`；完整 `for` 已按 runtime 依赖迁移至候选 0182 |
| [0035](./0035-aggregate-class-allocation-drop.md) | 建立 typed SSA/LLVM 聚合、class/Box heap owner、allocation 与显式 drop/free 后端基元（`done`） | 0034、0029 `done`；[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted`；0183/0188 facts 与 0184 lowering 已完成 |
| [0036](./0036-sequential-container-runtime.md) | 生成顺序容器的单一连续缓冲区基元、边界检查和 drop 路径（`done`） | 0023、0030、0035 `done`；[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted` |
| [0185](./0185-declarative-type-roots-codegen.md) | 让声明型 class/value class/interface/enum roots 与既有标量 entry 共存（`done`） | 0020、0034 `done`；不实现 constructor 或 nominal operation |
| [0186](./0186-target-layout-preflight.md) | 在 LLVM 复合类型构造前预检 target size/alignment/stride（`done`） | 0033、0035、0036、0038 `done`；[ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md)、[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted`；源码用户诊断映射已由 0184 完成 |
| [0212](./0212-borrowed-sequential-iteration-ssa.md) | 封闭 borrowed length、Int/size_t bridge 与 provider SSA/LLVM primitives（`draft`） | 0034、0036、0186、0195 `done`；v0.37 待启用；ADR-0023 `proposed` |
| [0182](./0182-sequential-for-lowering.md) | 把 validated `for` typed/ownership/provider plans 拼装到 SSA/LLVM/native（`draft`） | 0179、0211、0212；v0.37 待启用；ADR-0023 `proposed`；不依赖 0181/0191/0046 |
| [0184](./0184-nominal-construction-lowering.md) | 把 0183/0188 的 nominal/enum/Box constructor、projection、destructuring、ordered delivery 与 root drop facts lower 到 SPEC-0035 aggregate/heap-owner SSA，并把 0186 布局失败映射到来源类型诊断（`done`） | 0183、0188、0035、0186 `done`；v0.29 已生效；instance method receiver 仍排除 |
| [0191](./0191-instance-receiver-lowering.md) | 把 instance receiver 与 Borrow-only 静态委托 lower 到 SSA/LLVM（`in-progress`） | 0180、0181、0219、0220、0221 `done`；基础 receiver/native、ordinary-class Inout/MoveOnly payload、无状态 object Borrow、non-generic enum Borrow/Value receiver 与 MoveOnly 空 case owner、non-generic value/enum Inout exclusive ABI/inline field read/MoveOnly root take-rebind、Copyable/MoveOnly field inline mutation（含 MoveOnly owner、旧字段析构与递归 drop glue）、concrete `StaticSelf` default/`super<I>`、非泛型 Value interface default、abstract requirement→本地 override/inherited default（含有限 inherited `List` / 布局参数无关单参数 class recipe）、generic delegation slots、same/changed-identity chain、参数无关/direct owner `T`、pointer-like direct `T?` 及 exact descriptor 授权的有限递归 `List` / 单参数 ordinary-class runtime recipe 已完成，含 nested replacement/delegation native；object Inout/Value 由 guide 明确禁止，MoveOnly field read/部分移动、参数增长型 runtime 与 dependent/self-growing/value-class/多参数/其他 nested inherited recipe 保持门禁；ADR-0016 accepted |
| [0220](./0220-compilation-unit-pointer-nullable-storage-lowering.md) | 把 class/Box/Rc concrete nullable 与 generic direct `T?` 字段接入 compilation-unit SSA/LLVM/native（`done`） | 0035、0184、0196、0199、0218、0219 `done`；ADR-0017 `accepted`；当前持续 Goal 的站立授权；不含 nullable control flow、inline nullable ABI 或递归 owner-definition cycle |
| [0221](./0221-move-only-empty-enum-case-lowering.md) | 修复整体 MoveOnly 的 non-generic enum 空 payload case owner construction/transfer/drop（`done`） | 0184、0188、0198、0199 `done`；local/return/Value call/Value receiver 与 Empty+Full native 已闭环；不含 generic enum、MoveOnly `when` 或新 receiver IR |
| [0204](./0204-pointer-nullable-when-lowering.md) | 把 owned-root/temporary class/Box/Rc nullable `when` lower 到 verified SSA/LLVM/native（`draft`） | 0034、0184、0196 `done`；0202、0203；v0.35 待启用；ADR-0017 `accepted` |
| [0207](./0207-pointer-non-null-assertion-lowering.md) | 把 owned-root/temporary class/Box/Rc `!!` lower 到 NullableBranch/Take/Abort 与 native（`draft`） | 0034、0039、0184、0196 `done`；0205、0206；v0.35 待启用；ADR-0017 `accepted` |
| [0209](./0209-associated-constant-lowering.md) | 把单文件 scalar/Char/String const use 重新物化到 SSA/LLVM/native（`draft`） | 0034、0039、0185、0189、0192 `done`；0026、0208；v0.36 待启用 |
| 0037 | 生成 `Map` / `MutableMap` 查询与修改的 runtime 基元 | 0024、0031、0035；[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted`；接受 Map 存储策略 ADR |
| [0038](./0038-closure-environment-codegen.md) | 生成捕获闭包环境和无捕获函数指针（`done`） | 0032、0034、0035 `done`；[ADR-0009](../adr/0009-concrete-closure-internal-abi.md) `accepted` |
| [0039](./0039-native-object-entry-link.md) | 生成 object、链接显式 entry，并为后续标准库 `error()` identity 提供 abort 边界（`done`） | 0035、0038 `done`；[ADR-0010](../adr/0010-first-native-object-and-linker-contract.md) `accepted`；源码 entry 选择与标准库 identity 不按名称猜测 |
| [0040](./0040-dwarf-line-tables-lldb.md) | 生成首个 DWARF 行表并用 LLDB 验收源码断点（`done`） | 0039 `done`；[ADR-0011](../adr/0011-first-dwarf-line-mapping.md) `accepted`；当前持续 Goal 的站立授权 |
| [0199](./0199-multifile-native-lowering.md) | 对完整 unit 做 reachability/单态化并生成单 object executable（`done`） | 0198 `done`；v0.32 已启用；ADR-0020 `accepted` |
| 0041 | 提供用户可见 `extern` FFI | 0039；新 guide 定义 FFI 与所有权边界，非 v1 主路径 |

### Phase 5：最小标准库

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0042](./0042-standard-library-bootstrap.md) | 用编译器构建并运行 `lang-std` 目标语言源码（`done`） | 0039 `done`；[ADR-0008](../adr/0008-internal-value-and-allocation-abi.md)、[ADR-0012](../adr/0012-standard-library-bootstrap.md) `accepted` |
| [0043](./0043-standard-error-abort.md) | 发布标准 `error()` identity 并接入既有 Abort（`done`） | 0039、0042 `done`；[ADR-0010](../adr/0010-first-native-object-and-linker-contract.md)、[ADR-0012](../adr/0012-standard-library-bootstrap.md) `accepted` |
| [0189](./0189-standard-println-output.md) | 发布 `println(String)` Borrow identity，并把非插值 UTF-8 literal 接入 stdout native runtime（`done`） | 0039、0042、0043、0184 `done`；当前持续 Goal 的站立授权 |
| [0044](./0044-standard-pair-result.md) | 实现条件可复制的 `Pair` 与 `Result`（`done`） | 0042、0028、0035、0183、0185、0188、0184 `done`；v0.29 固定且 v0.30 沿用 `Result.Ok(success: T)`；当前持续 Goal 的站立授权 |
| [0045](./0045-shared-rc-owner.md) | 实现非 nullable 共享 `Rc` owner core（`done`；通用 Borrow/nullable 后继为 0195/0196） | 0042、0028、0035、0183、0185、0188、0184 `done`；v0.30 已生效；[ADR-0015](../adr/0015-shared-owner-runtime-abi.md) `accepted`；当前持续 Goal 的站立授权 |
| 0046 | 提供 Array / List / MutableList 的目标语言公共 mutation/relocation API 与顺序算法 | 0036、0043、0180、0181、0191；新 guide 封闭 intrinsic container member 与 relocation effect；iteration provider 独立，不因本项完成而自动存在 |
| 0047 | 提供 Map / MutableMap 的目标语言公共 API 与键值算法 | 0037、0043、0045；新 guide 明确完整 Map 契约 |
| 0048 | 为顺序容器实现 `map` / `filter` / `reduce` / `forEach` | 0046、0038 |
| [0192](./0192-general-string-runtime.md) | 实现可持有、传递和返回的 UTF-8 `String` runtime（`done`） | 0042、0043、0184、0189、0195 `done`；v0.31 已启用；[ADR-0018](../adr/0018-string-owner-runtime-abi.md) `accepted` |
| 0049 | 实现同步 File / BufferedReader / 标准流 | 0026、0039、0043、0180、0181、0191、0192；新 guide 封闭具体 IO API；接受同步 IO runtime ABI ADR |
| 0050 | 实现 thread / channel | 0032、0042、0044、0180、0181、0191；新 guide 封闭具体返回类型与 API；接受 thread/channel runtime ABI ADR |
| 0051 | 实现目标语言测试发现与断言 runner | 0042；新 guide 定义最小 `@Test` 语法 |

### Phase 6：工具链

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0190](./0190-public-single-file-build-run.md) | 公开单文件 `kovenc build/run` 并验证仓库外 Hello World（`done`） | 0039、0042、0043、0184、0189 `done`；当前持续 Goal 的站立授权 |
| [0193](./0193-conventional-zero-argument-main.md) | 省略 `--entry` 时选择唯一顶层 `fun main(): Unit`（`done`） | 0190 `done`；v0.30 已生效；不接入 argv；当前持续 Goal 的站立授权 |
| [0194](./0194-parameterized-main-argv.md) | 接入 `fun main(args: Array<String>): Unit` 与 argv owner（`done`） | 0193 `done`、0192 `done`；[ADR-0019](../adr/0019-parameterized-process-entry-bridge.md) `accepted`；v0.31 现行语义 |
| [0195](./0195-interprocedural-borrow-lowering.md) | 把 Borrow 参数与调用期 loan lower 到 typed SSA/LLVM（`done`） | 0029、0034、0035、0045 `done`；[ADR-0016](../adr/0016-interprocedural-borrow-abi.md) `accepted`；当前持续 Goal 的站立授权 |
| [0196](./0196-nullable-handle-lowering.md) | 把 pointer-like nullable owner lower 到独立 SSA/null-niche LLVM（`done`） | 0045、0195 `done`；[ADR-0017](../adr/0017-nullable-handle-ssa-abi.md) `accepted`；当前持续 Goal 的站立授权 |
| [0052](./0052-minimal-project-manifest-source-set.md) | 解析最小 `project.toml` 并产生本地 base source-set snapshot（`done`） | 0025 Stage 1 `done`；v0.32 已启用；ADR-0020/0022 `accepted`；当前持续 Goal 的站立授权 |
| 0053 | 实现依赖解析与确定性 `project.lock` 核心 | 0052；接受解析 / 锁定策略 ADR |
| [0054](./0054-local-project-build-run.md) | 由 package CLI 编排显式 entry 的无依赖本地 project build/run（`done`） | 0052、0199 `done`；v0.33 已启用；ADR-0020/0022 `accepted`；不等待 0053 |
| 0200 | 编排 dependency-aware project build | 0053、0054；跨 compilation-unit export/ABI guide 与 ADR |
| [0055](./0055-single-document-lsp-diagnostics.md) | 让 LSP 对打开的单文档发布完整 frontend 诊断（`done`） | 0002、0003、0018–0023、0027–0030、0032 `done`；跨文件诊断继续等待 0025；当前持续 Goal 的站立授权 |
| [0056](./0056-single-document-definition.md) | 让 LSP 对打开 buffer 提供单文档语义跳转定义（`done`） | 0055、0018–0023、0067 `done`；跨文件目标明确排除；当前持续 Goal 的站立授权 |
| [0187](./0187-multifile-lsp-diagnostics-definition.md) | 把 LSP 诊断与跳转定义扩展到跨文件 package/import（`done`） | 0025、0197、0198、0055、0056 `done`；v0.32 已启用；ADR-0020/0021 `accepted` |
| [0057](./0057-conservative-source-formatter.md) | 实现稳定、幂等的格式化器（`done`） | 0014、0006 `done`；[ADR-0013](../adr/0013-conservative-source-formatting.md) `accepted` |
| [0058](./0058-textmate-grammar.md) | 提供 TextMate grammar 与回归 fixture（`done`） | 0014、0015 `done`；当前持续 Goal 的站立授权 |
| [0059](./0059-tree-sitter-grammar.md) | 提供 Tree-sitter grammar 与 corpus（`done`） | 0014、0015 `done`；当前持续 Goal 的站立授权 |
| [0070](./0070-tree-sitter-word-contract.md) | 锁定 Tree-sitter external scanner 与生产 Lexer 词表契约（`done`） | 0006、0059 `done`；当前持续 Goal 的站立授权 |
| [0071](./0071-textmate-lexical-contract.md) | 执行 TextMate symbol / literal 正则并交叉验证生产 Lexer（`done`） | 0006、0058 `done`；当前持续 Goal 的站立授权 |
| [0116](./0116-grammar-bridge-frontend-invariants.md) | 强化 TextMate / Tree-sitter 交叉测试的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0014、0058–0059、0070–0071、0103–0115 `done`；当前持续 Goal 的站立授权 |
| [0117](./0117-parser-expression-suite-output-invariants.md) | 强化 expression Parser 核心 suite 的双 Lexer / 双 Parser 产物不变量（`done`） | 0006–0007、0093、0103–0105、0115–0116 `done`；当前持续 Goal 的站立授权 |
| [0118](./0118-parser-declaration-suite-output-invariants.md) | 强化 declaration Parser 核心 suite 的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0008、0093、0103–0105、0115–0117 `done`；当前持续 Goal 的站立授权 |
| [0119](./0119-parser-block-suite-output-invariants.md) | 强化 block Parser 核心 suite 的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0009、0093、0103–0105、0115–0118 `done`；当前持续 Goal 的站立授权 |
| [0120](./0120-parser-lambda-suite-output-invariants.md) | 强化 lambda Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0010、0093、0103–0105、0115–0119 `done`；当前持续 Goal 的站立授权 |
| [0121](./0121-parser-call-argument-suite-output-invariants.md) | 强化 call argument Parser 核心 suite 双入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0012、0093、0103–0105、0115–0120 `done`；当前持续 Goal 的站立授权 |
| [0122](./0122-parser-local-destructuring-suite-output-invariants.md) | 强化 local destructuring Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0013、0093、0103–0105、0115–0121 `done`；当前持续 Goal 的站立授权 |
| [0123](./0123-parser-control-flow-suite-output-invariants.md) | 强化 control-flow Parser 核心 suite 双入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0016、0093、0103–0105、0115–0122 `done`；当前持续 Goal 的站立授权 |
| [0124](./0124-parser-error-propagation-suite-output-invariants.md) | 强化错误传播 Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0016、0063、0093、0103–0105、0115–0123 `done`；当前持续 Goal 的站立授权 |
| [0125](./0125-parser-class-family-suite-output-invariants.md) | 强化 class-family Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0017、0093、0103–0105、0115–0124 `done`；当前持续 Goal 的站立授权 |
| [0126](./0126-parser-interface-delegation-suite-output-invariants.md) | 强化接口委托 Parser 核心 suite 的双 Lexer / 双 declaration Parser 产物不变量（`done`） | 0006、0017、0064、0093、0103–0105、0115–0125 `done`；当前持续 Goal 的站立授权 |
| [0127](./0127-parser-implicit-unit-suite-output-invariants.md) | 强化隐式 Unit Parser 核心 suite 的双 Lexer / 双 declaration Parser 产物不变量（`done`） | 0006、0011、0093、0103–0105、0115–0126 `done`；当前持续 Goal 的站立授权 |
| [0128](./0128-parser-file-suite-output-invariants.md) | 强化完整文件 Parser 核心 suite 双入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0014、0093、0103–0105、0115–0127 `done`；当前持续 Goal 的站立授权 |
| [0129](./0129-lexer-core-suite-output-invariants.md) | 强化 Lexer 核心 suite 的双运行公开产物不变量（`done`） | 0006、0073、0093、0103、0106、0115、0128 `done`；当前持续 Goal 的站立授权 |
| [0135](./0135-parser-internal-lexer-input-invariants.md) | 强化 Parser 私有算法单元测试的双 Lexer 输入不变量（`done`） | 0006–0009、0093、0129、0130–0134 `done`；当前持续 Goal 的站立授权 |
| [0136](./0136-frontend-internal-error-determinism.md) | 强化 Lexer / Parser 内部边界错误的精确双运行确定性（`done`） | 0006–0011、0117–0120、0127、0129、0135 `done`；当前持续 Goal 的站立授权 |
| [0137](./0137-parser-invalid-lexeme-stream-matrix.md) | 建立 Parser 非法 Lexeme 流的六消费者精确拒绝矩阵（`done`） | 0006–0009、0065、0135–0136 `done`；当前持续 Goal 的站立授权 |
| [0138](./0138-parser-invalid-lexical-owner-matrix.md) | 建立 Parser 非法 lexical-owner 流的精确拒绝矩阵（`done`） | 0006–0009、0065、0135–0137 `done`；当前持续 Goal 的站立授权 |
| [0139](./0139-parser-invalid-recovery-diagnostic-matrix.md) | 建立 Parser recovery diagnostic/token 关联拒绝矩阵（`done`） | 0003、0006–0009、0065、0135–0138 `done`；当前持续 Goal 的站立授权 |
| [0140](./0140-parser-lexer-diagnostic-anchor-contract.md) | 锁定 Parser 的 Lexer diagnostic anchor 契约（`done`） | 0003、0006–0009、0065、0129、0135–0139 `done`；当前持续 Goal 的站立授权 |
| [0141](./0141-parser-lexer-diagnostic-stream-identity.md) | 锁定 Parser 的 Lexer diagnostic 流身份（`done`） | 0002–0003、0006–0009、0065、0129、0135–0140 `done`；当前持续 Goal 的站立授权 |
| [0142](./0142-parser-lexer-poison-diagnostic-coverage.md) | 锁定 Parser 的 lexical poison 诊断覆盖（`done`） | 0003、0006–0009、0065、0129、0135–0141 `done`；当前持续 Goal 的站立授权 |
| [0143](./0143-parser-lexer-diagnostic-anchor-uniqueness.md) | 锁定 Parser 的 Lexer diagnostic anchor 唯一性（`done`） | 0003、0006–0009、0065、0129、0135–0142 `done`；当前持续 Goal 的站立授权 |
| [0144](./0144-parser-suffix-truncation-matrices.md) | 建立完整文件与独立入口 UTF-8 后缀截断矩阵（`done`） | 0006–0009、0014、0068–0069、0079、0085、0099、0111、0114、0143 `done`；当前持续 Goal 的站立授权 |
| [0145](./0145-parser-interior-deletion-matrices.md) | 建立完整文件与独立入口 UTF-8 内部区间删除矩阵（`done`） | 0006–0009、0014、0068–0069、0079–0080、0085–0086、0099、0111、0114、0144 `done`；当前持续 Goal 的站立授权 |
| [0146](./0146-parser-scalar-duplication-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 重复矩阵（`done`） | 0006–0009、0014、0068–0069、0082、0087、0099、0111、0114、0145 `done`；当前持续 Goal 的站立授权 |
| [0147](./0147-parser-scalar-transposition-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 相邻交换矩阵（`done`） | 0006–0009、0014、0068–0069、0084、0090、0099、0111、0114、0146 `done`；当前持续 Goal 的站立授权 |
| [0148](./0148-parser-scalar-replacement-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 固定字母表替换矩阵（`done`） | 0006–0009、0014、0068–0069、0081、0088、0099、0111、0114、0147 `done`；当前持续 Goal 的站立授权 |
| [0149](./0149-parser-scalar-insertion-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 固定字母表插入矩阵（`done`） | 0006–0009、0014、0068–0069、0082–0083、0087、0089、0099、0111、0114、0148 `done`；当前持续 Goal 的站立授权 |
| [0150](./0150-lexer-large-input-mode-depth-stress.md) | 建立 Lexer 大输入与深模式压力矩阵（`done`） | 0006、0073、0103、0129、0149 `done`；当前持续 Goal 的站立授权 |
| [0151](./0151-parser-large-flat-recovery-stress.md) | 建立 Parser 四入口大平坦列表与恢复压力矩阵（`done`） | 0006–0009、0014、0117–0119、0128–0129、0135、0150 `done`；当前持续 Goal 的站立授权 |
| [0152](./0152-parser-recursion-budget-boundaries.md) | 锁定 Parser 四入口递归预算的精确公开边界（`done`） | 0007–0009、0014、0117–0119、0135–0136、0151 `done`；当前持续 Goal 的站立授权 |
| [0153](./0153-parser-caller-stack-isolation.md) | 锁定 Parser 四入口的调用者栈隔离（`done`） | 0007–0009、0014、0136、0152 `done`；当前持续 Goal 的站立授权 |
| [0154](./0154-lexer-small-stack-stress.md) | 锁定 Lexer 深模式的小调用栈行为（`done`） | 0006、0129、0150、0153 `done`；当前持续 Goal 的站立授权 |
| [0155](./0155-parser-owner-rich-stress.md) | 建立 Parser 四入口大规模 lexical-owner 压力矩阵（`done`） | 0006–0009、0014、0075、0095、0135、0151、0154 `done`；当前持续 Goal 的站立授权 |
| [0156](./0156-parser-string-poison-stress.md) | 压力验证 Lexer 字符串错误向四入口 Parser 的唯一传播（`done`） | 0006–0009、0014、0095、0140–0142、0155 `done`；当前持续 Goal 的站立授权 |
| [0157](./0157-parser-standalone-poison-matrices.md) | 扩展 Parser 变换矩阵至 L0007 / L0008 独立 lexical poison（`done`） | 0006–0009、0014、0081、0083、0088–0089、0111、0114、0142、0156 `done`；当前持续 Goal 的站立授权 |
| [0158](./0158-parser-standalone-poison-stress.md) | 建立 Parser 四入口 standalone lexical poison 压力矩阵（`done`） | 0006–0009、0014、0129、0140–0143、0150–0151、0157 `done`；当前持续 Goal 的站立授权 |
| [0159](./0159-parser-lexical-owner-recursion-boundaries.md) | 锁定 Parser 四入口 lexical-owner 递归预算边界（`done`） | 0006–0009、0014、0075、0095、0150、0152–0155、0158 `done`；当前持续 Goal 的站立授权 |
| [0160](./0160-lexer-long-invalid-lexeme-stress.md) | 建立 Lexer 超长非法 lexeme / terminal owner 压力矩阵（`done`） | 0006、0073、0103、0129、0150、0154、0157 `done`；当前持续 Goal 的站立授权 |
| [0161](./0161-parser-long-lexical-error-bridge.md) | 建立 Parser 四入口超长词法错误桥接矩阵（`done`） | 0006–0009、0014、0095、0140–0143、0158–0160 `done`；当前持续 Goal 的站立授权 |
| [0162](./0162-parser-mixed-long-lexical-error-stream.md) | 建立 Parser 四入口混合超长词法错误流矩阵（`done`） | 0006–0009、0014、0095、0140–0143、0155–0161 `done`；当前持续 Goal 的站立授权 |
| [0163](./0163-parser-mixed-long-recoverable-error-stream.md) | 建立 Parser 四入口混合超长可恢复词法错误流矩阵（`done`） | 0006–0009、0014、0075、0095、0140–0143、0155–0162 `done`；当前持续 Goal 的站立授权 |
| [0164](./0164-parser-long-utf8-line-recovery.md) | 建立 Parser 四入口超长 UTF-8 string owner 换行恢复矩阵（`done`） | 0002、0006–0009、0014、0078、0098、0100、0110、0112、0150、0160–0163 `done`；当前持续 Goal 的站立授权 |
| [0165](./0165-parser-long-utf8-nested-line-recovery.md) | 建立 Parser 四入口超长 UTF-8 nested string/interpolation 换行恢复矩阵（`done`） | 0002、0006–0009、0014、0078、0098、0100、0110、0112、0150、0155、0160–0164 `done`；当前持续 Goal 的站立授权 |
| [0166](./0166-parser-long-utf8-char-line-recovery.md) | 建立 Parser 四入口超长 UTF-8 invalid Char 换行恢复矩阵（`done`） | 0002、0006–0009、0014、0073、0095、0103、0150、0155、0157–0160、0163–0165 `done`；当前持续 Goal 的站立授权 |
| [0167](./0167-parser-long-invalid-number-boundaries.md) | 建立 Parser 四入口超长 L0008 maximal-region / operator-boundary 矩阵（`done`） | 0002、0006–0009、0014、0073、0095、0103、0150、0155、0157–0163、0165–0166 `done`；当前持续 Goal 的站立授权 |
| [0168](./0168-parser-long-block-comment-line-breaks.md) | 建立 Lexer/Parser 超长 UTF-8 block comment 非嵌套与逻辑换行矩阵（`done`） | 0002、0006–0009、0014、0073、0093、0103、0129、0150–0151、0160–0167 `done`；当前持续 Goal 的站立授权 |
| [0169](./0169-parser-long-line-comment-boundaries.md) | 建立 Lexer/Parser 超长 UTF-8 line comment / newline trivia 边界矩阵（`done`） | 0002、0006–0009、0014、0073、0093、0103、0129、0150–0151、0160–0168 `done`；当前持续 Goal 的站立授权 |
| [0170](./0170-parser-large-file-header-stress.md) | 建立 Parser 4,096 项合法 / 恢复 package-import 文件头压力矩阵（`done`） | 0002、0006、0014–0015、0093、0103、0128–0129、0150–0151、0168–0169 `done`；当前持续 Goal 的站立授权 |
| [0171](./0171-parser-large-qualified-header-paths.md) | 建立 Parser 4,096-segment package/import 路径与末尾恢复矩阵（`done`） | 0002、0006、0014–0015、0093、0103、0128–0129、0150–0151、0170 `done`；当前持续 Goal 的站立授权 |
| [0172](./0172-parser-large-file-header-separators.md) | 建立 Parser 4,096-import 混合文件头分隔与 L0053 恢复矩阵（`done`） | 0002、0006、0014–0015、0062、0078、0093、0103、0128–0129、0150–0151、0170–0171 `done`；当前持续 Goal 的站立授权 |
| [0060](./0060-machine-readable-diagnostics.md) | 提供版本化 JSON Lines 机器诊断协议（`done`） | 0003、0055 `done`；ADR-0014 `accepted`；当前持续 Goal 的站立授权 |
| 0061 | 构建首个支持平台的 compiler + stdlib 发行包 | 0040、0042–0051、0054；接受发布矩阵 ADR |

增量编译不预留在 Phase 0–6 主链中。它依赖稳定 package identity、package lock、SSA 和依赖
图；推荐在 SPEC-0054 完成后另建 Phase 6+ Spec，并先接受缓存键与失效策略 ADR。

现行 v0.34 沿用 v0.14 已确定的规则：v1 的 `Transferable` 与 `Copyable` 一样由编译器结构化自动推导，不开放
手动实现；标准库并发类型的例外由后续实施 Spec 逐项锁定，`Shareable` 连同跨线程共享原语
延后到 v2。该规则及跨线程 effect identity 已由 SPEC-0032 实施，不属于下列未决推荐。

## 未决决策的推荐方向

v0.26 的参数契约、调用期 loan 与 ASAP 析构点已经接受并实现，不再属于未决推荐；参数契约
由 SPEC-0176 实现，调用期 loan 与 ASAP 析构点由 SPEC-0029 实现。

以下是起草后续 guide / ADR 时的默认推荐，不是已经接受的决策；触及对应 Spec 前仍需正式
文档批准。

| 决策 | 推荐方案 | 需要的权威文档 |
|---|---|---|
| `lang-std` bootstrap / runtime | `.ko` 标准库保持独立真源；最小 ABI 支撑先收敛在 codegen 的私有 runtime 边界，证明需要独立发布后再提新增 crate 的 ADR | ADR |
| SSA | 采用 typed SSA + block parameters，显式表达 move / drop；用 verifier 锁定类型、CFG 与所有权不变量 | ADR |
| LLVM / target / linker | 固定一组经兼容矩阵验证的 LLVM major 与 `inkwell` feature；先支持单一 host target，再扩展 CI 矩阵 | ADR |
| FFI | 不放入 v1 主交付路径；待内部 ABI 稳定后，以受限 C ABI 和显式 `unsafe` / 所有权边界起步 | 新 guide + ADR |
| `@Test` | v1 只定义编译器保留的最小 `@Test`，不顺带实现通用运行时注解或反射 | 新 guide |
| 机器诊断 | 已由 ADR-0014/SPEC-0060 实现 schema v1 stderr JSON Lines；完整 build event 与 machine operational error 仍后置 | 后续 ADR |
| package / lock | `project.toml` 只保留 package、target、dependency 最小字段；`project.lock` 完全由工具生成并确定性排序 | ADR |
| 首发平台 | 先验收开发主机 `aarch64-apple-darwin`，再增加一个 Linux CI target；跨平台承诺以发行 ADR 为准 | ADR |

## 路线图维护规则

- guide 改变 Phase 或语言语义时，先更新 guide，再调整尚未批准的路线图候选。
- 已批准或已完成 Spec 不因路线图重排而改号；需要替代时使用 `superseded` 并建立双向链接。
- ADR 只决定 guide 留白处的长期方案；路线图中的“推荐”不能替代 accepted ADR。
- Architecture 只描述已经落地的事实，不复制本页计划。
