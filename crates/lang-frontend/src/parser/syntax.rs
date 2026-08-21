use crate::{
    ast::{AstFile, ExpressionId, ItemId, StatementId, TypeRefId},
    source::Span,
};

/// Parser 各入口共享的具体索引式 AST。
pub type SyntaxAst = AstFile<Item, Statement, Expression, TypeRef>;

/// 表达式解析使用的具体索引式 AST；保留旧名称作为兼容别名。
pub type ExpressionAst = SyntaxAst;
/// 恢复可见的源码名称。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameMarker {
    /// 一个实际的普通 Identifier token。
    Present(Span),
    /// 没有消费名称 token；范围必须为空。
    Missing(Span),
    /// 为名称位置实际消费的非空错误区域。
    Error(Span),
}

/// 变量声明的可变性。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VariableKind {
    /// `val`。
    Val,
    /// `var`。
    Var,
}

/// class-family 与声明 wrapper 保存的显式可见性。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibilityModifier {
    /// `public`。
    Public(Span),
    /// `internal`。
    Internal(Span),
    /// `private`。
    Private(Span),
}

/// 一条声明前缀的源码修饰符；顺序已经由 Parser 验证。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeclarationModifiers {
    /// 可选的唯一 visibility。
    pub visibility: Option<VisibilityModifier>,
    /// 实例函数可选的 `override`。
    pub override_span: Option<Span>,
}

/// class-family 声明种类及其真实关键字范围。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassifierKind {
    /// `value class`。
    ValueClass {
        /// 真实 `value`。
        value_span: Span,
        /// 真实 `class`；缺失恢复时为空范围。
        class_span: Span,
    },
    /// `class`。
    Class {
        /// 真实 `class`。
        class_span: Span,
    },
    /// `interface`。
    Interface {
        /// 真实 `interface`。
        interface_span: Span,
    },
    /// `enum class`。
    EnumClass {
        /// 真实 `enum`。
        enum_span: Span,
        /// 真实 `class`；缺失恢复时为空范围。
        class_span: Span,
    },
    /// 具名 `object`。
    Object {
        /// 真实 `object`。
        object_span: Span,
    },
}

/// class/value class 主构造器中的一个存储字段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassField {
    /// 字段完整范围。
    pub span: Span,
    /// 可选可见性。
    pub visibility: Option<VisibilityModifier>,
    /// `val` / `var`。
    pub kind: VariableKind,
    /// 真实 `val` / `var` token。
    pub keyword_span: Span,
    /// 字段名称或恢复 marker。
    pub name: NameMarker,
    /// 名称后的 `:`；恢复插入时为空范围。
    pub colon_span: Span,
    /// 字段类型或错误 TypeRef。
    pub type_ref: TypeRefId,
}

/// class/value class 的显式主构造器。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimaryConstructor {
    /// 真实 `(`。
    pub left_paren_span: Span,
    /// 源码顺序的存储字段。
    pub fields: Vec<ClassField>,
    /// 真实 `)`；缺失恢复时为 `None`。
    pub right_paren_span: Option<Span>,
}

/// 一个源码有序的 supertype entry。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupertypeEntry {
    /// entry 完整范围。
    pub span: Span,
    /// 唯一 TypeRef。
    pub type_ref: TypeRefId,
    /// ordinary class 可选的接口委托子句。
    pub delegation: Option<DelegationClause>,
}

/// `Interface by field` 的 Phase 1 源码结构。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DelegationClause {
    /// 子句从 `by` 到最后真实目标 token 的范围。
    pub span: Span,
    /// 上下文软关键字 `by` 的真实 Identifier token。
    pub by_span: Span,
    /// 委托目标字段名称或恢复 marker。
    pub target: NameMarker,
}

/// enum 变体的一个关联数据参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumVariantParameter {
    /// 参数完整范围。
    pub span: Span,
    /// 参数名称或恢复 marker。
    pub name: NameMarker,
    /// 真实 `:`；恢复插入时为空范围。
    pub colon_span: Span,
    /// 参数类型或错误 TypeRef。
    pub type_ref: TypeRefId,
}

