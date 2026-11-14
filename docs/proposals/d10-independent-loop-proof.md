# D10 独立 loop producer 与 CFG 证明候选

> **性质**：非规范工程候选 · **状态**：未启用、未实现 · **读取时机**：设计 D10 独立 loop producer 与 replay 时 · **唯一真源**：本候选；现行行为仍以 SPEC-0341 与代码为准

状态：架构候选，未实现、未启用。基线为本地 `e24640cc`；不替换现行
SPEC-0341、全 Replace receipt 或生产 selector。本文不裁决 Origin-based。

## 为什么不能继续从日志补出回边

当前真实来源已有条件/body 边界、producer/绑定/写入、borrow pending 移除和
清理事件，但这些是静态 planner 遍历，不是动态执行或实际 SSA predecessor。

- `drop_planner/owner_transport.rs::commit_owner_primitive` 在旧 observation
  inactive 时返回 None；此时 ValueState 中残留 owner 不能作为当前代。
- 同文件结束观察时不一致会置 Unsupported 并清空 definitions/events。
- `owner_transport/loops.rs::owner_while_statement_scoped` 克隆 body_state，
  静态遍历 body 并记录 continuation；成功返回不自动证明 header 方程。
- `lang-codegen/src/ssa/refine/build/control_bridge/nested_body.rs::NestedBody::new`
  要求既有 `nested_normal_body_v1`，并拒绝冲突的观察种类。不能绕过它把 Carry
  old==new 填入要求每根都有新 producer 的旧协议。

因此需要新增独立权威生产者，而非让日志分析器自行铸造已验证事实。

## 最小架构改变

### 1. 独立、每次分析唯一的 catalog

在测试域增加与旧 observation 不共享生命周期的 catalog。仅受限候选分析可开启；
普通生产入口不读此结果。catalog 的 identity 绑定同一次 parsed/named/typed/owned
输入，token 构造器私有。旧 observation Unsupported 既不使它自动成功，也不能给
它提供残留 token。独立 catalog 遇未覆盖操作必须标记失败并禁止导出证书。

它由实际表达式求值、binding、Replace commit 处形成 producer/根环境；不得从
完成后的日志倒填 token。Literal 的表达式 ID 标识定义位置，不是某次运行时分配。

### 2. 显式结构边，不用 source span 排序代替 CFG

实验 producer 必须直接拥有 Entry、Header、Body、Backedge、FalseExit、Resume
等结构节点和有向边。节点只能在实际选定的受限控制路径创建，并绑定同源语法身份。
条件真/假后继和 normal body continuation 各有独立边；错误/return/break 未支持时
整图拒绝。已有 checked arithmetic abort 的终止边及元数据必须保留并由 replay 核对，
不能当未知错误删掉；本候选不新增 abort-drop 承诺。所有边在构图时登记来源，
最终必须完整消费，不允许 verifier 补边。

现有 `WhileTraversal` 不满足这一接口。第一版只选一个正常 while，先验证实际
代码生成后的基本块和 predecessor；父子嵌套在单层闭环验收之后再加。

### 3. 根环境传递使用符号 SSA 值，不模拟动态代数

每条边携带 root → symbolic value 的环境。值为 Initial、Producer 或 HeaderPhi；
必须区分“静态定义一次”和“循环每轮执行产生值”。HeaderPhi 在结构建立时占位，
闭合时必须恰好具有 entry/backedge 两条实际输入；不能用递增整数假装动态 owner。

- Replace：验证当前 root 值存在，incoming producer 已在当前路径形成；产出新根值，
  old-result 明确引用旧值，并与真实清理事实配对。
- Carry：只允许边上传递同一个已知 symbolic value；必须证明该路径没有覆盖外的
  写、move、drop 或借用效果。这里的“同一”是逻辑根经 edge→header 参数映射
  的同一来源，允许 Initial→HeaderPhi 及跨块实体不同，不能要求原始 SSA ID
  相等。值相等本身不是有效 Carry 证据。不同活动根不得别名，Replace 新代
  不得与仍活动的 Carry 代混同；这些约束须由模型检查和最终 replay 分别验证。
- 合流：输入来自实际前驱集合，类型、source identity 与根集合一致；任何遗漏、
  多余或重复前驱都失败。不给缺失值填默认值，不从另一条臂复制缺失状态。

### 4. 独立 emitter 与 replay 的职责分离

emitter 消费成功的实验结构模型，生成真实 SSA blocks、分支、phi 输入与清理，
同时记录实际 emitted block/value 映射。replay 读取最终 SSA，而不是 emitter 自报
“成功”或同一手写拓扑：核 predecessor 集合、phi 输入、Replace old/new、drop 点及
终结出口。模型与 SSA 不一致不得发布任何实验结果，更不能回落成旧 receipt。

前端模型和最终 SSA 必须分别检查；LLVM verifier 是补充，不能代替所有权证明。
验收未完成前没有跨 crate 的生产 capability，不能修改默认准入或旧 oracle。

## 可执行工作顺序与停止条件

1. **catalog 与根环境**：实现独立身份/私有构造，接实际 Literal/Bind/Replace；普通
   无循环正例先闭合。任何当前值不明或清理不匹配即拒绝，保留旧 Unsupported。
2. **单层结构 producer**：从实际控制选取生成条件真/假和 normal backedge；覆盖清单
   完整，不能把未观察的 For/Loop 当作“没有影响”。先只准入已明确定义的正常 while。
3. **真实 emitter/replay**：实际 CFG predecessor/phi 与源模型逐项一致；同源身份、
   两条输入的来源及出口清理均可检查后，才可称单层实验闭环。
4. **Replace/Carry 与嵌套**：加入原候选 a Replace/b Carry，再加入父子层；不能跨过
   前三步直接重用旧全 Replace 接口。条件 break、return、复杂 producer 单独延期。

普通本地验收包括：正常单层、零次路径、真实循环后使用、未覆盖调用/提前退出拒绝，
以及源模型与实际 CFG 的独立一致性检查。故意破坏模型/映射/回边的注入验收仅在 CI；
本地可编译、列举而不执行。普通拒绝测试不等于恶意 corruption 测试。

## 当前交付与不可宣称项

已有真实事件和静态分区作为调试/来源审计材料保留，但不是本 catalog 或证书。
本轮只完成架构接口与实施约束设计，未实现上述独立 catalog、CFG emitter、replay，
没有新增生产能力；不能以此报告 Carry、代际推广或 D10 已完成。

## 第一批实现：无循环 producer catalog（测试域）

后续本地实现独立 per-planner catalog，token 私有构造且以独立 Arc identity 区分
不同 planner；不读取旧 `UnitOwnerId`。实际 Literal、Binding、Replace commit、
Named Drop hooks 直接更新 producer/pending/root 环境：Replace 把旧 token 交给
真实 old-result initializer，新 token 留在根；两者不能误认为同一 producer。

普通源码验证两个 literal producer、一次 Replace、最终根清理及重复分析 token
不相等；同时比较启用前后生产 facts/observations 不变。已观察的 call、move、
branch、while 均置失败，普通源码拒绝测试不做损坏注入。

这是第 1 步的内部状态实现，尚无完整源码覆盖检查、完成证书或导出 API。
`failed == false` 只表示已到达的 hooks 未冲突，绝不等于函数已验证；未观察的语义
仍可能存在。catalog 暂无 loop/header phi、CFG、replay 或生产权限，不可被下游
当 capability 使用。后续必须先补覆盖/闭合检查再考虑成功导出。

当前诊断字段仍可由测试域修改，不是不可伪造资格；Named Drop 暂只更新根环境，
尚未保存 token→具体 DropFact 退休映射，不能宣称完整 old-result 清理证明。

## 第二批实现：覆盖预筛与实际退休配对

catalog 启用前增加保守 AST 变体预筛：单个 Function，仅 Function/Variable item、
Block/LocalVariable statement、纯文本 String/Name 与已检查的单根 Replace 调用。
其他表达式或控制变体立即失败；普通整数 literal initializer 和独立 loop 源码被拒绝。
这只是当前窄形态预筛，不替代动态 hook 消费或语义完成检查。

每次实际 Named Drop 现在保存被移出根环境的 token 与原始完整 UnitDropFact。
正例验收每个 literal token 恰一次退休、pending 与 roots 均耗尽，退休事实顺序
与未启用 catalog 的真实 facts 完全一致。未用跨度猜清理点，也不合成 DropFact。

这些仍是测试诊断和验收断言；没有导出成功证书。尤其 AST 变体白名单不等于
全函数已完成验证，后续须核函数/body 可达覆盖、类型/根角色以及私有闭合接口。

预筛当前允许 Name，不代表已排除 scalar 参数拷贝；Function/Variable 的参数、
泛型、返回形式和声明类型尚待进一步收紧，不能扩大为“所有 scalar 都拒绝”。

## 第三批实现：最窄无循环闭合结果

源码覆盖收紧到一个无参数、无泛型、无 extension 的 implicit-Unit 函数；body
恰为两个局部声明：`var a = "literal"` 与 `val old = replace(&a, "literal")`。
核两个真实符号均为 String、Replace 单完整根指向第一个声明；所有 item、statement、
expression 数量和角色逐一匹配，无额外表达式或未归属 Name。显式返回形式及任何
其他源码形态暂拒绝，不以“同样应该安全”放宽。

catalog 的可变字段已私有化，仅提供只读诊断。私有构造的测试域 Closed 只在
形态匹配、实际 producer/binding 顺序一致、一次 Replace 的旧/新 token 正确、
root/pending 耗尽、每 token 恰一次真实 DropFact 且目标分别对应 a/old 后产生。
它由当次 planner 独立 identity 绑定，不接收旧 token 或外部拼装事实。

这仅闭合上述两声明无循环形态。Closed 没有跨 crate 或生产导出，不代表 CFG、
loop/header phi、Carry 或一般函数准入。新增参数拷贝与额外局部声明负例防止之前
仅按 AST 变体放行的空洞重新出现。普通源码测试不进行损坏注入。

完成检查进一步限定真实清理点：新根为该 Replace 的 CallReturn，旧结果为第二个
声明的 AfterStatement。Closed 是可重复检查的测试域成功标记，不是消费式 emitter
capability。负例采用 implicit Unit 形态，避免只被返回形式门槛遮蔽。

## 真实 Unit loop CFG 复用审计

在 `ed86eb04` 后核对现有 codegen，实验 emitter 不应先另造一套普通循环 lowering：

- `ssa/unit_lower/loop_control.rs::create_loop_context` 已从真实 bindings/loans/
  pending operands 建立 carried slots 和 header block。
- `lower_while` 在实际 condition_state 上建立 body/false successor，分别 rebind；
  正常 body 完成经 `record_natural_continue` 捕获实际 BranchExit。
- `finish_continues` 使用该跳转快照，调用 `cfg.rs::carried_edge_from` 并实际安装
  指向 header 的 terminator。该方法核 entity type、pending/temporary alias 一致性，
  缺失事实返回错误，而不是从静态日志补值。

所以优先复用这一真实发射路径，再独立读取最终 SSA 做实验 replay；“独立”要求
验证责任独立，不要求复制 emitter。是否能接收目标候选必须用普通源码实际验证，
不能从上述通用代码存在推断候选已经通过。

尤其不能硬编码四个 block：常量 false 只做退出清理，不产生 body/backedge；
常量 true 没有 false successor。首个实验形态应显式限定真实双向条件，或在模型中
分别表示这三种已生成图，不能给缺失边补假 predecessor。

仍缺的接合面是：当前无循环 catalog 的私有 token 与实际 emitted entity/根 slot
关联、单 loop 的 header entry/backedge 根方程、退出 DropFact 消费以及检查失败时的
结果封闭。这些需独立实验适配器，不能把测试 Closed 当生产资格，也不能重用旧
全 Replace receipt 冒充 Carry。此审计未放宽 catalog 源码形态，未实现 loop 闭环。

复用审计发现明确路由阻塞：`ssa/unit_lower/call.rs::lower_call` 对
`typed.ownership_primitive(call).is_some()` 直接返回 UnsupportedNode；
`orchestrate.rs` 先选择 refine 路由，而缺少相应观察时不能因此自动允许普通
Replace 发射。因此这里可复用的是 CFG 构件，不是已经可用的 Replace/Carry
完整 emitter。先对已支持、无 Replace 的普通 loop 产物独立 replay，再选择受限
primitive/route 适配或实验 emitter。不能删除此拒绝来“完成”本候选。

## 无 Replace 的真实 SSA 独立 replay 首片

测试直接调用现有普通 Unit lowering，读取最终函数而非自建 CFG 或前端事件日志。
首片只验 `fun run(flag: Boolean) { while(flag) {} }`：header 前驱恰为实际 entry
和 body，body 的真实 terminator 回到 header；所有 edge 参数数量/类型与目标参数
一致；追踪同一个 flag 的 entry→header→body→backedge 对应；false edge 沿实际
边达到正常 Return，不能重入 body/header。Rooted/MemoryDocument 同时验收。

`while(flag){break}` 与常量 false 是合法源码，但不满足该 normal-backedge replay
形态，须拒绝，不为它们补假回边。只编译/验证图，不原生执行无限循环。
该首片不包含 owner、Replace、Drop 或 Carry，不得作为其已支持证据。

受限 primitive 路由仍需独立 capability：必须把私有 catalog 的成功结果与同次
分析/源 callable 绑定，在实验入口确认 exact shape 后才能选独立 primitive emitter；
普通 `ownership_primitive → UnsupportedNode` gate 不删除、不放宽。新入口不能仅凭
SSA形状或测试 Closed 复用生产权限，失败不得发布部分结果或伪造旧 receipt。

实际首轮图显示 Boolean 参数以 loan 进入，header 的 Read 产生条件值。replay 因而
沿 Read 的真实 source loan 核参数传递，不错误要求条件值本身是 header 参数。
同时核唯一 Read/其结果、其他块无指令、全块集合被消费、空值 Return，以及真实
源码 while/condition 的 Origin。这里不另行宣称验证 Shared ABI 模式或 owner 语义。

## 受限 primitive route 接合合同（候选，尚未启用）

不能给 `lower_call` 加一个可由调用者传入的 `allow_replace: bool`。实验入口应持有
同一次 `OwnedCompilationUnitView` 借用，并由独立 catalog 验证器私有构造包含
source/callable、完整源码覆盖、producer 身份、根角色和 cleanup 对应的能力。
分析一致性由现有 view factory 校验，catalog nonce 只区分一次观察，不替代该校验。
目前 catalog 为 frontend `cfg(test)` 私有实现；codegen 测试不能直接获得它，且
其 Closed 只覆盖无循环源码。因此跨 crate 交付接口和 loop catalog 扩展均是实际
前置缺口，不能用另一套 AST 扫描补造等价 producer 事件。

最窄实现顺序：

1. 先验证普通无 Replace 的 String owner 经真实 loop carried slots、false exit、
   suffix 使用及清理，确认现有 CFG 与 owner/drop 消费路径确实可复用。
2. catalog 从真实 planner 观察 producer/bind/primitive，单循环入口、回边与退出
   必须独立建模；静态 producer 标识只命名源码 producer，动态代际用 header 方程，
   不能用每次静态遍历递增的 token 假装实际迭代次数。
3. 独立实验入口只接受上述私有能力，复用真实 loop 构件。在实际 Replace 发射点
   取走 incoming temporary、更新目标 binding、将旧 owner 作为结果交付；三者的候选 lowering 状态更新与结果发布须
   原子完成，不新增运行时 abort 清理承诺。不能只更换 bindings 而保留 incoming temporary 别名，亦不能提前
   drop 旧值。必须按原真实 DropFact，
   各自在对应 UnitDropPoint 消耗 old-result 和最终根。
4. 另行 replay 最终 SSA 的真实 predecessor/edge 参数与 producer→实体对应，
   在正确路径验证唯一清理；Drop 的 Origin 是 `value_origin`，本身不能证明它
   位于正确 `UnitDropPoint`，还须核实际 block/指令顺序及该 point 的来源映射。
   局部构建失败时不交付 Program，也不生成任何旧 all-Replace receipt。

上述第 2–4 步尚未完成，普通 primitive gate 保持原样。即使第 1 步成功也只证明
现有无 Replace owner loop，不能证明混合 Replace/Carry、嵌套或复杂出口准入。

第 1 步已新增普通源码基线：`var a = "initial"; while(flag) {}; println(a)`。
独立 replay 消费全部四块和七条实际指令，逐边核 owner/flag 参数与类型，确认
唯一 entry/backedge、真实 false exit 上同一 owner 的 RootPlace、Shared Borrow、
PrintString、BorrowEnd、Drop 顺序。取同次 ownership 的唯一 Named DropFact，与
声明 `a` 的实际 Symbol/来源相符；该 fact 的 CallReturn 必须指向真实 suffix
`println(a)`，实际 Drop Origin 与 value_origin 相符。Rooted 与 MemoryDocument
均覆盖，带 break 的合法邻近形态不满足 normal-backedge replay，保守拒绝。
这仍不是新 primitive emitter，未交付独立 catalog token 到 SSA 的映射或 Carry 权限。

