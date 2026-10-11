# 范围 carrier 的当前事实

> **性质**：实现事实快照 · **状态**：current · **读取时机**：实现或审查 N1a 首片阶段交接时 · **唯一真源**：当前代码与测试；语言语义见 Guide

## 类型与来源签名

`IntrinsicTypeConstructor::View` 使用外部类型环境的显式绑定；源码同名 nominal 保持
普通类型。single 与 unit 检查显式类型位置及 body 推断位置，generic 实例化也拒绝
carrier 实参。typed callable/已选择 call 以封闭 `CallableResultSource` 分别保存
Owned、Borrow 和 Carrier；Carrier 运输真实 from/source Span 与唯一参数/receiver
索引，验证 Borrow List/View 与结果的元素类型 identity 一致。

声明授权由 TypeEnvironment 中的 SourceMap-owned SourceId 显式授予；文件名、目录、
package 和同名类型都不授予权限。CLI project 宿主把仓库内编译期绑定的标准源码加入
SourceMap，只授权由该加载步骤创建的 SourceId；用户同名来源不继承授权。
真实 std 加载边界同时显式登记独立 extension authority；producer 授权不自动升级。
Phase 2 recovery product 中的 RangeExtensionBinding 保存同次分析的 SourceId、canonical
callable、compiler-bound List/View 接收者、元素类型模板及 Borrow/from this 区间。
member 候选只从名称阶段的可选词法查询提示取得，unit 复用既有 package/import/alias
绑定；仅获授权的真实声明能成为候选。接收者参与既有泛型推导，List/View 同名重载
按 receiver 类型区分，同 receiver 的 alpha 等价重复签名仍拒绝，mode 不参与 shape。
普通函数调用不能省略扩展的 receiver。String 与 MoveOnly 使用同一选择与实例化路径。

此绑定本身仍只是签名与类型选择。Phase 3 为真实扩展声明建立合成 Borrow 参数，
锚定到 receiver TypeRef 的源码 Span；`this` 的名称引用指向该参数，unit 使用
source-qualified UnitSymbolId。普通 nominal `this` 保持既有 receiver 路径。
producer 证明从实际构造/已选择转发与返回根取得，不能仅凭 `from this` 发布来源。
caller 用真实 receiver expression 建立 Shared source loan，View 派生续接原根；
新描述符保持 root-flat，既有 metadata 借用保留父依赖。结果 end 与元素/provider
结束共同约束权限恢复，临时链只延续到实际立即消费者并在其返回后清理一次。

可信扩展的来源与续接已接到 single/unit 静态 RangeCall ABI。lowering 消费已绑定的
合成 `this` symbol 与 receiver_type，receiver 为入口参数 0，显式参数后移，
carrier_return=0；function.receiver 仍为 None，复用现有 verifier/LLVM，不从名称
推断身份。仅移除已接通声明路径的 L0164；未证明 producer、来源错误与其他能力门保留。
语义错误或 Deferred 仍按既有原子门撤销事实。
真实 `ranges.ko` 已有 List/View Borrow receiver take wrapper，body 转发既有顶层
`take(this, count)`；View 仍不是 owned sequential container。

## 已有 metadata 借用

`ownership_range_carrier` 对 single/unit 同一合法源码检查 incoming View 参数的真实
origin、call source loan continuation、parent metadata 依赖和 child-before-parent end。
Int、String、MoveOnly 使用相同合同；Borrow metadata 不拥有或清理元素。返回 sibling
而非声明来源时 L0162 定位实际返回表达式，并且不发布错误 origin。

这是已有存储借用的验证，仍使用普通 Borrow mode。

## 构造与稳定根绑定

compiler-bound `rangeView(source, begin, end)` 发布元素 identity、List/View 来源种类与
真实三操作数；只允许已授权源码调用，源码同名函数不获得原语权限。标记区分真实
DeclarationFrom 和 PrimitiveCall；原语没有伪造 from。无效类型输入原子撤销构造事实。

Phase 3 的 BorrowBindingStorage 区分普通存储借用、既有 metadata 借用和新内联
descriptor。稳定 List 构造续接实际 caller source loan 并发布 end；View 派生继承
真实根而不持有父 metadata，metadata alias 仍依赖父绑定。父、子、兄弟对根的保护
覆盖到最后使用结束，随后允许移动 owner。三组 single/unit 回归提供直接事实证据。

## producer 返回与标准源码

single/unit 对 expression body 或唯一 terminal return 收集实际构造依赖，按已选择
callee identity 验证转发到声明参数；没有构造依据的递归环不取得证明。body 所有权再
检查实际 root，错误 sibling/local 来源 L0162，未证明转发 L0164；错误或 deferred
原子撤销返回和 continuation。RangeReturnOriginFact 与普通 borrow-return facts 分离，
转发保留真实 source loan，caller 新 descriptor 绑定/root/end 复用已验证路径。

