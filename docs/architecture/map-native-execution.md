# Map 原生执行当前边界

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 Map 的类型、所有权、SSA 或 LLVM 时 · **唯一真源**：代码与测试；交付账本见 [SPEC0288](../archive/specs/0288-map-native-execution.md)

Map 的实现按具体 K/V 实例建立布局，不以 `Map<String, Int>` 特例承载其他类型。
单文件与编译单元都消费 typed Map 描述符。签名及表达式入口均检查键的 Hashable 能力和
值的结构化存储约束；查询结果对已 nullable 的 V 保持归一化，不制造嵌套 nullable。

## 所有权与 SSA

`contains`、`get`、`requireValue`、`withValue`、`remove` 按 Borrow 使用键；`put` 和下标赋值按 Value 交付键和值。
交付只退休 MoveOnly 的源 binding，Copyable 标量与 value class binding 仍可复用。
变异要求 mutable receiver 的 Inout 权限；普通调用事实和下标赋值遍历都执行检查。
SSA 的查询键是 Value 或 active Shared Loan，exclusive/失效 loan 被 verifier 拒绝。
`MapPut` 消费旧 owner、键和值，产生更新后的 owner；`MapRemove` 消费旧 owner，
产生更新后的 owner 和 owned nullable 结果。调用交付后不再运输已消费的键和值 binding。

资源类型规划递归检查 Map 的 K/V，登记元素 drop glue。LLVM Map drop 遍历 Occupied 槽位，
分别析构 MoveOnly 键和值，再释放 buffer。覆盖先析构旧键和值，再存入新条目。
删除析构旧键并交付原值；原值由结果 owner 的清理事实负责，Map 不再析构它。

单文件与 compilation-unit 的普通短路清理按 RHS 求值出口与跳过出口分别规划，并在 SSA 合流前消费对应 drop 事实，
避免 Map 最后一次查询只在 RHS 路径释放而导致合流缺少 binding。

Copyable 且非 nullable 的 V 查询采用 Module 显式登记的独立 `{Boolean presence, V payload}`
aggregate identity；它保留 V? 与 V 的区别，复用 aggregate 布局。verifier 检查 presence 类型、
payload 所有权和注册身份；MapGet 拒绝裸 V、同形的未登记 aggregate 以及 MoveOnly 的复制查询。
缺失查询返回 false 和零初始化 payload；已存入的 0/false 仍有 true 标记。源码 null 比较读取
presence；标量 nullable 等值比较同时检查存在性与值；窄化读取及 `!!` 使用受检 Copy 提取，
只有缺失提取才 Abort。多文件签名先登记 reachable 查询结果类型，再交接只读类型事实。
查询下标的 receiver/key 在同步查询完成后清理，求值期间的控制转移仍保留已求值 owner 的义务。

String 和 MoveOnly inline V 的 owned remove 复用登记的 presence/payload aggregate，
结果所有权与 V 一致。条件 drop 只在 present 分支析构 payload；缺失的零值不承担 owner。
`!!` 复用 NullableBranch 的真实 owner/proof 关系和 NullableTake 的消费，缺失边直接 Abort。
payload view 使用入口栈槽，不产生隐式 Box；verifier 封闭普通 aggregate explode、
field/place/loan 对 owned 结果的绕过。Resource class 仍使用现行 pointer-niche nullable ABI，
删除结果保持 lexical 生命周期，并到达原隐藏 deinit。单文件只允许 typed Map 描述符
已登记的 nullable 结果绕过普通 Resource 包装拒绝，未启用无 Map 的 Resource? 参数。两条 drop planner 与活跃性路径
按 typed Map put 描述符消费下标赋值的 K/V，并保护 receiver 和已求值参数至交付点。

## requireValue 的真实 receiver 与槽位借用

single/unit 的 typed `requireValue` 描述符保留实际 receiver、key、K/V 和唯一 receiver
返回来源；结果 target 是 V 本身，nullable V 不再包装一层 nullable。它与普通函数共用
结果绑定、caller source continuation 及 scope-end 事实。unit Shared receiver 对该调用
另发布真实 place 的 LoanFact，后端据实际 call/argument identity 建立来源 loan。
key loan 在同步查询后结束，receiver loan 延续到结果作用域退出；借用期间 put/remove
或移动 Map 报 L0135，普通 owned 绑定报 L0163。结果先结束，来源随后结束，再恢复变异
或 owned 交付权限。普通 receiver/Inout 返回形状没有因此开放。