### 同分析 catalog 交付前置（仅 frontend 测试域）

catalog 现在保存创建时真实 typed product 的 `analysis_owner`，覆盖记录实际函数
`UnitItemId`。私有 `authorize` 必须先完成已有无循环 `close`，再核请求 typed 的
同分析身份及确切 source-qualified callable；成功结果借用 catalog，不复制或暴露
可写状态。其 producer 查询只接受本次实际 planner 产生的私有 token，返回该 token
已核对的源码 producer 与真实 DropFact。typed clone 保留分析身份；重新分析相同
文本、错误 callable、另一 planner 的 token 均应拒绝混入该交付；这些混用负控
置于 ignored + GitHub Actions 门禁测试，本地只编译列举，执行结果待 CI。

这是跨 crate 接口之前的封闭 payload 原型，仍全部 cfg(test)，不是 ordinary gate
许可，不可由 codegen 调用；也尚无单循环 catalog 覆盖、动态代际方程或实验 Replace
SSA 发射。不能把授权函数名称理解为已批准生产 lowering，后续跨 crate 桥还须借用
经校验的 owned view，并将实际发射结果作独立 replay。

### 跨 crate 发布必须晚于最终 ownership 构造

进一步核查发现：上节 typed 身份只封闭 catalog 查询，**不能**单独成为跨 crate
交付资格。同一 typed 可重新运行 ownership；`CompilationUnitOwnership` 另持独立
`analysis_owner`，`is_same_analysis` 按此身份比较。现有 owned-view factory 允许
同 typed 的重新检查结果，因此“可与 view 兼容”不等于“由该次 owned 生成”。
当前测试通过单独重跑 planner 得到 catalog，也尚未证明它是某个最终 owned 的
原始分析过程产物；不可直接借该测试路径发行 codegen 权限。

实际发布顺序及必须保留的失败边界：

- `dataflow/drop_planner.rs::plan_with_observations` 中，`planner.run()?` 后还要完成
  `iteration_plans()?` 与 `map_iteration_plans()?`。resource deferred 分支可返回
  默认清空的 Analysis。此处最多产生内部 Pending 候选，不能对外取走。
- `dataflow::analyze` 在 `checker.run()?` 后仍进行 no-carry/nullable 收尾及后续
  source 分析；任一失败必须丢弃全部 Pending，不允许第一个 source 先发布。
- `analysis::analyze` 在诊断存在或类型事实无效时会清空 drops、primitives 等。
  候选也必须清空；不能保留先前快照绕过这些清空。
- 最终 `CompilationUnitOwnership::new` 才创建该次 owned 的独立身份。专用入口
  必须在完整产物验证成功、无 deferred 且精确覆盖满足后，私有提交 Pending，
  将其与这个 owned 身份及同源 callable 绑定，再返回配对结果。

推荐实现为显式 sidecar 随内部返回值传递，而非 TLS drain：

`真实 planner → PendingCatalog → dataflow 暂存 → 最终 validated owned + BoundCatalog`

默认入口继续原返回类型；专用实验入口才接收 sidecar，产出不可拆换的配对包。
不能给普通 `CompilationUnitOwnership` 的相等性无意加入每次 catalog nonce，
也不能提供接受任意已存在 owned 和旧 catalog 的“补绑定”公开函数。未来 consumer
须核配对包所持 owned 与 view 的 `is_same_analysis`，不只核 typed。

如果采用作用域 TLS，必须另外证明重入拒绝、panic/错误自动清空、无公开 drain、
多次分析不混包以及最终成功才 commit；为减少这种隐式状态，此候选不优先采用它。
所有异分析/错配/中途失败负控仅 CI 执行，本地只编译列举。

该发布 sidecar 在上一设计阶段尚未实现；下节落实无循环首片，但仍不导出
旧 typed-only `authorize` 给 codegen，不新增 Replace emitter 入口，也不删除
ordinary primitive gate。后续才扩展单循环 catalog 与真实 SSA/Drop replay；不能跳过 owned 身份
缺口，或用 AST 重建替代真实观察以赶上发射进度。

### 显式 sidecar 无循环首片实现

新增默认关闭的 `d10-catalog-experiment` feature。启用时，实际 planner hook 收集
catalog，完整 planner 收尾后才 close 成 Pending；随 dataflow 返回值汇总，失败
自动丢弃，诊断清空时亦清空候选。最终 ownership 构造后，专用实验入口检查完整
无 deferred/diagnostic、真实 DropFact 精确配对及 validate，才返回私有构造的
BoundCatalog。包持有这次实际最终 owned，`matches(view)` 核 ownership 独立身份。
不使用 TLS，也不提供把旧 catalog 补绑定已有 owned 的公开 API。

导出数据保留原 planner 的 opaque producer nonce/index，以及真实 Replace 的根、
call、old/new producer 关系和每个 producer 对应的精确 DropFact；只读 getter，
不接受外部构造 token。feature 开启时普通 analyze 也采集后丢弃 sidecar；默认
feature 关闭时这些字段、hook 与桥均不进入依赖图。原 ownership product 未增加
实验字段，不改其 clone/相等性或普通 primitive gate。

外部集成测试覆盖 Rooted/MemoryDocument 的现有无循环正例、普通 loop 与带额外
suffix 的未覆盖合法源码拒绝。另一 ownership run/producer 身份错配验收仅 CI，
本地只编译列举。仍未实现单循环 catalog、Replace SSA 发射、跨 crate codegen
消费或 cleanup replay；这个只读配对包不是生产 lowering 资格。

中途失败不发布目前依据显式返回值和清空路径的独审，尚无已执行的专门故障负控；
此次新增 CI 目标仅验最终 owned/producer 身份错配，不能代替失败原子性验收。

组合 feature 回归中，独立 planner 的 nonce 不应相等。比较器现先比较实验快照的
source/cleanup 完整投影及 old/new 各自唯一 producer 索引，再取出实验 sidecar，
原样比较全部 legacy 字段。未改 producer nonce 或 ownership product 的相等性。

### 跨 crate 无循环 RootReplace 发射闭环（实验测试域）

codegen 的同名 opt-in feature 转发 frontend feature 并复用既有 observation 测试
支持。独立测试入口只接收 BoundCatalog 与同一个最终 owned 的 view，不修改普通
primitive gate，也不构造旧 all-Replace receipt。当前 catalog 仍拒绝 loop，因此
这里先完成无循环消费，不能据此声称已经发射或证明 loop Replace/Carry。

实际发射按源求值顺序建立一个函数/一个 block：初始 literal、root place、exclusive
loan、incoming literal、RootReplace、new root Drop、detached old Drop。RootReplace
自身结束 loan；两个返回 SSA value 分别对应安装后的新根与分离旧值。按真实
CallReturn(call) 先清理根、再按 AfterStatement 清理旧结果，不按 span 排序。

独立 replay 读取最终 SSA 的全部七指令、operand/result 与顺序，核 literal 的真实
解码字节和 source origin、RootReplace old/new 对应及两次 Drop。旧清理点还必须
解析到真实 LocalVariable，其 initializer 是原 Replace call、声明 Symbol 等于
旧结果 Named root；新清理点必须是原根的精确 CallReturn。局部 Program 完成后
通过 replay 与 SSA verifier 才返回，无中途结果交付。

普通验收为两个实际源码（含中文与转义）× Rooted/MemoryDocument；另外进行 LLVM
IR lowering/verifier。没有原生执行、故障注入或 loop predecessor 验收。本首片仍
只在 test+feature 路径使用，不是默认生产入口；循环根环境/回边/Carry 仍未实现。

### 单循环的 generation 缺口：不能沿用无循环 producer→单 DropFact

对真实源码 `var n=2; var a="first"; while(n>0) {
val old=replace(&a,"next"); n-=1 }; println(a)`，无循环首片的 `Producer { identity, expression, cleanup }` 不能直接
推广。在支持零次迭代和至少一次迭代的一般循环合同中，`first` 的动态值分别由 suffix 或第一次 `old`
清理；同一个 `next` 源表达式每次迭代产生不同动态值，既可能在下一轮成为 `old`，
也可能由 suffix 清理。因此不能用某个固定 literal token 配一个固定 DropFact，
也不能把 loop body 静态遍历所得最后 token 当作 false edge 的当前根。

新增普通真实源码验收 `loop_replace_cleanup_is_root_relative_and_not_a_closed_producer_catalog`
覆盖 Rooted/MemoryDocument 以及有/无第二个后用根 `b`。它核实际最终 owned 的精确
Named root、同分析 Symbol/来源 span、`old` 的 AfterStatement 和 `a`/`b` 的实际
suffix CallReturn；专用无循环 catalog 仍拒绝这些合法 loop 源码。该测试不执行
循环，不模拟运行次数，也不把现有 DropFact 当作循环 generation 权限。

下一版独立实验能力需区分三类身份，不能复用 `ProducerIdentity` 的含义硬塞循环：

1. 源 producer site：同次 planner 的实际构造事件及源表达式身份；不是每轮动态值。
2. 根环境位置：同次 callable/control 的 entry、header、body、normal backedge、
   false exit；必须包含根 Symbol 和真实控制边来源，不能仅存 span 或遍历序号。
3. generation 方程：header 根参数来自 entry 的初值，或 backedge 的本轮结果。
   Replace 消费 body incoming 根参数，产生独立的 old-result 和 backedge 新根；false exit
   从 header 参数接收当前代。规则描述任意轮，不展开、枚举或猜测迭代次数。

受限一根候选应要求 entry→header、header→body、body→header、header→exit 四条
实际边全部完成，并对应到最终 SSA predecessor 与参数。清理义务绑定消费位置，
不是绑定一个 literal：body detached-old Drop 消费 Replace 返回的 old-result，而非再次消费原 header SSA 值，
并与实际 AfterStatement 配对；exit
当前根 Drop 与实际 suffix CallReturn 配对。零轮路径不可读取 body 定义。第二根
`b` 若保持不变，也需每条实际边传递参数和 exit 清理证明，不能因 AST 没有写入
就发布 Carry。借用条件的 loan 参数及其允许存续同样不得遗漏。

实现顺序应为私有 loop sidecar 的上述受限方程、实际 planner 控制完成证据、最终
owned 绑定，再到独立 emitter/replay；任一步缺失不给发射能力。现有
`origin_while` 只记录静态访问且拒绝 catalog，`ValueState` 克隆也不是完成的
回边证明；此次不改变它们，不把已有 all-Replace receipt 改名使用，不放开普通
primitive gate。单循环 catalog/Replace emitter/Carry 验收仍未实现。

### 独立符号根环境候选首片（仅测试）

`owner_transport/loop_generation.rs` 从已运行的实际 DropPlanner 自身读取完整 probe
序列和发布的 DropFact，不接收调用方构造的事件。首片仅接受一个根、一次静态
Replace、正常 body 完成及 LoopExit 清理的精确事件序列；新增调用、第二次 Replace、
break 等普通邻接源码不产生候选。核实际 primitive 的单根关系、borrow 注册与 pending
移除的同一 fact、同 source 的 root/call/control、精确最终清理序列及 typed analysis 身份。

候选显式区分 EntrySite、Header(control,root)、IterationSite 和 Detached(call)：
Replace 输入是 header 变量，old-result 清理的是 Detached，正常回边拟传入当前
iteration site；出口保留 Header 变量，绝不采用一次静态遍历后的 incoming site。
候选同时保存 entry/header/backedge/exit 环境及实际两项清理来源，供下一步真实 CFG
映射核验。它不是每轮对象 token，不复用旧 owner token 或无循环 BoundCatalog。

这里的 `backedge`/`exit` 是**待证的符号方程位置**，不是已经观察到的 SSA edge。
虽然输入完整遍历序列被核对，probe 未记录每个 scalar expression，所以仍不证明
完整源码覆盖、终止性或每条动态路径；PendingBorrowRemoved 亦不单独证明 loan End。
当前类型没有公开构造/导出，也没有消费者能把它用作 lowering 能力。最终 owned
sidecar 绑定、封闭源码范围、条件/计数器及真实 CFG predecessor/参数 replay 仍须补齐。

普通验收核实际 initial/incoming/call/root 身份、出口 Header 不等于本轮 site、
old-result 单独清理及全部真实 DropFact，并用 break、额外 println、二次 Replace
源码核拒绝边界。不构造坏事实，不运行故障注入或 native loop；不改默认生产行为。

### 与 2026-11-14 主线的隔离组合验证

在独立本地分支将 `b15addda` 与 main `28a3c290` 组合，原工作树未重置。
主线新增 while-flow 的按实际退出筛选写入规则已保留；该组合上的 generation
候选普通正/拒绝源码、catalog handoff 与 loop cleanup、`inout_storage` 20 项
（含主线新增 loop-jump/when-generation 邻接回归）通过。

三处模块迁移冲突按独审结果解决：保留外置 checkpoint/owner test helper，主线
209 个 fault 枚举全部存在于本地 221 项集合；helper 函数仅路径/可见范围变化，
仍经同一 reexport 使用同一 FAULT/ACTIONS 实例。保留实际 `refine/folding.rs`
所需显式 path，不复活第二套 helper，不修改主线的其余自动合并内容。

组合 codegen 的 `ssa::d10_` 三项实际 SSA/LLVM 验证通过；更宽 `d10_` 过滤共
61 项普通回归亦通过，其中包含既有 D10-W09 native 用例。未运行 CI-only 故障。
格式、文档与 diff 门禁通过。此项是新主线兼容验证，不新增 loop catalog 的
source-completeness、最终 owned 能力或 Replace/Carry emitter。

### 封闭源码 envelope 与 counter 身份（测试域）

在 `56df1e86` 主线组合上，先用普通 `n += 1` 邻接源码复现旧候选会接受未记录
scalar 变化的缺口；这不是故障注入，也不是生产安全漏洞。新增私有 coverage 现
仅接受：单函数、一个缺省共享 Int 参数、一个纯 String 根、由该参数初始化的
Int counter、`counter > 0` 的单 while，以及 body 内一次 Replace 和 `counter -= 1`。

所有 item/statement/expression arena 节点必须与封闭形状恰一次对应。counter 的
初始化、条件与更新从真实名称解析 Symbol 对齐，类型用本次 typed 结果核对；
String producer、old-result 声明和实际 Replace/Exclusive loan 仍由原真实事件及
primitive 校验交叉核对。保存同源 callable、参数、counter、condition、step 身份，
不依赖变量拼写或以 span 顺序重建执行路径。错误步进与额外 scalar 声明明确拒绝。

这是对**受限源码范围**的封闭，不是通用准入或完整 loop 证明。真实 SSA
predecessor/参数、checked-abort 路径与最终 owned sidecar 绑定仍未实现；Header /
backedge 方程仍不能作为发射能力，普通 gate 与旧 witness 均未改变。

### 同次最终 owned 的 loop sidecar（实验 feature）

在同一个默认关闭的 `d10-catalog-experiment` 下，实际 planner 仅对 4 items /
7 statements / 12 expressions 的小规模输入启用 loop 观察，再由封闭 coverage
及完整已捕获事件序列筛选。完成 iteration/map 收尾且无 resource-deferred 早退后，
才生成独立 LoopPending；不是拿另一轮 planner 的候选补绑定原 owned。

LoopPending 随 dataflow 返回值传递；后续 source 失败自然丢弃，analysis 的诊断或
无效类型清空路径同时清空它。最终 owned 构造后，专用 `analyze_loop` 要求恰一个
source/一个候选、同 typed analysis 身份、无诊断/无 deferred、精确两项 DropFact、
精确实际 loan 及单根 Replace 计划，再 validate 并返回 BoundLoop。没有公开构造器
或接受任意旧 owned 的补绑定入口；`matches` 检查最终 ownership 身份，不只检查 typed。

BoundLoop 只读投影 callable/control、参数/counter/root、condition/step、两个构造
site、Replace call 与 detached-old/LoopExit 清理。site 不被改称每轮动态对象 ID，
记录的 pending loan 移除不被升级为独立 End 证明。普通入口仍返回原 ownership 并
丢弃实验 sidecar；无循环入口与循环入口分别筛选自己的 sidecar，不互相兜底。

外部 crate 普通验收使用 Rooted/MemoryDocument 的合法形状、错误步进、额外 scalar
声明；最终 owned/DropFact/loan 与源码角色均实核。另一 ownership run 的错配负控
加入既有 CI-only 脚本，本地仅编译列举、不执行。中途失败原子性仍是返回值与清空
路径独审，不能称已执行故障验收。