`lang-std/koven/algorithms/ranges.ko` 实现通用 List<T>/View<T> 顶层 take 及转发它的 receiver wrapper；负数 error、非负
clip 都由 Koven body 完成。CLI project 加载该固定资产并复用 import/alias 选择，
不按 take 名称重推算法。公共 CLI 已实际编译并运行 String、MoveOnly 与 Resource
实例，count 为负时 Abort，0/空来源、等长、超长与 Int 最大值均按真实标准库 body
处理；host facts 证明标准返回 from、caller root loan 与 end。

## SSA、verifier 与 native 首条路径

`RangeView` SSA type 保存精确 List storage type。`RangeConstruct`/`RangeCall` 发布
内联 metadata value 和保护根的 shared loan；`carrier_return` 与普通 `borrow_return`
签名分离。`RangeReturn` 验证实际根等于声明入口参数，允许该参数经过既有 CFG loan
传输；调用方继承根 loan，不能返回指向 callee 局部 descriptor 的指针。

LLVM metadata 为 `{root List header pointer, begin:size_t, end:size_t}`；构造检查真实
半开边界，读取 size 使用 end-begin，不复制或分配元素。`RangeEnd` 消费 metadata、
结束保护 loan，再结束调用方建立的祖先 root loan。metadata alias 与普通 Borrow
View 返回仍使用实际 metadata storage 的 loan；它们结束前不能结束 descriptor。
verifier 拒绝提前结束根/descriptor loan，以及把 metadata 交给普通 Drop/Consume、
owned callable 返回、字段、容器或捕获的擦除路径。纯负例没有传给 LLVM/native API。

single/unit 正常源码提供同一 ABI/metadata alias/返回与清理证据；公共 project 五项
native 测试覆盖 String/MoveOnly、Resource 析构一次、metadata alias 和 borrow-return、
owner 恢复、0/空/等长/超长/Int 最大值和负 count Abort。保留的健康红例 project 也
已 build/run 成功；精确收据为 `/tmp/koven-n1a-native-red-{build,run}-green.log.json`。

## 只读元素与 for

typed iteration 使用独立 `IterationProvider::RangeView`，不把 View 加入拥有元素的
sequential container 类型。single/unit 保留真实 element identity 与 Shared delivery；
循环中的根移动和 MoveOnly 元素移动有直接拒绝回归。file drop-planner 的 loop owner
摘要对 borrow val 按 Read 求值且不注册 owned binding，避免发布 metadata 的条件 Drop。

`RangeElementPlace` 接受 Shared metadata loan 与相对 cursor。LLVM 从 metadata 读取
实际 root header、begin/end，检查相对边界后按 List 的精确 element layout 取地址。
element loan 依赖 metadata loan；字段与调用派生 loan 复用既有依赖验证，结束子 loan
后才能结束 provider source。descriptor 自身及其 protecting/祖先 loan 显式穿过 CFG；
verifier 对所有 incoming edge 保持 value/root 配对，循环证明还必须可追溯到实际构造。
提前结束 metadata 或交叉替换两个同类型 descriptor 的 root 都被纯 verifier 回归拒绝。

公共 CLI 覆盖 Borrow View 形参的 String/MoveOnly/Resource 读取，以及同函数词法范围内
具名新 descriptor 的 for。既有 provider 清理计划承担 empty、break、continue、形参
函数 return 与 body-local Resource 清理；view/provider 结束不析构来源元素。源 List
在 descriptor 词法范围结束后可消费，Resource 每个元素只随源清理一次。

## 临时来源的同步续接

RangeUseFact 保留实际构造表达式、来源 loan 的 call/argument identity、真实根及
立即 Borrow/iteration 使用点。producer 返回后的真实来源 loan 转交给实际消费者，
而非以 descriptor 临时值代替根；普通 call 的后续参数求值仍受该 loan 保护。
unit 分析把这些事实与其他 ownership 产物一同聚合、稳定排序。single/unit 的 provider
schema 拒绝缺失 continuation、缺失来源 loan 或错误根；lowering 再核对 Shared 权限。

同表达式立即 Borrow 把隐藏 List owner 的清理义务交给外层同步调用；for-source
则交给既有 provider 退出计划。SSA 把 descriptor、protecting/祖先 loan 与实际 owner
沿既有 CFG slots 运输；先结束 element 与 provider metadata loan，再 RangeEnd，
随后结束祖先 loan 并 Drop 真正的临时 List。continue 保留 provider 和 owner，break/
return/正常耗尽各在规定边界结束；body-local Resource 先按逆序清理。
连续两次构造按各自 live descriptor 匹配槽位，不能用已结束 descriptor 的复用索引。

