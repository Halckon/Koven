# Unit 容器存储

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 Unit 列表式容器、零大小 element layout 或 callable ABI 边界时 · **唯一真源**：`lang-codegen` 代码与测试

## 单文件构造边界

单文件 lowering 支持 `arrayOf<Unit>`、`listOf<Unit>`、`mutableListOf<Unit>` 的列表式构造。
既有 `EmptyMutableList` 路径的 `MutableList<Unit>()` 也使用同一存储表示；本页仅描述列表式路径；后继runtime-length initializer的已接入范围见
[源码ABI](callable-source-abi.md)。现行语义与存储决定仍来自 Guide 和 [ADR-0008](../adr/accepted/0008-internal-value-and-allocation-abi.md)。

`ssa/lower_frontend/container.rs` 先按源码顺序 lower 每个 operand 一次，遇到
`LoweredValue::Unit` 时核对 resolved frontend element / expression 的内建 Unit identity，
取得 canonical SSA type，再追加 `Constant(Unit)` 作为 container operand。
Diverged 保留已执行前缀并终止构造，不生成后续 operand 或 ContainerConstruct。
普通 Unit expression 仍使用原无结果表示；Unit 是 Copyable，不引入 element owner/drop。

## 独立 element storage 与 ABI

`llvm/layout.rs` 用目标 DataLayout 查询空 literal struct `{}` 的 size/alignment，再进入
既有 checked plan；size=0、stride=0，alignment 不从宿主常量表推断。
`llvm/type_map.rs` 将 Unit 存在独立 `container_unit_storage` 表，仅
`container_element_type` 查询可取到；普通 `basic_type` 仍拒绝 Unit。
非 Unit element 复用原 basic type，foreign/unknown/Opaque identity 没有空表示 fallback。

三种 container header 保持原 shape、size、field offsets 与 target alignment。
空与非空 Unit buffer 都指向非空、对齐、module-private readonly sentinel；没有 Unit element
GEP/store、buffer malloc/free call，也没有 Copyable Unit element drop loop。
普通 Unit producer/entry 仍为 SSA returns `[]`、LLVM `void`，factory 返回既有 header。
这次接入不开放 Unit-field aggregate、shared-control payload、Value(Unit) 参数或显式
SSA Unit return；一般 callable/storage 能力不能通过 container 查询旁路取得。

## 逻辑读取与 CFG

element place 仍先执行 signed Int 的负数和 `index >= length` guard。
直接零大小 place 的 Read 可使用 Unit 常量；loan 跨 CFG 由 pointer phi 携带时，LLVM 可产生
合法的 `load {}`。该 load 的 store/ABI size 均为 0，alignment 来自同一 DataLayout，且仅能从
bounds-success block 进入；不能据此泛称所有 Unit Read 均无 load。

真实 `for` 仍按逻辑 cursor/length 和 owner/place/loan 身份迭代，不比较 sentinel 地址合并元素。
checked `count += 1` CFG 后的 named Unit Read 使用重绑定 loan；正常回边结束本轮 element
loan 并递增 cursor，耗尽边结束 source loan 后 drop temporary container owner。

## 直接证据与边界

2026-10-02 Linux 本地及 PR24 的 Ubuntu/macOS 双宿主测试覆盖：

- 三容器 × 0/1/3 native 固定 bytes，元素 producer 源码顺序一次、source 一次、body/count 精确；
  另有 discard、if 后 Read 和 `MutableList<Unit>()` 零次迭代。
- 独立 SSA 九格检查 FunctionId/entity、preheader、guard、edge arguments、CFG 后 Read、
  loan cleanup/owner drop；三 provider Diverged 保留前缀，一格 fresh frontend chains 的全部
  SSA/LLVM bytes 相等。
- 三 header 真实 target layout、九格 LLVM sentinel/void ABI、跨 CFG 的零字节 Read；低层
  verified SSA Array 的索引 0/2 各输出 `read-ok\n`，-1/length/empty 0 在 marker 前 SIGABRT。
- Unit/Int 两项 OperationContract、L0087/L0135 精确 span、mixed-analysis 写出前拒绝且保留
  旧 object bytes，以及一般 Unit ABI/aggregate/shared payload 的原 Unsupported 边界。