本片尚不提供真实 loop CFG、checked arithmetic abort 事实或 codegen 准入。
后续必须让实际 SSA predecessor/参数及 counter checked-step 路径独立 replay；
不能因同次 owned 绑定完成，就绕过 ordinary primitive gate 或声称 Carry 成立。

### 同次 sidecar 到既有单根 Refine SSA 的独立 replay

后续实测修正了一个重要边界：上述封闭单根倒计时 Replace 源码，普通 Unit 总入口
可以经现有 Refine selector 产生真实 SSA；普通 compatibility primitive gate 仍保留。
因此本片复用正常路由，不新造 loop emitter，也不把 gate 拒绝作为实现结果。

测试域 replay 先核 BoundLoop 与本次 owned view 身份，实际调用普通入口并验证 SSA，
随后核六个互异 block 的完整 successor/predecessor 结构、全部十二条指令归属：
初始 owner/counter 入 header，condition 两臂分别携带 body 根与 false-exit 根；
RootReplace 产出新根及 detached old，old 当轮清理，新根经 checked-step 的正常
continuation 回传 header，false-exit 清理当前根。单根值不是固定绑定到某一次
literal 对象；header/block parameters 表示实际 loop SSA 环境。

同源 literal bytes、Replace/condition/step origin、Exclusive loan begin、两项
DropFact 的 target/point/声明关系，以及 Shared Int32 入口均与真实分析交叉核对。
checked subtraction 保留真实失败臂、AbortReason 与 operand，不因 counter 条件
推断消除 abort；完整图拒绝额外 predecessor、指令或未知出口。测试使用两种合法
命名/字面量及错误步进、不同减量、break 邻接源码，覆盖 Rooted/MemoryDocument：
四个正例通过 SSA replay，六个邻接例在 BoundLoop 入口拒绝；不称执行过损坏 SSA
负控。旧根 Symbol 身份沿用可信 BoundLoop 校验，本片独立补清理点与 initializer 关系。

这只是**已有单根 Replace 路由**与新同次观察的独立结构验收。没有更改生产 gate、
扩张 selector、构造旧 receipt 或提供 Carry 许可，也没有动态执行或本地故障注入。
后续 Replace/Carry 双根、child 未修改根及更一般父子代际仍需各自权威事实与 replay，
不能从此单根正例外推。`replay` 为私有测试检查器，正常路由产物在当前调用内生成；
不能把带相同 source span 的外部 Program 当作携带 frontend provenance 的能力。

### 双根同次观察：真实 suffix 保活（尚非 Carry 准入）

在已验证单根基础上，新增且仅新增封闭形态：loop 前第二个纯 String 根，loop 中
仍只 Replace 第一根，loop 后实际 `println` 读取第二根。开启观察的容量门同时允许
5 items / 9 statements / 16 expressions；完整 arena 与实际事件均须通过，容量本身
不构成准入。没有 suffix 或改为读取第一根时，第二根可能提前退休，明确拒绝。

真实 trace 必须保留第二根 Literal/Bound，原 loop 十二事件，以及 suffix 四事件：
已知调用、实际 Shared loan 注册、对应 pending 移除、该根 CallReturn Drop。
不删除未知事件再匹配。suffix 还须由 typed call 证明 prints_line、非 abort、无
receiver、Unit 返回，唯一实参真实引用第二根；声明和 begin/end span 精确对应。

最终 LoopPending 与既有延迟交付共享生命周期，bind 精确要求三项 DropFact 和两项
loan，并保持同次最终 owned 身份。RetainedLoopRoot 只读提供实际 source site、root、
suffix、loan 与 DropFact；不增加 producer 动态 token，不发布 Carry/CFG 权限。
普通候选测试及跨 crate Rooted/Memory 验收通过，后者两正例与十个入口拒绝案例。
独审未见生命周期或单根/default-feature 隔离退化。跨轮错配及中途失败故障未在本地执行。

真实双根 SSA 上 root/参数各条边的保留、suffix BorrowEnd 与清理、checked-abort
运输仍须独立 replay；pending 移除不能替代 BorrowEnd，单根测试也不能替代双根证明。

### 双根实际 CFG 与 suffix 生命周期 replay（测试域）

该封闭双根形态也已通过现有普通入口的 Refine 路由产生真实 SSA，而非实验手写图。
共六块、十八指令：入口实际构造 a/b；header/body 以 `[a, b, counter]` 对应，
Replace 后两个 checked-step 分支实际运输 `[new_a, body_b, new_counter]`；正常
continuation 原序回 header；false-exit 运输 `[header_a, header_b]`。replay 逐边核
参数关系，不要求不同 block 的 EntityId 相等，不从静态日志假造运行时迭代次数。

实现共享单根 replay 的十二个核心角色，但仅引用原 Program 对应指令，不重建或
过滤图；第二个实际 literal 与 suffix 五指令单独精确检查。完整 block 指令列表、
节点互异、终结器与参数均覆盖原始十八条指令和全部边。suffix 必须依次对同一 b
执行 RootPlace、Shared BorrowBegin、PrintString、真实 BorrowEnd、Drop；此时才有
codegen 层 End 证据，而不再依靠前端 pending 移除推断。false-exit 先清理 a，b
在真实 suffix 调用后清理；abort 保持现有进程失败路径及 checked arithmetic 元数据。

这是封闭单 loop、单 Replace、第二根仅 suffix-use 的**既有路线结构闭环**，不是
生产 Carry selector、通用 generation receipt 或任何嵌套父子代际证明。未知源码
入口拒绝，任意损坏 IR 负控与运行时 native 验收不在本轮本地执行。旧 oracle 与普通
primitive gate 均未改动，后续不得以本片取消未等价的旧见证。

本批实际验证：双根两种命名/Unicode 字面量 × 两入口，四个真实 SSA replay 正例；
`ssa::d10_` 五个普通测试全部通过。前端外部 handoff 四个普通测试通过、两个 CI-only
身份负控保持 ignored；严格前端 all-targets Clippy、codegen lib/tests Clippy 与默认 feature-off check 通过。
本地没有执行 native、损坏 IR 或故障注入；语义范围仍以上述封闭源码合同为限。

### 两层 child-only Replace：实际普通路径见证，不扩展单环 sidecar

下一批限定源码见证为：一个 Shared Int `count` 参数初始化 outer；父 body 每轮
重新读取 count 初始化 inner；child 仅 Replace a 与递减 inner，父只递减 outer；
b 在两层结束后实际 `println(b)`。不加入父 Replace、条件退出或更深嵌套。

前端独立观察完整二十五事件，核对实际 final owned 的三处 Drop/两项 loan：
child detached old 在对应语句结束清理，当前 a 在**父 LoopExit** 清理，b 在 suffix
CallReturn 清理。child Exit 无 a Drop；不能仅凭词法嵌套推断此位置。这个测试中的
观察来自显式重新运行的 planner，虽事实与原 owned 一致，仍不是同次最终 NestedPending。

codegen 从同次 validated owned view 经正常 Unit/Refine 入口获得真实十一块、二十三
指令，不手写或过滤 SSA。独立 replay 核全部边：count 始终是 Shared loan，不等同于
outer/inner 值；父 body 真实 Read count；child header 分别接父当前根与 child 新根；
child false 路径含零轮情形，保留当前 a/b、outer 和 count；child 回边只更新 a/inner，
父回边只更新 outer。两个 checked decrement 分别核 Subtract operand、失败 flag、
AbortReason 与当时活根运输；不承诺失败路径额外析构。

全图三处清理为 child old Drop、父 false-exit 当前 a Drop、suffix b Drop，并非父
出口清理三次。suffix 是同一 b 的真实 Shared Borrow、Print、BorrowEnd、Drop。
普通源码矩阵含 ASCII/Unicode 两个正例与 inner 错接 outer、不同减量、后用改读 a
三个邻接拒绝，分别覆盖 Rooted/MemoryDocument；四个正例进入实际 SSA replay，六个
邻接例只在 fixture facts 检查拒绝。

**未完成边界：**本片 `source::facts` 按固定 fixture 的名字/文本与实际身份取证，
不是完整 AST 白名单或任意输入准入器。没有 NestedPending，没有将另一轮观察补绑
原 owned；原 `analyze_loop` 对嵌套仍拒绝。若下一步需要封闭嵌套能力，须新增独立
完整源码/控制观察合同并接原 planner 的延迟最终 owned 交付，不能把这些见证或单环
sidecar 直接升级成通用 Carry/嵌套权限。本片无 LLVM/native 或本地故障执行。

本批验证：前端 loop-generation 三个普通测试通过，D10 codegen 六个普通测试通过；
严格前端 all-targets（两实验 feature）和 codegen lib/tests Clippy、格式与文档门禁
通过，独审与最终增量复审无阻断。未执行损坏 IR/故障注入或远端推送。

### 独立嵌套 envelope 与同次最终 owned sidecar

后续片补齐上述固定见证之外的独立准入观察：`nested_generation_coverage` 不以变量名
或整段文本作判定，沿真实 AST/名称/类型身份覆盖全部六个 item、十三个 statement、
二十三个 expression，每个节点恰归属一次。仅允许前述两层单 child Replace，两个
counter 均从同一个 Shared Int 参数读取并各自 `> 0` / `-= 1`；a/b 初始化和 incoming
为纯字符串，old 为 val，b 有真实 suffix builtin 读取。额外节点、错误根或 counter
关系、其他更新/出口均拒绝。计数只是采集预算，不是准入证明。

原 planner（不是另起的对照遍历）记录完整二十五事件；独立 `nested_pending` 精确
匹配父子控制、Replace、old 绑定、全部 Drop 与 suffix loan。无事件过滤、残留 token
回填或 AST 伪造轨迹。`NestedPending` 独立穿过 dataflow，在诊断或无效类型事实时
清空；最终比对同 typed owner、完整三 Drop/两 loan/唯一 Replace 后才 validate owned。
`analyze_nested_loop` 只返回封闭 `BoundNestedLoop`，无公共构造/补绑入口。内部复用
单环的**私有最终事实 binder**，但不调用单环 candidate、不对外透出 BoundLoop，
`analyze_loop` 仍拒绝嵌套。

外部真实源码矩阵为两正例（含整体 counter/parameter 重命名与 Unicode）和十二种
邻接拒绝，各覆盖 Rooted/MemoryDocument，计四正、二十四拒绝。同 typed 下另次
owned 身份拒绝单列为 ignored CI-only 测试，本地只编译、不执行。

已有实际十一块二十三指令 replay 现在使用 `BoundNestedLoop` 自己的 final owned
构造正常 lowering view，四正例进入实际 SSA，六邻接例在独立源码观察端拒绝。
codegen 的 fixture 取证仍是固定见证适配，并未宣称任意嵌套 SSA 已获认证；前端
静态控制观察也不是动态 CFG 权限。生产入口、选择器、旧 oracle 和 Carry 合同未改；
一般父写入、多次 Replace、更多层嵌套或复杂退出仍不在本片范围。

本片验证：nested 外部普通矩阵一测试、既有 catalog 四测试、planner 三测试及
codegen D10 六测试通过；三个 CI-only 身份测试均 ignored。前端默认配置编译、
前端 all-targets（catalog/loan-observation）及 codegen lib/tests 严格 Clippy 通过；
格式、956 篇文档结构门禁、独审和增量复审通过。未执行 native、LLVM 验证或故障注入。

### 父一次 Replace／child 仅 counter 的反向受限组合

下一片为同一封闭两层源码增加第二种且仅一种布局：父 body 严格是
`old = replace(&a, literal)`、`inner = count`、child While、`outer -= 1`；
child body 仅 `inner -= 1`，没有根写入。不是容许任意调序或合并两种布局：
原 child-only 与新 parent-only 各自匹配完整真实二十五事件和对应 AST envelope，
额外 Replace、不同 counter 来源、break、错误 suffix 仍拒绝。完整 arena 预算
仍为六／十三／二十三，各节点恰归属一次。

新增只读 `replacement_control()` 指出真实 Replace 所在 parent/child，与始终在
父 LoopExit 的当前根清理区分。复用同次 NestedPending 的延迟绑定与全部事实核验，
不把静态日志当动态回边或产生现行 Carry 权限。

普通 Unit/Refine 确实可发射新形态的十一块二十三指令，独立 parent replay 不重排
或伪造 SSA。父 body 真实 Replace 产生新 a，清理 detached old 后重新读取 count，
把新 a/b/outer/count 与 inner 初值送入 child header。child 的零轮出口、body、
checked-abort 和正常回边均运输当前 a/b，只有 inner 值变化；child 完成后父仅
更新 outer，将当前根送回父 header。两个计数算术失败元数据分别核验，不声称
失败时额外析构。最终父 false-exit 清理当前 a，suffix 对 b 完成真实 Shared
Borrow/Print/BorrowEnd/Drop。四个计数常量也与实际条件/更新 RHS 源身份配对。

前端矩阵累计四正例、十七邻接拒绝，各有 Rooted/MemoryDocument，计八正、三十四
拒绝。codegen 两种布局各 ASCII/Unicode 正例及三个邻接拒绝，各有两种输入：
八例实际 SSA replay，十二例在封闭观察端拒绝。固定 fixture 的 replay 仍不是
任意输入认证器；生产 gate 与普通单环准入不变。未启用父 child 同时写、多次写、
三层嵌套、复杂退出，也不以此替代未来新主线退出写回组合验收。

本片实测：前端外部 catalog 四测＋nested 矩阵一测、planner 四测、codegen D10
六测全通过；前端 all-targets 双实验 feature 与 codegen lib/tests 严格 Clippy、
格式、文档 956 和独审通过。三项 CI-only 身份测试保持 ignored，未跑 native、
LLVM 或本地故障注入；本地基线仍为 main `28a3c290`，未自行并入后续退出写回变更。

### 父子各一次 Replace 的独立双操作观察

此批从 `ed389f22` 建立独立树，原树保留；实时只读核对 main 仍 `28a3c290`，
PR #428 在检查时 open、head `5400321f`，未借讨论内容合并该变更。

双操作使用新的 `DualNestedPending` / `BoundDualNestedLoop`，不是取消旧 binder
的单操作限制。封闭源码为父先 `parentOld = replace(&a, parentLiteral)`，然后
`inner = count` 和 child；child 内 `old = replace(&a, childLiteral)` 后递减
inner，父再递减 outer；b 保持两层后真实 suffix 读取。完整七 item、十四 statement、
二十七 expression 各归属一次，真实原 planner 三十一事件严格整体匹配。

两次 Replace 的 call、incoming site、Exclusive loan 和 detached old declaration
分别核对，不能用第一个 plan 替代第二个，也不能按字符串内容把 producer site
合并。父 old 在 child 之前实际 AfterStatement 清理；child old 在 child body 中
清理；当前 a 在父 LoopExit，b 在 suffix CallReturn。独立第五路观察在后续错误时
清空，构造最终 owned 后核全量四 Drop／三 loan／两 primitive 与同 typed 身份，
然后 validate 才能交付；没有公开构造或补绑入口，旧单操作 API 继续拒绝此形态。

实际普通 Unit/Refine 路线的双操作图独立 replay 逐条核验：父新根进入 child header；
child 零轮退出保留父新根，child 每次 Replace 新根走自身回边；完成 child 后的当前
根通过父回边，下一父轮 Replace 的 old 为该 header 当前根，非某固定 literal。
两层 counter、Shared count loan 与 b 独立运输。四个真实 Drop、两个 checked-abort、
suffix BorrowEnd 及全图块／参数／前驱来源均需满足，不能以静态观察替代 CFG。

该能力仍为默认关闭的来源观察和固定 SSA 见证；不是一般多写／任意嵌套／Carry
许可，不接普通生产 selector。不同根写入、错误 counter 来源／步长、额外节点或
break、缺真实 b 后用均拒绝；后续主线退出写回必须另作组合测试。

普通正例验证纠正了一个来源角色误判：Exclusive loan 的 begin 来自实际 Inout `&`
marker，不是其 Name 表达式范围。候选现严格按既有 call contract 核 marker，同时
保留 argument ID 与 call-end 校验；未删约束、未移动/重造 loan 的 span。真实源码
包含同字面量内容但不同 parent/child producer site 的正例，防止按内容混淆身份。

这里的 child 零轮保证指实际 CFG 的 false edge 参数核验，并非 native 路径覆盖：
当前封闭形态两个 counter 共用 count，父可进入时 child 的初值也为正。要让父进入
而 child 初始为零，需要独立 counter 来源等新合同，不能据本片声称已测该运行场景。