普通真实源码的 native 身份计数覆盖 single/unit 的 String、MoveOnly、Resource，
立即 Borrow 和 for 的 0/clip/full/break/continue/return；每个元素与 List 分配的实际
释放身份都必须匹配。公共 CLI 加入来源只求值一次、连续构造、empty 及 local 清理回归。
保存或错误返回临时范围仍 L0162，不把根寿命扩大到词法绑定或调用返回之外。

当前支持 List/View 的顶层与可信 receiver take、size、只读元素、具名 for 和以上
同步临时来源。稳定 named root 的短期范围借用按已验证 Place origin 延续到外层
消费者 CallReturn，避免内部 take 返回时提前 drop；消费者结束后仍遵守原 liveness /
resource 清理规则。single pending call loans 与 unit argument frame 保存 receiver
跨 count 条件 return 的当前 loan，正常/早退分支均有 native 清理证据。

更广 CFG 保留精确门：producer count 分支缺来源证明仍 L0164；borrow val initializer
内 return 仍 UnsupportedNode(return)；receiver count break 的 single/source binding
与 unit/整个 for 仍 UnsupportedNode。未新增 View 索引、drop/dropLast、一般扩展、
consume/Clone 或 N1b。当前定向验收与本地提交/未发布边界见 active SPEC-0289 §19；
§§16–17 只记录此前前端与旧源码冻结结果。


## View 来源的新 descriptor 与转发

同一个 std take 通过既有普通重载选择支持 View<T>；计数仍在 Koven body，真实
compiler-bound rangeView 构造按相对父范围验证边界。LLVM 先读取父 metadata 中的
原始 List header pointer 与 begin/end，再构造 `{同一 root, parent.begin+begin,
parent.begin+end}`，返回新内联值；它不返回父或 callee 局部 metadata 的地址。

SSA signature/operation 同时检查 Shared List 或元素 identity 相同的 Shared View
来源与独立结果/root pair。root provenance 从 metadata 的真实 RangeConstruct/RangeCall
配对追溯，经借用续接和 CFG loan 运输保持唯一原根；当前活跃 capability 则来自输入
metadata 所对应的 protecting loan，不能要求已结束父来源的旧 token 仍活跃。
View 来源的新保护 loan 不依赖父 metadata；借用已有 metadata、元素和入口参数的
真实依赖仍保留。错误 sibling 返回与子范围活跃时释放集合有纯 verifier 拒绝证据。

嵌套立即消费的 Shared 实参 facts 继承已证明范围的实际根；临时 List 的清理义务
逐层转交给最终 Borrow consumer/provider，且只登记一次。父临时 descriptor 在实际
子构造返回后结束，最终消费者结束后清理原集合。String/MoveOnly/Resource 的正常
身份计数覆盖 single/unit，另有非零父起点与合法可信 producer 转发，公共 CLI 实际
build/run 同一个 take 的两种来源。当前 View 阶段定向证据见 SPEC-0289 §13，
同一源码的冻结全库终态见 §17。

本路径使用现有 SourceMap-owned SourceId 授权、compiler-bound View/原语 identity 和
已选择 canonical callable。独立可信 receiver 宿主及其名称、类型、实际来源与续接事实
已接通，静态 receiver ABI 与实际 std wrapper 也已有正常 native 证据。该路径未新增通用框架、
更多算法或更广借用 CFG 语义。

## 调用前缀的条件 return

同一 take 的立即 Borrow 实参已构造时，后续实参条件 return 的 single/unit 路径保持
真实 descriptor/保护根 loan 到控制转移边界。single 先结束已发布的外层调用 loan，再
结束该调用当前已构造的短期范围，最后消费 frontend 的 owner drop facts；未求值的
范围不产生结束动作。unit 用 CFG 重绑定后的前缀槽结束 metadata loan、RangeEnd 与
祖先根 loan，并分别保存 then/else 的 descriptor source-loan 状态，避免跨分支 loan。

能力预检只接通具有 range-use、相同根来源证明的范围调用前缀，普通借用与范围调用
之外的更广 CFG 仍保持拒绝。条件求值不启用借用绑定的完整 last-use；named View
使用现有词法结束点恢复根权限。正常路径提交 owned 参数，早退路径清理已求值 owner，
不执行后续实参或调用消费者。健康 native 回归覆盖四种 List/View 来源与三类元素；
公共 CLI 和阶段门禁的实际验收由 SPEC-0289 §14 记录。