`MapRequireValue` 输入 active Shared Map loan 和只读键，输出 Shared V loan；类型合同
不要求 V Copyable。alias roots 与 parent-child 检查将真实 slot 依赖连接到 receiver，
verifier 拒绝 owned 结果、错误来源 target 和活跃子 loan 前的来源终止。
LLVM 的 contains/get/requireValue 共用有界 lookup，得到 Occupied 条目的实际 slot 地址；
requireValue 在 found 分支返回 V 字段地址，不读取、复制、移动或 retain V。Missing
走既定 Abort；present-null 的 nullable payload 仍返回非空的槽位位置借用。

正常 single/unit native 覆盖 String、含 String 的 MoveOnly value class、Resource、现行
pointer-like nullable V 的 present-null/present-value；各 scope 后恢复 Map 变异，Resource
覆盖和最终清理到达原 deinit。独立 missing 源码执行正常 Abort 路径。readonly Map 参数
的 receiver 合同及普通 borrowed wrapper 另有前端/SSA 正例；非空 readonly Map 的 native
命中场景未单独验证。旧 get/remove 的 nullable V 边界保持。

## withValue 的作用域访问

single/unit 的 typed 描述符保留 receiver/key/action 与通用 K/V。参数和 receiver 使用
真实 Borrow 所有权事实；unit 共享 capture 对 exclusive receiver loan 保持只读限制。
`MapWithValue` 同步操作的 verifier 要求 active Shared Map/key/action 和唯一 Borrow V
→ Unit callable signature。LLVM found 分支取得 Occupied slot 的 V 字段地址，交给现有
closure ABI 调用一次并返回 true；missing 返回 false。没有 V owner、copy、move 或
retain，也没有 caller 条件借用结果。Borrow 参数非逃逸使用现有 frontend 检查。
参数清理复用 pending call 事实，unit 以稳定 pending slots 跟随 CFG loans；callback 后
结束实际建立的输入 loans，再消费参数临时值与 CallReturn drops；临时 key/action owner
在其 loan 结束后析构。独立输入 loan 的相对结束次序不作为语义要求。

unit callable planner 将实际 action LoanFact 对应的封闭来源登记到既有同步 callback
路由表，Resource lambda 使用同一来源登记与 deinit 规划，不再被未登记保护提前拒绝。
Borrow callback 参数布局复用现行通用 storage mapper。callback 正常 return 消费既有
清理事实；既定 Abort 保留已求值前缀的非展开终止，不以活跃临时值误判失败。

正常 single/unit native 已证明 String、含 String 的 MoveOnly inline、Resource、共享
capture、Missing 不调用、callback 局部 return 与之后 Map 变异；nullable V 的实际
`item == null` 分支区分 Missing、Found(null)、Found(value)，没有改写为固定输出。
空 readonly Map 的独立 Missing 路径同样执行。正常逐指针 malloc/free 计数覆盖临时
String key、共享 capture 和 Resource 清理，每个 owner 精确释放一次。纯 SSA 合同测试
查询未改写的正常产物，检查实际 V-slot/callback signature、输入 loan 终止及临时 owner
清理；负例只查询错误类型/交付合同，不生成或运行故障 IR。

前端所有权负例的源码先通过 single/unit parser/type，分别拒绝 callback 内 put/remove、
owned 交付 Map/value、字段存储、move capture 和共享 closure 的 owned 逃逸，诊断保留
实际 Span。`return value` 另由 callback 的 Unit 结果合同产生 L0087，不把该类型拒绝
冒充所有权拒绝。新增 last-use 的接线见 SPEC0288 §2.10，前一冻结片双 crate 全库见 §2.11；本轮 alias/promotion 见 §2.12。

## 普通借用返回与 caller continuation

Guide v0.42 的普通结果在 parser/type 层保留声明模式与唯一 `from`。Phase 3 的
`BorrowReturnOriginFact` 由 single/unit 的实际返回交付点发布：稳定参数、只读字段投影、
generic/nullable 对象及 typed 命名实参映射的普通包装返回。错误来源报 L0162，owned
绑定/返回/实参交付即使 Copyable 也报 L0163，primary 和来源 label 保留真实 Span。

`BorrowResultFacts` 另保存显式结果绑定 identity、调用方真实来源 lease、父结果绑定和
原来源调用 loan 的 call/argument identity。成功返回只结束无关实参 loan，来源 loan
交接给结果；普通包装返回交付原来源 loan，不能在内部 CallReturn 结束。建立绑定前
须找到实际 Shared active loan 并完成交接，命名实参按原 LoanFact 的 begin Span 匹配。
callee 的 payload 投影仍由 actual-return origin 描述，来源 lease 不能冒充 payload 地址。