本片验证通过：新 dual 外部矩阵六正／十八拒绝；合计外部普通六测试、planner
五测试、D10 codegen 七测试，新双操作包含六个实际 SSA replay 与十个来源端拒绝。
四项身份错配测试仍 ignored。前端/后端严格 Clippy、默认前端编译、格式、文档956
和独审通过。未执行 native、LLVM 或本地故障注入。收尾期间获知 #428 已合入
`467f88b5`；本片提交仍保留旧基线，下一隔离组合单独验收该主线。

### 与 #428 新主线的隔离组合

双操作批次在 `a20bc343` 留存，原 `ed389f22` 树也未修改。另树自动合入
`467f88b5`，组合头为 `5da38ba2`；无冲突且 merge 相对双操作父提交的 diff
与主线原始 diff 一致，未在冲突处理中覆盖 D10 代码。

组合除 D10 普通回归外，验证 #428 仓内 `type_callable/inout_storage.rs` 的
Elvis／短路／循环退出写回用例。后端额外对照严格区分出处：Elvis 源复用
`assert_nested_loop_flow` 的 wrapper 和 `if (other || (write(&x) && false))
{ break }; return 0`；另一份 explicit-if 是本次明确改写的本地对照。
后者不是 SPEC §227 未收录完整文本的原始 CLI 探针，不能据相似错误码冒称复现
了同一输入。两者均非 D10 封闭候选，必须保持 sidecar 拒绝。

组合已验证前端普通 53 测试（其中 #428 所在 type_callable 47 项）、真实
planner 5 项、D10 实际 SSA 8 项。新边界回归对 Rooted/MemoryDocument 均断言：
Elvis 为 UnsupportedNode 且 span 文本等于完整 control；本地 explicit-if 为
MissingFact 且 span 文本为 `x`。后者不声称已定位多个 `x` 中某一特定节点。
两者正常前端分析成功，但不属于双操作 sidecar 的封闭范围；这项回归保留普通
后端拒绝，不能把 #428 的前端修复或本次组合解读为后端已支持这些控制表达式。
独立复审无阻断；未运行 native 或本地故障注入，原四项 CI-only 身份用例仍 ignored。

### 双操作嵌套的独立 counter 来源

在上述 #428 组合基线上，实验 dual envelope 保留单 Shared Int 参数，同时允许
两个不同的默认模式 Int 参数：声明第一个只初始化父 counter，第二个初始化子
counter。初始化仍是完整源码中的真实 Name 引用；交换、重复使用而闲置另一参数、
额外参数、own/inout 参数以及合法 Long 倒数邻接均拒绝。所有局部根与参数身份分离，
原 7/14/27 arena 覆盖、31 个 planner 事件、最终 owned 的两 primitive／三 loan／
四 Drop 核验不变。`counter_parameters()` 与 `counter_initializers()` 暴露实际角色
及表达式身份；旧 `roots().3` 仍为父参数，不再代表所有 counter 的共同来源。

真实普通 Unit SSA 对双参数源码产生两个入口 Shared Int loan：父参数只用于入口
父 counter 的 Read；子参数随父／子循环 block 参数运输，在父 body 的子初始化处
读取。独立 replay 核验这两个 Read、类型、来源与同次 bound sidecar 对账，保留
11 block／28 指令的完整根环境、predecessor、清理与回边检查，未重写或筛选 SSA。
child false edge 携带父 Replace 后的当前根，父回边携带 child 出口当前根。

这使 `run(parentCount, childCount)` 的父正／子零输入可独立表达，修复此前共用
count 时无法单独取该输入的见证限制；本片仅实际 SSA 验证，没有执行 native 或
声称动态跑过该输入。固定名称的后端源码适配仍是 fixture 证据，不是任意双参数
程序认证器。普通 selector、Elvis 分期、其他 nested sidecar 准入均未放宽。

本片验证：三个 handoff 外部套件合计六项普通测试、真实 planner 五项、D10 SSA
八项均通过；Rooted 与 MemoryDocument 均覆盖双参数及同次 initializer 身份。
前端与后端严格 Clippy、默认前端编译、文档 956、独立复审通过；四项 CI-only
身份错配测试保持 ignored。新增 Long 拒绝源码采用 `0L`／`1L`，前端合法后才
验证候选拒绝，不以类型错误代替 sidecar 边界。

### 真实循环后 a/b 双后用

后续封闭变体将末尾限定为 `println(a); println(b)`，而非只读取保留根 b。
实际 planner 轨迹是共同 25 事件及原 6／新 9 事件尾部（总 31／34），不删除、
重排或合成事件。新形态完整覆盖 7 items／15 statements／30 expressions，核验
两次内建调用的真实 argument/root、Shared loan begin/end 和声明来源。
原形态的 a LoopExit Drop 保留；新形态要求 a 的 Drop 精确位于其实际 CallReturn，
再接 b 的 CallReturn。最终 owned 全量匹配四 Drop、四 loan、两 Replace。

`final_root_use()` 明示这一不同形态；其中 source site 仅为最初构造位置，不是
当前动态代际。后端取真实父退出 block 参数作为 a 当前值，独立验证 RootPlace、
Shared Borrow、Print、End、Drop，随后验证 b 的同样链。实际普通 Unit 图保持
11 blocks，新形态为 32 instructions；仅用显式布局对应复用核验逻辑，新增四条
指令和整个 finished block 都逐条核对，没有过滤或修改原 SSA。bound 的参数与
initializer、final-use loan 来源也与独立真实源码证据对账。

该形态使后续原生输入矩阵可以观察 first／parent／next 的最终选择。当前只验证
真实源码、最终 owned、SSA 与 LLVM adapter，不宣称已经动态执行父零／子零／多轮。
StringLiteral 使用全局 bytes 与零 capacity，不产生对应 malloc，因此不能套用
分配计数声称证明本例的代际清理。原生调用入口还需显式测试适配器且遵循原超时；
它不属于源码准入合同，也不修改生产 entry。普通门禁、Elvis 延后和任意复杂退出
均未改变。

本片完整普通回归通过：handoff 六项、planner 五项、D10 SSA 八项（含双后用
实际 LLVM render/verify）；四项 CI-only 身份测试仍 ignored。前后端严格 Clippy、
默认前端编译、格式、文档 956 及独立复审通过。本地未执行故障注入或 native。

### 仅测试域原生输出适配器

独立 native 测试通过只读回调接收已完成上述完整 replay 的原 Program；明确要求
Rooted 和 MemoryDocument 两次交付，避免筛选空集假通过。先核真实函数 ID、两个
Shared Int32 loan 与无返回值 ABI，再按现有 LLVM 命名规则使用实际函数名。测试
只在原 LLVM 文本之后追加 `main` trampoline，原函数字节前缀、链接属性、SSA 与
sidecar 不修改；trampoline 不是源码入口合同或生产 entry 能力。

`main` 顺序调用 `(0,0)、(0,2)、(1,0)、(2,0)、(1,2)、(2,2)`，分别预期
`first、first、parent、parent、next、next`，每次随后输出 `kept`。Clang 将真实
LLVM 编译为 `.o` 再链接该对象；对象编译与动态执行是两个独立测试。执行复用 E2
既有 10 秒 deadline／10 毫秒轮询逻辑，只在超时时终止本测试启动的 child；失败
保留 IR、对象及输出，不改变系统设置或超时标准，不运行故障注入。

本轮实现时 native 窗口仍被既有完整测试占用，先只验证编译与静态门禁。输出矩阵
动态执行仍待空闲窗口，不能把 `--no-run` 或对象编译当作通过。即使后续通过，也
仅证明该封闭输入矩阵的输出，不声称 malloc/free 代际计数或通用 Carry 证明。

适配器 checkpoint 验证：Rust `--no-run`、Rooted/Memory 两份真实对象编译＋链接、
原 D10 SSA 八项与严格 Clippy 通过，独审无阻断。首轮适配器错误假设函数名为
`run`，实际 Unit 名为 `koven.run.d0`；已改为从真实 FunctionId/name 按 adapter
规则生成符号，未修改原函数名。动态测试尚未执行，不能列为已通过。

### 有界的实际 SSA 路径见证（非 native）

在原生窗口占用期间，仅对已由上述真实源码 sidecar＋完整独立 replay 核验的
双参数／双后用 Program 增加测试域路径执行器。它读取实际 block、instruction、
terminator 和 edge 参数，循环回边先快照实参再绑定目标参数；不是拼接 planner
静态日志，也不生成新的 SSA 或权限。支持操作集严格限于本片的 literal、Int32
Read/比较/checked subtract、RootPlace/Borrow/Replace/Print/End/Drop 和显式
branch/return；其他操作、未定义值、异常边或 2048 步预算耗尽立即拒绝。

每次执行 StringLiteral 创建的是**测试模型里的逻辑 owner 代际**，不是堆分配。
记录真实指令发生顺序，检查 Exclusive Replace 退休、Shared 借用打印与 End、
Drop 时无活跃借用及每代只清理一次；结束要求逻辑 owner 和局部 loan 均已清理。
九组正常输入含父零、子零、多轮、负 counter，核对最终 `first/parent/next`、
`kept` 和逻辑 Drop 顺序；另以合法输入的一步预算验证有界拒绝，不破坏任何 facts。

这只是封闭 SSA 子集的可执行路径见证，不能当作通用解释器、native 结果、实际
内存释放证据或通用 Carry 完备证明。原对象动态矩阵仍待空闲窗口按既有标准执行。

本片首轮及全 D10 九项普通 SSA 回归通过；Rooted/Memory 合计十八条正常路径、
两条一步预算拒绝均已执行。严格 Clippy、格式、文档 956 及独审通过。收尾只读
复核仍有既有 native 子进程在途，故本片没有启动原对象动态矩阵或终止其他任务。

### 完整 sealed sidecar 与真实 source/owned 对账

原 fixture replay 仅接 owned view，调用处只显式核对部分 counter 与后用 span。
现在 replay 必须同时接收同次 BoundDualNestedLoop，先核 owned analysis 身份，
再将独立从 AST、names、types、实际 owned 发现的全部 ID/事实与 sealed sidecar
逐字段对账：callable、父子 controls、根与 counters、参数和 initializer、条件与
更新、三个 producer site、两次 Replace、完整 cleanup/loan，以及 retained 和
可选最终根后用。入口 declaration 由实际 callable 的 source/item 唯一映射，
不再依赖函数名 `run`。独立 facts 不读取 bound 来填充预期值。

这补齐实验交接的闭合检查，不是新生产准入。source adapter 的其他固定名称与
11 blocks、28/32 instructions 布局限制仍保留；Program 自身没有新增 owned
provenance token，因此不宣称任意外部 Program 的不可伪造授权。原生动态矩阵
仍待空闲窗口，SSA 路径见证不是 native。

新增真实第二次 ownership 分析的双向错配拒绝控制，保持 ignored 并要求 CI
环境；现有 identity CI 脚本负责精确列举后执行。此类身份负控本地仅编译列举，
不执行、不伪造或改写任何 facts。正常源码/SSA 回归与故障负控分别报告。

本片正常 D10 SSA 九项通过（含真实函数改名后的入口映射），一项新身份负控
ignored；精确列举确认该项存在但未本地执行。严格 Clippy、格式、文档 956 与
独审通过。现有 native 占用期间未启动对象动态矩阵，未修改生产代码或门禁。

### 名称无关的封闭源码角色发现

后端独立 source evidence 不再按 `a/b/count/outer/inner` 或条件、调用的源码文本
选取节点。新增只读角色发现，从唯一函数体、固定父子语句层级中识别声明、两次
Replace、counter 初始化/更新及真实后用；声明 span 查唯一实际 Symbol，Name
表达式通过实际 reference 指向该 Symbol。类型、默认 Shared 参数、0/1 规范
字面量及 builtin call descriptor 仍逐项验证，不从 bound 复制角色或事实。

所有 item/statement/expression 必须在封闭角色覆盖中恰好出现一次，再与真实
owned 的 primitive、DropFact、LoanFact 以及 sealed sidecar 全量对账。贷款的
实际 argument ID、开始标记和调用结束也交叉核对；未知结构仍拒绝。保持同样的
11-block/28或32-instruction CFG replay，不扩大前端准入、控制流或 Carry 语义。

普通 SSA 源码见证新增全量变量/参数/函数改名和空白换行变体；native callback
只选择原精确矩阵 fixture，不再用名字子串猜测，仍要求 Rooted/Memory 两次交付。
动态对象矩阵与通用 Program provenance 问题没有因此解决。

本片前端 handoff 一项通过／一项 CI-only ignored，普通 D10 SSA 九项通过／一项
CI-only ignored；改名与空白正例均经过真实 owned、SSA replay 与适用的 LLVM
核验。严格 Clippy、格式、文档 956 和独审通过。本地未执行 native 或身份故障。

### child 结束之后再执行 parent Replace：独立前置证据

下一候选将 parent Replace 移至 child while 之后，保持两独立 Int counter、同一
被更新根和 retained 根的真实后用。它不等于现有 parent-before-child 形态：
child 零轮时 parent 接到的是进入 child 的旧代；child 多轮时接到的是 child
最后一次更新的代。不能从 producer site 或 planner 静态日志推出二者相同。

真实普通 ownership 和 Unit lowering 已能形成该 fixture 的 11 块／32 指令图。
新独立结构见证核实际 block 指令完整成员/顺序、header/body/exit/backedge 参数
运输，以及 child 假边的当前 owner 进入 parent Replace 所在块。child 更新值
经实际回边回到 child header，parent 更新值再经父回边回父 header；各次 detached
old 必须是相应真实 Replace 的第二个结果，Drop 与该次实际 owned fact 对账。
最终两个根核 Shared Borrow/Print/End/Drop 链。原始图不重排、不改写，LLVM
render/verify 另行核验；checked-abort 边保留。

这只是新候选的实际控制边、操作和清理前置证据，不是新的完整源码封闭 replay，
没有新增最终 owned sidecar，也没有动态执行零轮或多轮。现有 nested/dual sealed
候选继续拒绝该顺序。后续仍需完整 AST envelope、将已观察的 34 事件纳入候选收束及独立延迟绑定，
再把抽象 header/backedge 代际与原图对账；不得将本 fixture 的成功当生产准入。

本片真实前端完整轨迹 exact 一项通过，D10 普通 SSA 十项通过／一项 CI-only
ignored；新边见证含 Rooted/Memory 两类输入，最终精确复验通过。严格前后端
Clippy、格式、文档及独审作为提交门禁；未执行 native 或本地故障注入。

另跑全仓 Rust 尺寸检查发现继承的 `ssa/mod.rs` 1021 行与 owner_transport 的
`loops.rs` 1003 行仍超 1000 门槛；本片把新测试注册放在已有 D10 测试模块，
不再增长根 `ssa/mod.rs`，没有修改尺寸政策。该全仓门禁不能记为通过，需在后续
整合中另行处理；上述功能测试结果不代表尺寸问题已解决。

### AfterChild 独立封闭交付候选

在 main `cc83d9ec` 的隔离组合基线上，新增独立 `AfterChildPending` →
`BoundAfterChildNestedLoop`，显式 `ReplacementOrder::ChildThenParent`。
controls/counters/conditions 保父子角色顺序；replacements、incoming producers、
Exclusive loans 和旧值 cleanup 则始终保留真实静态访问的子→父次序，不把真实
事件或最终 owned 数组重排成旧候选的输入。旧 nested/dual API 不放宽。

候选只收双独立默认 Shared Int 参数、两根 literal 初始化、child 后 parent 各
一次 Replace、父子各减一、末尾 a/b 两个真实 prints_line 后用。完整 AST 7/15/30
节点覆盖与原始 34 事件直接匹配同时成立才产生私有 pending。独立第六通道从实际
planner 传至 analysis；诊断/类型失败清空，最后精确核原 owned 的两 primitive、
四 loans、四 drops 的原序及 typed 身份，validate 成功后才交付，不开放 retrofit。

跨 crate 测试从新 bound 的原始 owned 形成 Unit view，按真实 callable 选择入口。
在已有实际 11-block/32-instruction 图上补齐每条操作、全部块成员/边/terminator
来源、Shared Int32 参数运输、literal 实际解码、条件/更新、Replace 第二结果的
清理及最终 Shared 使用链。所有检查都针对实际原图，不修改 SSA 或替换原普通门。
源码能力权威来自前端新封闭 matcher；此后端层按它的真实 ID 反查源节点、与 SSA
对账，**未独立重做上一 parent-before-child 版本的完整 source-role 发现**。

因此这仍是 default-off 实验交付和同源图验证，不是生产 selector 准入、通用
Carry、外部任意 Program 的 provenance 能力、原生对象零/多轮执行或实际堆释放证明。
原 native 窗口与尺寸门禁遗留问题仍分别跟踪；新跨分析身份负控只布置 CI，不在
本地运行。


