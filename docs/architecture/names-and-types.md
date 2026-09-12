# 名称解析与类型事实

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改名称解析、类型检查或 compilation-unit 事实时 · **唯一真源**：`lang-frontend` 代码与测试

## 单文件分析

`name_resolution::resolve_names` 在只读 `ParsedFile` 上建立 scope、symbol、reference 和 overload set。
调用方通过 `NameEnvironment` 显式提供外部类型和值；resolver 不读取文件系统或隐式加载 prelude。

单文件解析使用类型/值双命名空间。顶层和成员先收集后解析，block local 在 initializer 完成后可见，
嵌套作用域允许遮蔽。引用结果保留声明身份和源码 Span，未解析与歧义形成结构化诊断。

`type_checking::check_types` 消费名称产物和显式 `TypeEnvironment`，返回 `TypedFile`。类型事实按 AST ID
索引，包含表达式类型、类别、call/receiver/argument mapping、nominal projection、container、Rc、
copyability、destructuring 和 flow facts。错误输入保留 recovery 事实；后端不能消费未验证产物。

对应覆盖位于 `name_resolution`、`type_checking`、`type_callable` 和 `type_copyability` integration
suites。

## 单文件常量类型资格

`check_item` 在普通类型检查成功后为 const 声明检查封闭类型集合；非 Boolean/整数/Char/String
的已知类型使用 L0155，显式类型定位 type-ref，推导类型定位 initializer。Any 的 Deferred 表示
仍属于已知禁止类型；Error 与其他尚未确定的类型不追加资格诊断。普通变量不进入该门禁。
对外 use/materialization facts 尚未实现；这些类型检查不表示常量能力
已可交给 ownership/codegen。直接覆盖位于 `type_constants`，注册表覆盖位于 `diagnostic_model`。

## 单文件关联常量选择

`checker/constants.rs` 从 classifier/object/companion 声明建立关联命名空间索引，依据已解析的
symbol 选择常量，复用预声明类型。object 的 type/value 身份归一到同一声明；普通 value 接收者
仍走字段或 callable 路径。private 越界使用 L0154，不存在的关联常量使用 L0080，接口常量不继承。
Enum 的已声明 companion 常量不再被名称阶段误报为不存在的 case；已有 case 解析路径保留。
成功选择的内部 use 索引参与 callable trial 回滚，用于将读取分类为 Temporary，而非字段 Place。
validated constant capability 尚待 SPEC-0026 后续切片；跨文件入口未在此处接线。

## 单文件常量表达式与依赖

`checker/constants/expressions.rs` 按封闭表达式集合记录语法依赖，非法表达式使用 L0156，
companion initializer 中的 `this` 使用 L0153。普通类型错误优先；短路 RHS 仍参与资格与依赖。
`graph.rs` 按依赖顺序清除 initializer 类型缓存并复核前向操作数；失效依赖抑制后继环诊断。
迭代式 SCC 检测对每个环发布一次 L0157，主 Span 与其余 label 按声明位置稳定排序。
该内部图不对外发布 ConstValue；普通函数体中先前读取的 Deferred
尚未由此统一重查，不能视为完整的常量阶段能力。

`constant_value.rs` 实现不依赖单文件 symbol 表的纯值运算：整数保留精确 builtin 类型，
Char 保存 Unicode scalar，String 保存 compiler-owned UTF-8 bytes。
`constants/evaluation.rs` 按依赖顺序以显式栈求值，除零、余零、溢出与 signed MIN/-1 使用
L0158 定位运算符；短路 RHS 不进入值求值。失败依赖不发布值，也不追加后继求值诊断。
这些值目前仅保留在 checker 内部，尚非 ownership/codegen 可消费的公共阶段产物。

## 单文件 nullable when 事实

`TypedFile::nullable_whens` / `nullable_when` 发布 expression/subject 身份、来源类别、稳定 symbol、
entry 输入/剩余域、alternative 匹配/未匹配域及 body 的非空 subject view。Boolean/enum 保留
有限域原子；开放 nullable 域区分 null 与 non-null。native eligibility 仅标记 owned root/temporary
承载的 class/Box/Rc；字段和元素的 proof 不绑定后续重新求值的源码表达式。

condition 沿未匹配路径传递事实，entry body 独立检查；赋值和显式 inout 实参调用更新 source
版本，防止后续条件把旧 subject proof 重新关联到已改变的 binding。identity 不存入类型收窄表，
以免干扰嵌套或后续 if；descriptor 与版本表参与 callable trial snapshot/rollback。

这里描述单文件 `TypedFile` 产物；compilation-unit 类型产物尚未发布等价 nullable when descriptor。
所有权与 lowering 尚未消费这组新计划，不能将类型事实视为 native 能力已经交付。

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
实施范围。

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

对应覆盖位于 `compilation_unit_index` 与 `multifile_name_resolution` integration suites。

## Compilation-unit 类型链

`type_checking::compilation_unit` 先冻结全 unit 签名图，再检查 body：

1. 收集 classifier、type parameter、field、variant、top-level/member/companion callable 与常量签名；
2. 建立 canonical unit type table、能力图和 owner/callable generic 参数环境；
3. 按稳定 source/declaration 顺序检查 initializer 和 callable body；
4. 发布 source-qualified call、construction、projection、container、Rc、nullable、assignment、lambda、
   receiver 和 control-flow facts；
5. 无类型诊断且无阻塞 deferred fact 时产生 validated typed unit。

callable 选择使用已冻结签名与 overload candidate 隔离；codegen 不按名称重新选择 target。名义类型、
泛型、interface/default/override、静态委托、`Copyable`/`Transferable`、String、Box/Rc 和顺序容器共享
canonical type identity。ordinary class、value class、enum、interface 与 intrinsic 身份保持区分。

单文件与 unit checker 共享语义模型，但 unit facts 使用 source-qualified ID，不能拿单文件 ID 拼接成
多文件结果。

对应覆盖位于 `multifile_type_signatures`、`multifile_type_checking`、
`multifile_type_member_graph` 与 `multifile_type_capability_graph` integration suites。

## 核心不变量

- TypeRef、call target、member target 和 receiver 选择必须可追溯到唯一源码或显式 external identity。
- expected type 只能按语义允许的方向约束 child；不能用后端布局反向决定语言类型。
- 条件 `Copyable`、layout recipe 与 runtime type identity 使用同一 canonical 类型图。
- 诊断存在时可保留 recovery 数据，但 executable facts 必须原子失效。