词法 scope-end 基线中，stable place 和只读 alias/reborrow 不取得 owner；来源 owner
受独立保护，不按 owned ASAP 结束 loan。initializer 前缀仅保护来源，正常完成后才建立
结果；提前 return 清理原调用前缀，不发布未建立结果的 end。scope/return 边先结束子结果，
再解除来源/父依赖，最后清理 owner；同点的 `BorrowBindingEndFact` 在 drop facts 前消费。
借用绑定和调用结果不生成 payload owner/drop，父结果在子 scope 退出后继续有效。
single/unit 使用各自封闭 ID，诊断或 deferred 时原子清空新结果事实。

未显式绑定的 caller 调用结果、嵌套结果实参、控制 initializer、closure capture、循环
动态实例及 receiver/Inout 返回来源仍以 L0164 拒绝。

直线局部序列另使用独立的未来使用证明，不复用 owned ASAP liveness。证明按真实 name
symbol 收集剩余表达式中的访问，并沿 binding 的实际 parent 链保留全部存活祖先；多个
独立结果分别保留来源 lease。只有完整可遍历、含本层 borrow val 声明的序列才提前结束
本层绑定，继承的外层绑定保持词法存活。nested block、return、条件求值、closure、cast
及循环等不在该证明内，整个序列保守保留 scope/return 处理。正常语句完成后发布
AfterStatement 的真实 end facts，checker 同步撤销该结果 continuation；drop planner
按子到父顺序发布 ends，再按原 owned/Resource 规则决定来源清理。新 native 矩阵证明
same-scope owned 交付、Map 覆盖权限恢复和 Resource 词法析构顺序，见 SPEC0288 §2.10。

后端已接通单 Borrow 参数的表达式返回、普通包装调用和具体泛型实例。来源与结果分别
保存 semantic target，返回字段允许与来源根对象类型不同。函数签名记录
唯一参数来源；`BorrowCall` 产生 shared 子 loan，`BorrowReturn` 交接指定入口来源链。
verifier 对齐参数、target 与结果 delivery，校验 active source、storage alias、父子依赖和
终止次序。lowering 消费实际 origin/source/end facts，借用读取使用真实返回 loan，含透明
Group；词法及已证明的 last-use 终止先结束结果，再结束本调用实际建立的来源 loan，
然后消费 owner drop。调用派生的子结果复用实际父 loan，父不得在子结束前终止；
直接引用已存活 borrow parent 的 alias 验证真实 parent/origin/name/semantic target 后生成独立
SharedReborrow，复用父的实际 payload pointer，消费既有 ends；多个 alias 与调用子 loan
都先于父/source 结束，之后恢复 owner 交付与 Map 变异权限。owned root/字段直接 initializer
没有 parent，仍在 SSA 前带 Span 拒绝。
LLVM 返回指向原 target storage 的非 owning pointer，不制造 owner、copy 或隐藏 RC；callee
不析构来源。返回投影逐层匹配 actual origin 的字段 symbol 与实际根参数，再复用
SharedFieldLoan/SharedHeapFieldLoan；verifier 检查真实布局 target 和唯一来源链。
single/unit 的 String、Int、含 String 的 MoveOnly value class、Resource、inline/heap 字段
和现行 pointer-like nullable storage 已有正常 object/link/run。null payload 仍具有存储位置
的 shared loan；LLVM 身份返回直接返回入口 storage pointer，包装不读取或 unwrap payload。
Resource 源在借用 scope 后仍可 owned 交付，并且原 deinit 只执行一次。纯 verifier 合同
负例只查询正常源码产物，不改写 IR 或注入动态故障。

owned root/字段直接 initializer 已消费真实 origin/parent/projection 与结束事实，创建 root
及字段 loan 并保存新祖先，按叶到根清理；parent 的实际 payload 与 canonical source lease
分别验证，不能由 lease 猜测字段地址。single/unit 五组正常 native 与精确清理回归通过。
更多参数、索引投影、block 返回和 caller CFG 运输仍在 SSA 前带实际 Span 拒绝。类型能力受现有 storage mapper 限制，未启用通用 inline nullable 或新的
Resource wrapper。现行 pointer-like nullable 的 owned null 比较复用 NullableIsNull；Borrow 参数的 null 比较
使用 NullableLoanIsNull，verifier 要求 active Shared nullable storage loan，LLVM 仅加载
该 storage 的 pointer bits 并比较 null，不创建 payload owner、复制、retain 或 unwrap。
该接线只处理已发布 NullComparisonDescriptor 的存在性比较，不开放借用 payload 的通用
窄化 view 或 `!!`。实际三态 callback native 和原 unit 比较源码均有回归；验收状态见
[SPEC0288](../archive/specs/0288-map-native-execution.md)。