在完成新 bound 与全部真实 SSA/CFG 对账后，复用既有 2048 步封闭路径模型，
不改变它支持的操作或失败拒绝规则。九组父/子零轮、多轮和负 counter 输入核验
最终根：父未进入取真实 initial literal，父进入后总取其真实 parent incoming
literal；retained 输出与全部逻辑 owner 的清理顺序也由实际执行到的指令验证。
预期字节来自该次 source producer 的实际解码，含改名及不同文字的源码变体。
这是对原图的有界逻辑路径见证，不是原生程序或 allocator 的动态验收。

本片验证：新旧四组前端 handoff 共七项普通测试通过、五项 CI-only ignored；
D10 SSA 十一项通过、一项身份负控 ignored。新能力的三种正源码 × 两类输入 ×
九组 counter 合计 54 条有界逻辑路径通过，最终 exact 复验通过。新身份负控已
精确编译列举一项且接入现有 CI 脚本，未本地执行。默认前端编译、严格前后端
Clippy、格式、文档 957 和独审通过。继承的两项 Rust 尺寸超限仍未解决；没有
运行 native、故障注入或推送。

### AfterChild 后端独立源码对账

补上上一片明确保留的缺口：独立测试 oracle 只从真实 AST 的 child→parent
顺序发现角色，不读取 sealed bound 来生成预期。两参数、声明、控制、条件/更新、
调用和后用必须覆盖全部 item/statement/expression 各一次；Symbol/Reference、
类型及实际 builtin descriptor 均来自该次 Unit。再核原 owned 的两 primitive、
四 loan 和四 Drop 的真实顺序及作用点，最后逐项对账 bound 的全部公开身份字段。
原 SSA/CFG replay 与有界路径模型不变，不能以源码形状检查代替授权。

普通合法邻界在独立 ordinary owned 上先检查角色拒绝，再检查 sidecar 是否交付，
避免负例仅因前端拒绝而跳过独立 oracle。新增父→子顺序拒绝及空白正例，保留
完整改名和 Unicode literal 见证。四正源码 × 两类输入 × 九 counter 组合仍只是
受限 SSA 逻辑路径，不是 native 或 heap 清理验收。生产入口、默认 feature 和
普通拒绝门均未改变；原生对象矩阵与两个继承尺寸失败仍未解决。

本片 D10 普通 SSA 回归十一项通过、一项 CI-only ignored；包含上述 72 条有界
路径。严格 codegen Clippy、格式、文档 957 与独立只读审查通过。未运行 native
或身份故障控制；尺寸只检查新增文件均低于 1000，未重跑或宣称全仓尺寸通过。

### Default-off AfterChild 实际图校验与只读交付

新增 `d10_experiment::lower_after_child`，仅在原有 default-off 实验 feature 下编译。
接收完整普通 owned view 与同次 sealed bound，核最终 ownership 身份后由原始
source/root 唯一选入口，仍调用现有普通 Unit lowering，任何既有拒绝原样保留。
随后必须通过独立可失败图 checker，才构造私有字段的 `AfterChildProgram`。
它保留 view 和 bound 借用，不允许可变或 consuming Program 逃逸；只提供来源
元数据及通过现有 LLVM adapter 校验的 IR 文本，不生成 native entry glue 或对象。
失败只返回阶段错误，未构造的局部 Program 丢弃，不发布部分产物。

checker 不使用测试 fixture 的块号、指令号或参数槽号，也不把旧 assert oracle
转为准入许可。它按同次 bound 的实际 source Origin、opcode 和依赖唯一定位
条件、读参数、literal、Replace 和步进，再沿实际 Branch/Conditional 边发现
父子 header/body/exit、checked-abort 与 resume。每条边按 argument 对实际目标
parameter 建立角色双射，核 child 零轮出口当前 owner、两类更新回边和最终双根
后用。所有块、指令、顺序、终结符/边、loan/Drop 和 checked-abort metadata
必须完整匹配；遗漏、歧义和未知布局均拒绝。前端 sealed 观察仍是源码/owned
事实权威，checker 证明的是实际图与该受限合同的一致性，不自行扩展语言规则。

原测试源码 oracle 保持独立：正例改走新封包成功后，再用原 AST/owned 对账和
固定图 oracle，以及 72 条有界逻辑路径复核。CI-only 另登记 foreign-owned
封包拒绝与保 IR-local 有效的 literal、步进常量和来源损坏；本地只编译列举，
未执行这些故障控制，失败原子性尚不能据此宣称已经动态故障验收。

E2 现有不可变发布类型绑定其专用事实及 test feature，不能当作 D10 capability
复用；本片复用的是已有普通 Unit gate、SSA verifier 和 LLVM adapter，不新造
lowering 或绕开真实前端产物。默认 driver/selector 未接入，通用 Carry、更广
嵌套或复杂退出、native 动态矩阵及堆清理验收仍不在本片完成范围。

本片最终验收：非 test 的实验 feature 编译通过，普通 D10 SSA 十一项通过／两项
CI-only ignored，含经新封包交付后的 72 条有界路径。新负控精确编译列举一项，
未执行；实验 tests 严格 Clippy、默认 codegen check、格式、文档 957、全仓尺寸
（基底 main `cc83d9ec`）及独立源码/生命周期/全图审查通过。没有运行 native、
本地故障注入或推送；远端 CI 及动态故障拒绝仍待实际执行。

### BeforeChild 独立顺序闭环

新增 `lower_before_child` 和独立 `BeforeChildProgram`，只收现有 Dual bound 中
双独立 counter 且 a/b 均真实后用的子集；同参数或仅 retained 后用即使已有普通
bound 也明确拒绝。它不借用 AfterChild 权限，默认 selector 和原普通 Unit gate
均未修改。共用的只是顺序无关图查询、操作组和 edge→parameter 角色映射；原
AfterChild inspect 拓扑核验保持不变，并以独立搬移比对复核。

BeforeChild checker 按真实来源发现父 Replace 与 child counter Read 同块的
前序结构；父新 owner 进入 child header。child 假边进入仅父步进的块，父回边
保留该假边运输的实际当前 a，不把它回填为静态父 producer。因此 child 零轮
使用父新代，多轮使用 child 最后代；所有块/指令/loan/Drop/checked-abort 仍须
完整匹配。该证明来自真实边与实体，不从有限执行样本或 producer site 外推。

已有独立源码/owned 与固定图 oracle 在新封包成功后继续交叉核验。前序四种
正源码（原文、改名、空白、Unicode）× Rooted/Memory × 九组 counter 的 72 条
逻辑路径检查最终输出与清理；同样数量的后序路径保持回归。它们依然不是 native
或 allocator 验收。前序身份/保 IR 有效的来源与常量损坏仅安排 CI-only 控制，
本地编译列举，不执行。普通合法但未获该窄封包支持的形态走实际拒绝路径。

本片最终 D10 普通 SSA 十一项通过／三项 CI-only ignored，新前序负控精确编译
列举一项而未执行。默认与实验 feature 的非 test codegen check、实验 tests
严格 Clippy、格式、文档 957、全仓尺寸（main `cc83d9ec`）及顺序/生命周期/CI
隔离独审通过。native 窗口仍有其他实际测试进程，本片未启动 native，未本地
注入故障或推送；完整动态与远端 CI 验收不继承这些定向结果。

### 从顺序封包到生产入口的剩余合同

当前前/后顺序封包已通过真实普通 Unit lowering，再独立检查完整实际图；这不等于
生产 driver 已消费该证明。`native::validate_unit_entry` 只允许既有零参入口或
单个 `borrow Array<String>` 参数入口，两 Shared Int 的候选函数不能冒充其中任一
进程 ABI。测试专用 trampoline 也不能作为生产入口授权。两侧 checker 与前端
完整源码 envelope 仍限定单函数，不能为了接 main wrapper 简单删除函数数量检查。

下一生产组合必须独立证明：合法 entry wrapper 到原 callable 的解析身份、同次
多函数 owned/闭包计划、实际 Call 参数和 loan 生命周期、callee 全图及 wrapper
全部操作/清理覆盖；然后才可复用现行原子对象发布。当前原生零/多轮动态矩阵和
CI-only 负控仍待实际执行。有限 SSA 路径与 LLVM/object 验证均不能替代这些验收。

### Default-off 真实库对象字节交付

两种顺序封包增加 `verified_library_object_bytes()`。只在原 default-off feature 下，
对私有不可变 Program 再走同一 SSA verifier、LLVM verifier 和宿主 TargetMachine，
成功完成内存对象发射后才返回完整字节；不构造 process entry/debug glue、不写
用户文件，不把内部符号提升为稳定外部库 ABI。原 native 文件发布与入口门不变。

普通源码测试分别对前/后序原始 Rooted/Memory 四份对象检查宿主格式、relocatable
类型，将真实字节写入独立临时文件，使用系统 nm 核原函数恰好一个已定义符号且
没有 main；不重新编译 LLVM 文本，不链接或执行对象。该结果只补齐后端产物证据，
不能称作生产 native 接入或运行时清理验收。

本片最终 D10 普通回归十一项通过／三项 CI-only ignored，包含四个库对象及符号
检查；默认/实验 feature 非 test 编译、实验 tests 严格 Clippy、格式、文档 957、
全仓尺寸及独立生命周期/权限/失败不交付审查通过。未执行生成程序、CI-only
故障控制或推送。已有对象动态矩阵仍因共享 native 窗口被占用而未运行。

### 两函数入口候选的独立封闭合同

下一候选不能扩大旧 singleton bound。拟限定同一 source 的两个普通非泛型、
无 receiver 函数：零参数 Unit wrapper 正常返回，仅一次调用已解析的双 Int
callee；callee 保留指定前/后顺序、双独立 counter、双根真实最终使用。先用
实际普通源码验证实参运输，不预设 Int 必有或必无 loan，literal 与局部变量
实参也不擅自视为同一种权限。

需独立的完整两函数 coverage，对三个 AST arena 无遗漏、无重叠；callee 局部
角色不是全源码许可。新的 pending 必须来自实际 planner，记录真实 wrapper
Call、来源与实际生命周期；binder 对全部 owned facts 建立保序且无遗漏的职责
分区，不能过滤 wrapper facts 再塞入旧 bound。diagnostics/invalid-types 清空、
typed/owned 同次身份和全部事实对账必须全部成功，才交付独立 bound。

后端必须在原普通 reachable-closure lowering 的同一个 Program 中检查 entry
与 callee 的真实 FunctionId 和 Call 关系，核 wrapper 全部操作/运输/结束与
callee 全图；不复制、裁剪或重编号 Program 后冒充旧 singleton 检查。共同检查
完成前不发布封包或产物。此段是未启用候选，不允许通过删除旧单函数拒绝或
使用测试 trampoline 来替代实现；原入口、ABI 与默认 gates 保持不变。

先行普通源码对照采用 literal 实参 `run(0, 0)`、`run(1, 0)`、`run(1, 2)`，
两替换顺序 × Rooted/Memory 共十二组合。它走真正普通 owned/entry reachable
closure，不手工插入 main 或修改 SSA；旧两种 singleton sidecar 仍必须返回 None。
对实际 wrapper 单块核六条 constant/place/borrow 构造、DirectCall、逆序两条
BorrowEnd 及 Return，依真实源码实参位置对账 owned 原序 Shared temporary loan。
增强验收发现 integer literal 不在 `observes_call_loan` 白名单，公开
`direct_call_loan_ends` 为零；不能声称同次前端独立结束权威已经具备，也不能
用实际 SSA BorrowEnd 或描述符 end_span 倒填它。literal 形态因此仍不能作为
新 wrapper bound 的准入。
callee 的真实身份及 Shared Int 参数匹配，并通过完整 Program SSA/LLVM verifier。
这些是可重复的普通调用组合见证，尚非两函数新 bound 或 callee 独立全图授权。

具名局部补充探针 `val p = 1; val c = 2; run(p, c)` 实测在两顺序与两输入布局
四组合都产生两个真实 Shared 根 loan，每条恰有一条同 fact 的 direct-call 结束
记录且 point 为该调用 CallReturn，普通 SSA/LLVM 同样通过。这与 literal 的零
记录形成明确差异；下一新 bound 候选可仅研究具名 Int 局部实参，但仍须完整
两函数 AST、原序全 owned 分区和独立共同图证明，不能仅凭此四例签发权限。
本批没有实现新的 WrapperPending/Bound，也未使现有 singleton 接受额外函数。

具名探针尚未独立核两 root 的互异及逐实参绑定，也未完成其全指令 replay；这些
仍是新合同实现前置，不由上述 End 事实存在性结果外推。本片最终 D10 普通回归
十三项通过／三项 CI-only ignored，严格 tests Clippy、格式、文档、尺寸与独审
通过。未修改生产代码、执行生成程序、运行故障控制或推送。

### 新 bound 前置：真实 source-root 事件边界

实际 `DropPlanner` 是每 source 一个实例，先顺序遍历全部 roots，再处理需要
访问的 lambda，并非每 callable 一个实例。旧自动 probe 只对既有完整 AST 预算
采样，named 两函数不在其范围。不能过滤全 trace 后宣称得到旧 callee 权限。

本 checkpoint 在测试域显式启用独立 root-range 记录：每次真实 `item(root)`
调用前记起点，成功后才记录 source-qualified root 与结束点；错误传播不交付
部分 planner。普通生产遍历未变，旧自动采样预算未扩大。为避免历史大文件增长，
完整 run 方法移到子模块，除测试采样和父模块所需可见性外，正文逐字等价。

具名真实源码实测得到 callee 的 34 条原始事件和 wrapper 的 5 条事件，全部
连续且未过滤；wrapper 的实际 call、两条 Shared root loan 与最终 owned 六条
loan 的原序身份核验，两条逆序 DirectBorrowEnded 对应真实 End/CallReturn，
callee 四个 Drop 与全 owned Drop 保持相等。显式无 lambda 才要求这两段覆盖
全部事件，不外推其他源码。开启采样前后的原事实/观测相等，旧 pending 仍拒绝。

这只是新通道所需的实测前置，不发布 WrapperPending/BoundNamedWrapper。下一片
仍需把局部 callee coverage 与完整两函数覆盖区分、将实际两段观察送到同分析
最终 owned binder，并检查所有事实类别无剩余；共同 Program 图证明仍未接入。

本 checkpoint 精确前端普通测试通过，下游 D10 十三项通过／三项 ignored；前端
实验 tests 严格 Clippy、默认编译、格式、文档 957、全仓尺寸及独审通过。没有
运行生成程序或故障控制，没有推送；不能据此宣布新两函数 bound 已完成。

### 独立 named 两函数前端 Bound

新增 default-off `analyze_named_wrapper` 与不可外部构造的 `BoundNamedWrapper`。
最窄范围固定 BeforeChild callee→零参数 wrapper 的 source-root 顺序；wrapper
只有两个不同具名 `val Int` literal 初始化和一次按位置调用，callee 有两个独立
counter 与双根真实后用。literal 直接实参、同根复用、换位、额外调用/声明等
不在范围。借用型 `NamedWrapperCallee` 只读原局部事实，不可转换为旧 Bound。

旧 callee 严格检查通过私有字段提取复用。旧入口仍要求唯一 root 和原完整 AST；
新入口把已核 wrapper 节点加入同一原始 AST 的逐 ID 覆盖，拒绝遗漏或重叠，不
裁剪 ParsedFile。受限新观测通道记录真实两个 root 的连续 34＋5 事件并全部
消费，核实际 call 进入/实参完成/可dispatch来源，以及原始两 Loan 的逆序 End。
新第七分析通道独立传递 pending，沿既有错误路径清空；最终 binder 同次 typed
身份、无诊断/deferred、全 owned 对账与 validate 全通过后才交付。

四 Drop、六 Loan、两 End 与两 Replace 保原序核验；八调用合同先与实际 AST/
typed 逐字段核验，再与最终 owned 双向唯一对账，不重排 owned，也不声称合同
数组与另一个构建序列逐项相等。参数 bindings 两角色、formed locals 八个互异
角色精确覆盖；receiver/closure/borrow transport 等不适用事实通道明确拒绝。
剩余合法 value-delivery/owner-observation 等保留现有 validated owned 含义，本
Bound 不授予新的动态 owner 状态证明。

实现验收纠正了两个假设：合同的 argument_span 是实际表达式节点，不含 `&`
标记；参数 bindings 与局部 formed-owned 列表不能混用。都依据真实普通源码及
现有合同修正，没有减弱未知事实拒绝。第七通道使历史大文件需要结构收尾，完整
construction descriptor 收集 helper 原样移入既有 construction 子模块；独审
确认归一化最窄可见性与路径限定后函数正文完全相等，尺寸政策未改。

