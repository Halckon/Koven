# M2B 通用借用访问结果（候选）

> **性质**：非规范候选设计 · **状态**：draft / 未启用 · **读取时机**：评审 M2B 来源、权限、结果与集合接口时 · **唯一真源**：本页维护候选合同；现行规则仍由 Guide 定义

## 1. 方向、名称与现行边界

用户所称 **M2B** 对应原计划的 **M2 借用访问方案**，是其通用能力设计方向，
不新增或重编号已有 M1B/M3B，也不声明原 M2 已完成。
本稿仅扩充设计计划；须经过方案审查、用户明确启用新 Guide、必要 ADR 与正式 Spec 后实施。
现行 [Guide10](../guide/10-ownership-borrowing-drop.md) 没有 borrow-return 类型、可存储 borrow
value 或跨调用 loan；[Guide12](../guide/12-collections-destructuring.md) 的元素 place 直接
Borrow 是封闭能力，不等于结果可以局部绑定或由包装函数返回；开放用户索引仍有 v2 边界。
[旧 Map proposal](map-ownership.md)仅作为只读历史，不采用其过时 marker/getRef 签名或版本表述。
SPEC-0279 的现行 runtime constructor 交付、独立 PR/CI 不受本稿影响。

目标不是仅支持 `Map<String, Int>`：同一来源和 loan 核心须覆盖 `Map<String, String>`、
`Map<String, Resource>`、`List<MoveOnly>`、名义字段与 Map→List→字段的组合访问。
这里的 Map 类型和新借用结果均是候选，不是可编译的现行示例。

## 2. 三层职责与首版边界

| 层 | 本稿要求 | 不应承担的工作 |
|---|---|---|
| 所有权核心 | 稳定来源/selector、Shared/Exclusive、parent/child reborrow、有效期与失效、恢复与清理 | 为每种集合复制权限规则，依赖物理地址判别身份 |
| 函数/接口合同 | 结果明确借自 receiver 或指定一个参数；验证 body，跨模块/泛型保留同一关系 | 名称识别特权、读 callee body 猜来源、接口引入 v2 `dyn` |
| 库 API/语法 | 集合与用户库以同一合同声明访问；区分查询、owned copy/move 与借用结果 | 把普通 `V?` 解释成引用，隐式 clone/retain，先拍定语法再补语义 |

首版建议：单一明确来源、局部非 owning 绑定、只读重借用、可静态核验的跨函数转发，
以及结构化独占修改；不返回可任意流转的 exclusive 结果，不默认引入完整 NLL。
局部借用结果的上界先以结构化块/访问域界定；结束块后可恢复 owner 操作权限，
不能以最后一次读、优化器分析或 runtime 地址偶然稳定来缩短 loan。
源 owner 可来自 caller 的具名 owner、Borrow 参数或有权限的 Inout 参数；
首版新结果不借自 callee 局部 owner 或临时 receiver，也不隐式延长临时 receiver 生命周期。
现行直接 Borrow temporary 的调用期合法行为保留；临时 receiver 的新结果延寿另行评审。

非目标：普通对象/容器长期存借用、多来源或来源集合、逃逸 closure、跨线程/async、
引用环、任意用户自定义索引/迭代、slice 语法、隐式 pinning 和借用复制协议。
其它视图、树、parser、IO 只作为同一核心的后继需求，不进入首版验收。

库操作按所有权效果分别表达；下表的 `get`/`remove` 是需求名称，不批准具体新签名。

| 使用意图 | 结果与 owner 关系 |
|---|---|
| 查询 / `get` | Shared 借用结果依赖容器；MoveOnly 不复制、不移出 |
| 独占修改 | 结构化 Exclusive 访问结束后恢复父能力，不交付第二个 owner |
| 移出 / `remove` | 元素成为独立 owned 值；容器更新须先处理活跃借用冲突 |
| 显式 `clone` | 按已批准类型合同产生独立 owned 副本；不作为查询的隐式替代，不预先开放通用 clone |

