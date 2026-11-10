# Unit loan 实际动作源

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改普通 loan 观察、cleanup 投影与覆盖边界时 · **唯一真源**：`compilation_unit/loan_actions.rs` 与 `dataflow/drop_planner/action_source.rs`


## 当前有界来源

Unit drop planner 的 acquisition 注册与原 `iteration_actions` cleanup 现保存于同一
append-only `ActionSource`。旧 direct/prefix/provider 切片只迭代其中 Cleanup，旧 cleanup
下标保持，acquisition、receiver及result观察记录不参与旧计数；不从旧表排序拼出顺序，也不增加第二次 End/Drop 执行。
`loan_action_observations()` 按 source 发布 Begin 和已有实际 cleanup 的观察，错误/deferred
仍在 raw 与公开边界全局撤销。静态 record 的封闭 catalog ID 不可跨 source 产品或重分析解析；
结构快照相等不表示 catalog 相同，记录顺序不是跨分支、循环或 callable 的执行时间线。

source 总体仍只发布 Partial 或 Unsupported：有记录仍可能遗漏 result handoff、receiver
activation 和权限恢复；空 source 动作表不能证明 absence。没有整个 callable、动态路径或
activation 的完整证据，未退休 Single oracle。receiver正常End、结果binding提交和End表已为同源投影；其他result观察独立保留。

## 普通 direct call 正常返回区域

`regions()` 单独声明 protocol 1 的调用者侧正常返回区域与两族义务：Shared实参建立/结束，
以及对应String临时owner Drop。只准入顶层普通函数中独立call语句（整body无其他语句）
或单call表达式body，callee为普通Name/Declaration、无泛型/receiver、Owned Int/Unit结果；
实参必须为无插值String字面量直接传到String Borrow参数，也允许零实参。已有pending
loan/temporary/capture/provider、控制、命名变量实参、Group/nullable转换及nested call不纳入首片。
这是有界正常边，不证明callee动态必然返回，也不覆盖callee内部或Abort路径。

真实planner进入通用Call分支前记录混合动作游标，最后一项call清理后才登记正常完成。
独立validator逐项核完整未筛选区间：源码顺序的全部Begin、逆序End、逆序temporary Drop，
身份与CallReturn均须同call，Drop须匹配独立AST字面量span。按call索引复用checker事实，
不逐call重扫所有loan，不从旧表排序或删除额外动作。符合此固定协议的region可Complete；
零参数须同时有实际planner入口、正常完成、AST/typed零参数与空区间，才是Complete-empty。
未准入为Unsupported，已准入却未完成/验证不全为Partial；source总体不因此Complete。

专用String/Rc/construction早返回可能没有region，缺席不证明absence；区域记录仅为静态
同轮catalog事实，不是运行时轨迹。`owns_region`同时保护零动作证书的catalog归属，
clone保留归属，重分析即使source/call下标相同也不接受。原始候选与公开region随source错误/deferred一起撤销。
实现、真实普通测试和CI-only负控的未运行边界见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md)。

## Owned 参数 Shared receiver 正常返回区域

protocol 2与protocol 1分开声明SharedReceiver/NamedSourceDrop两族，只准入上述flat caller
中的零实参普通Symbol方法、Owned Int/Unit结果。receiver须为无Group/字段的直接Name，
对应本caller显式Own参数、Owned binding与仍在ValueState中的真实OwnedValue；类型须为
无泛型、已判定非Resource的MoveOnly普通Class。typed receiver、契约、静态fact、place
及独立AST使用span逐项一致，Named Drop origin另取参数形成marker，不借receiver End推断owner。

Shared receiver Begin来自实际pending注册，正常End来自原pending token提取点及原bounded
筛选门；旧`receiver_call_return_ends`按原顺序投影该源，observer关闭仍保留End，新观察不进入
旧iteration cleanup执行流。独立validator核完整未过滤区间恰为Begin、同token正常End、
同CallReturn的Named root Drop；最后一项的owner origin必须匹配独立参数形成span。
有真实进入/正常完成且三项闭合才为Complete，根仍live未Drop为Partial，未准入为Unsupported。

Borrow/Inout参数、Two-Phase、this、投影/转换、temporary/Rc、provider/capture与borrowed
result仍不准入；borrowed-result原诊断不变。仅证明当前Shared token退休及实际owner销毁，
不是通用permission restoration证书，不证明callee必返或source/callable Complete。
receiver专用on/off全产物差分与跨source错误/deferred撤销均保持；catalog身份仍不能跨重分析。

## 单Named参数源 direct call 正常返回区域

protocol 3声明DirectSharedArgument/NamedSourceDrop两族，只接上述flat caller的普通
Name/Declaration调用，无receiver/泛型、Owned Int/Unit结果。唯一实参必须直接引用本caller
显式Own String参数，唯一callee参数为同型Shared Borrow；命名实参仍保持typed参数映射。
Own marker、Owned binding和仍在ValueState中的根及形成span共同证明来源，callee契约、
operand的Place category、loan/place身份、使用与call span都独立匹配。未准入的Named候选
为Unsupported，不能把其空动作解释成absence。