/// 一个 enum class 变体。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumVariant {
    /// 变体完整范围。
    pub span: Span,
    /// 变体名称或恢复 marker。
    pub name: NameMarker,
    /// 可选关联数据参数列表的 `(`。
    pub left_paren_span: Option<Span>,
    /// 源码顺序的关联数据参数。
    pub parameters: Vec<EnumVariantParameter>,
    /// 真实 `)`；无参数列表或缺失恢复时为 `None`。
    pub right_paren_span: Option<Span>,
}

/// class-family body 的源码结构。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifierBody {
    /// 真实 `{`。
    pub left_brace_span: Span,
    /// enum 专用的源码顺序变体；其他 classifier 为空。
    pub variants: Vec<EnumVariant>,
    /// enum 变体区与成员区之间的真实 `;`。
    pub enum_member_delimiter_span: Option<Span>,
    /// 源码顺序的 member Item。
    pub members: Vec<ItemId>,
    /// 真实 `}`；缺失恢复时为 `None`。
    pub right_brace_span: Option<Span>,
}

/// 一个 class-family Item 的完整 payload。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassifierDeclaration {
    /// classifier 种类与关键字。
    pub kind: ClassifierKind,
    /// 声明名称。
    pub name: NameMarker,
    /// 源码顺序的类型参数。
    pub type_parameters: Vec<TypeParameter>,
    /// `<...>` 合成范围。
    pub type_parameter_list_span: Option<Span>,
    /// class/value class 的可选主构造器。
    pub primary_constructor: Option<PrimaryConstructor>,
    /// supertype list 前的真实 `:`。
    pub supertype_colon_span: Option<Span>,
    /// 源码顺序的 supertypes。
    pub supertypes: Vec<SupertypeEntry>,
    /// 可选 body。
    pub body: Option<ClassifierBody>,
}

/// 一个 companion object Item 的完整 payload。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionObject {
    /// 真实 `companion`。
    pub companion_span: Span,
    /// 真实 `object`；缺失恢复时为空范围。
    pub object_span: Span,
    /// 唯一 body。
    pub body: ClassifierBody,
}

/// 具名函数的互斥 body 形态。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionBody {
    /// 没有语法 body。
    Absent,
    /// `= expression` body。
    Expression {
        /// 真实 `=` token 范围。
        equals_span: Span,
        /// body 表达式。
        expression: ExpressionId,
    },
    /// `{ ... }` block body。
    Block(StatementId),
}

/// 具名函数返回标注来源与 body 的封闭组合。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionForm {
    /// 省略返回标注且没有语法 body；返回类型固定为 `Unit`。
    ImplicitUnitAbsent,
    /// 省略返回标注的 block body；返回类型固定为 `Unit`。
    ImplicitUnitBlock(StatementId),
    /// 已提交显式返回标注（包括缺失标注的定点恢复）。
    Explicit {
        /// 返回类型前的真实 `:`；恢复插入时为空范围。
        colon_span: Span,
        /// 显式返回类型或恢复建立的错误 TypeRef。
        type_ref: TypeRefId,
        /// 显式分支的互斥 body 形态。
        body: FunctionBody,
    },
}