后继复用场景包括对象内部 String/资源访问器、原文本或字节缓冲区的局部视图、
由树/AST 拥有的子节点、依赖输入缓冲区的 token 内容、同步 IO 已读缓冲区的临时视图。
各库只负责定位；原缓冲区替换、重填、树节点删除等失效操作统一交给来源核心检查。

## 3. 来源、权限与有效期

每个候选结果必须关联可审计描述：source/analysis identity、根 owner 的当前实例、
按实际顺序排列的字段/元素/Map entry selector、目标类型、loan 权限、parent loan、
依赖 owner、产生 Span、开始点及各条可达退出边的结束/恢复动作。
selector 的表达式/声明身份不等于目标存储身份；求值各一次后发布稳定逻辑目标，
不能把 key 变量名、hash 值、零大小 sentinel 或地址当作条目/元素唯一身份。
owner move/replacement、删除、重排与 relocation 必须检查仍有效的依赖，不以值相等恢复旧结果。

Shared 允许同一目标的多次只读访问和只读 child reborrow，不允许移出 MoveOnly、
变异、drop 或冲突 exclusive 访问；Copyable 的显式按值读取仍产生 owned copy。
Exclusive 只来源于已验证 mutable place/Inout 能力；只读 child 存活时父 exclusive 的
冲突权限暂停，child 结束后恢复；结构化修改期间目标始终完整初始化。
借用结果只投影权限，不能因为 `.field` 或一层包装调用自动升级为可变/owned 能力。
重借用沿根来源组合，不取得中间容器或字段 owner；child 不能超过 parent 或根 owner。
首版无法证明 selector 不重叠时保守冲突；不同 root 只有证明不共享来源才视为独立。

结果用完只结束自己的 loan，不能析构所借元素；结束顺序为 child→parent→允许的 owner
清理。正常块退出、break/continue/return 与可恢复错误必须逐边发布事实；
无效程序可产生诊断恢复状态，但不能据此生成可执行 loan/drop 计划。
`Nothing`/Abort 沿用现行不展开合同，不要求 Abort 执行 scope cleanup。

## 4. 函数、接口与受控转发

声明需要表达“结果种类是借用访问，来源为 receiver 或一个指定参数”，
与返回的目标类型、可选缺失协议共同构成 callable contract；它不是普通返回 `T` 的注释。
本稿只写语义关系，不规定 `&T`、Rust lifetime 参数、返回 marker 或新的关键字。
备选语法由独立方案审查决定；无需显式生命周期参数不等于可以不发布来源事实。

callee 每条正常返回路径必须证明结果派生自声明的唯一输入，不能借自本地 owner、
其它输入、闭包环境或已失效 selector。包装函数可继续投影该输入或转发已验证结果，
但不能抹掉权限、parent chain 或期限，也不能把不同来源合并成一个未经证明的结果。
caller 将声明来源替换为实际 receiver/argument；普通调用 loan 在 CallReturn 结束，
结果依赖必须另有明确延续/重借用事实，不能全部提前结束或将所有实参 loan 一并延长。
CallReturn 必须有已验证的结果交接：将结果关联到 caller 仍有效的 owner/parent loan，
再结束只属于本次 callee 的参数 loan；不能让结果继续引用已经结束的 callee loan。
交接前后都检查 root 当前实例、selector、权限与依赖，不能借交接跳过 exclusive 冲突。
调用时更早 operand 的 loan、后续 operand 副作用与 receiver reservation 仍按现行规则检查。

跨模块、泛型实例、静态接口约束与用户库使用同一声明级合同；实现不得更换声明来源。
这里的接口只指现行静态能力，不启用动态派发/vtable/裸 interface 存储。
同名类型/函数、同签名不同来源、不同 source/analysis 或实例不得共用错误的事实与 memo。
不声明开放 function-value 的借用返回 ABI；该间接调用扩展需要另行选范围与证明。

## 5. 查询 key、缺失与嵌套访问

