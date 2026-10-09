# Parser AST 工程合同

> **性质**：现行编译器工程合同 · **状态**：current · **读取时机**：修改声明、statement 或 lambda 的 AST 表示时 · **唯一真源**：本页所列内部表示与构造不变量

本页约束编译器内部表示，不定义新的语言接受形式。语言语义、诊断与源码范围仍以
[Guide](../guide/README.md) 为准；当前代码事实见[前端架构](../architecture/source-and-syntax.md)。
未迁入的工程段落继续留在 Guide，不能据此推定本目录已经覆盖全部编译器合同。

## 声明 Root 与索引引用

独立入口返回一个带 `SourceId` 的索引式 declaration root。声明中的 initializer / expression
body 引用现有 expression ID；显式类型标注、显式返回标注、参数类型、泛型上界和调用点
类型实参都引用现有 TypeRef ID，不得把源码片段或解析后的类型名称复制成另一套无 `Span`
字符串模型。省略返回标注必须用下文 `FunctionForm` 这类同时封闭返回来源与 body 的状态表示，
不能伪造 `Unit` TypeRef、冒充存在的 `:`，也不能丢失“显式 `: Unit`”与“省略标注”的源码
差异。

## 函数返回来源与 Body 封闭表示

函数 item 的返回来源与 body 必须是同一个封闭状态，至少等价于：

```text
FunctionForm::ImplicitUnitAbsent
FunctionForm::ImplicitUnitBlock(StatementId)
FunctionForm::Explicit {
    colon_span: Span,
    type_ref: TypeRefId,
    result_source: Option<FunctionResultSource>,
    body: FunctionBody,
}
```

`FunctionResultSource` 用封闭枚举区分 `Borrow(BorrowReturnSyntax)` 与
`Carrier(CarrierReturnSyntax)`，普通返回为 `None`。现行普通借用的 `BorrowReturnSyntax`
独立保存 `borrow`、`from` 与唯一参数/receiver 来源的真实 `Span`；目标类型仍复用
`type_ref`，不得把结果来源模式编码为可嵌套的 TypeRef。
局部显式借用以 `VariableKind::BorrowVal(Span)` 保留声明 marker。

`Carrier` 是首片声明前置的内部表示，保存真实 `from` 与来源，不伪造 `borrow` marker。
函数 item 的 `extension_receiver: Option<ExtensionReceiverSyntax>` 同样只保存 receiver 的
TypeRef ID 与分隔点 Span，显式 mode 复用 `DeclarationModifiers.receiver_mode`。
这些字段不启用候选语言语义；carrier 与扩展声明在 single/unit 类型阶段必须明确拒绝，
不能误交付为普通 owned callable。现行 Guide 与后续语义批准边界保持不变。

## Statement Table 与函数 Body

本节所说的“下述范围”由[Guide 的 Statement Span 表](../guide/06-blocks-control-flow.md#statement-astbody-表示与-span)唯一定义。

block AST 使用有 payload 的 statement table；所有 block element 都以有序
`StatementId` 保存，不把 element 混入 expression table，也不创建无 `Span` 的源码字符串：

- `Statement::Block { elements: Vec<StatementId> }` 表示独立、嵌套或函数体 block；
- `Statement::LocalVariable { declaration: ItemId }` 引用按[声明规则](../guide/05-declarations-callables.md)构造的 `val` /
  `var` item，禁止引用 `const val` 或 `fun` item；
- `Statement::Expression { expression: ExpressionId }` 引用既有 expression；
- `Statement::Error` 只覆盖本次实际消费的错误区域。

实现可采用可证明同样保持 typed ID、顺序与下述范围的等价枚举命名，但不能把 block 降为
`Vec<ExpressionId>`。函数 item 必须用一个合并的封闭 sum type 同时保存返回标注来源与 body，
至少等价于 `ImplicitUnitAbsent | ImplicitUnitBlock(StatementId) | Explicit {
colon_span: Span, type_ref: TypeRefId, result_source: Option<FunctionResultSource>, body: FunctionBody }`；其中显式分支的 `FunctionBody` 才可为
`Absent | Expression { equals_span: Span, expression: ExpressionId } | Block(StatementId)`。
表达式体必须继续精确保存[声明规则](../guide/05-declarations-callables.md)规定的真实 `=` token `Span`。
不得把返回标注和 body
暴露为可独立构造的字段，不能制造 `ImplicitUnit + Expression`、“双 body”或“半个显式标注”
状态，也不得为隐式 `Unit` 伪造 TypeRef / `:` Span。独立 block 入口返回带 `SourceId` 的
statement root。

## Lambda Payload 字段

AST 至少等价保存 `move_span: Option<Span>`、有序参数名称 Span、`arrow_span: Option<Span>`、
有序 `StatementId` body 和可判定的 tail expression。

## Lambda Header 与 Body 不变量

`arrow_span == None` 精确表示没有 header，此时参数必须为空；参数非空时必须存在真实
`arrow_span`，而“参数为空且有真实 `arrow_span`”唯一表示 `{ -> ... }`。lambda body ID 必须
指向 `Statement::LambdaBody`；该 variant 不能直接成为 Block / LambdaBody 的 element，也不能
成为孤儿。strict probe 失败时参数为空、arrow 为 `None`，失败 token 只能进入 body 的
Expression / Error statement。