/// block 中按源码顺序保存的 statement payload。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    /// 恢复过程中显式插入的错误 statement。
    Error,
    /// 顺序拥有 child statement ID 的 block。
    Block {
        /// 源码顺序的 block element。
        elements: Vec<StatementId>,
    },
    /// Lambda 专用的有序 body；最后一个 expression statement 提供尾值。
    LambdaBody {
        /// 源码顺序的 lambda body element。
        elements: Vec<StatementId>,
    },
    /// `if` / `when` 专用的有序 body；是否读取尾表达式由拥有它的控制表达式上下文决定。
    ControlBody {
        /// 源码顺序的 control body element。
        elements: Vec<StatementId>,
    },
    /// 只引用既有简单变量 Item 的局部声明。
    LocalVariable {
        /// 对应的 `Item::Variable`。
        declaration: ItemId,
    },
    /// block 或 lambda body 中的局部 `val` 解构。
    LocalDestructuring {
        /// 真实 `val` 关键字范围。
        val_span: Span,
        /// 真实 `(` 范围。
        left_paren_span: Span,
        /// 源码顺序的 binding marker。
        bindings: Vec<NameMarker>,
        /// 真实 `)`；缺失时不伪造范围。
        right_paren_span: Option<Span>,
        /// 真实 `=`；缺失时不伪造范围。
        equals_span: Option<Span>,
        /// 唯一 initializer 表达式。
        initializer: ExpressionId,
    },
    /// `while` statement。
    While {
        /// 真实 `while` token。
        keyword_span: Span,
        /// 条件表达式。
        condition: ExpressionId,
        /// 唯一普通 block body。
        body: StatementId,
    },
    /// `for` statement。
    For {
        /// 真实 `for` token。
        keyword_span: Span,
        /// 名称或解构 binding。
        binding: ForBinding,
        /// 真实 `in` token；恢复插入时为空范围。
        in_span: Span,
        /// 只求值一次的 source 表达式。
        source: ExpressionId,
        /// 唯一普通 block body。
        body: StatementId,
    },
    /// `loop` statement。
    Loop {
        /// 真实 `loop` token。
        keyword_span: Span,
        /// 唯一普通 block body。
        body: StatementId,
    },
    /// expression statement。
    Expression {
        /// 对应表达式。
        expression: ExpressionId,
    },
}

/// `for` header 中的源码 binding。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForBinding {
    /// 单一名称；错误恢复可保存 [`NameMarker::Missing`] / [`NameMarker::Error`]。
    Name(NameMarker),
    /// 完整解构 binding；`_` 作为普通真实名称范围保留，语义层解释为丢弃。
    Destructuring {
        /// 真实 `(`。
        left_paren_span: Span,
        /// 源码顺序的名称或恢复 marker。
        names: Vec<NameMarker>,
        /// 真实 `)`；缺失时不伪造。
        right_paren_span: Option<Span>,
    },
}

/// `when` entry 中的条件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WhenCondition {
    /// 普通表达式条件。
    Expression(ExpressionId),
    /// `is` / `!is` 类型条件。
    TypeTest {
        /// 真实运算符范围。
        operator_span: Span,
        /// 是否为否定形式。
        negated: bool,
        /// 目标类型。
        type_ref: TypeRefId,
    },
    /// `in` / `!in` 包含条件。
    Contains {
        /// 真实运算符范围。
        operator_span: Span,
        /// 是否为否定形式。
        negated: bool,
        /// 右侧容器表达式。
        expression: ExpressionId,
    },
}

/// 一个源码有序的 `when` entry。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhenEntry {
    /// 从首条件或 `else` 到 body 结束的范围。
    pub span: Span,
    /// 非 `else` entry 的一个或多个条件。
    pub conditions: Vec<WhenCondition>,
    /// `else` entry 的真实关键字范围。
    pub else_span: Option<Span>,
    /// 真实 `->`；恢复插入时为空范围。
    pub arrow_span: Span,
    /// 唯一 control body。
    pub body: StatementId,
}

/// 一个函数类型参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameter {
    /// 完整参数范围。
    pub span: Span,
    /// 参数名称或恢复 marker。
    pub name: NameMarker,
    /// 可选上界前的 `:`。
    pub colon_span: Option<Span>,
    /// 可选的唯一上界。
    pub bound: Option<TypeRefId>,
}

/// 一个函数值参数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueParameter {
    /// 完整参数范围。
    pub span: Span,
    /// 声明侧显式参数模式；缺失表示按值参数。
    pub mode_marker: Option<ParameterModeMarker>,
    /// 参数名称或恢复 marker。
    pub name: NameMarker,
    /// 名称后的 `:`；恢复插入时可为空范围。
    pub colon_span: Span,
    /// 参数类型或显式错误 TypeRef。
    pub type_ref: TypeRefId,
}

/// callable 参数契约的显式源码 marker。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterModeMarker {
    /// `borrow` 关键字范围。
    Borrow(Span),
    /// 声明侧 `inout` 或调用点 `&` 的真实范围。
    Inout(Span),
}

