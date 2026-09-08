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
