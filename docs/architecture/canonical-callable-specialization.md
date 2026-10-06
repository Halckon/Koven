# Callable 的只读 canonical 类型替换

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改泛型 helper 的 Function 类型替换时 · **唯一真源**：两入口 concrete type resolver 与直接测试

单文件与 compilation-unit resolver 对 Function 的有序参数和返回类型递归替换，保留参数
mode 与 `move_only`。结果只查找 frontend 类型表已有的 exact canonical identity；缺失目标
返回结构化 `MissingFact`，后端不 intern 或扩大 frontend arena。

单文件 `TypeTable::find` 复用两类型表共有的 canonical 查询核心。公开的
`UnitFunctionParameterType::new` 仅组合已有类型身份与模式，供结构查询使用，不创建类型身份，
也不授予构造 validated typed/ownership 产物的权限。

顺序容器返回类型沿各入口既有支持边界替换；unit 的 direct-element、nested nominal 拒绝、
recipe/preflight 和实例预算机制保留。模式、move 标签、Array 返回及缺失 canonical 身份的
直接测试与相关 planner/lowering 回归见
[验收收据](../development/evidence/runtime-constructor-0279/canonical-callables/receipt.json)。
这组事实不证明 helper concrete ABI、source 构造器或 native 链路已经完成。