/// 函数类型中内嵌的一个参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionTypeParameter {
    /// 从显式 marker（若有）或类型起点到类型结束的完整范围。
    pub span: Span,
    /// 声明侧显式参数模式；缺失表示按值参数。
    pub mode_marker: Option<ParameterModeMarker>,
    /// 唯一参数类型。
    pub type_ref: TypeRefId,
}

/// 命名实参前缀的封闭源码表示。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamedArgumentPrefix {
    /// 真实参数名范围。
    pub name_span: Span,
    /// 真实 `=` 范围。
    pub equals_span: Span,
}

/// 调用表达式中按源码顺序内嵌的一个实参。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallArgument {
    /// 从名称、模式或值的最早起点到最后实际消费位置的范围。
    pub span: Span,
    /// 可选命名前缀。
    pub named_prefix: Option<NamedArgumentPrefix>,
    /// 调用点显式模式；缺失表示未标注实参。
    pub mode_marker: Option<ParameterModeMarker>,
    /// 唯一实参值表达式。
    pub value: ExpressionId,
}

/// 独立声明 payload；子节点只通过 typed ID 连接。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// 恢复过程中显式插入的错误声明。
    Error,
    /// 为既有声明或 class-family member 保存显式修饰符，不复制 child payload。
    Modified {
        /// 已验证顺序的修饰符。
        modifiers: DeclarationModifiers,
        /// 被修饰的唯一声明。
        declaration: ItemId,
    },
    /// `val` / `var` 声明。
    Variable {
        /// 声明可变性。
        kind: VariableKind,
        /// 声明名称。
        name: NameMarker,
        /// 可选类型标注的 `:`。
        colon_span: Option<Span>,
        /// 可选显式类型。
        type_ref: Option<TypeRefId>,
        /// initializer 前的 `=`；恢复插入时可为空范围。
        equals_span: Span,
        /// initializer 或显式错误表达式。
        initializer: ExpressionId,
    },
    /// 固定以 `const val` 开始的常量声明。
    Constant {
        /// `const` token。
        const_span: Span,
        /// 正常 `val` token或缺失/错误 marker。
        val_marker: NameMarker,
        /// 声明名称。
        name: NameMarker,
        /// 可选类型标注的 `:`。
        colon_span: Option<Span>,
        /// 可选显式类型。
        type_ref: Option<TypeRefId>,
        /// initializer 前的 `=`；恢复插入时可为空范围。
        equals_span: Span,
        /// initializer 或显式错误表达式。
        initializer: ExpressionId,
    },
    /// 具名函数声明。
    Function {
        /// 函数名称。
        name: NameMarker,
        /// 源码顺序的类型参数。
        type_parameters: Vec<TypeParameter>,
        /// `<...>` 的合成范围；没有类型参数表时为 `None`。
        type_parameter_list_span: Option<Span>,
        /// 源码顺序的值参数。
        parameters: Vec<ValueParameter>,
        /// 返回标注来源与 body 的封闭组合。
        form: FunctionForm,
    },
    /// 顶层 class-family 声明。
    Classifier(Box<ClassifierDeclaration>),
    /// `companion object` 关联命名空间。
    Companion(Box<CompanionObject>),
}

