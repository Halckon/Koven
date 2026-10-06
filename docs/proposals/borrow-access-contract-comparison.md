# M2B 声明与结果类型：两种候选的可比较合同

> **性质**：非规范候选合同比较 · **状态**：draft / 未启用 · **读取时机**：选择借用结果声明、绑定和缺失协议时 · **唯一真源**：本页维护备选差异；共同候选合同见通用借用访问结果，现行规则见 Guide

性质：只读设计研究；两种方案都未启用、不可按现行语法编译。本页不是 Guide、ADR、Spec 或红测证据。`from`、`maybe`、`borrow val`、`return borrow`、`Access`、`LookupAccess`、结果分支及复制操作均只是候选记法；不批准新关键字、Map、用户索引、动态分发或新的版本语义。

依据：[通用合同与 R01–R15](general-borrow-access-results.md#6-待冻结的正反验收矩阵)及
[现行 Guide](../guide/README.md)。研究时读取 SPEC-0279 已启用 v0.41 的源码快照；
计划分支最初基于 v0.40，现已重基到 PR57 合并后的主干 v0.41。
本页没有运行编译或原型；下面的 API 位置只证明复用点，不证明新能力已实现。

建议先比较 A：返回访问 mode + 唯一声明来源。它能直接表达字段、元素、局部绑定和包装返回，新增表面类型较少。B：受限访问结果类型，对查询的状态与转发更显式，但必须增加独立的非存储能力分类。两者共享同一来源/selector/loan 核心；B 不会消除跨调用 loan 交接的成本。

## 1. 两方案共同必须成立的合同

共同来源、权限、期限与清理以[通用候选](general-borrow-access-results.md)为准；
本节只说明两个备选必须如何表达同一合同，不建立第二套来源规则。

声明保存结果的目标类型、Shared 权限、结果存在协议，以及唯一来源 `Receiver` 或 `Parameter(declaration identity / formal index)`。来源名称仅用于解析；改名、命名实参重排、跨文件同名均不改变身份。具体 field/index/entry 路径由 body 验证后和实际求值各一次的 selector 组成，不写成来源参数的字符串注释。

每条正常返回必须从该输入的有效 place 或已有 child access 派生。另一输入、callee 局部 owner、临时 receiver、闭包环境、失效或不同实例来源都不能返回。Missing 可以没有目标，但不成为借另一来源的豁免。受控转发可以投影并交接 child；不能丢 root、权限或 parent 链，也不能把来源集合简化成一个参数。

局部结果有效期采用最近结构化访问域的确定上界，建议首版直接用声明所在词法块；根 owner 或 parent 的上界更短时取更短者。不得由最后一次读取自动结束 loan。借用结果不是所借 T 的 owner；结束仅释放访问保护，不调用 T 的 deinit。显式 owned Copyable 读取另有 Copy fact；MoveOnly 不允许隐式 clone、retain、Value 交付或 owned 返回。

同来源并不等于同一个 loan：多个只读 child 可以共享根来源，但分别具有存在条件、依赖和结束动作。首版 root/selector 不能证明不重叠时保守保护整个来源；不承诺 Map 不同 entry 或未知 List 索引可并行 Exclusive。

### 输入 mode 与调用实参

| 被声明为结果来源的输入 | 候选 Shared 结果 | 必要约束 |
|---|---|---|
| Borrow receiver / 参数 | 允许 | 实际来源必须是 caller 保持的有效具名 owner 或已有 parent access；不能把临时 caller owner 延寿作为默认行为 |
| Inout receiver / 参数 | 有条件允许 | Shared child 持有时不得恢复父 Exclusive 的冲突权限；必须存在可验证的 continuation protector 或 caller 上游 parent，并发布交接/恢复事实 |
| Own/Value receiver / 参数 | 拒绝 | 其 MoveOnly owner 将由 callee 移动或析构；Copyable 也只是 callee 的 owned copy，借用不能因无 deinit 就指向将结束的栈槽 |
| callee local / temporary / captured source | 拒绝 | 不属于声明的唯一输入；也没有首版允许的 caller lifetime 承载 |

caller 的 `val owner` 传给声明端 Borrow 完全合法；这与声明端 Own 不同。`query(makeOwner())` 即使普通 Borrow temporary 调用本来合法，首版新结果绑定/转发仍拒绝；继续保留既有只在本次调用中使用 temporary 的行为。Copyable temporary 若需要独立结果，应走 ordinary owned-copy API，不能伪装成借用结果。

### CallReturn 的安全交接，不得直接延用旧 end

1. caller 按现行顺序求值 receiver/实参、建立已选 mode 的输入保护。后续实参冲突、receiver reservation/activation、Nothing 短路不改变。
2. callee 在返回交付前提交来源和 selector 证明，结果只引用尚有效的目标。返回 operand 自身 Abort 时不生成结果或 cleanup edge。
3. caller 按 formal receiver/parameter identity 映射到实际 owner/上游 parent，并在结束 callee 视图前完成结果依赖交接。不得让输出 child 的 parent 留在即将结束的 callee loan 上。
4. 若来源为 caller 具名 owner，可建立 caller 持有的 continuation protector，再将输出依赖接上；若来源已有 caller parent，则结果接到该有效 parent，期限不超过它。callee 的临时输入视图和不相关 key/argument loans 按调用合同结束；不能把所有实参一起延长。
5. Inout 来源若需要从本次 exclusive 保护继续交付 Shared 结果，必须定义原子保护转移/重接父链：无可写权限空窗、无活跃 child 下结束 parent。一个保守候选是将所选源的必要 Exclusive 保护转为 caller 持有的 continuation protector，直到所有结果 child 结束才恢复父权限；它可能比纯 Shared 更严格，但不得默默“结束 Exclusive 并遗留 child”。精确操作及 ABI 仍是冻结前决定项，未有事实/表示时应拒绝该子集。
6. 正常域退出、break/continue/return、可恢复错误逐边结束 child，再结束本域 protector，最后允许 owner 的既有清理。受控 wrapper return 先交接结果的依赖，不能按普通块退出先结束它。Abort 不展开。

上述是两个方案必须新定义的工程合同，不是现有 loan API 已具备的操作。要避免以重新求值 key/index 来重建结果，或因交接而恢复旧 selector/current instance 的有效性。

## 2. 方案 A：返回访问 mode 与来源，局部绑定显式区分

候选声明记法，全部不可编译：

```text
fun text(holder: Holder): borrow String from holder
fun first<T>(xs: List<T>): borrow T from xs
fun wrap<T>(xs: List<T>): borrow T from xs {
    borrow val selected = first(xs)
    return borrow selected
}

// Map、maybe 结果及此 member 声明均为候选。
fun lookup(key: K): maybe borrow V from this
```

`borrow T` 是结果 mode/局部 binding category，不能把它默认提升为可放进普通字段/容器的引用 TypeRef。普通 `: T` 继续表示 owned 返回，`T?` 继续是普通 nullable owned 值。`maybe` 表示目标是否存在，与 T 是否 nullable 分开保存。

```text
// 候选局部绑定；names 是 caller 具名 owner。
{
    borrow val selected = wrap(names)
    inspect(selected)
    inspect(selected)
} // selected child/protector 结束；names 后续权限恢复

// 候选查询分支，entries 的 V 是 String?。
borrow val hit = entries.lookup(key)
when (hit) {
    Missing -> println("missing")
    Found(slot) -> when (slot) {
        null -> println("present null")
        nonNull(value) -> inspect(value)
    }
}
```

这里 `Found(slot)` 建立真实 nullable slot 的 child access；null 分支只是读取该 slot 的空状态，不是缺失。内层分支语法、nullable view refinement 都需独立定义，不能复用 ordinary owned `T?` 的 Elvis/copy 行为而漏掉 loan。

对 Int 等 Copyable 目标，候选 `val snapshot: Int = selected` 是显式 owned local context，发布 Copy fact，并得到不再依赖来源的副本；`borrow val alias = selected` 则发布 Shared reborrow，不是复制 owner。String/Resource 的前者仍拒绝。是否允许省略 `borrow` 的 inferred `val` 自动成为访问绑定，应先决定；本比较建议首版不允许，以避免同一 `val` 因 T 的 Copyable 性质悄然改变查询合同。

包装函数可以返回其同一参数来源下的 `selected`，但必须显式借用返回合同并在所有路径核验。将 `return borrow other.text` 写在 `from holder` 的函数中应在 Phase 3 拒绝；普通 `return selected` 不能擦掉 mode。局部 binding 离开 defining function，仅允许该声明合同授权的交接，不允许逃逸 closure、字段或无合同的 ordinary returned value。

优点：调用后的目标很接近已有 place/Borrow 使用；字段与 List 不需要暴露新普通泛型 wrapper；结果类别、owned copy 与 owner move 容易在同一表达式 use contract 下区分。

代价：需要扩展声明返回语法、局部声明和 return delivery；bare access 的类别要跨分支/局部推导保存；partial 结果仍需独立 tag/refinement。现有函数值类型 `(T)->R` 未携带 result provenance，因此首版只能允许静态已选具名调用，不能据此开放 function-value borrow-return ABI。

## 3. 方案 B：受限结果 wrapper 类型，声明仍携带唯一来源

候选记法，`Access`/`LookupAccess` 是具有编译器识别身份的受限类型，不能由同名普通 class 冒充；它们不是已批准的 std 类型：

```text
fun text(holder: Holder): Access<String> from holder
fun first<T>(xs: List<T>): Access<T> from xs
fun wrap<T>(xs: List<T>): Access<T> from xs {
    val selected: Access<T> = first(xs)
    return selected
}

// Map 与查询成员仍为候选。
fun lookup(key: K): LookupAccess<V> from this
```

`Access<T>` 为必定存在的目标能力；`LookupAccess<T>` 为 Missing/Found 的受限联合结果。两者仅能在限定域中作为局部 access handle、受控形参及有来源合同的结果运输；绝不是 ordinary owned value class/enum。它们不析构 T，不满足 Copyable/Transferable，不可存到普通 field/container/enum/Box/Rc、逃逸闭包或裸 Any；泛型包装也不能间接藏入普通 storage。必须增加“可存储/访问能力”的分类，单靠现有 MoveOnly 与 Copyable 不足以拒绝这些用法。

```text
// 全部为候选；这个 val 由明确 Access 类型决定其非 owning 分类。
{
    val selected: Access<String> = wrap(names)
    inspect(selected.target)
    inspect(selected.target)
}

val hit: LookupAccess<String?> = entries.lookup(key)
when (hit) {
    Missing -> println("missing")
    Found(slot) -> when (slot.target) {
        null -> println("present null")
        nonNull(value) -> inspect(value)
    }
}
```

`.target` 是候选受检查的 place/refinement 操作，不是普通字段读成 T。默认 handle 是 affine：`val next = selected` 迁移 handle 的结束义务，而非按位复制 loan；多个只读 alias 用显式 Shared child reborrow，不能把 affine 限制误读为禁止 R11 的共享访问。候选 `copyTarget<T: Copyable>(access: Access<T>): T` 可产生独立 owned copy，不给 String/Resource 添加隐式 clone，也不依赖现行尚无的“仅 Copyable 时才出现成员”规则。

受控包装返回会迁移能力的来源证明/结束义务。若以后允许 `forward(access: Access<T>): Access<T> from access`，来源须归一到 access 的根 owner/parent，不能只借 wrapper 栈槽；这项新增形参运输需明确合同。首片可以只接受 owner 参数并在 body 转发局部 Access，仍然满足用户的局部绑定与包装返回要求，不是 callback-only。

优点：API 明确区分总访问、查询结果与 ordinary V?，结果 tag 有独立类型身份；用户库的静态泛型包装更容易在 signature 看见结果类别；普通 val 的能力性质有类型证据。

代价：新增受限类型及其存储/泛型/参数使用检查；增加 handle 迁移、reborrow、tag refinement；不能直接利用 ordinary enum constructors 构造 Found，因为那会允许伪造任意 pointer/source。即使运行时可采用 tag + 目标地址，仍需来源、parent continuation 与现行 verifier 的跨调用证明；包装成 ValueId 不会自动使 loan 返回安全。

## 4. 同场景比较及跨声明边界

| 问题 | A | B |
|---|---|---|
| 局部绑定、重复只读 | `borrow val` + access category；每次 nested Borrow 有 child fact | 明确 Access local + target place；handle/child 分开 |
| 包装返回 | 返回 mode/source 相同，显式借用交付 | Access 类型/source 相同，迁移受限能力 |
| 非存储 | access category 不能进入 ordinary owned delivery/storage | 类型层拒绝任何递归包含 Access 的 ordinary storage；仍须检查 provenance |
| 三状态 | `maybe` 存在 tag + nullable target view | LookupAccess tag + Access 的 nullable target view |
| 表面语法成本 | 新返回/局部/return grammar，存在协议仍要命名 | 新受限类型和 target 操作，仍要来源 clause |
| 核心工程成本 | 相同：来源核验、selector、loan continuation、CFG/返回 ABI | 相同；另增加能力类型的泛型与存储分类 |

有序嵌套访问的对照：A 可先 `borrow val list = root.lookup(key)`，在 Found 分支把 list 的有效 List place 交给 `fieldAt(xs: List<Resource>, index: Int): borrow String from xs`，再绑定返回的字段 access；B 先得到 `LookupAccess<List<Resource>>`，在 Found 分支把 `listAccess.target` 交给 `fieldAt(xs: List<Resource>, index: Int): Access<String> from xs`。这里所有拼写、Resource 字段与 Map API 都是候选简写。两者最终来源都必须是原 root 的当前实例，路径按 entry→index→field 拼接；输入 List 形参只是转发关系，不能把它的 callee 栈槽或局部 handle 当作新 owner。query 的 key 和 index 都各求值一次并携带已保存的 selector，不用 wrapper return 重查。

跨 module：signature 必须携带 result kind/target/source/permission/存在协议。调用者只消费声明级合同与经过验证的产物，不读取函数 body 猜来源，不按 API 名字给 get 特权。外部/native 声明若没有可验证 producer 合同，首版不得宣称可安全返回 access；模块序列化/可单独编译验证的信任边界另需决定。

静态 interface：接口 requirement/default、override 及 delegate forwarder 精确复制规范化结果合同。formal `from this` 与 `from parameter 0` 不相同；formal parameter 改名等价但换参数不等价。源不参与 overload shape，不能靠结果来源或 mode 创建新 overload；但完整 override contract 必须比较它。具体静态 Self 替换保持来源关系；delegate `.field` 可形成 `this → field` 路径，不能把下一跳 receiver 抹成无关根。接口仍仅用于 bound/supertype/delegation，不生成裸 interface access value、dyn 或 vtable。

generic：目标 T 规范化替换，来源 formal identity 不随 T 重写；所有实例 memo 包含 source/analysis、已选 declaration、完整 invariant type args 与结果合同；动态 root 当前实例仍属每次调用的 ownership/handoff 事实，不以 runtime owner 为 monomorphization key，不把同签名不同声明/来源共用事实。`<T>` 不要求 T:Copyable 才能返回 Shared Access<T>；只有独立 owned copy 需要 Copyable bound。A 的 access category、B 的特殊类型均不能因未替换 T、转成 Any 或放到普通 Holder<T> 而丢失非存储限制。两方案均不引入型变、生命周期参数、开放间接调用或动态分发。

短期不逃逸 closure 对新结果的 capture 在 A/B 中采用相同待冻结判定：必须另有
source/parent 保活、捕获结束与调用范围证明，不能从 A 的 binding category 或 B 的 wrapper
自动推导合法。首版未批准该合同前均不开放；它不同于已明确排除的逃逸 capture。

Missing/Found(null)/Found(value)：Missing 无 element loan、无可读 target；Found(null) 的 target 是存在的 nullable 存储槽；Found(value) 的 non-null child 仍依赖该 slot 和 root。建议首版统一保守持有 result domain 的 root protector，包括 Missing 状态，但 Missing 不生成 element loan；是否在已证明 Missing 分支提前结束 protector 是后续可选语义，不能从 runtime pointer==null 推导。key 的调用期保护仍在查询结束清理，临时 key 可释放，结果不得依赖 key 栈槽或地址。

普通 T?：仍是拥有所有权的 nullable value。Int? 可按其 Copyable 合同产生 owned copy；String?/Resource? 不因 null 分支可能发生就获得 Copyable。两方案的 Access<T?> 都是借用 nullable slot，LookupAccess<T?>/maybe-borrow T? 另有“条目不存在”层。不能以新增 T??、把 Found(null) 合成 Missing 或 null-pointer loan 混淆这两层。

## 5. R01–R15 的共同验收与差异

下面均为待绑定真实源码的候选预期，不是已执行测试。

| ID | A 与 B 必须给出的事实/拒绝 | 差异或首片边界 |
|---|---|---|
| R01 | 现行 List<Resource> 直接 Borrow 基线不改，ordinary owned read 拒绝 | 新结果仅静态具名声明；不开放用户索引 |
| R02 | Map<Int> 借用与 owned copy 分开，ordinary V? 不冒充 access | A mode；B Access；Map 仍另需批准 |
| R03 | String 局部重复只读、query 后 key 清理/可用，owner 不得提前替换/drop | A borrow local；B typed handle |
| R04 | Resource/字段路径保持根来源，结果不调用目标 deinit，owner 正常析构一次 | 需要未来正常 native 观察+真实 deinit oracle，不能只用 stdout |
| R05 | Map entry→List index→field 有序 selector 各一次，child≤parent/root | 两方案都必须改当前 place 模型，wrapper 不能跳过 |
| R06 | 同/跨文件静态 generic wrapper 可转发，另一参数/错误实例拒绝 | full result contract 传递给 interface/delegate |
| R07 | 同根且证明兼容的 selector join 可以合流；多根/未知 selector join 拒绝 | 首片不合并未经证明的目标集合；域上界不因 loop 自动延长 |
| R08 | 普通 Borrow temporary 基线保留，新结果来自 local/temp/Own 输入拒绝 | B handle 不能负责偷偷延寿 owner |
| R09 | key 比较/哈希各一次，仅 key call loan 结束，result protector 继续 | key 投影 API 必须声明 key 来源 |
| R10 | 三状态/refinement 明确，Missing 没有可读 loan | A 存在 mode；B LookupAccess tag |
| R11 | 共享 aliases 合法，活动结果阻止 root move/drop/relocation/重叠修改 | B 显式 reborrow 而非 handle Copy |
| R12 | mutable place/Inout 的结构化 Exclusive + Shared child 结束后恢复父权限 | 两方案都不返回任意 Exclusive capability；读结果不升级权限 |
| R13 | 每条退出 child→parent→owner，受控 return 先交接；Abort 不展开 | B handle cleanup 仅结束 loan，绝不 free target |
| R14 | frontend/SSA 校验同一个身份、权限、当前实例/内容；伪造事实拒绝 | A/B 都要新的结果 producer/handoff 消费合同 |
| R15 | 普通存储、逃逸 closure、线程/async 拒绝；短期同步 Borrow 可行 | B 必须阻止 generic/nullable/container 间接包住 access；短期不逃逸 capture 两方案同样待冻结 |

## 6. 已读的真实复用点与不能冒充已有能力的缺口

| 已有事实/API | 可复用内容 | 仍需新增 |
|---|---|---|
| [CallableDescriptor](../../crates/lang-frontend/src/type_checking/model.rs)、[UnitCallableSignature](../../crates/lang-frontend/src/type_checking/compilation_unit/model.rs) | receiver、参数 mode/identity、return type，签名先于 body | 返回 kind/source/权限/存在协议；界面匹配/委托复制及序列化 |
| [CallDescriptor](../../crates/lang-frontend/src/type_checking/call.rs)、[UnitCallDescriptor](../../crates/lang-frontend/src/type_checking/compilation_unit/bodies.rs) | callable instance、实际 receiver、argument→formal index、category、实例化类型 | 声明来源到实际来源的 substitution；result category；producer/refinement |
| [ElementPlaceDescriptor](../../crates/lang-frontend/src/type_checking/container.rs)、[unit descriptor](../../crates/lang-frontend/src/type_checking/compilation_unit/bodies/container.rs) | receiver/index 表达式与 element type，源码求值身份 | 有序混合 selector、Map entry 逻辑目标，不靠地址/hash/名称断言 alias |
| [OwnershipPlace](../../crates/lang-frontend/src/ownership_checking/model.rs)、[UnitOwnershipPlace](../../crates/lang-frontend/src/ownership_checking/compilation_unit.rs) | 稳定 root/field、保守 overlaps、Known/Unknown index | single 字段后索引不能再 field；unit 仅 terminal 单 element；两者不能直接表示 Map→List→field，也未承载当前动态 owner 实例 |
| [LoanFact](../../crates/lang-frontend/src/ownership_checking/model.rs)、[UnitLoanFact](../../crates/lang-frontend/src/ownership_checking/compilation_unit.rs) | Shared/Exclusive、target、call/argument identity、begin/end Span，receiver reservation | 非 owning local access binding、范围、parent continuation、结果交接及逐边 end/restore；不能只把 end_span 延后 |
| [unit places/delivery](../../crates/lang-frontend/src/ownership_checking/compilation_unit/dataflow/places.rs)、[single loan](../../crates/lang-frontend/src/ownership_checking/checker/loan.rs) | Copy/Move 分类、projection、temporary origin、冲突检查 | 新 result use/delivery class、局部 access 与返回来源证明 |
| [SSA model](../../crates/lang-codegen/src/ssa/model.rs)、[verifier](../../crates/lang-codegen/src/ssa/verify_ownership.rs) | EntityType Loan、LoanId、reborrow/field/element、block loan transport，拒绝 active child 下结束 parent | 当前 Function returns 是 SsaTypeId value 列表，Return 是 Vec<ValueId>；DirectCall/返回 verifier/LLVM 不能直接运输新 loan 结果。A 需结果返回类别；B 即使沿 ValueId 包装，也需定义/核验依赖和 provenance 转移 |

当前 pending call/runtime constructor 在成功 CallReturn 结束本帧 created loans，Nothing/Abort 保留已求值前缀。这是已支持调用期合同，不是借用返回基础已经完成。SSA current-content 与 parent dependency 校验可复用，但新跨函数 producer/consumer 的事实仍须由 frontend 发布，LLVM 只消费验证后的 ABI 计划。

## 7. 冻结前必须决定的事项与交付顺序

1. 选择 A/B，以及 Missing 的结果表示、局部绑定写法、owned copy 的明确触发方式；决定首片是否接纳 Inout 来源。两者不依赖 callback-only，也不要求完整 NLL。
2. 冻结声明 result contract/source identity、body 返回证明、结构化有效期、非存储限制与源选择一致性；更新 Guide 必须由用户明确启用，不能从本页推定。
3. 单独审查跨调用 continuation/parent handoff、Inout 保护恢复、tagged-loan ABI、generic instance/跨模块合同与静态接口委托；长久表示与 ABI 决策需 ADR。首次共享 source 核心的 Spec 应用 fields/List 的具名 owner 和受控 wrapper 验证，Map 需另行批准类型/API/entry identity；不得通过 get 名字特判。
4. 正式 Spec 将 R01–R15 分片绑定真实 source、Phase/Span、typed/owned facts、SSA/verifier 与普通 native oracle，先真实失败再实现。Copyable、String、Resource 与 nullable slot 都需正反控；运行未发生前没有绿测或成本结论。

主要未决风险是输入保护与返回结果的父链交接，尤其 Inout 恢复过早；其次是动态 selector/current instance 与同一来源的不一致、Missing/refinement 假 loan，以及泛型 wrapper 偷渡存储。任何一个方案若只补语法或把 pointer 包进普通 T，都无法满足 R01–R15。
