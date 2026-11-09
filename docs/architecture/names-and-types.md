# 名称解析与类型事实

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改名称解析、类型检查或 compilation-unit 事实时 · **唯一真源**：`lang-frontend` 代码与测试

## 单文件分析

`name_resolution::resolve_names` 使用只读 `ParsedFile` 与显式 `NameEnvironment`，建立 scope、symbol、reference 和 overload set。
Unit 按文件复用该原语，再叠加跨文件声明身份与 package/import 规则。resolver 不读取文件系统或隐式加载 prelude。

单文件解析使用类型/值双命名空间。顶层和成员先收集后解析，block local 在 initializer 完成后可见，
嵌套作用域允许遮蔽。引用结果保留声明身份和源码 Span，未解析与歧义形成结构化诊断。

`type_checking::check_types` 消费名称产物与显式 `TypeEnvironment`，返回按 AST ID 索引的 `TypedFile`：
表达式类型/类别、call/receiver/argument mapping、nominal projection、container/Rc、copyability、destructuring 与 flow。 `canonical_type_tables` 用原空 package 源码验证双 Unit 入口的 signature/raw 完整21种种子顺序；旧 Single checker 消费者已退休，local/unit 类型表直接对照留在 canonical 核心单测。
Unit调用与逻辑递归帧隔离保留pending语义（[新主线适配](../specs/active/0341-shared-lowering-core.md#unit-栈帧候选适配新主线2026-11-08)）；raw检查保留recovery，旧validate边界不变；`complete_file_types` / `complete_compilation_unit_types`另授绑定原分析的
[最终消费资格](tooling.md#最终类型消费准入)，不等同ownership/native能力；普通变量 `=` 按storage检查RHS、产Unit并kill旧facts（[0325](../archive/specs/0325-simple-variable-assignment-types.md)）。
`type_callable` 的 println 身份、实参映射、lambda 模式与泛型实例由双 Unit 入口覆盖。失败 trial 的双 Unit 见证核对原诊断范围、四项被撤销表达式的类型/类别与既有合法 body 保全；旧 Single 保留失败 trial 的封存哨兵见证，见[复审账本](../specs/active/0392-compiler-layered-review.md#98-r0r2-泛型实例身份与待定调用消费者)。`type_checking::rc_unit` 的构造/share/payload 与失败原子性两项原消费者、`nominal_unit::signatures` 的接口闭包与泛型 this 两项原消费者使用双 Unit raw type 入口。`nominal_unit::stress` 保留主线96层泛型/64层接口图及128个枚举分支/96层smart-cast回归，未重复注册旧候选。覆盖见 `name_resolution`、`type_checking`、`type_callable`、`final_type_admission`、`control_branch_join`（18份普通同型/终止及 expected-mismatch 源码走双 Unit，runtime Any 的 Single 差异仍保留）与双 Unit 的 `lambda_completion`；while 保守流事实已具双入口 FE/SSA、有界 native 与 PR116 双平台实现 CI 验收，历史与限定见[0330](../archive/specs/0330-while-conservative-flow.md)。for 与 loop 同样清除回边可能写入的事实，for 先检查 source。loop/while 发布普通完成性，有限域 when 发布实际 selector 前缀，见[复审 §135](../specs/active/0392-compiler-layered-review.md#135-r2r3r4-循环完成性与穷尽-when-前缀)。Elvis 和 if 按正常出口合并事实与来源版本，见[分层复审 §25](../specs/active/0392-compiler-layered-review.md#25-r2-循环回边和-elvis-出口的流事实)。lambda expected/capture 见[专题](expected-move-literals.md)。

两入口将同expression已选EnumCase构造归为Temporary，Group透传；Name/Member拼写不授予place，真实binding/field不变。
final仅沿已选case的静态qualifier准入；无descriptor的零payload推导失败须原typed L0144、case Value引用与raw失败表示共同证明，仍保留失败资格及独立根诊断。覆盖见`enum_case_borrow_categories`，不扩展enum when或泛型enum native。 `type_checking::raw_followup` 的11项构造/smart-cast/when 原消费者现走双 Unit raw type 入口；顶层溢出保留 L0167 与 L0090 并存及原 Single witness，范围见 [迁移验收](../specs/active/0341-shared-lowering-core.md#独立批次构造与控制类型消费者2026-11-07)。

## 数值字面量解码

`type_checking::integer_literal_magnitude` 为单文件/unit 类型检查、常量求值、元素索引身份与
后端整数 lowering 提供同一精确幅值解码。具名位运算与 inv 见[整数位运算链路](integer-operations.md)。十进制、hex/bin 前缀、数字间下划线和已分类后缀
在此统一处理；范围/default/expected type 与负号诊断仍由各 checker 保留原有合同和 Span。
浮点字面量只在 finite 检查前移除数字分隔符，不扩大 native 浮点支持。String 长度检查见[创建边界](string-length.md)。
`numeric_literals` integration suite 覆盖两套类型/常量/ownership 入口和源顺序确定性；native
值证据见 SSA/LLVM/Runtime 页。

## 顺序迭代类型计划
`TypedFile::sequential_iterations` / `sequential_iteration` 按 StatementId 发布 intrinsic
Array/List/MutableList provider 交付 Borrow；NumericRange 按 IntRange/LongRange 身份交付 Int/Long Value。
名称 binding 获得 element 类型，discard 不创建 symbol；value-class 解构按字段声明顺序替换
泛型参数，保留 discard 分量位置。局部 owned 解构按字段名匹配，描述符按布局顺序发布。局部解构、具名迭代分量和字段访问共用 declaring classifier 的私有边界。待定值保留分量恢复类型，资源释放按绑定声明顺序逆序规划。
计划参与 callable trial 快照；错误输入与最终后置约束失败不发布计划。
unit 同名查询以 `UnitStatementId` 发布跨文件 provider/binding/projection，body 前精确化类型，
trial 回滚与错误原子性一致。Unit 中待定 List、数值 Range、View 和 Map 源保留已知绑定类型，依赖这些绑定的存储操作随之待定。循环头恢复标记只供诊断使用，不发布迭代计划或授予执行资格。借用与数值 provider 的不同实施阶段见[迭代事实](finite-sequential-iteration.md)。

## 单文件常量类型资格

`check_item` 在普通类型检查成功后为 const 声明检查封闭类型集合；非 Boolean/整数/Char/String
的已知类型使用 L0155，显式类型定位 type-ref，推导类型定位 initializer。Any 的 Deferred 表示
仍属于已知禁止类型；Error 与其他尚未确定的类型不追加资格诊断。普通变量不进入该门禁。
Phase 3 materialization facts 尚未实现；Phase 2 类型检查不替代 ownership/codegen 验证。
直接覆盖位于 `type_constants`，注册表覆盖位于 `diagnostic_model`。

## 单文件关联常量选择

`checker/constants.rs` 从 classifier/object/companion 声明建立关联命名空间索引，依据已解析的
symbol 选择常量，复用预声明类型。object 的 type/value 身份归一到同一声明；普通 value 接收者
仍走字段或 callable 路径。private 越界使用 L0154，不存在的关联常量使用 L0080，接口常量不继承。
Enum 的已声明 companion 常量不再被名称阶段误报为不存在的 case；已有 case 解析路径保留。
成功选择的内部 use 索引参与 callable trial 回滚，用于将读取分类为 Temporary，而非字段 Place。
跨文件入口未在此处接线。

## 单文件常量表达式与依赖

`checker/constants/expressions.rs` 按封闭表达式集合记录语法依赖，非法表达式使用 L0156，
companion initializer 中的 `this` 使用 L0153。普通类型错误优先；短路 RHS 仍参与资格与依赖。
名称解析在 companion 常量上下文中识别 `this`，并在正常 lookup 失败后识别被隔离的实例
字段、方法和外层类型参数（含 `T.member` receiver），使用 L0153 且保留 Unresolved。
合法外层同名声明仍优先；普通 companion 函数的查询规则不变。声明内已有上游诊断时，
类型阶段不再追加 const 资格/依赖诊断，但仍检查其他独立声明。
`graph.rs` 按依赖顺序清除 initializer 类型缓存并复核前向操作数；失效依赖抑制后继环诊断。
迭代式 SCC 检测对每个环发布一次 L0157，主 Span 与其余 label 按声明位置稳定排序。
常量阶段在运行时 initializer/function body 之前完成，后者直接读取最终常量类型；常量声明
不会在普通 item 遍历中再次检查。

`constant_value.rs` 实现不依赖单文件 symbol 表的纯值运算：整数保留精确 builtin 类型，
Char 保存 Unicode scalar，String 保存 compiler-owned UTF-8 bytes。
`checker/constants/evaluation.rs` 保持依赖排序并适配共享 `constant_evaluation.rs` 显式栈内核；
后者通过私有 context 保留 source-qualified child identity，集中处理短路与 Char/String 解码。
unit `bodies/checker/constant_evaluation.rs` 已适配同一内核。除零、余零、溢出与 signed MIN/-1 使用
L0158 定位运算符；短路 RHS 不进入值求值。失败依赖不发布值，也不追加后继求值诊断。

`TypedFile::constants()` 在无上游/类型诊断且所有常量均完成求值时发布 `ValidatedConstants`。
声明包含 symbol/type/ConstValue/语法依赖，bare/qualified use 包含 expression/target/type/value
与 Temporary 类别。集合携带 typed analysis owner，`matches` 拒绝另一轮分析；克隆同一产物
保留身份。事实在 trial 完成后统一构建，失败/recovery 不发布半成品；该标记不证明 Phase 3
所有权、String 物化或 native 能力。

## nullable when 与非空来源事实

Unit selector 提前退出只截断尚未匹配的路径，保留前序匹配和独立的真假待定写回。 `TypedFile::nullable_whens` / `nullable_when` 保留 subject、来源类别、稳定 symbol、entry/alternative
输入与剩余域；Boolean/enum 保有限域原子，nullable 域区分 null/non-null，字段/元素不绑定重新求值。
Unit 为有 subject 的 `null -> …; else -> …` 发布 `UnitNullableWhenDescriptor`，以 source-qualified
expression/subject/body、真实 reference、symbol 与 generation 连接分支域及 `UnitNullableWhenUseWitness`。
Name/Group 的非空 use 必须属于当前 entry/callable 且同一 generation；真实赋值或 inout 失效后，
不能复用旧 When witness。descriptor、活跃 frame 与版本状态随 callable trial 回滚，错误不发布成功产品。
单文件 condition 沿未匹配路径传事实、body 独立检查；其原 source-version 与 identity 隔离规则保留。
Unit null 比较沿同源 Group 识别实际 Null 叶并透传 expected type，仍保原 Binary/外层operand身份；
双null、无上下文null与非nullable错误不放宽。Unit 相容查询归入 type_graph，Nothing? 可适配已确定 nullable，拒非nullable及deferred-inner；原Single与双Unit的L0123/L0124见证保留。类型事实不是owner/loan或native资格，后继仍核真实产物。
owned root/temporary 的 class/Box/Rc 有界消费及 field/element 等边界见 [SSA/LLVM](ssa-codegen-runtime.md)。 `type_checking::nullable_when_unit` 的13项原消费者走双 Unit raw type 入口；进入when时仍为nullable的稳定subject已按实际剩余单case补payload事实，非nullable及已提前收窄的subject不扩张；三项独有字段oracle仍保留Single，见[有界修复验收](../specs/active/0341-shared-lowering-core.md#119-nullable-when-剩余单-case-类型修复2026-11-07)。

## 非空断言事实

`TypedFile::non_null_assertions` / `non_null_assertion` 为 `!!` 发布 assertion 与 operand AST
身份、operator Span、nullable/inner 类型、place/temporary 与 root/field/element 来源类别。
Copyability 复用条件能力查询，分别表达复制或整体消费候选；这些类型事实不证明所有权合法。

每个 descriptor 自带封闭的 `AssertionFailureEffect::Abort`，身份绑定到 assertion，不查询
源码 `error`，也不发布 synthetic CallDescriptor；显式 `error(...)` 仍按普通 callable 选择。
descriptor 与 expression/type/category 事实一起参加 overload/lambda trial rollback，并按 AST
identity 排序。Error、Deferred 和非 nullable operand 不发布 extraction。

`CompilationUnitTypes::non_null_assertions` / `non_null_assertion` 发布等价的
`UnitNonNullAssertionDescriptor`，表达式与类型使用 source-qualified unit identity；来源类别仍不
授予移动权限。unit descriptor 纳入 `UnitNullableFacts` 的整体 trial 快照，按 canonical source 和
expression identity 排序。单文件和 unit ownership 均消费各自描述符；SSA/LLVM 接线仍在后继
实施范围。 `type_checking::non_null_unit` 的8项原诊断、来源类别/条件能力与identity消费者已迁至双 Unit raw type 入口，保留21个展开source case及原oracle，见[验收](../specs/active/0341-shared-lowering-core.md#独立批次非空断言类型消费者2026-11-07)。

## 原子置换静态类型身份

`TypedFile` 与 `CompilationUnitTypes` 的 `ownership_primitives` / `ownership_primitive`
发布 compiler-bound replace/swap 的种类、交换类型和源码顺序的两个 operand identity。
身份来自显式 TypeEnvironment binding，而非名字；普通 call 参数模式契约同时保留。
unit 使用 source-qualified identity，并在普通及 const-enabled 验证入口交叉核对
source/type/category/call 关系；精确 AST argument identity 由封闭 producer 绑定，后继
读取 AST 时仍需比对 operand 顺序。错误输入清空整表，trial 回滚和稳定排序覆盖新事实。

Nothing operand 不删除静态调用结构；Deferred operand 不发布成功原语事实。
这些产物本身不授予 ownership commit 或 native 执行权限。[Owned root 原语](root-ownership-primitives.md)
已另外消费可信 ownership plan 接通两入口 native；[一级字段 replace](direct-field-replace.md)
使用独立 field plan，已有有界本地和双平台验收。普通 unit call 的 flow 失效、已收窄 nullable place 的 storage-T 推导不因 root 或直接字段切片而扩大。

## Compilation-unit 名称链

多文件路径由 `name_resolution::compilation_unit` 提供，身份链如下：

```text
SourceUnitInput
  → CompilationUnitIndex
  → ValidatedCompilationUnitIndex
  → CompilationUnitNames
  → ValidatedCompilationUnitNames
```

`SourceRootIdentity + LogicalSourcePath` 构成稳定 source key；package、可见性、exact/alias/wildcard import
和限定名都在共同 index 上解析。`DeclarationId` 标识 unit declaration，`UnitSymbolId` 标识局部或成员
binding。稳定产物按逻辑 source key 和源码身份排序，不以 `SourceId` 或调用方输入顺序为语义。

validated wrapper 只在对应阶段无诊断且身份链一致时产生。混用另一份 index、names、SourceMap 或
environment 会在遍历前返回内部错误。

对应覆盖位于 `compilation_unit_index` 与 `multifile_name_resolution` integration suites。 同文件非空 enum 的两段 TypeRef 复用已绑定 root 补全前向 case 身份；缺 case 保持 L0080，不回退同名顶层类型。遮蔽、双入口及排列覆盖见 `multifile_name_resolution::forward_enum`。

## Compilation-unit 类型链
`type_checking::compilation_unit` 保留原身份门，按稳定 root/path 对齐一次 AST 借用供签名与 body 共享；先冻结全 unit 签名图，再检查 body：
1. 收集 classifier、type parameter、field、variant、top-level/member/companion callable 与常量签名；
2. 建立 canonical unit type table、能力图和 owner/callable generic 参数环境；
3. 按稳定 source/declaration 顺序检查 initializer 和 callable body；
4. 发布 source-qualified call、construction、projection、container、Rc、nullable、assignment、lambda、
   receiver 和 control-flow facts；
5. 无类型诊断且无阻塞 deferred fact 时产生 validated typed unit。
callable 选择使用已冻结签名与 overload candidate 隔离；codegen 不按名称重新选择 target。名义类型、
泛型、interface/default/override、静态委托、`Copyable`/`Transferable`、String、Box/Rc 和顺序容器共享
canonical type identity；Unit 签名和 body 的结构能力保留确定否决，泛型调用与名义构造在字段能力待定时延迟发布描述符；构造的部分推断仅用于诊断。Box 签名仅接受 value/enum 名义 kind，实参根错误不追加 kind 诊断。ordinary class、value class、enum、interface 与 intrinsic 身份保持区分；顺序容器和 Rc 的签名存储校验保留重载形状。泛型调用签名、成员与顶层函数体的有界实例化拒绝不可存储的具体实参；名义字段、payload 与接口上界通过有限参数义务传播至类型引用及构造入口，失败候选撤回本轮新增类型。[泛型body需求](unit-container-storage.md#普通泛型body的具体需求)仍由frontend发布。

Unit 签名对已解析 enum case TypeRef 发 L0114（完整 TypeRef/root enum span），不实例化为可存储类型；type-test 仍走 body 独立入口。Unit 名义签名 L0091 的 primary 为 use-site 类型名、label 为声明名，不跨 source 解读 TypeRefId；覆盖见 `type_checking::nominal_arity_spans` 与 `multifile_type_signatures`。两入口共享语义模型；unit facts 保留 source-qualified ID，不拼接单文件 ID。整数 compound 在 statement role 发布 storage 事实，
Group/直接 control body 透传该 role；Map 写目标按写入角色准入，不伪造 query fact，覆盖见 `final_type_admission`。 `type_checking::nominal_unit` 的12项原enum/构造、静态接口及返回流消费者已迁至双Unit raw type入口，15份原源码与原oracle保留，见[验收](../specs/active/0341-shared-lowering-core.md#独立批次名义构造与接口类型消费者2026-11-07)。

Unit inline-layout 先解包声明修饰器，保留 enum/value class 身份与 L0116；`multifile_type_signatures::enum_identity` 覆盖三可见性双入口、跨包 public enum 及输入排列。
unit runtime layout 仅发布字段完整且 concrete 的 owner；无诊断恢复字段跳过该 owner，不报内部缺声明、不泄露字段前缀，保留其他完整 owner；真实类型诊断仍使整表失效。冻结 owner 快照与 identity 校验不变。

`type_containers` 的六项核心构造、诊断、遮蔽、泛型推导与 size 原测试走双 Unit 入口；signature storage 诊断和 deferred recovery 两项 Single witness 保留。对应覆盖位于 `multifile_type_signatures`、`multifile_type_checking`、`multifile_type_member_graph`、`multifile_type_capability_graph`、`type_copyability` 与 `type_map`（两目标均保留双入口原 oracle，无 Single 消费者）。
`multifile_type_checking`保持原target，107项按17私有领域与原baseline域组织，详见[迁移验收](../archive/development/multifile-type-test-migration.md)。 `type_checking::resource_unit` 六项原 deinit/replace/swap 类型消费者保留双入口原 oracle（[范围](../specs/active/0341-shared-lowering-core.md#resource-类型消费者有界迁移2026-11-06)）；不扩张资源所有权准入。 `type_checking::scalar_unit` 与 `interface_unit` 的九项原数值、局部/返回和接口诊断消费者走双 Unit raw type 入口，保留原诊断顺序、span 与自定义环境；顶层数值溢出因 Unit 额外 L0167 保留原 Single witness，不增加 final/ownership 前置。 四批合并后原82项测试与新增顶层对照共83项全部保留，见[组合验收](../specs/active/0341-shared-lowering-core.md#115-raw-type-消费者四批集中集成2026-11-07)。 `type_callable` 的调用失败分类与泛型推导冲突两项消费者使用同源双 Unit 入口，保留原有序诊断码与 primary span。nullable overload ambiguity 与 nullable bottom 的诊断由同源双 Unit 入口验证；唯一候选 lambda 的 callback label 取所选源码参数的名称范围，原 Single 诊断测试由双 Unit 入口承接；函数类型的隐式/显式 borrow 规范身份也在两个 Unit 入口验证，见[分层复审 §84](../specs/active/0392-compiler-layered-review.md#84-r0r2-调用参数诊断来源与类型身份消费者)。 expected lambda 的六个命名参数、七个lambda参数及函数类型模式也已由同源双Unit固定oracle承接（[验收](../specs/active/0341-shared-lowering-core.md#callable-expected-参数模式消费者迁移2026-11-08)）。 成功overload lambda的唯一候选及泛型多lambda消费者已迁至双Unit，保留同names重复分析、返回类型、参数模式及原实参顺序，失败trial的Single恢复见证仍保留（[验收](../specs/active/0341-shared-lowering-core.md#成功-overload-lambda-消费者迁移2026-11-08)）。

### Unit 关联常量选择

unit resolver 保留 static constant symbol，由类型阶段消费签名记录的 owner DeclarationId 和
visibility；跨文件 private 访问产生 L0154，import target 的 L0148/L0149 仍在名称阶段处理。
关联常量与裸常量读取形成 source-qualified Temporary recovery fact，不产生字段 projection；这些
选择随 overload trial 一起回滚。基础 `CompilationUnitTypes::validate()` 拒绝包含任何常量声明或选择记录的
产物，避免提前授予 ownership/native 能力。

闭合初始化器按 source-qualified symbol 建立语法依赖图，保留短路右侧依赖；无环部分按依赖
顺序重查以推断前向引用类型，环内具体类型通过工作队列传播，失效沿反向边传播后才发布
L0157。错误注解只使所在常量及其依赖者失效，跳过常量检查的 initializer 仍做普通类型恢复。
内部图使用与单文件共享的显式栈 SCC 算法，按稳定 symbol 排列诊断及 labels。常量类型沿用
共享闭合集合，普通类型错误优先。L0155 指向注解或推断初始化器，签名错误及包含 Error 的
复合依赖类型不追加资格级联。表达式资格复用依赖遍历，以首个非法子表达式的 Span 产生 L0156。
普通类型错误与 L0155 先处理，短路 RHS 仍检查资格。无环节点在类型资格通过后调用共享 evaluator，
适配器映射 source-qualified 表达式、已选常量值和整数类型。短路、String/Char 解码及算术由共享内核执行。
求值失败产生 L0158，阻断依赖求值和环诊断，保留类型与资格检查。循环不求值。声明计数、值完整性、
读取类型/category 和诊断共同控制 `UnitConstantFacts` 的原子发布，错误时不保留部分常量事实。
`validate_constants()` 发布独立 `ConstEnabledTypedUnit`，只暴露相同分析的 recovery 视图；
解包后基础 validate 仍拒绝常量，旧 ownership/native 不接收该 capability。 `type_constants` 三项求值消费者以双入口保留算术失败、短路/依赖边界与关联常量字宽的诊断/span oracle。

同文件已绑定 classifier/object 的两段常量路径复用 static member 选择并补全 terminal symbol；不存在的本地静态成员报 L0080。
private 非常量成员可见性、普通值接收者及同名参数遮蔽仍走原类型路径。

String clone 的 typed identity 与后续阶段运输见 [String clone 链路](string-clone.md)。

## 核心不变量

- TypeRef、call target、member target 和 receiver 选择必须可追溯到唯一源码或显式 external identity。
- expected type 只能按语义允许的方向约束 child；不能用后端布局反向决定语言类型。
- 条件 `Copyable`、layout recipe 与 runtime type identity 使用同一 canonical 类型图。
- 诊断存在时可保留 recovery 数据，但 executable facts 必须原子失效。Unit 普通成员读取确认接收者仍可空且没有待定依赖时记录根因 Error，显式类型实参的调用回退保留该 Error 并检查独立实参。缓存的待定字段保留类型和诊断所需的身份；六个 `MutableList` 修改方法保留实参检查及已知结果类型，在接收者或实参待定时暂缓发布描述符。