/// 具体表达式 payload；子节点只通过 typed ID 连接。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expression {
    /// 恢复过程中显式插入的错误节点。
    Error,
    /// 普通名称；拼写由节点 `Span` 回查。
    Name,
    /// `this`。
    This,
    /// 标量字面量。
    Literal(LiteralKind),
    /// 显式括号分组。
    Group {
        /// 被括号包围的表达式。
        expression: ExpressionId,
    },
    /// 分段字符串。
    String {
        /// 按源码顺序排列的文本与插值。
        parts: Vec<StringPart>,
    },
    /// 普通或显式 `move` lambda literal。
    Lambda {
        /// 可选的真实 `move` token 范围。
        move_span: Option<Span>,
        /// 严格 header 中源码顺序的真实参数名称范围。
        parameters: Vec<Span>,
        /// 真实 `->`；`None` 精确表示 header 缺席。
        arrow_span: Option<Span>,
        /// 唯一对应的 [`Statement::LambdaBody`]。
        body: StatementId,
    },
    /// Kotlin 风格条件表达式。
    If {
        /// 真实 `if` token。
        keyword_span: Span,
        /// 条件表达式。
        condition: ExpressionId,
        /// then control body。
        then_branch: StatementId,
        /// 真实 `else` token；缺失时为 `None`。
        else_span: Option<Span>,
        /// else control body 或嵌套 `else if`。
        else_branch: Option<StatementId>,
    },
    /// Kotlin 风格多分支条件表达式。
    When {
        /// 真实 `when` token。
        keyword_span: Span,
        /// 可选 subject。
        subject: Option<ExpressionId>,
        /// 源码顺序的 entries。
        entries: Vec<WhenEntry>,
    },
    /// 最近 callable 的返回表达式。
    Return {
        /// 真实 `return` token。
        keyword_span: Span,
        /// 同一逻辑行上的可选返回值。
        value: Option<ExpressionId>,
    },
    /// 最近 loop 的退出表达式。
    Break {
        /// 真实 `break` token。
        keyword_span: Span,
    },
    /// 最近 loop 的继续表达式。
    Continue {
        /// 真实 `continue` token。
        keyword_span: Span,
    },
    /// 接口默认方法消歧义 receiver。
    SuperMember {
        /// 真实 `super` token。
        keyword_span: Span,
        /// `<...>` 内接口类型。
        interface: TypeRefId,
        /// 真实 `.`；恢复插入时为空范围。
        dot_span: Span,
        /// 成员名称；恢复插入时可为空范围。
        name_span: Span,
    },
    /// 前缀表达式。
    Prefix {
        /// 前缀运算符语义。
        operator: PrefixOperator,
        /// 运算符原始范围。
        operator_span: Span,
        /// 操作数。
        operand: ExpressionId,
    },
    /// 类型转换。
    Cast {
        /// 被转换表达式。
        expression: ExpressionId,
        /// `as` / `as?`。
        operator: CastOperator,
        /// 运算符原始范围。
        operator_span: Span,
        /// 目标类型。
        type_ref: TypeRefId,
    },
    /// 右侧仍是表达式的二元运算。
    Binary {
        /// 左操作数。
        left: ExpressionId,
        /// 运算符语义。
        operator: BinaryOperator,
        /// 运算符原始范围。
        operator_span: Span,
        /// 右操作数。
        right: ExpressionId,
    },
    /// `is` / `!is` 类型测试。
    TypeTest {
        /// 被测试表达式。
        expression: ExpressionId,
        /// 是否为否定测试。
        negated: bool,
        /// 运算符原始范围。
        operator_span: Span,
        /// 被测试类型。
        type_ref: TypeRefId,
    },
    /// 赋值表达式；目标合法性留给 Phase 2。
    Assignment {
        /// 语法左侧。
        target: ExpressionId,
        /// 赋值种类。
        operator: AssignmentOperator,
        /// 运算符原始范围。
        operator_span: Span,
        /// 语法右侧。
        value: ExpressionId,
    },
    /// 普通或空安全成员访问。
    Member {
        /// receiver。
        receiver: ExpressionId,
        /// `.` 或 `?.` 范围。
        operator_span: Span,
        /// 成员名称范围。
        name_span: Span,
        /// 是否为空安全访问。
        safe: bool,
    },
    /// 普通或 typed 实参调用。
    Call {
        /// 被调用表达式。
        callee: ExpressionId,
        /// 显式调用点类型实参。
        type_arguments: Vec<TypeRefId>,
        /// `<...>` 合成范围；普通调用为 `None`。
        type_arguments_span: Option<Span>,
        /// 源码顺序的实参。
        arguments: Vec<CallArgument>,
    },
    /// 单表达式索引。
    Index {
        /// receiver。
        receiver: ExpressionId,
        /// 唯一 key 表达式。
        index: ExpressionId,
    },
    /// postfix 非空断言。
    NonNullAssert {
        /// 被断言表达式。
        operand: ExpressionId,
        /// `!!` 范围。
        operator_span: Span,
    },
    /// postfix `?` 错误值传播。
    Propagate {
        /// 被传播的 `Result` 表达式；类型约束留给 Phase 2。
        value: ExpressionId,
        /// 真实 `?` 范围。
        question_span: Span,
    },
    /// 未绑定或绑定 callable reference。
    CallableReference {
        /// `None` 表示 `::name`，否则表示 `receiver::name`。
        receiver: Option<ExpressionId>,
        /// `::` 范围。
        operator_span: Span,
        /// 引用名称范围。
        name_span: Span,
    },
}