Map 查询可以借用具名或临时 key。key 仅在哈希/比较阶段被读取；若合同不保存 key
或借自 key，查询结束后 key 的 loan 应结束，临时 key 可清理，结果只依赖 receiver owner。
若某 API 真正返回参数 key 的投影，必须声明该来源，不能套用上述 receiver 关系。
查找/比较副作用须各一次；碰撞不等于条目相同，相等 key 查询同一条目不能绕过 alias 检查。

缺失协议需区分“没有条目”“有条目且 V 自身为 null”“有非 null 的借用目标”。
这可采用受控查询结果/访问分支等候选编码，但本稿不批准 `Option`、`getOrNull` 或新 `V?` 语义。
Missing 分支不得生成可读的 element loan；Found 分支关联真实目标和 receiver 依赖。
`V?` 仍只表示普通 nullable owned 值；其 Copyable copy 与借用到 nullable slot 是不同合同。
缺失检查不能复制 owner、伪造 null 指针 loan 或将无效分支默认为有效目标。

Map→List→字段必须贯穿根 Map owner、entry selector、List index、field declaration 与
parent/child loan；中间查询不得重算 key/index、隐式 clone、提前 drop 或提前恢复父权限。
修改条目、搬迁 Map storage 或替换嵌套 List 时，即使最终字段地址未变也必须检查依赖。
首版可保守保持整个 receiver 的存储保护，不承诺未证明的不同 entry 并发 exclusive 能力。

## 6. 待冻结的正反验收矩阵

每行在正式 Spec 前绑定最小源码、owner/selector/loan 来源、预期 Phase/Span、事实与 native oracle。
`Resource` 行同时核观察值与正常路径精确析构；String 行不得靠隐式 clone 通过。
以下是候选预期，不是已执行测试；语法冻结前不用伪代码宣称编译通过。

| ID | 来源与操作 | 正向验收 | 负向验收 |
|---|---|---|---|
| R01 | 具名 List<Resource> / index / 调用期 Shared | 现行直接 Borrow 可用、owner 不变 | 普通 owned 读取/移出继续拒绝；直接 Borrow 不证明结果可绑定 |
| R02 | Map<String,Int> / entry / receiver Shared | 借用结果与显式 owned copy 各有事实 | 以普通 owned V? 假冒借用结果拒绝 |
| R03 | Map<String,String> / entry / receiver Shared | 局部绑定、重复只读、嵌套 Borrow；query 后 key 可用 | Value 交付、提前 drop/替换 Map、隐式 clone 拒绝 |
| R04 | Map<String,Resource> 与具名对象 / field / Shared | 字段投影沿根来源，结果无 deinit，owner 正常退出精确一次 | 从借用结果移出 Resource、析构 borrowed target 拒绝 |
| R05 | Map→List<Resource>→index→field / parent chain | 局部逐层投影、包装转发，所有 selector 各一次 | 丢 root、element 后 field 来源缺失或提前结束 parent 拒绝 |
| R06 | receiver/指定参数 / 静态泛型 user-library wrapper | 同文件和跨文件相同来源/权限/结果关系 | 借另一参数、错误实例/memo、同签名不同来源混用拒绝 |
| R07 | 同一根的 branch/loop aliases / CFG loan | 同来源且兼容 selector 合流、迭代内结束子 loan | 多来源 join、未经证明的 selector join、loop carry 超期限拒绝 |
| R08 | callee 局部/临时 receiver / 返回结果 | 现行直接 Borrow temporary 作为基线仍合法 | 首版新借用结果绑定或转发临时/局部 owner 均拒绝 |
| R09 | 临时/具名 String key / receiver-result loan | key 查询结束可清理/复用，receiver 保护继续 | 让结果依赖 key 地址，或连同 key 结束 receiver loan 拒绝 |
| R10 | Missing、Found(null)、Found(value) / 条件事实 | 三状态可区分，只有有效分支可读取目标 | missing 未检查读取、把 V? null 直接当缺失拒绝 |
| R11 | shared aliases、同/未知 selector / overlap | 多个只读 alias 合法，域结束后可移动 owner | 存活结果期间 move/drop/relocation 或等价 key 修改拒绝 |
| R12 | mutable source / 结构化 Exclusive 与 Shared child | child 结束恢复父能力，replace 后完整初始化 | Shared 升级 Exclusive、child 活跃时父变异、重叠独占拒绝 |
| R13 | 正常/break/continue/return/可恢复错误 / cleanup | 每边 child→parent→owner，受控返回先交付依赖 | loan double-end、提前恢复、重复/遗漏 owner drop 拒绝；Abort 不冒称展开 |
| R14 | 同一核心 / fields+Map+List / forged facts | frontend 与 SSA/verifier 使用同一发布身份 | 缺来源、失效 parent、目标类型/权限/analysis 不一致结构化拒绝 |
| R15 | 存入普通 field/container 或 closure / escape | 短期库调用只读 reborrow 后归还权限 | 长期存借用、多来源、逃逸捕获、跨线程/async 均拒绝 |