真实 integration 测试不在 cfg(test) 的库内构造能力，覆盖十三种合法源码各
Rooted/Memory，六接受、二十拒绝，并逐项核旧两种 singleton 仍拒绝。实际
root-trace/pending 普通测试通过。此片仅前端源码和最终 owned 交接，尚未实现
共同 Program checker、LLVM/native入口接线；静态错误清空审查不能冒充动态
故障验收。本树仍基于旧 main 组合，未合入后续 #429。

本批最终前端 integration 二十六组合与实际 root/pending 精确测试通过；旧 D10
下游十三项通过／三项 CI-only ignored。前端实验与默认 tests 严格 Clippy、非
测试 codegen 实验编译、格式、文档 957、全仓尺寸和独审通过。AfterChild 替换
顺序及 wrapper-first 声明顺序也作为合法源码明确拒绝。未执行生成程序、故障
控制或推送；后续 #429 新主线组合、共同 Program与 native 验收仍分别待做。

### #429 隔离组合与 named 共同 Program

本轮在独立工作树组合 main `21490e9c`，保留原 `ac134d2b` 工作树。唯一冲突
位于条件 lowering：先保留 scalar-entry 的实际条件记录，再执行主线 Copyable
可达性与 literal 折叠，不能任取一侧。旧 scalar 条件是原参数而非 literal；
完整 diamond verifier 未放宽。来自 #428 仓内源码的本地 explicit-if 变体现在
要求普通 SSA 与 LLVM 验证通过；原 Elvis 仍要求 UnsupportedNode，不启用语义。

新增 default-off named 后端路径从新 Bound 的真实 wrapper declaration 发起
一次普通 Unit lowering，由真实直接调用可达性形成共同 Program，不启用测试
root-set 接口、不合并两个独立 Program、不生成 main。全图检查要求一个 module
和恰好两个原源码 callable；callee 的原 BeforeChild 函数内图证明复用私有只读
投影，旧 singleton 入口仍独立要求同 owned 身份和唯一函数。

wrapper 检查原 literal 初始化、参数位置与 Shared Int ABI、RootPlace/BorrowBegin、
唯一实际 DirectCall 目标、两条逆序 BorrowEnd、完整单块指令与 Return，均关联
同分析 bound 和原来源。全部图检查成功后才创建私有不可变 NamedWrapperProgram；
其持有原 view/bound，不能提供可变或 consuming Program。LLVM 和对象发射复用
现有验证适配器，返回 bytes 不写入磁盘、不链接执行、不授予稳定 ABI/进程入口。

此闭环不等于普通 driver 的 D10 新准入，不扩张 Origin、泛化 Carry 或额外控制
流。动态损坏/身份失败负控仍待 CI；本地只使用合法源码、静态图与对象检查。

组合使 condition.rs 从两侧各 981／990 行合成 1011 行，尺寸门禁初次失败。
仅将末尾既有 receiver_ci_tests 模块原样去缩进移到命名文件，保留 cfg(test)、
原模块名、可见性及所有测试路径；反向缩进字节等价核验通过，未执行这些 CI-only
控制。生产条件文件回到 823 行，未修改尺寸政策或提高例外。

新后端普通验收覆盖九种合法源码各 Rooted/Memory：六接受、十二拒绝；原始两份
实际库对象均核两个原函数的 defined symbol 和无 main。完整 D10 SSA 回归十四
通过、三项 CI-only ignored，包含 #429 边界；四个实际 scalar 入口／部分清理／
concat／三根恢复普通 SSA 回归各通过。前端二十六组合在新主线组合也通过。
没有运行生成程序、图损坏或身份故障；共同 Program 对象证据不替代 native 验收。

最终 feature all-targets 严格 Clippy、默认非实验 codegen 编译、格式与 diff 检查
通过，文档 957 项及全仓 2234 Rust 文件尺寸门禁通过。独审确认新全图封包边界、
旧 singleton 准入与 CI-only 模块字节等价；没有提高例外或把旧测试拒绝当作新
功能实现。原工作树保留，本轮仅独立树本地提交，未推送。

### named 源码零参 entry 与原子 process object

在已有不可变 NamedWrapperProgram 上增加显式 `emit_process_object`。仅该封包
能进入 feature-only native桥，不接受调用者提供任意 Program、FunctionId或
替代 entry。先重新核同次 owned、bound 的唯一源码 declaration、零参 Unit，
再核原共同 Program 完整图与实际 LLVM entry plan；全部在预留输出前完成。
不重新 lowering，不生成测试 SSA main；由已有 LLVM process glue调用真实
wrapper，wrapper再调用原 callee。旧无 main库对象方法与普通 driver gates未变。

发射复用原 SiblingObject：同目录 create_new临时文件、既有LLVM验证/对象写出、
成功后rename提交；失败沿原错误分类与Drop清理。普通对象测试以真实合法对象
完成Rooted/Memory各新建和覆盖，核 main及两个原函数defined符号、无临时遗留。
该成功测试不等于失败原子性验收。新增ignored且CI环境门控的失败控制，覆盖有/
无既有目标、实际emission_failure hook、Backend分类、旧字节/目录不变；只接入
既有CI-only脚本，本地仅编译列举，不执行。

真实源码native验收单独注册在native测试域，不混入 `ssa::d10_` 静态过滤器。
源码本身含零参entry及两具名Int实参；直接发射上述process对象后用既有Clang
链接，不存在此前测试ABI trampoline。父零、子零和父子多轮四种配置各两输入
布局，复用未改动的10秒/10毫秒执行helper，失败停止并保留源码、对象与输出。
这只针对当前封闭BeforeChild形态，不推广AfterChild、通用Carry或Origin语义，
也不以结果输出声称完成运行时分配/释放计数或sanitizer验收。

本批 native精确测试一次通过：八个真实源码配置总耗时8.13秒，每个生成程序仍
使用原10秒上限，未重试或调整安全策略。正常 D10 SSA回归十五通过／四项
CI-only ignored；新原子失败控制及既有CI精确列表全部以 `--list-only`通过，
从未本地执行负控。实验all-targets严格Clippy、默认codegen编译、格式与diff
检查通过，全仓2237 Rust文件尺寸门禁通过。独审确认source入口、同图重核、
事务和native证据边界；普通生产准入、其他顺序/形态推广与跨宿主CI仍分别待验。

跨crate公开方法另以 `cargo check --workspace --all-targets --features
lang-codegen/d10-catalog-experiment` 离线验证通过，包含CLI/LSP消费者与测试编译；
文档957项检查通过。该check不执行任何native或ignored控制。

### 独立 AfterChild named 源码闭环

另设 `analyze_named_after_child_wrapper`、`BoundNamedAfterChildWrapper` 与
`NamedAfterChildProgram`，不接受 BeforeChild Bound，不把事件数组转成另一顺序。
控制/counter仍父→子；producer、Replace、Exclusive Loan和detached-old清理
保持实际child→parent。独立第八pending通道通过同次最终owned绑定，错误沿原
路径清空。局部wrapper五事件和最终owned核验只共用私有顺序中立字段投影，
原AfterChild三十四事件与两root全AST覆盖独立成立，旧singleton仍检查唯一root。

AfterChild函数内图core保留child false边向parent Replace运输当前A：child零轮
也进入parent Replace，parent旧值必须来自实际child退出边。新共同图先核自身
bound身份，再用自身callee checker；共享wrapper局部检查不签发任何另一顺序的
权限。两个module-level公开封包不互换，原BeforeChild入口保持原接收集。

新封包独立原子对象桥复用既有native entry校验与SiblingObject，不重新lower。
普通对象/CI-only失败测试由各自父模块绑定各自analyze/lower接口，源码与能力不
跨用。AfterChild真实source-entry native矩阵期望父零轮为first、父正轮无论
child零/多轮均为parent，第二根为kept；仍复用原10秒执行器，不改旧BeforeChild
期望，不把单一退出值外推为运行时分配/释放计数或一般Carry证明。

本批普通前端矩阵二十六个源码/布局案例通过；后端完整 D10 静态及对象回归
十七通过／五项CI-only ignored。真实源码 native 精确验收使用本批已编译测试
二进制，AfterChild八程序一次通过（6.45秒），旧BeforeChild八程序回归一次通过
（6.28秒）；单程序仍原10秒，不重试、不注入故障。独立检查确认 child零轮仍
执行parent Replace，两root原序事实未被旧顺序封包替代。运行时分配计数、
sanitizer、跨宿主CI及普通生产driver准入仍不在这些结果之内。

共享过程对象断言改为单注册helper与显式源码/analyzer/lower参数的测试suite，
保持两种Bound独立和CI exact名称，避免重复加载模块；修正后实验all-targets
严格Clippy与上述十七项普通回归重新通过。CI完整精确列表仍只以 `--list-only`
验证；默认codegen、workspace all-targets实验编译通过，格式/diff、957份文档和
2244个Rust文件尺寸门禁通过，独立复审无阻断。以上均本地验收，未推送。

### 同次 named entry 判别前置，不是普通 driver 准入

现有两个公开analyzer各自执行完整ownership分析，调用者依次试探两个顺序会
生成两套身份。新增default-off统一入口仅调用一次catalog分析，候选计数必须
恰为Before一项/After零项或反之；缺项、多项、同时存在均拒绝。选择后将同一
最终owned移入原独立binder；绑定失败不转试另一顺序。私有判别封包只读投影
保留原sealed bound，后端统一入口只接受整体封包，不接受外部拼接的view。

后端按已封闭变体调用原Unit lowering、完整图checker和原子对象桥，不复用
另一顺序权限、不返回部分Program、不自动fallback。统一对象方法同样不链接
或执行。这减少后续driver整合时二次分析身份分叉，不能被表述为正式准入：
CLI仍使用complete type admission及Basic/Constant capability；实验feature仍
默认关闭，其test-support依赖、正式选择与诊断合同、跨宿主验收尚须独立处理。
本批不修改CLI、默认feature或普通生产gate。

单次判别批次验收：前端十二源码/布局组合及旧两analyzer回归通过；后端两顺序
各Rooted/Memory真实图与对象接线通过，普通D10套件十八通过／五CI-only跳过。
实验all-targets严格Clippy、默认codegen及workspace all-targets编译通过。
本批未新增native执行或本地故障注入，未将此前两独立入口native结果称为统一
入口的新native验收；复用原对象事务不是新的失败原子性负控结果。

### 显式 final-admitted prepare 与非测试库边界

`ValidatedCompilationUnitTypes` 的Basic验证不代替CLI采用的最终类型准入；
ownership有诊断时会清除catalog候选，因此旧Option返回值不能独自区分非法
源码和超出封闭形态。本批新增显式prepare先执行完整final admission，再做
一次实际catalog分析，先保留ownership诊断/deferred，再判唯一候选及原binder。
不支持形态、歧义、证据绑定失败和真实前端错误分别处理，不转试其他路径。
准备产物持有自己的typed与bound，后端从它们构造同次view并复用原完整图检查。

D10特性不再隐式开启loan-observation-test-support；默认planner本就记录真实
loan事实，该测试特性仅使其可由测试hook控制。外部integration只用公开API，
使前端及后端库均在非cfg(test)下编译，验证prepare至真实process对象；不链接
或执行对象。既有Refine单元观察helper只在启用测试support时有全部消费者，
独立D10的lib-test组合仍触发其历史dead-code边界，未以allow或删见证消除。
CI-only单元脚本现在显式声明测试support，生产/外部integration不依赖它。
这不是CLI默认选择或正式功能启用，默认features与普通Basic/Constant门不变。

本批非test库外部integration四个真实对象（两顺序×两输入布局）通过；前端十二
组合通过，包含合法Unsupported、真实use-after-move原ownership诊断，以及
Basic验证成功后final admission拒绝的L0080及fooBar精确span。旧Option回归
通过。独立D10的库/integration严格Clippy、显式测试support下十八项D10普通
单元及all-targets严格Clippy通过，五项CI-only保持ignored；CI脚本仅列举通过。
默认codegen编译通过。deferred-only场景、真实故障、跨宿主及普通CLI选择仍
未由本批验证；本批无native执行，独立lib-test组合的旧helper限制如上保留。

### 显式实验 CLI 对象命令

仅CLI编译开启 `d10-catalog-experiment` 时注册
`experimental-d10-object <source.ko> --entry <name> -o <object>`。这是builtin-only
单文件实验loader，不是普通build/run/project；不追加或裁剪标准源码snapshot。
复用原package/path规则、完整名称前缀与human/JSON renderer，随后basic typed、
final-admitted prepare、精确source-qualified entry核对、原完整图与对象事务。
不猜main、不接常量/普通fallback，不链接或执行生成程序。语言诊断exit1，
usage/IO/Unsupported/证据拒绝等操作错误exit2，stdout空；默认CLI没有该命令。

CLI最窄路径策略拒绝一切已存在输出（含symlink、dangling symlink、hardlink），
要求父目录已存在且是目录，规范化父目录和单一文件名并排除输入重合；发射前
再次核验，所有gate之前不reserve/mkdir/truncate。成功复用SiblingObject原子
rename，失败沿原清理；双检查不等同对恶意并发创建/路径替换的no-clobber保证，
本实验未新增该并发保证。普通build/run代码与默认help不改变。

CLI批次验收：实验对象集成三项通过，包含两顺序成功及错误entry、parse/name/
type/final/ownership、合法Unsupported无产物；已存在和各类别名/缺父目录拒绝，
保留原字节，JSON实核L0080原source路径及fooBar精确span。默认关闭命令及help
验证通过，旧check集成六项、main分派/help/global-options三个精确回归通过。
默认与实验CLI all-targets严格Clippy、默认build、格式、2253文件尺寸和957文档
门禁通过，独审无阻断。本批只运行编译器并检查对象，不链接/执行生成程序；
未运行native集成套件或故障注入，不宣称覆盖并发恶意路径替换。

### 实验 CLI 的原子 create-if-absent 发布

本节更新上一节发布点的检查后竞态限制：CLI现在调用独立
`emit_new_process_object`，仍执行完整身份、entry、图、LLVM验证与真实对象写出，
但最终使用同目录 `hard_link(temporary, target)` 创建目标目录项，不再把预检查
视为no-clobber证明。已有任何目标项均不得替换；发布失败不退回rename/copy。
旧 `emit_process_object`、普通native和E2的替换语义均未改。

hard-link成功就是唯一提交点，随后原guard尽力删除临时名字；删除失败不能
返回“未发布”错误或删除已提交target回滚。因此正常环境验收无temp残留不等于
所有故障情况下必清理。文件系统必须支持同卷硬链接，不支持时安全失败。
本合同不保证断电持久性、恶意父目录/临时文件替换或跨宿主可用性；真正检查后
竞争测试须另在CI运行，本地不注入竞态或清理故障。

create-only批次普通验收通过：非test库公开API两顺序×两布局直接调用新方法，
验证新目标成功、既有文件/目录/软链/悬空软链不替换及旧字节不变；该检查不经过
CLI预检。CLI三个集成回归通过；旧D10含原替换对象路径十八通过／五CI-only
ignored，未改变原替换能力。独立库/integration及CLI严格Clippy、默认编译、
格式、2253文件尺寸与957文档门禁通过，独审无阻断。仅本机普通文件系统验证，
未执行竞态、故障注入、跨宿主或新native矩阵。

### 双宿主发布验收与稳定冲突分类

实验create-only的真实目标占用现在返回feature-gated `OutputAlreadyExists`，
其他hard-link/LLVM失败仍为原backend错误；不会用本地化错误文本猜测冲突。
原替换路径与默认enum不变。CI原D10单元oracle显式加测试support，独立非test
公开库与CLI验证分开Cargo调用且禁用default features，避免feature union掩盖
证明。接入既有macOS15/Ubuntu26.04、LLVM21矩阵，不另设未核宿主。

新增ignored且GitHub Actions门控的公开发布竞争测试：两个线程各自真实分析、
图验证、生成不同参考对象，然后释放开始发布同一目标；只接受恰一成功及另一
真实OutputAlreadyExists，最终文件须等于成功者事先独立对象，核main符号与
普通清理。该测试不声称两系统调用必同时发生或覆盖所有调度；只验证此并发
发布场景。channel等待有界，CI步骤另设五分钟整体上限覆盖LLVM/join/nm。
本地仅编译与精确列举，竞争执行及Linux结果均仍待CI，不据此宣称已通过。

本批本地验收：新增竞争控制仅精确编译/列举通过，未执行；公开非test库真实
对象及稳定冲突分类通过，实验CLI三个集成、default命令拒绝、frontend prepare
及旧单元oracle命令通过。新增CI测试严格Clippy、默认编译、格式、2254文件
尺寸、957文档与workflow YAML解析通过，独审无阻断。workflow已经布置不等于
Linux或竞争控制已绿；仍须对实际推送头取得双宿主CI结果。

