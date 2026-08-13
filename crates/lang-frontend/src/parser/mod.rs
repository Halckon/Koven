//! Koven v1 的 Pratt 表达式 Parser 与具体索引式 AST。

mod engine;

use std::{error::Error, fmt, thread};

use crate::{
    ast::{AstError, AstFile, ExpressionId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticCodeError, DiagnosticError},
    lexer::LexedFile,
    source::{SourceError, SourceId, SourceMap, Span},
};

/// 一份表达式解析使用的具体索引式 AST。
pub type ExpressionAst = AstFile<(), (), Expression, TypeRef>;

/// 从一份确定性词法产物解析唯一独立表达式。
///
/// 用户语法错误保留在返回产物中；source identity、AST、诊断模型或实现资源边界失败才返回
/// 具体内部错误。资源边界是编译器实现保护，不是 Koven 语法拒绝规则。
pub fn parse_expression(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedExpression, ParserInternalError> {
    // Pratt、分组与 TypeRef 都天然递归。固定隔离栈避免调用线程的栈配置改变结果；统一
    // 递归预算则在耗尽该实现资源前受控返回错误，而不是让用户输入触发栈溢出。
    thread::scope(|scope| {
        thread::Builder::new()
            .name("koven-expression-parser".to_owned())
            .stack_size(PARSER_STACK_SIZE)
            .spawn_scoped(scope, || engine::parse(sources, lexed))
            .map_err(|error| ParserInternalError::ParserThread(error.kind()))?
            .join()
            .map_err(|_| ParserInternalError::ParserThreadPanicked)?
    })
}

const PARSER_STACK_SIZE: usize = 32 * 1024 * 1024;
// 该值是实现安全预算，不是 Koven 语法语义；超过预算由 ParserInternalError 明确报告。
pub(crate) const MAX_RECURSION_DEPTH: usize = 1024;

/// 拥有具体 AST、根节点与两阶段有序诊断的解析产物。
#[derive(Debug)]
pub struct ParsedExpression {
    pub(crate) ast: ExpressionAst,
    pub(crate) root: ExpressionId,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ParsedExpression {
    /// 返回产物关联的源码身份。
    #[must_use]
    pub const fn source_id(&self) -> SourceId {
        self.ast.source_id()
    }

    /// 返回只读具体 AST。
    #[must_use]
    pub const fn ast(&self) -> &ExpressionAst {
        &self.ast
    }

    /// 返回独立表达式根节点 ID。
    #[must_use]
    pub const fn root(&self) -> ExpressionId {
        self.root
    }

    /// 返回 Lexer 与 Parser 诊断的确定性合并全序。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
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
    /// 基本位置实参调用。
    Call {
        /// 被调用表达式。
        callee: ExpressionId,
        /// 位置实参。
        arguments: Vec<ExpressionId>,
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
    /// 十进制整数。
    Integer,
    /// 十进制浮点数。
    Float,
    /// 单 scalar `Char`。
    Char,
    /// 布尔值。
    Boolean(bool),
    /// `null`。
    Null,
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
        /// 参数类型。
        parameters: Vec<TypeRefId>,
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

/// Parser 内部边界失败；不表示 Koven 用户语法错误。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParserInternalError {
    /// Source identity 或范围不变量失败。
    Source(SourceError),
    /// 索引式 AST 不变量失败。
    Ast(AstError),
    /// 生产错误码目录不变量失败。
    DiagnosticCode(DiagnosticCodeError),
    /// 结构化诊断构造或排序不变量失败。
    Diagnostic(DiagnosticError),
    /// 词法产物不满足公开 Lexer 不变量。
    InvalidLexemeStream,
    /// 无法创建隔离 Parser 递归栈的工作线程。
    ParserThread(std::io::ErrorKind),
    /// Parser 工作线程因编译器缺陷而 panic。
    ParserThreadPanicked,
    /// 输入超过当前实现可安全处理的内部递归预算。
    NestingLimitExceeded {
        /// 当前实现允许的内部递归预算单位。
        limit: usize,
    },
}

impl fmt::Display for ParserInternalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "parser source error: {error}"),
            Self::Ast(error) => write!(formatter, "parser AST error: {error}"),
            Self::DiagnosticCode(error) => {
                write!(formatter, "parser diagnostic code error: {error}")
            }
            Self::Diagnostic(error) => write!(formatter, "parser diagnostic error: {error}"),
            Self::InvalidLexemeStream => {
                formatter.write_str("parser received an invalid lexeme stream")
            }
            Self::ParserThread(kind) => {
                write!(formatter, "failed to create parser worker thread: {kind:?}")
            }
            Self::ParserThreadPanicked => formatter.write_str("parser worker thread panicked"),
            Self::NestingLimitExceeded { limit } => {
                write!(
                    formatter,
                    "parser exceeds the implementation recursion budget of {limit} units"
                )
            }
        }
    }
}

impl Error for ParserInternalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Ast(error) => Some(error),
            Self::DiagnosticCode(error) => Some(error),
            Self::Diagnostic(error) => Some(error),
            Self::InvalidLexemeStream
            | Self::ParserThread(_)
            | Self::ParserThreadPanicked
            | Self::NestingLimitExceeded { .. } => None,
        }
    }
}

impl From<SourceError> for ParserInternalError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<AstError> for ParserInternalError {
    fn from(error: AstError) -> Self {
        Self::Ast(error)
    }
}

impl From<DiagnosticCodeError> for ParserInternalError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}

impl From<DiagnosticError> for ParserInternalError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}