/// 标量字面量类别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiteralKind {
    /// 十进制整数及规范化后缀身份。
    Integer(IntegerLiteralKind),
    /// 十进制实数及规范化后缀身份。
    Float(FloatLiteralKind),
    /// 单 scalar `Char`。
    Char,
    /// 布尔值。
    Boolean(bool),
    /// `null`。
    Null,
}

/// Parser AST 中稳定的整数字面量身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerLiteralKind {
    /// 无后缀整数，类型由 expected type 或默认规则决定。
    Unsuffixed,
    /// `L` 固定 `Long`。
    Long,
    /// `u` / `U` 无符号整数约束。
    Unsigned,
    /// `uL` / `UL` 固定 `ULong`。
    UnsignedLong,
}

/// Parser AST 中稳定的实数字面量身份。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatLiteralKind {
    /// 无后缀实数，固定 `Double`。
    Double,
    /// `f` / `F`，固定 `Float`。
    Float,
}

/// 字符串内部的一个可观察分段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringPart {
    /// 非空原始 text 范围。
    Text(Span),
    /// `${...}` 插值。
    Interpolation {
        /// 包含 `${` 与匹配 `}` 的合成范围。
        span: Span,
        /// 插值根表达式。
        expression: ExpressionId,
    },
    /// Lexer 已诊断且 Parser 已消费的非法字符串片段。
    Error(Span),
}

/// v0.6 的三个前缀运算符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefixOperator {
    /// `!`。
    Not,
    /// 一元 `+`。
    Plus,
    /// 一元 `-`。
    Minus,
}

/// 类型转换运算符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastOperator {
    /// `as`。
    As,
    /// `as?`。
    SafeAs,
}

/// 右侧为表达式的二元运算符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOperator {
    /// `*`。
    Multiply,
    /// `/`。
    Divide,
    /// `%`。
    Remainder,
    /// `+`。
    Add,
    /// `-`。
    Subtract,
    /// `..`。
    InclusiveRange,
    /// `..<`。
    ExclusiveRange,
    /// 唯一中缀软词 `to`。
    To,
    /// `?:`。
    Elvis,
    /// `in`。
    In,
    /// `!in`。
    NotIn,
    /// `<`。
    Less,
    /// `>`。
    Greater,
    /// `<=`。
    LessEqual,
    /// `>=`。
    GreaterEqual,
    /// `==`。
    Equal,
    /// `!=`。
    NotEqual,
    /// `&&`。
    LogicalAnd,
    /// `||`。
    LogicalOr,
}

/// 赋值运算符。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignmentOperator {
    /// `=`。
    Assign,
    /// `+=`。
    AddAssign,
    /// `-=`。
    SubtractAssign,
    /// `*=`。
    MultiplyAssign,
    /// `/=`。
    DivideAssign,
    /// `%=`。
    RemainderAssign,
}

/// 具体类型引用 payload。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeRef {
    /// 恢复过程中显式插入的错误类型。
    Error,
    /// 限定名类型；只有末段可携带类型实参。
    Qualified {
        /// 路径各段。
        segments: Vec<TypePathSegment>,
        /// 末尾 `?` 范围；存在即表示 nullable。
        nullable_span: Option<Span>,
    },
    /// 函数类型。
    Function {
        /// 可选 `move` 标记范围。
        move_span: Option<Span>,
        /// 带 callable 契约的源码顺序参数。
        parameters: Vec<FunctionTypeParameter>,
        /// `->` 范围；恢复时可为空。
        arrow_span: Span,
        /// 返回类型。
        return_type: TypeRefId,
    },
}

/// 限定类型路径的一段。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypePathSegment {
    /// 名称范围。
    pub name_span: Span,
    /// 仅末段允许的递归类型实参。
    pub arguments: Vec<TypeRefId>,
}