## 哈希表与执行证据

header 是 `{buffer, size, capacity, tombstones}`；槽位为 `{state, K, V}`，有 Empty/Occupied/Deleted 状态。
Int/Boolean/Char 使用确定性标量哈希，String 使用 UTF-8 字节哈希与相等性检查。
初始容量 16，按 size+tombstones 的 75% 阈值扩容并移动条目；删除增加墓碑计数，复用减少计数，rehash 清零。
探查最多扫描 capacity 个槽位，记录首个墓碑；整表都是墓碑时也能复用并终止。

正常 native 回归覆盖 String 键和值、Resource 覆盖及作用域析构、owner 删除结果、碰撞与
墓碑复用、整表墓碑连续增删、扩容，以及 Boolean/Char 查询。Char 使用已支持的 const
物化入口；这些证据不扩大普通 Char literal 的 lowering 边界。
正常 malloc/free 计数逐指针核对释放；多文件 Map<String, Resource> 验证正逆输入顺序的
确定 SSA、跨文件构造、覆写、键借用复用与两次用户 deinit。

## 尚未闭合的边界

- nullable V 的 requireValue 已区分 missing Abort 与 found-null/found-value 的槽位借用；
  withValue 三态 callback 已由 single/unit 正常 native 证明。Copyable nullable V 的旧 get/下标
  在前端报 L0130/Span，提示 withValue；MoveOnly 保持 L0136，非 nullable Copyable 查询
  仍按值返回。nullable owned remove 保持后端 UnsupportedNode/Span，不增添 owned 三态表示。
- owned remove 已有 String、含 String 的 MoveOnly value class 和 Resource class 的
  单文件/unit 条件析构、消费提取与缺失结果回归。该证据不启用通用 inline nullable。
- 普通 Borrow 参数 origin、显式绑定的 caller continuation 与词法 scope-end 已有 single/unit
  证明；单参数现行 storage 类型的泛型实例与只读字段投影已接通 SSA verifier/LLVM ABI，
  requireValue 的真实 receiver slot 和 withValue 同步 callback 已接通；直线局部 last-use
  有独立 frontend ends 与调用派生结果的 SSA/native 证据，复杂 CFG/NLL 未实现。
  非空 readonly Map 没有现行构造/转换入口，只有空 readonly Missing
  和已填充 MutableMap 的命中 native 证据，不能由此宣称 readonly 非空命中已验证。
  parser 已保存结果/绑定 marker；single/unit 的签名及调用 descriptor 携带声明来源，
  Phase 3 另发布实际返回来源，不能混同于 caller result loan；正式启用依据由 Spec 保存。
  尚未证明的复杂路径保留 L0164，后端在 SSA 前封闭未接通的 ABI 形状。条件借用不属于当前批准范围。
- MapPut 已按 typed V 适配直接 constructor 与 owned local 的 T→T?，原 owner 在 wrap 前
  交付，exact-type verifier 保持；single/unit 的 Resource class、MoveOnly Packet class/Box
  与 String class 字段的覆盖/写 null/native 精确清理通过。Resource? 的 Map V storage
  只复用已有 pointer niche，非 Map resource wrapper guard 保留。直接 String?/Packet?/Int?
  的 Map storage 不扩展 inline ABI，single/unit 均带 Span UnsupportedNode。
  §2.12 的 alias/promotion 与旧全库结果属于其冻结片；§2.13 另记录稳定 place、查询诊断
  及缓存边界的红绿证据。普通源码已复现先缓存查询 result 再构造 inline nullable Map 的
  错误接受；Map V storage 现在独立要求已批准 NullableHandle，不能复用 result aggregate
  绕过 storage 边界。single/unit Int?/String?/MoveOnly Packet? 缓存组合负例均通过。
- macOS 的正常计数不证明 ASan/LSan 动态检出；完整 Linux sanitizer 验收未运行。

上述限制决定 SPEC0288 保持 in-progress；实际命令、平台限制与故障类检查的未运行范围
由 Spec 逐项记录。
