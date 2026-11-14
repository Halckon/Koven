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