### 显式声明选择的公共准备合同

`prepare_selected_named_entry` 把此前CLI私有的wrapper选择约束放到可复用的前端
准备入口。调用方借用本次names索引中的真实 `UnitDeclaration`，完整final type /
ownership与唯一候选绑定成功后，核声明对象确为该索引成员，再核Function类别和
source-unit/root等于sealed wrapper。值相同的跨分析ID不充当来源能力，克隆声明
也不接受。选择器不是独立ownership权限，不能扩大两种已证明顺序或普通准入。

旧prepare保留。CLI只负责字符串解析和呈现，解析结果延后消费；找不到入口时仍
先保留原语言/准入错误，成功解析则调用新入口且不重复分析、不fallback。独立
公开库对象路径同样复用新合同。默认build/run、单源码builtin-only限制、最终
create-only发布与失败清理边界均不变；没有接入普通项目/std-source流程。

本批普通验收：两顺序×两布局的真实wrapper成功/callee拒绝，公开非test库
同路径对象，以及CLI真实错误加缺失入口仍保相同human/JSON诊断均通过。
默认CLI拒绝、默认build/check、前端/公开对象/CLI严格Clippy、2254文件尺寸及
957文档门禁通过。新增跨分析同值ID和克隆声明负控有原声明成功正控，已接既有
CI脚本；本地只精确编译列举，未执行，实际拒绝验收仍待CI。未运行新native。

### 两个真实源码根的声明顺序

本节更新前文wrapper-first仍拒绝的历史范围，保留其他未授权形态拒绝。
封闭两函数入口现在允许wrapper出现在callee之前。该范围扩展不改变父/子
Replace顺序，也不开放第三根、递归或额外调用。角色由唯一零参数函数与其真实
typed direct-call目标确定，另一个根必须确为callee；原完整AST访问集合和预算
保持不变。source root、FunctionId、generation顺序是三个不同概念。

planner保留源码遍历次序，要求两个原root ranges与parsed roots逐项对应、首0/
连续/末尾覆盖全部事件；仅借用各角色的原连续切片，不过滤、排序或拼接事件。
私有 `NamedRootOrder` 从通过核验的实际range产生，最终binder据其构造完整期望
loan序列与原owned数组逐项比较，不修改owned。Drop、primitive、End与其他通道
继续原完整检查，不能因为wrapper先执行源码遍历就重排实际callee事实。

后端仍由完整source origin和原typed callable核验实际图，未新增普通Unit准入、
普通build/run选择或自动fallback。反向声明只扩大此default-off封闭源码合同。

本批实测：BeforeChild真实planner全39事件测试覆盖两种源码root顺序，原
34+5和新5+34均逐项保留原事件/owned loan顺序；AfterChild由完整handoff与
后端图、对象矩阵验证，不冒充同一细粒度trace测试。公开非test库两种Replace
顺序×两种声明顺序×两种输入布局共八组合生成真实对象成功；CLI四种成功
形态及literal/第三root/错误entry等拒绝通过。旧D10 SSA十八通过、五CI-only
ignored；四个前端handoff套件通过。最初AfterChild测试保留的固定loan预期
在新增反向源码上失败，已按实际源码root次序修正expected并复验通过，实际
owned数组从未重排。未执行新native、故障控制或跨宿主验收。

### 普通多源码 project 与独立 D10 封包的差距校准

必须区分两个问题：独立D10 named封包当前只接受单source，不等于普通Unit/driver
无法编译相同受限程序。本批没有新增生产许可，而是以真实普通路径核实已有能力：

- 两个合法rooted source分别放wrapper与callee，保持完整names/types/owned；
  两Replace顺序×canonical source角色顺序×inputs反序共八组合，ordinary Unit
  生成完整两函数Program、SSA/LLVM验证与真实跨源DirectCall FunctionId通过。
  原D10 named入口对这两个inputs仍拒绝，未裁Unit调用单source图checker。
- 默认无feature的 `kovenc build --project ... --entry app.entry`，通过真实
  project与标准源码loader，四种顺序组合均编译链接成功；没有选择实验命令、
  自定义prelude或删除标准source。只检查成功产物，不执行生成程序。
- 前端同typed下逐source重跑真实Checker/Planner观察，与完整shared owned核
  规范source次序的6loans、4drops、2Ends及2primitives，八组合通过；完整34/5
  事件保留。该测试是独立事实对应，不声称捕获shared分析的同一nonce；8个
  argument contracts此层仅计数，不能替代新binder的逐项合同验证。

因此，对上述确切形态，“普通多source/project编译接入”不能继续列作待实现。
此 checkpoint 时尚待的是新 split 形态 native 行为验收（后续结果见下一节）、
独立 D10 跨 source 封闭证明、以及未获准的
更广producer/代际形态；不得由本批成功推断一般多source/标准库函数均已获得
D10能力。若继续构建独立封包，应新增真实wrapper单source Pending，并在同次
完整final-owned后与callee Pending唯一组合，严格绑定canonical source顺序；
图checker按每个source-qualified角色定位AST，不能通过裁剪Unit复用singleton。

### 普通 split project 原生行为验收

随后新增独立 native exact 测试，仍使用默认无 feature 的普通 project/std loader、
真实两源码 `app.entry` 和 CLI 编译链接产物，没有测试 trampoline 或替换 SSA。
两 Replace 顺序 × 两 canonical 文件角色顺序 × `(parent, child)` 为
`(0,0)/(0,2)/(2,0)/(2,3)` 共 16 个真实程序，一次顺序执行全部通过；每个程序
保留原 10 秒期限、首失败即停止、不重试，只终止并回收自身 child。失败保留
临时项目用于诊断，成功清理。本次 macOS 实测总 21.85 秒。

独立源码语义期望：父零轮为 `first`；BeforeChild 父正且子正为 `next`；其他
父正为 `parent`，最后未修改根均输出 `kept`。核 stdout、stderr 与退出状态，
不从实际 SSA 或运行输出倒填期望。此动态结果不等于 allocator/Drop 次数证明，
也不生成跨 source sealed nonce 或放宽独立 D10 singleton admission。
CI 将原编译证据和新增 native exact 分为两步；远端跨平台结果仍待推送后验证。

### 跨 source 前置：原 planner 与最终 owned 的同次身份

在新增 split 封包之前，先强化现有受限两种顺序的私有 pending。此前 pending
只携 typed analysis Arc，而最终 ownership run Arc 在构造 final owned 时才产生；
相同 typed 的两次 ownership 分析不能仅凭 typed 身份区分。

仅 default-off feature 下，真实 dataflow 在遍历各 canonical source 前创建一次
run Arc，每个实际 Checker 在运行前获得同一身份，planner 的 Dual/After pending
保存它，最终 owned 原样保留该 Arc。两个 singleton binder 与 NamedValidation
同时核 typed 与 run 的 `Arc::ptr_eq`。独立 fixture Checker 默认产生自己的身份，
不得通过把最终 nonce 填回重跑观察来制造同源。所有原诊断、deferred、完整事实
和 AST 拒绝保持；nonce 本身不证明成功或任何循环语义。

无 feature 的默认构建仍按原 constructor 分配身份。此批不新增 split wrapper
collector 或跨源能力；下一步必须把真实 wrapper pending 同样纳入本次身份并与
callee/final owned 三方核验，完整 contracts 按原 source/call/argument 次序核对。
同 typed 跨 ownership run 组合的负控制仅在 CI 执行，本地只编译/列举。

### 独立 split-source 同次完整 ownership 观察能力

新增 default-off `analyze_split_named_entry` / `BoundSplitNamedEntry`，仅两实际
source、各一个完整 root：一个既定双计数 callee（BeforeChild 或 AfterChild），
一个具名 Int 局部参数 wrapper。原单 source named gate 完全保留，不将旧 bound
作为拼装材料，也不重跑 Checker 观察来替代实际分析 nonce。

真实 planner 在原遍历完成后收集 callee 完整 34-event pending，或 wrapper
完整五事件及唯一连续 root range；wrapper 的两 loans 与实际 DirectEnd/precedence
逐项核验。局部 contracts 分别完整消费四 calls/六 contracts 与一 call/两 contracts，
核真实 AST 参数、typed descriptor、类别、mode、类型、span 与 cross-thread。
同次 dataflow 按 canonical source 原序汇集，经最终 owned 后才唯一组合。

聚合核实际 declaration target、异 source 角色、wrapper/callee/final owned 三方
run 身份及 typed 身份；contracts 仅整理独立预期，按原 producer 的 source/call/
argument 排序键逐项比较完整最终数组。实际 owned loans、drops、Ends、primitives
和 contracts 不排序、裁剪或改写；旧完整事实与未知通道拒绝仍由共用只读 checker
执行。这个 checker 输入不是旧 source capability，不产生旧 bound。

正常验收包含两 Replace 顺序、两 canonical source 角色顺序、两 inputs 顺序共八
组合，以及合法但越界的 wrapper/callee/第三 source。跨 run 全包和混合一侧的
身份负控只注册 CI、仅本地编译列举。该结果仅为前端同次完整 ownership 观察，
尚未新增 final-admission prepare、跨 source 独立 CFG replay 或对象发布权限；
普通项目此前的编译/native 验收不自动补齐这些独立许可。

### split-source final admission 与显式原声明选择

新增独立 `PreparedSplitNamedEntry`：先对完整 Unit 完成 final type admission，
再恰一次 ownership 分析；原 diagnostics/deferred 优先于 source scope 与 selector
错误，逐项原样保留。没有第二次分析、普通 fallback、constant 重写或截取 source。

两 source 各一 root 的范围内，缺 wrapper/callee 角色为 Unsupported；存在多个
可选角色组合为 Ambiguous；唯一组合但最终 target/身份/全事实绑定失败为 Unproved。
正常 split 是两个不同角色的 pending，不能套用单源“总数大于一即 ambiguous”。
原 `analyze_split_named_entry` 的 Option 合同保持，不用 prepare 改写它的语义。

`prepare_selected_split_named_entry` 只接受从本次原 names 声明表借用的实际
wrapper Function；真实 callee 为 EntryMismatch，克隆/外国声明为 ForeignEntry。
selector 校验在完整语言 admission 与 sealed binder 后进行，不遮蔽原语言诊断。
Prepared 仅提供同次 typed/bound 的只读引用，未授予旧单源能力或对象权限。

公开非 test-library 的正常测试覆盖 Rooted 两 Replace 与 canonical/input 顺序、
同次完整 view、真实错误 entry，以及合法越界、String
use-after-move 与 final member L0080/span 的错误保留。没有已核实的 deferred-only
真实样例，因此不宣称该分支已实测；原 deferred 保留逻辑仍显式存在。
原 index 合同不允许多 MemoryDocument（MemoryDocumentRequiresSingleSource），
已保留并单独见证拒绝，不为新能力扩张 source-set 合同。跨源独立 CFG replay 与
对象发布仍是后续步骤，不能由 prepare 成功推断完成。

### split-source 完整 CFG replay 与不可变库对象封包

独立 `SplitNamedEntryProgram` 只消费原 `BoundSplitNamedEntry` 与同次完整 owned
view。普通完整 Unit lowering 产生原 Program 后，先核唯一 module、恰两函数、
原 source wrapper/callee、真实 DirectCall target 与全部 wrapper 参数/loan End，
再按已经封闭的顺序独立核 callee 全 CFG、predecessor、generation 运输与清理。
任何失败均不返回封包，不 fallback、重新分析、裁剪 Program 或转换旧单源 bound。

共用 function-local checker 按 canonical SourceUnitId 找唯一 SourceId，再匹配原
input/parsed 身份；不使用 inputs 数组下标。旧 singleton 和旧 named 入口显式
保持单 source gate。新只读 `SplitNamedCallee` 只借用完整 split bound 中的真实
pending，不能自行构造；Before/After 实际 producer/cleanup 数组原序保留。

封包持有原完整 Unit、bound 与私有 Program；公开 LLVM 和库对象字节方法每次先
重检完整图，再调用真实 LLVM 验证/host object emission。没有稳定外部 ABI、main
胶水、磁盘发布事务、link 或 native 执行接口。普通项目此前的 native 证据不被
挪作本封包的运行验收。正式 split process/driver 发布仍是后续明确合同，不能从
库对象字节成功推断其已接入。

正常矩阵覆盖两 Replace 顺序、两 canonical source 角色顺序和两 inputs 顺序。
公开 integration 将 frontend/codegen 作为非 cfg(test) 库，核真实对象格式、原两个
定义符号且无 main；合法 literal/extra-root 邻界保持拒绝。错源 call origin 和错
DirectCall target 控制仅在 CI 执行，本地只编译列举，不运行故障注入。

本地验收：正常完整图八组合通过；公开非 test-library 集成两个测试通过（其中
正例为八组合，并核真实库对象符号）；旧单源 named 两种顺序各一回归通过。
frontend 严格 Clippy、codegen 公开库/测试与 support 共存严格 Clippy、默认无 feature
构建均通过。全仓尺寸报告 2271 handwritten Rust、37 项历史超长，门禁通过；
文档结构检查 957 Markdown 通过。没有 native 执行或本地故障注入。CI 负控只列举：
错源 origin 先经普通 SSA verifier 成功才要求独立 checker 拒绝；错 target 同时
违反签名，仅证明一般拒绝，不能冒称合法 SSA 的独立 target 反例。

### split-source 真实 process entry 与原子对象发布

`SplitNamedEntryProgram` 新增显式 `emit_process_object` 与
`emit_new_process_object`；没有默认 driver/CLI 自动选择。独立 native bridge 只
接受不可变封包，从原 source/root 查唯一真实声明，核同次 ownership、零参数 Unit
签名、完整跨源图及原 FunctionId 的 LLVM entry plan；全部先于文件系统 reserve。
LLVM 原对象发射器产生真实 main glue，不伪造源码 main 或借另一次分析的入口。

发布复用既有 SiblingObject。普通 emit 在成功发射后 rename commit，其替换能力
受宿主平台/文件系统限制，不承诺任何目标类型均可替换。create-only 使用同目录
hard-link，已存在 file/directory/symlink 均不替换，不支持 hard-link 则失败且无
rename/copy fallback。link 成功即发布，之后 sibling 清理为 best-effort；不承诺
fsync/断电持久性或抵抗恶意父目录替换。没有修改普通/E2 的共享发布实现。

验收继续经非 cfg(test) 公开库的八个原 source/order 组合。库对象仍无 main；新增
process 对象核一个真实 main 与两个原源函数符号。fresh create 成功后，对已有
对象、目录、symlink 和 dangling symlink 的普通 create-only 冲突保持目标不变，
并检查没有残留 sibling。这里只检查对象和普通文件冲突，不链接、不执行 native，
不将其称作新增并发或 backend 故障注入证据。跨源专用 CLI 与动态入口验收仍待做。

本批本地结果：公开 split 集成 2 passed（正例含八组合），旧单源 process 集成
1 passed；严格 Clippy、默认无 feature 构建、格式/差异检查通过。尺寸报告
2272 handwritten Rust、37 历史超长，门禁通过；957 Markdown 结构检查通过。
这些结果不包含远端 CI、跨平台运行或新增故障注入验收。

### 显式 builtin-only 双源 CLI 与真实动态验收

仅 `d10-catalog-experiment` 启用时增加：

```text
kovenc experimental-d10-split-object <source1.ko> <source2.ko> --entry <name> -o <object>
```

命令只读取明确传入的两份完整源码，以原 canonical 文件路径记录来源并拒绝重复
canonical 路径（包括 symlink 别名）；不追加或裁剪 std，不冒充 project loader。
按完整双源 snapshot 一次完成 names/types/ownership 与 sealed split admission。
入口名称在真实 Function 声明中唯一选择，最终仍由同对象 selector/binder 核身份。
语言错误优先于 selector/scope 拒绝；既有输出/无效路径预检仍可先失败，不能将
该优先序泛化到所有文件操作错误。重复源不是跨平台 inode/hard-link 同一性检测。

两个输入都参与输出重叠保护，最终由真实 create-only hard-link 事务发布。普通
build/run、旧单源实验与默认无 feature 的帮助/命令集合不变，无 fallback。该命令
本身只发射对象，不链接或执行；测试另行显式调用宿主 cc 与真实生成程序。

正常 CLI 验收含两 Replace 顺序、canonical 角色和参数次序的八组合，原/错误
selector、已有目标、两源输出重叠、重复源与 symlink 别名；语法、名字、类型、final
member、ownership 语言错误及合法范围外形态均不发布产物。JSON 实际按行解析，
final member L0080 的 source 路径和 fooBar byte span 对回原 entry 文件。