不增加动作生成或旧投影：复用已有真实Begin、EndCallLoan和Named Drop，并验证完整
未过滤区间恰好三项同call闭合，Drop origin必须是参数形成span；真实进入后根仍live
缺Drop为Partial。protocol 1/2仍各按原合同认证，未知协议编号不认证Complete。
Borrow/Inout/local源、Group/投影/转换、多参数、borrowed result及控制/嵌套仍不准入。
这是有界正常边证据，不证明通用权限恢复、callee必返或整个source/callable。

## 结果binding提交与真实退休记录

`BorrowHandoffCommitted`在原成功initializer的continues提交点发布，保留真实pending
source loan→binding映射；未完成/放弃的initializer没有提交记录。它不是source End，
旧fact的CallReturn与auxiliary descriptors只是历史来源描述，不能把辅助End倒放到
binding提交时刻。真实辅助临时Drop可以先于该commit；本观察不补造缺失的辅助退休动作。

`BorrowBindingEnd`在已初始化binding从真实状态移除、原transport处理之后，owner Drop
之前记录，保留现有child→parent退休和同源siblings的独立身份。原始记录按实际发布各自
保留，不用binding/point相等合并；旧borrow_ends投影仍按首次出现去重，handoff投影保序。
两族在observer关闭时仍保存旧表所需源记录，不进入旧iteration Cleanup计数或执行流。

这一步只补Partial生命周期证据，不新增Complete协议，不引入跨函数LoanId或动态parent
身份。callee返回来源、完整parent权限依赖、一般result/consumer/wrapper/provider/capture
及辅助退休顺序尚未认证；source/callable不会因新表非空成为Complete。现有protocol1/2/3
仍核完整区间；错误/deferred继续在raw和公开边界撤销新观察与旧投影。

## 同步consumer实际依赖边

`BorrowConsumerTransfer`在真实source pending移入PendingTransport时记录；不透明句柄随
transport保留，`BorrowConsumerRetired`在正常完成且真实source/consumer token退休处
引用它。旧borrow_call_transports是退休记录的同源保序投影，observer off仍保留该源，
两类新记录不进入旧iteration Cleanup计数或执行流。

独立validator核同catalog、先前transfer、完整source token、consumer call/argument、
Shared与root，以及实际caller的SourceUnitId和SourceId；callee parameter_span允许跨源。
失配仅使公开依赖引用为None，不删真实退休或旧投影。快照结构Eq不含不透明身份语义，
getter返回catalog-bound ActionId，clone可解析而重分析不可。

句柄不参与原PendingTransport语义Eq；同前缀clone保留句柄，语义相同但句柄不同或缺失的
合流保留原transport资格并清空观察引用，不任选分支。实际later Return输入仍被既有
L0164拒绝并全局撤销，没有借此开放语言准入或补造End。只补Partial关系证据，不认证
完整acquisition依赖、wrapper/unwind、动态唯一消费、权限恢复或source/callable Complete。


同一catalog的Begin现在返回ActionHandle，随实际pending token进入transfer与retirement，
不扫描事实表补来源。独立acquisition验证只沿既有合法transfer引用核source Begin < transfer
< consumer Begin < retirement、Begin类型与完整token，公开两个catalog-bound引用；缺失或
失配只撤这两个引用，原transfer与transaction仍保留。PendingLoan的Eq只比较旧action/depth，
合流仍保留原末状态pending，只对输入中缺项、不同深度、缺失/不同句柄或重复语义候选清空
观察元数据；无歧义clone保留，不回填缺失引用。receiver句柄本片为None，observer off不增
Begin。本证据仍Partial，不证明动态唯一消费、wrapper或权限恢复。

## 同步结果链normal-return区域

protocol 4复用实际region入口/正常完成及同一动作catalog，独立核未过滤区间恰为source
Begin、实际transfer、consumer Begin、真实consumer/source退休、同CallReturn的Named Drop。
前三条依赖句柄必须指向本区间的对应绝对记录且属于本catalog；tokens、call/argument、
Shared/root、source及Drop origin逐项一致，不凭非空、排序旧表或局部链代替完整区域。

准入只含普通非receiver函数内唯一首个不可变local String纯字面量声明，随后一个单层
nested-result调用及普通Name/标量/String实参direct调用。内外无receiver/泛型/abort，
内层单Shared String参数声明唯一Borrow返回来源，外层单Shared String消费且Owned Int/Unit
结果；独立typed合同、唯一loan及真实BorrowCallUse映射核一致。本地val没有普通参数
binding descriptor时以声明、formed_owned及唯一仍在state的真实owner为正证；若有descriptor
则必须Owned。literal形成span与owner name marker分开，实际Drop.value_origin匹配后者。

现存owner choice/loop/branch、replacement、活跃replace_consumer及全部pending/transport/capture
状态显式排除。
根仍live无Drop则Partial；不符合准入则Unsupported，参数根旧transport未支持也不补End。
protocol1/2/3不变，source/callable仍Partial；不证明callee内部、Abort/unwind、wrapper、
动态唯一消费或通用权限恢复。与owner合批后已显式拒绝活跃replace_consumer；当前caller
shape与严格B不相交，但不把该隔离当永久保证，也不把人工状态负控当普通可达性证明。