## 7. 真实复用点与缺口

- Phase 2：[ElementPlaceDescriptor](../../crates/lang-frontend/src/type_checking/container.rs)、
  [unit descriptor](../../crates/lang-frontend/src/type_checking/compilation_unit/bodies/container.rs)
  已保留 receiver/index/type；[CallDescriptor](../../crates/lang-frontend/src/type_checking/call.rs)
  已保留 instance/receiver/argument 映射，但尚无声明级借用结果来源合同。
- Phase 3：[OwnershipPlace/LoanFact](../../crates/lang-frontend/src/ownership_checking/model.rs) 与
  [UnitOwnershipPlace/UnitLoanFact](../../crates/lang-frontend/src/ownership_checking/compilation_unit.rs)
  可复用稳定 root、Shared/Exclusive、来源 Span、loan/drop 边界；当前是调用期等封闭能力。
  single fields 后可接索引但不能再接 field；unit 仅支持一个 terminal element。
  二者不能直接表示 Map→List→字段，须先审阅有序 selector 与相同来源/alias 的共享模型。
- [single loan](../../crates/lang-frontend/src/ownership_checking/checker/loan.rs) 与
  [unit places](../../crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/places.rs)
  可复用 place、temporary origin、冲突和 Copy/Move 分类；不能把原 CallReturn end 当新结果期限。
- Phase 4：[SSA model](../../crates/lang-codegen/src/ssa/model.rs) 已有 LoanId、field/element 投影、
  reborrow 与 block loan transport；[verifier](../../crates/lang-codegen/src/ssa/verify_ownership.rs)
  的 current-content/依赖校验可作消费者，但不等于已有跨函数返回 loan ABI。
  前端必须先发布新增事实，后端不从 AST、名字、物理地址或 layout 猜来源。

## 8. 审查、实施依赖与外部参考

先冻结 R01–R15 的语义和语法比较，再明确启用必要 Guide；按来源核心、声明/调用合同、
库访问适配分有界 Spec，每层独立红测和非作者复审，不先做 Map<String,Int> 特判。
必须证明单一来源不等于单一 loan：嵌套存储、重借用与包装返回仍保留完整依赖链。
主要风险是声明的单一来源与真实 CFG/selector 不一致，以及 CallReturn 提前结束保护；
其次是 Missing 与 nullable slot 混同、temporary/key 延寿误绑、parent 恢复导致悬垂或重复清理。

[Rust Vec 官方文档](https://doc.rust-lang.org/std/vec/struct.Vec.html)仅作语义分层参考：
[get](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.get)、
[get_mut](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.get_mut) 与
[remove](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.remove)分别展示只读访问、
独占访问与取出 owned 元素的不同合同；不移植 Rust lifetime 语法、NLL、索引、
线程能力或 API 名称为 Koven 规则。本稿没有运行原型、源码/native 验收或安全/成本实验。