单独 native exact 测试包含 32 程序：两顺序 × 两 canonical 角色 × 两输入次序 ×
(parent,child)=(0,0)/(0,2)/(2,0)/(2,3)。期望输出按源码零/多轮语义独立给出，真实
CLI 对象经 cc 链接后执行，核 status/stdout/stderr。保持每生成程序 10 秒/10 ms，
仅超时才 kill/reap 该测试自身 child；首次失败即停且保目录，无重试或安全设置变更。
CLI/cc 子进程未另设逐步超时，CI 给整个 exact 步骤 5 分钟上限。

本地在已协调且只读确认的空窗顺序执行一次，32 程序全部通过，exact 1 passed、
42.34 秒。此为显式 split 路径动态行为，不是通用 Carry、allocator/Drop 次数或
跨平台运行证明，也不改变默认普通项目接收集；没有故障注入。

最终本地普通验证：split 对象与诊断各 exact 1 passed，默认关闭 exact 1 passed，
旧单源 CLI 3 passed；实验/默认两配置 CLI 严格 Clippy 通过。格式、YAML、diff
检查通过；尺寸为 2274 handwritten Rust、37 历史超长且门禁通过，文档结构检查
957 Markdown 通过。未推送，因此不宣称本批远端 CI 已完成。

### 跨源专用对象失败原子性 CI 验收

在原八组合的真实 split prepare/lower/publication 上新增独立 ignored CI 控制，
分别调用 rename 发布与 create-only 发布。复用现有 thread-local、仅 cfg(test)
的真实 LLVM object-path 故障器，指向普通文件之下的路径使实际写入返回 ENOTDIR，
核 backend 确实被调用一次、原目标存在/不存在均保持原态、blocker 字节不变及
sibling 清理。create-only 的已存在目标情形也先经历 backend 写入失败，不把
OutputAlreadyExists 当作后端失败证明。

非普通矩阵模式先要求 GITHUB_ACTIONS，测试标为 ignored；现有 CI script 用 exact
列举确认一项后才在 CI 执行，本地仅编译/列举及运行普通矩阵。该批不改变发布代码、
不新增全局或生产注入开关；在远端真正运行前只称“已布置”，不称失败原子性实测完成。

本地仅列举新增 exact 控制为 1 test，未执行；普通八组合 exact 1 passed（0.36 秒），
codegen/support 共存严格 Clippy、格式/脚本/diff 检查通过；尺寸 2274/37 与文档
957 Markdown 检查通过。远端实际故障验收保持待验证。

### 真实完整项目闭包差异审计（未新增项目准入）

新增可重复 `real_project_keeps_standard_facts_outside_the_split_envelope`：实际创建
manifest 与两份 app 源，通过原 `prepare_project_unit` 加载，完整追加并授权三份
宿主 std 源；经原 `check_unit_frontend` 获得同次 Basic typed/owned，再以原完整
五源 view 发射普通 process 对象，不执行它。两 Replace 顺序得到相同事实规模：

| 实际逻辑源 | roots | loans | call contracts | Drop value-origin | bindings |
| --- | ---: | ---: | ---: | ---: | ---: |
| koven/algorithms/ranges.ko | 12 | 42 | 36 | 0 | 24 |
| koven/core/result.ko | 1 | 0 | 0 | 0 | 0 |
| koven/text/text.ko | 8 | 23 | 23 | 1 | 13 |
| app/a.ko（callee） | 1 | 4 | 6 | 4 | 2 |
| app/z.ko（wrapper） | 1 | 2 | 2 | 0 | 0 |

全 owned 为 5 drops、71 loans、67 contracts、0 receiver facts、1 closure、8
construction plans、2 ownership primitives；没有诊断或 deferred。表内 Drop 按
value-origin SourceId 统计，不冒称已完成控制点/声明归属证明。路径仅用于这份真实
loader fixture 的审计预期，不是标准源授权或生产 whitelist。

普通对象实际只定义两个源码函数 app.run/app.entry（及 main/runtime），未发现 std
源码函数定义符号；这不单独证明所有 std 语义不可达。没有裁剪输入、AST 或 owned 数组。独立重新分析只观察原 split
prepare 对完整项目返回 Unsupported，不把其 nonce/facts 与普通产物拼接。本审计
不赋予新 capability，且 object 符号观察不替代完整 Program 证书。

这给出下一批必要工作而非把计数门删掉：

1. 原同次 planner 汇集每条通道事实的可验证 source/callable 归属；Drop 还需真实
   point 与 value-origin 的对应，未知/跨 callable 归属必须拒绝。
2. 新独立项目 binder 保留完整五源与完整 owned，逐项核受限入口事实和全部未达
   事实的归属账本；不能仅筛出旧 4/6/8 数组再调用旧 binder。额外用户 root、常量
   admission 与未知通道仍拒绝；不凭 `koven-std` 名字或文本作授权。
3. 从原完整 Unit 的真实 planner/worklist 证明入口可达闭包，并与最终 Program 的
   declaration/FunctionId 映射核一致，拒绝本候选范围外的 std 调用。已有 root-set
   planner 可复用，但多 root 排程本身不证明完整项目事实与可达闭包之间的关系。
4. 只有新账本与完整图一起成立，才考虑显式项目对象能力；普通 project 已有成功
   不能给独立 D10 旁路提供许可。现行 singleton/split gates 保持原样。

本批本地验证：完整项目审计 exact 测试 1 passed（内部两种顺序），CLI feature
all-tests Clippy `-D warnings`、默认 CLI check、fmt/diff/YAML 检查通过；Rust
尺寸门禁通过（2275 文件、37 项历史债），文档检查通过（957 文件）。仅发射并
检查真实对象，未链接或执行生成程序；未运行任何故障注入。独立审阅无阻断。

### 同 run source 遍历批次账本前置（非 callable/项目权限）

在原完整 dataflow source 遍历前保存四通道起点，实际 Checker 与 Drop planner
汇集完成后捕获保序切片，包括零事实源；不在每次 push 时记录，避免把局部 trial
回滚的临时事实当成最终发布。`SourceCollectionLedger` 保留原 analysis Arc，最终
owned 仅在成功且无 deferred 时保留；公开读取必须使用 validated owned，并重新
逐项核 loans、receiver facts、construction plans、drops 四个完整通道及原 run 身份。
不存在重新分析、筛选标准源、替换 nonce 或将该账本转为准入能力的 API。

这只是实际 **source 遍历上下文**，不表示事实引用的 target 必须属于本源，更不
证明 callable、Drop 控制点或可达闭包。contracts/captures 等其他通道尚未覆盖。
真实完整项目用原 canonical 源序和四通道全量拼接核验，保留标准库 closure 的事实，
没有将其错误算作外层函数独占事实。默认 feature 不新增此状态，现行 gates 不变。

本批验收：真实五源完整项目 exact 1 passed；正常 String、use-after-move 诊断及
真实 IndexPlace deferred 三样本的发布边界 exact 1 passed，后两者不保留 ledger 且
不能 validate；原 basic_unit_ownership 全事实/身份回归 25 passed。前端 lib/tests
与 CLI tests feature Clippy `-D warnings`、默认 CLI check、fmt/diff/YAML、尺寸
（2276 文件/37 历史项）及文档（957 文件）检查通过。独审无阻断；未执行 native、
故障注入或跨 run 人工组合。尚未完成 callable 与剩余通道账本，不能用于项目准入。

### 同次静态调用合同全量对账（仍非完整项目准入）

在上述四个实际 source 遍历批次之外，ledger 新增两个独立全 Unit 数组，直接取自
原 dataflow 接收的 `CallInputs.arguments/receivers`。它们是本次原 collector 的完整
静态合同，不是 source 遍历时新增的事实；保留 collector 已确定的原顺序，不再
分组、排序、去重或按入口筛选。读取同时核最终 owned 两数组逐项相等，以及每条
call 的 SourceUnitId 存在于原实际 source 记录；未知源拒绝。

静态合同可以描述未被 dataflow 访问的调用，不能用合同存在证明已执行、可达或某
callable 的独占归属。现在覆盖的是“四个遍历批次通道＋两个静态合同通道”，并非
六个执行观察通道。capturing/callable、Drop 控制点与剩余通道的账本和真实可达
闭包仍待完成；不签发新项目能力，不改变普通/旧 split gate 或默认 feature。

本批验收：五源完整项目 67 条 argument 合同全序对账 exact 1 passed；发布边界
exact 1 passed（普通 println、现行 Worker 成员 receiver、真实语言错误、真实
IndexPlace deferred），原全事实回归 25 passed。前端 lib/tests feature Clippy
`-D warnings`、默认 CLI check、fmt/diff、尺寸及文档检查通过，独审无阻断。
试探 String extension 曾被既有 L0164 trusted Borrow 门拒绝，未放宽该门；最终
非空 receiver 正例使用已有成员调用语义。未运行 native、故障注入或跨 run 拼接。

### 最终 capture/closure 发布通道对账

closure 输入不能直接沿用 dataflow 开始时的候选表：最终 owned 构造首先执行原
`filter_unvisited`。新账本只在该过滤完成、成功且无 deferred 后，消耗原 ledger
并封闭复制最终 captures/closures 原数组；读取需同时核完整数组、原 run 及 lambda
源身份。未封闭的内部 ledger 没有该部分，读取拒绝。无公开补绑定入口。

非 constant-control 模式的原过滤不代表“所有未访问 lambda 都删除”，主要排除未
访问的 exclusive-assignment capture；本片保留原规则，不另行推断执行可达性。
四个遍历通道、两个静态调用合同通道与这两个最终发布通道仍是不同阶段的观察。
transferability、其余通道及 callable/Drop 控制点/完整可达图仍不在该账本证明范围，
没有因此开放完整项目准入。

本地验收：五源 project 精确审计 1 passed，最终 closure 过滤与原发布边界 2 passed；
前端 lib/tests feature 严格 Clippy、默认 CLI check、fmt/diff、尺寸（2276 Rust 文件，
37 项既有超限）与文档结构（957 页）通过，独立静审无阻断。未执行 native、
故障注入或跨 run 混配；本片不授予完整项目准入。


### 原 planner 的 source-root / visited-lambda Drop 批次

feature-only 观察沿 `root_traversal::run` 的真实调用边界采集：每次 `item(root)?`
成功后记录 `SourceRoot(UnitItemId)`，原第二 pass 的每次 visited lambda 成功后记录
`Lambda(UnitExpressionId)`。保留空记录及原顺序；classifier/companion 的 root 可含
多个成员，绝不冒称 callable。不是从 Drop target、Span 或最终闭包表重建归属。

原 `push_fact_with_binding_drop` 先规范化 constant temporary/真实 value-origin，再
对整个 planner facts 去重，最后 append。批次只记这次 traversal **新增**的规范化
flat Drop，不包含被去重的提案，不声称每个 root 独立拥有全套 cleanup，也不包含
另行发布的 conditional receiver drops。完整每源拼接须等于原 source drops，且
这些源批次全序等于最终 owned.drops；同时检查原 run Arc、traversal/point source。
所有权错误或 deferred 沿原最终闸门拒绝发布，未新增 rebind 或 lowering 能力。

正常源码验收：source_ledger 三项 3 passed，覆盖多 root 的两种源码顺序、空 root、
包含成员的 classifier root、visited 非空/空 lambda、不可达 exclusive lambda 及
原语言错误/IndexPlace deferred 不发布；真实五源 project exact 1 passed，全事实
basic_unit_ownership 25 passed。独立静审及补充复审无阻断；历史超限父文件仅
字段/传递增长 +6/+1，既有尺寸上限和拆分计划不变。不本地运行人工混配、故障注入
或 native。不证明一般 callable 归属、全部控制边或完整可达闭包。

本切片 feature frontend/CLI all-target strict Clippy、默认生产 workspace 及 frontend all-target check、fmt、
docs、尺寸与 JSON/YAML/diff 检查通过。扩大检查 `cargo check --workspace --all-targets`
在本 D10 树失败：codegen lib-test 的 10 项 `deny(dead_code)`，涉及 branch/child/
forward/fresh-loop/loop/multi/single/binding receipt 测试辅助项；codegen 无本批改动。
不放宽 lint、不将此失败算作通过，待与 E2/新 main 的联合树独立复验；该树的通过
结果不能反证此旧基线已过。本切片没有运行 native、故障注入或跨 run 人工混配。


### 条件 receiver 义务的独立 source 批账本

仅在 D10 feature 下于原 dataflow 每源入口保存 conditional_receiver_drops 的长度，
在该源原 planner 完整归并后截取原序增量。记录每条原 point、owner、receiver_type、
value_origin、preceding_drops，不重新推导条件类型或 Drop 序号。同 run ledger 读取
核 point 的遍历 source 以及全源拼接与最终原数组完全相等；owner/origin 不被强制
限制在该 source。不改 planner、root/lambda DropTraversal、lowering 或现行准入。

这些仍是 StaticSelf 模板义务，不是实际析构。具体实例的 MoveOnly/Copyable 判定
及 provider 清理顺序由原下游负责；preceding_drops 只定位同 point 的普通 Drop，
不证明 iteration 动作顺序，也不授予 callable/CFG/完整可达闭包能力。错误或 deferred
仍由原最终闸门撤销账本。本片没有增加人工混配或故障注入的本地执行。

正常验收：source_ledger 4 项通过，新条件通道用真实三源、正反输入序、非空/空批次、
preceding_drops=2 与原样清理 origins、真实语言错误/deferred 不发布；五源 project
exact 1 passed，iteration formation-scope 邻控 1 passed。独立静审无阻断，dataflow
父文件仅 +2 接线，既有尺寸上限不变。上一 Drop 切片记录的 default-workspace-all-target 失败
没有重跑或放宽 lint，联合基线的通过不代替此树验证。

本片 feature frontend/CLI all-target strict Clippy、默认 frontend all-target check、
fmt/diff/YAML、文档结构（957 页）与尺寸（2277 Rust 文件、37 历史超限）通过。
未运行 native 或人工故障；conditional 独立提交尚未在联合树重建验证。

联合复验：在 `7ccf84a6`（含 main `dcd9d7a5`、D10/E2 与原始 Drop 批）上组合
`4f8efa20` 后，source ledger 4 项、五源 project 1 项、默认 workspace all-target check、
frontend/codegen/CLI all-target 严格 Clippy（D10、two-source、E2 与 loan observation
支持同时启用）通过；fmt、文档 957 页、尺寸 2323 Rust 文件/37 历史超限通过。
本次没有重跑 native 或故障注入；这些证据不扩大上述分析能力边界，也不抹去独立旧树
的 default workspace all-target 失败记录。

### 条件形成 scopes 与最终 iteration 计划三通道

D10 source record 新增独立分组，只从原 source 汇集入口/出口截取
iteration_conditional_scopes、iterations、map_iterations 三个原增量。未封闭的内部
record 不通过读取；同一 run 全源拼接与最终三数组完整 Eq，不重建 action 顺序，
不从 AST 或 Drop ordinal 推断 scopes，不要求 owner/template 与遍历 source 同源。

计划保持原 provider 排列及嵌套共享同 point 的完整 exits/actions，不能 flatten
去重或解释为多次执行。原 ActionSource 的 Cleanup 投影不等于其全部事件历史；
owner_scopes、loan_ends、required_exits 与两类 templates 尚未纳入此分组，仍由原
validator 核验。此账本不扩大 callable/CFG/可达闭包/native 或项目准入能力；不改
planner、validator、lowering 及原失败/deferred发布门。

正常源码 source_ledger 5 项通过，新增三源样本覆盖 sequential/Map、嵌套共享 point
两份完整 actions、夹在内外 provider 中间的 conditional scopes、空源和反向输入序，
并核真实错误/deferred不发布。独立静审无阻断；dataflow父文件1232→1245仅+13接线，
既有max1248及拆分计划不变，后续增长仍需复审。其余见证通道与联合验证不在本片内。

本片另有真实五源 project exact 1 passed；feature frontend/CLI all-target strict Clippy、
默认 frontend all-target check、fmt/diff/YAML、文档（957页）及尺寸（2279文件/37历史项）
通过。未运行native、人工故障/混配或旧default-workspace-all-target失败项；未在联合树
复验。旧target串行复用，各编译入口核磁盘≥8Gi，未新建target或扩大准入。

iteration 联合复验：`dc22b651` 与 `439f0920` 组合后，source ledger 5 项和真实
五源 project 1 项通过；默认 workspace all-target check 及 frontend/codegen/CLI
all-target 严格 Clippy（D10、two-source、E2、loan observation 同时启用）通过。
fmt、文档957页、尺寸2325文件/37历史项通过；不重复运行native或人工故障。
各编译入口均核可用磁盘不少于8Gi。此验收不把未采集的五类验证输入算作已覆盖。