测试位于 `ssa/sequential_for_lowering_tests/unit_storage*_tests.rs`、
`native_sequential_for_tests/unit_storage*_tests.rs` 和 `llvm/unit_storage_tests.rs`。
本地完整 codegen 为 730 unit + 4 doctests，通过且无 ignored/filtered。PR24 实现 head
`c77a9aa0a6fc1b0fcf951afcf4ea9ea945778cfd` 的
[CI 37024363674](https://github.com/Halckon/Koven/actions/runs/37024363674) 已 9/9 jobs
success，两宿主逐名核同一 32 项 Unit storage 测试均 `ok`（31 新增 + 1 旧 Unit root）。
Ubuntu 完整 codegen 730 passed，macOS 729 passed + 1 既有 LLDB 权限 ignored；两宿主各
4 doctests 通过。ignored 不计调试器通过，性能/RSS 未测；该结果不推定后继文档 head 的 CI。
原始红绿与精确验收可按需追溯 [SPEC-0248](../archive/specs/0248-unit-container-storage.md)。

Borrow Unit 跨函数读取、一般 Unit call operand / Value ABI、Unit-field value class 构造、
CompilationUnit `for`、Inout/field source 与 MoveOnly ZST 完整析构仍不由此证明。
这些合法源码能力缺口不等于语言禁止；Copyable Unit 不替代 MoveOnly ZST drop 验收。

## Source size与unit String读取

普通compilation-unit `.size` 经frontend source-qualified descriptor和同步SharedLoan生成
`ContainerLength`，复用当前Borrow参数loan，只结束本次新建loan，再执行CallReturn清理。
具名/Value/Borrow/group/temporary沿用同一header；unit Borrow String `==`/`!=`/`+`消费
current binding，RHS CFG后使用重绑定loan，不隐式clone/消费源。前端replacement保护旧root
至完整RHS完成；提前return仍按ControlTransfer清理。0274双宿主各37条公共命令及15项CI已通过；完整字节证据见[验收身份](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/word-frequency-0274/ci.json)。
`||`不同路径容器last-use合流以及一般投影仍有既有拒绝边界。

String 字段的 shared receiver 复用已有 owned 单层字段 place 校验，生成 BorrowBegin，
不执行 MoveOnly 字段 Read；receiver 调用帧登记父 root，并沿既有 pending loan 协议清理。
Group receiver 和后续普通参数分支已有 native 计数；同父 sibling replacement 保持拒绝。
后续参数直接 return 仍受原 receiver control-prefix 门禁限制，泛型／嵌套字段及 Borrow 参数
owner 未因此开放；公开矩阵与剩余边界见[文本基础验收](../specs/active/0368-text-byte-foundation.md)。

## Unit 泛型函数直接容器签名

普通unit函数的直接`Array<T>`、`List<T>`、`MutableList<T>`参数和返回通过
`unit_plan/concrete_types.rs::resolve_concrete_type`消费frontend已发布的canonical identity。
resolver只替换直接元素实参，再用`find`查找具体容器；不intern，不递归展开模板。
Borrow size继续读取current SharedLoan，own参数/返回沿用原容器表示和唯一清理。

定向SSA/native覆盖三容器的Int/String空与非空、显式/推断实参、重复Borrow后源复用与own返回；
另覆盖CFG后的loan重绑定、跨文件同名T隔离、重复实例去重和输入顺序确定性。
真实资源元素在Borrow期间存活，own返回后逆序deinit；计数harness逐指针核对分配与释放，
禁止隐式String clone或shared retain。typed arena在lowering前后不增长。

直接T可以取已具体化的List<Int>，三种外层容器已有真实长度与完整清理证据。
即使frontend已发布List<List<Int>>，模板List<List<T>>仍按精确参数Span拒绝为UnsupportedNode。
native沿用UnsupportedNode→UnsupportedSource、MissingFact→InvalidModel映射。
recursive-template拒绝覆盖新/旧object目标，保全目录全部文件名、bytes与邻居文件，
不残留sibling temporary；上述实现边界不改变Guide允许的泛型语义。
0275实现及最终归档双宿主CI、actual main门禁已闭环；完整证据见
[0275交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generic-containers-0275-delivery.json)。

## 普通泛型body的具体需求

成员候选发现保留签名与接收者身份。继承成员记录闭包中的接口模板，按接收者实参在候选
检查点内具现。去重保留结构化签名形状后撤回临时类型。`super` 显式限定类型属于源码事实，
继承关系核验追加的临时类型在候选发现前撤回。
实参映射完成后，在同一类型表检查点内具现接收者和方法参数，失败时撤回本候选新增类型。
多候选重载先临时检查实例化与普通实参适用性，探测结果只保留候选模板和映射。
选定候选后重放类型替换。需要 lambda 试算时，每个候选的实例化和上下文事实共用
原有试算事务。无匹配、二义性或阻塞的未定试算均不会提交这些探测或试算产生的类型。

frontend在全部body/trial完成且无类型错误后按真实闭合source call物化body-only canonical类型，
先于field layout；原template facts、canonical ID与sealed分析身份保留。未调用symbolic模板不猜Int。
每种子按现有1024实例上限保留前缀与有限frontier，entry选择/拒绝仍归Phase4。
已提交的接口调用及有界泛型具现登记具体接收者。发布器用已选目标和编译单元内已出现的
`StaticSelf` 调用目标匹配签名中的 override 或委托合同，沿确切委托下一跳登记实际接收者。
具现后的 owner 实参与委托字段类型在冻结字段布局前可查询。确切转发链遇到重复声明和目标
即停止，跨默认方法的上下文传播按实例上限保留有限边界。Phase 4 继续消费已发布的路由和
规范类型，并执行自己的准入检查。
闭合查询、结构替换和field concreteness以completed memo/迭代栈读取共享DAG；flow-refined
enum case实参闭合性与runtime field支持分别缓存。backend
保持只读find。静态顶层与成员callee由已提交descriptor身份识别，不将其模板Function<T>当runtime
函数值登记；call结果/实参、generic function value仍走原检查。已有nullable class handle
沿inner规划hidden deinit，null/非空仍使用现有conditional drop，不扩inline nullable ABI。

名义存储要求在签名冻结前沿字段、enum payload、直接接口及参数上界传播；有限不动点只记录
所属泛型参数义务，不展开 `Grow<List<T>>` 等无限堆实例。签名 TypeRef、body TypeRef、构造和
调用 bound 替换在具体实参处拒绝违反义务的类型；接口值仍由既有 L0094 检查负责。
普通类用实际字段和拥有的载荷布局计算相关参数。无布局依赖的实参不影响句柄的存储资格。
布局参数集合增长时只重新检查读取该集合的声明。直接容器实参仍检查其表示要求。
body 模板按 callable identity 区分，只为需要类型替换的函数建立，实例参数顺序为 owner 后
method。按所属源码与函数范围一次分发类型及调用事实，闭合调用从全局已提交调用记录播种。
有界遍历沿真实成员调用传播 body-only 存储需求，未调用模板仍保留 symbolic 类型。

源码调用在检查实参期间暂存候选具现新增的类型。实参发布自身事实，lambda 可用期望类型
确定参数。普通名字、普通调用与其他无需上下文定型的实参在相容性探针中临时读取期望类型。
候选通过后，参数、返回值、实例实参和接收者用规范表身份发布。实参内部发出错误诊断时，
外层调用不发布描述符。需要传递上下文的恢复路径仍可能提前物化期望类型。
调用检查同时追踪实参中新增的未决类型事实。顶层已定型的 lambda 或字符串仍可保留内部
未决事实；外层调用保持 Deferred。顺序容器已发布描述符的 callee 占位独立处理。
泛型候选可保留待定的替换和上界。部分比较仍检查已知的类型结构、参数模式和存储限制，
待定候选不发布调用实例。
候选全部早拒时，普通实参仍产生自己的恢复事实，未提交的 lambda 留给最终准入识别。
显式类型实参已报 L0125 时，普通实参中同一规范名义实例的构造仍参与局部错误门禁。
封装类型产物时合并重复诊断，保留指向原始诊断的源码证据，供最终准入继续检查独立操作数。
Range 扩展 receiver 与普通实参共用未决推导，仍可拒绝已知外层类型冲突。
待定候选用原类型参数符号关联不变位置和不同实参的约束。约束合并的临时类型在检查后撤回，
已证实的矛盾可淘汰候选。
候选比较按名称解析身份共享未绑定外部类型的约束。显式类型引用中的名义、容器、
可空和函数结构仅在试算中保留这些身份，不改变最终类型表的待定表示。
存储 TypeRef 保留未决类型与已知结构，构造和依赖它的值仍须等待类型事实闭合。
最终准入使用源限定的待定记录继续检查独立操作数与标注。
构造器形参中未绑定的类型也保留待定结果。顺序容器与 Map 的 `.size` 在接收者
检查期间产生待定存储事实时仍保留 Int，延迟发布读取描述符。
不可变局部声明按已解析符号传播待定状态，兼顾显式标注、初始化器结果及其内部的待定事实。
名称读取参与相同的描述符发布门禁；候选试算同时恢复局部待定状态与表达式事实。
顺序索引、Map 查询及整数内建检查保留已知结果类型，待定操作数延迟发布描述符。
顺序写入与 inout 仍可检查待定元素的类型和可写性。Map 下标写入用私有恢复事实区分
写目标与查询，最终准入继续检查三个操作数，已拒绝或待定的写入不进入公开执行表。
可写局部绑定在现有流快照中保存待定状态。确定覆盖更新状态，可达路径汇合保留任一路径的
待定值。条件 RHS 的正常尾值用于更新绑定，赋值发布仍检查求值过程中的待定事实。
待定写回使用源限定的可写根身份。函数、成员和上下文 lambda 的 inout 形参随本体初始化，
已捕获的局部 var 仍可跟踪本体内的直接写入。闭包形成后恢复外层状态，smart cast 继续
使用原有的稳定性条件。
具名调用在正常返回边汇总可能产生待定值的 inout 形参。类型恢复按已选目标和形参位置
传递到调用者的可写根，待定实参仍受原描述符门禁。发现新写回事实后重查 Unit body。
可直接定位的非 move 闭包也保留正常退出的捕获待定状态及确定的无正常出口。
字面量和不可变别名按来源身份在调用处消费这些私有事实，闭包形成时恢复外层状态。
循环待定状态区分普通回边、continue、break 和条件出口。发现入口待定集合增长后，
在该循环事务内复查并撤回先前事实。`if`、短路运算和 `while` 按真假出口传递待定状态。
待定可达性属于私有类型检查事实，类型收窄和循环公开完成性继续使用原有的保守规则。
无宽度后缀的整数字面量保留符合符号族和数值范围的候选宽度，避免将默认宽度当作已定型值。
待定调用中的 `if` 和 `when` 保留分支共同可行的整数宽度、默认宽度和可空约束。
空顺序容器保留种类，和已定型分支合并所得的元素要求只参与候选比较。

SPEC0276本地新native7项通过，实际18个成功fixture执行object/link/run及逐指针计数；
覆盖三容器Int/String/Resource、三层relay、多body构造、deinit闭合helper和Nullable<Resource>。
Nullable<Int>已发布canonical但维持原null表达式的InvalidModel/MissingFact及输出保全。
原body-only MissingFact测试已迁移到真实成功；recursive-template/缺substitution保护保留。
预算及消费者的实现验收保留在[冻结记录](../archive/specs/0276-unit-generic-body-type-normalization.md)；
最终head及actual main双宿主CI已闭环，见[交付账本](https://github.com/Halckon/Koven/blob/8c6907c238cc0e040bed4541203e450a5f53552b/docs/development/evidence/generic-body-0276-delivery.json)。

## 类型布局诊断来源

Unit 类型 lowering 为 nominal、Box/Rc owner、class payload 与 enum case payload 保留首次类型需求的 source origin；
声明标签使用 qualified declaration 或 case 的名称 span。目标布局错误桥接据此生成 L0145，
而非因来源缺失退化为无 span 的 backend 错误；这不改变可接受类型或 E3/refine 能力。
真实 Unit lowering 的来源断言与合成 InvalidLayout 的桥接断言分别覆盖运输和诊断映射，不代表自然源码布局溢出复现。

Array/List/MutableList/Map/MutableMap 的 Unit storage 身份同样保留首次类型需求位置；
容器没有独立源码声明，声明位置等于 primary，不把容器 stride/header 错误标成元素或 value 类型声明错误。
共享 helper 也为 nullable Map 的内层 MapContainer 记录来源；nullable wrapper、View 与 MapResult 的额外身份不因此获得新来源或能力。
View 的合成 List source 复用同一 helper：View-first 时记录 View 类型需求位置，List-first 时保留已有来源；
这只是布局诊断来源，不替代 collection owner/loan 来源合同。RangeView 自身仍不新增 origin，元素名义声明来源不变。

容器 size 的 `type_tests::unit_lower_container_size_tests` 同时覆盖 legacy 与 strict Unit：三种容器的共享参数、唯一 owner 清理、临时接收者单次求值及 CFG 后 loan 身份均复用原断言，两入口分别核跨文件输入排列的 SSA/LLVM 确定性。旧入口及其独有见证没有删除。
