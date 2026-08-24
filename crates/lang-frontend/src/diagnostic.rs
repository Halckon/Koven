//! 结构化诊断模型、错误码目录与确定性聚合顺序。

use std::{error::Error, fmt};

use crate::source::{SourceError, SourceMap, Span};

/// 已发布的生产错误码目录。
///
/// 后续语言功能只能在分配正式语义错误码的 Spec 中向 [`ALL`] 增加条目。测试代码应通过
/// [`DiagnosticCodeCatalog::try_new`] 建立自己的目录，不能在这里注册样例编号。
pub mod codes {
    use super::{DiagnosticCodeCatalog, DiagnosticCodeError};

    pub(crate) const UNEXPECTED_CHARACTER: &str = "L0001";
    pub(crate) const RESERVED_WORD: &str = "L0002";
    pub(crate) const UNTERMINATED_BLOCK_COMMENT: &str = "L0003";
    pub(crate) const UNTERMINATED_STRING: &str = "L0004";
    pub(crate) const UNTERMINATED_INTERPOLATION: &str = "L0005";
    pub(crate) const INVALID_STRING_ESCAPE: &str = "L0006";
    pub(crate) const INVALID_CHAR_LITERAL: &str = "L0007";
    pub(crate) const INVALID_NUMERIC_LITERAL: &str = "L0008";
    pub(crate) const EXPECTED_EXPRESSION: &str = "L0009";
    pub(crate) const EXPECTED_CLOSING_DELIMITER: &str = "L0010";
    pub(crate) const EXPECTED_MEMBER_NAME: &str = "L0011";
    pub(crate) const NON_ASSOCIATIVE_CHAIN: &str = "L0012";
    pub(crate) const UNEXPECTED_TRAILING_TOKEN: &str = "L0013";
    pub(crate) const EXPECTED_TYPE_REFERENCE: &str = "L0014";
    pub(crate) const UNSUPPORTED_OPERATOR: &str = "L0015";
    pub(crate) const UNSUPPORTED_ARGUMENT_FORM: &str = "L0016";
    pub(crate) const EXPECTED_DECLARATION: &str = "L0017";
    pub(crate) const EXPECTED_DECLARATION_NAME: &str = "L0018";
    pub(crate) const EXPECTED_PARAMETER_NAME: &str = "L0019";
    pub(crate) const EXPECTED_INITIALIZER: &str = "L0020";
    pub(crate) const EXPECTED_RETURN_TYPE: &str = "L0021";
    pub(crate) const EXPECTED_VAL_AFTER_CONST: &str = "L0022";
    pub(crate) const EXPECTED_PARAMETER_COLON: &str = "L0023";
    pub(crate) const EXPECTED_LIST_ELEMENT: &str = "L0024";
    pub(crate) const EXPECTED_LIST_SEPARATOR: &str = "L0025";
    pub(crate) const UNSUPPORTED_TRAILING_COMMA: &str = "L0026";
    pub(crate) const UNSUPPORTED_PARAMETER_DEFAULT: &str = "L0027";
    pub(crate) const EXPECTED_BLOCK: &str = "L0028";
    pub(crate) const EXPECTED_BLOCK_ELEMENT: &str = "L0029";
    pub(crate) const UNSUPPORTED_BLOCK_ELEMENT: &str = "L0030";
    pub(crate) const EXPECTED_LAMBDA_BODY_ELEMENT: &str = "L0031";
    pub(crate) const UNSUPPORTED_LAMBDA_BODY_FORM: &str = "L0032";
    pub(crate) const EXPECTED_ARGUMENT_VALUE: &str = "L0033";
    pub(crate) const EXPECTED_ARGUMENT_SEPARATOR: &str = "L0034";
    pub(crate) const UNSUPPORTED_ARGUMENT_EMPTY_ELEMENT: &str = "L0035";
    pub(crate) const UNSUPPORTED_ARGUMENT_TRAILING_COMMA: &str = "L0036";
    pub(crate) const INVALID_ARGUMENT_MODE_ORDERING: &str = "L0037";
    pub(crate) const DUPLICATE_ARGUMENT_MODE: &str = "L0038";
    pub(crate) const DUPLICATE_PARAMETER_MODE: &str = "L0039";
    pub(crate) const EXPECTED_DESTRUCTURING_BINDING: &str = "L0040";
    pub(crate) const EXPECTED_DESTRUCTURING_SEPARATOR: &str = "L0041";
    pub(crate) const UNSUPPORTED_DESTRUCTURING_FORM: &str = "L0042";
    pub(crate) const UNSUPPORTED_DESTRUCTURING_CONTEXT: &str = "L0043";
    pub(crate) const UNSUPPORTED_DESTRUCTURING_TRAILING_COMMA: &str = "L0044";
    pub(crate) const EXPECTED_DESTRUCTURING_INITIALIZER_SEPARATOR: &str = "L0045";
    pub(crate) const EXPECTED_DESTRUCTURING_INITIALIZER: &str = "L0046";
    pub(crate) const EXPECTED_DECLARATION_SEPARATOR: &str = "L0047";
    pub(crate) const EXPECTED_PACKAGE_NAME: &str = "L0048";
    pub(crate) const EXPECTED_IMPORT_TARGET: &str = "L0049";
    pub(crate) const EXPECTED_IMPORT_ALIAS: &str = "L0050";
    pub(crate) const MISPLACED_PACKAGE_DIRECTIVE: &str = "L0051";
    pub(crate) const MISPLACED_IMPORT_DIRECTIVE: &str = "L0052";
    pub(crate) const EXPECTED_FILE_HEADER_SEPARATOR: &str = "L0053";
    pub(crate) const WILDCARD_IMPORT_ALIAS: &str = "L0054";
    pub(crate) const EXPECTED_CONDITION: &str = "L0055";
    pub(crate) const EXPECTED_CONTROL_BODY: &str = "L0056";
    pub(crate) const EXPECTED_ELSE_BRANCH: &str = "L0057";
    pub(crate) const EXPECTED_WHEN_ENTRY: &str = "L0058";
    pub(crate) const EXPECTED_WHEN_ARROW: &str = "L0059";
    pub(crate) const EXPECTED_LOOP_BODY: &str = "L0060";
    pub(crate) const EXPECTED_FOR_BINDING: &str = "L0061";
    pub(crate) const EXPECTED_FOR_IN: &str = "L0062";
    pub(crate) const EXPECTED_SUPER_INTERFACE: &str = "L0063";
    pub(crate) const EXPECTED_SUPER_MEMBER_SEPARATOR: &str = "L0064";
    pub(crate) const EXPECTED_WHEN_ENTRY_SEPARATOR: &str = "L0065";
    pub(crate) const EXPECTED_CLASS_KEYWORD: &str = "L0066";
    pub(crate) const EXPECTED_CLASSIFIER_NAME: &str = "L0067";
    pub(crate) const EXPECTED_CONSTRUCTOR_FIELD: &str = "L0068";
    pub(crate) const EXPECTED_CONSTRUCTOR_SEPARATOR: &str = "L0069";
    pub(crate) const EXPECTED_SUPERTYPE: &str = "L0070";
    pub(crate) const EXPECTED_MEMBER: &str = "L0071";
    pub(crate) const EXPECTED_MEMBER_SEPARATOR: &str = "L0072";
    pub(crate) const EXPECTED_ENUM_VARIANT: &str = "L0073";
    pub(crate) const EXPECTED_ENUM_VARIANT_SEPARATOR: &str = "L0074";
    pub(crate) const EXPECTED_ENUM_MEMBER_DELIMITER: &str = "L0075";
    pub(crate) const INVALID_DECLARATION_MODIFIER: &str = "L0076";
    pub(crate) const UNSUPPORTED_CLASS_FAMILY_FORM: &str = "L0077";
    pub(crate) const EXPECTED_DELEGATION_TARGET: &str = "L0078";
    pub(crate) const DUPLICATE_NAME: &str = "L0079";
    pub(crate) const UNRESOLVED_NAME: &str = "L0080";
    pub(crate) const NAME_USED_BEFORE_LOCAL: &str = "L0081";
    pub(crate) const BUILTIN_TYPE_ARGUMENTS: &str = "L0082";
    pub(crate) const CANNOT_INFER_TYPE: &str = "L0083";
    pub(crate) const TYPE_MISMATCH: &str = "L0084";
    pub(crate) const INVALID_OPERAND_TYPES: &str = "L0085";
    pub(crate) const RETURN_OUTSIDE_CALLABLE: &str = "L0086";
    pub(crate) const RETURN_SHAPE_MISMATCH: &str = "L0087";
    pub(crate) const MISSING_RETURN: &str = "L0088";
    pub(crate) const NO_COMMON_BRANCH_TYPE: &str = "L0089";
    pub(crate) const NUMERIC_LITERAL_OUT_OF_RANGE: &str = "L0090";
    pub(crate) const TYPE_ARGUMENT_ARITY: &str = "L0091";
    pub(crate) const INVALID_TYPE_BOUND: &str = "L0092";
    pub(crate) const TYPE_ARGUMENT_BOUND: &str = "L0093";
    pub(crate) const INTERFACE_RUNTIME_VALUE: &str = "L0094";
    pub(crate) const INVALID_SUPERTYPE: &str = "L0095";
    pub(crate) const INTERFACE_CYCLE: &str = "L0096";
    pub(crate) const DUPLICATE_CALLABLE_SHAPE: &str = "L0097";
    pub(crate) const CONCRETE_MEMBER_BODY: &str = "L0098";
    pub(crate) const INTERFACE_MEMBER_MISMATCH: &str = "L0099";
    pub(crate) const INVALID_OVERRIDE: &str = "L0100";
    pub(crate) const MISSING_INTERFACE_MEMBER: &str = "L0101";
    pub(crate) const DEFAULT_MEMBER_CONFLICT: &str = "L0102";
    pub(crate) const INVALID_DELEGATION_TARGET: &str = "L0103";
    pub(crate) const DELEGATE_INTERFACE_MISMATCH: &str = "L0104";
    pub(crate) const DELEGATION_MEMBER_CONFLICT: &str = "L0105";
    pub(crate) const INVALID_TYPE_TEST: &str = "L0106";
    pub(crate) const INVALID_WHEN_CONDITION: &str = "L0107";
    pub(crate) const DUPLICATE_WHEN_ELSE: &str = "L0108";
    pub(crate) const NON_FINAL_WHEN_ELSE: &str = "L0109";
    pub(crate) const DUPLICATE_WHEN_COVERAGE: &str = "L0110";
    pub(crate) const NON_EXHAUSTIVE_WHEN: &str = "L0111";
    pub(crate) const WHEN_BRANCH_TYPE: &str = "L0112";
    pub(crate) const INVALID_ENUM_PAYLOAD_ACCESS: &str = "L0113";
    pub(crate) const ENUM_CASE_TYPE_POSITION: &str = "L0114";
    pub(crate) const COPYABLE_TYPE_ARGUMENT_BOUND: &str = "L0115";
    pub(crate) const INFINITE_INLINE_LAYOUT: &str = "L0116";
    pub(crate) const INVALID_BOX_ARGUMENT: &str = "L0117";
    pub(crate) const DESTRUCTURING_ARITY: &str = "L0118";
    pub(crate) const NON_CALLABLE_TARGET: &str = "L0119";
    pub(crate) const INVALID_NAMED_ARGUMENT: &str = "L0120";
    pub(crate) const CALL_ARGUMENT_ARITY: &str = "L0121";
    pub(crate) const CALL_ARGUMENT_MODE: &str = "L0122";
    pub(crate) const NO_MATCHING_OVERLOAD: &str = "L0123";
    pub(crate) const AMBIGUOUS_CALL: &str = "L0124";
    pub(crate) const INVALID_CONTAINER_ELEMENT: &str = "L0125";
    pub(crate) const CANNOT_INFER_CONTAINER_ELEMENT: &str = "L0126";
    pub(crate) const INVALID_CONTAINER_CONSTRUCTION: &str = "L0127";
    pub(crate) const INVALID_CONTAINER_INDEX: &str = "L0128";
    pub(crate) const IMMUTABLE_CONTAINER_PLACE: &str = "L0129";
    pub(crate) const INVALID_CONTAINER_MEMBER: &str = "L0130";
    pub(crate) const USE_AFTER_MOVE: &str = "L0131";
    pub(crate) const PARTIAL_MOVE: &str = "L0132";
    pub(crate) const MOVE_FROM_BORROWED_BINDING: &str = "L0133";
    pub(crate) const IMMUTABLE_INOUT_PLACE: &str = "L0134";
    pub(crate) const LOAN_CONFLICT: &str = "L0135";
    pub(crate) const MOVE_FROM_CONTAINER_ELEMENT: &str = "L0136";
    pub(crate) const BORROWED_CLOSURE_ESCAPE: &str = "L0137";
    pub(crate) const ILLEGAL_OWNED_CAPTURE: &str = "L0138";
    pub(crate) const NON_TRANSFERABLE_DELIVERY: &str = "L0139";
    pub(crate) const GENERIC_CALL_INFERENCE: &str = "L0140";
    pub(crate) const TRANSFERABLE_TYPE_ARGUMENT_BOUND: &str = "L0141";
    pub(crate) const JUMP_OUTSIDE_LOOP: &str = "L0142";

    /// 已发布的生产错误码。
    pub const ALL: &[&str] = &[
        UNEXPECTED_CHARACTER,
        RESERVED_WORD,
        UNTERMINATED_BLOCK_COMMENT,
        UNTERMINATED_STRING,
        UNTERMINATED_INTERPOLATION,
        INVALID_STRING_ESCAPE,
        INVALID_CHAR_LITERAL,
        INVALID_NUMERIC_LITERAL,
        EXPECTED_EXPRESSION,
        EXPECTED_CLOSING_DELIMITER,
        EXPECTED_MEMBER_NAME,
        NON_ASSOCIATIVE_CHAIN,
        UNEXPECTED_TRAILING_TOKEN,
        EXPECTED_TYPE_REFERENCE,
        UNSUPPORTED_OPERATOR,
        UNSUPPORTED_ARGUMENT_FORM,
        EXPECTED_DECLARATION,
        EXPECTED_DECLARATION_NAME,
        EXPECTED_PARAMETER_NAME,
        EXPECTED_INITIALIZER,
        EXPECTED_RETURN_TYPE,
        EXPECTED_VAL_AFTER_CONST,
        EXPECTED_PARAMETER_COLON,
        EXPECTED_LIST_ELEMENT,
        EXPECTED_LIST_SEPARATOR,
        UNSUPPORTED_TRAILING_COMMA,
        UNSUPPORTED_PARAMETER_DEFAULT,
        EXPECTED_BLOCK,
        EXPECTED_BLOCK_ELEMENT,
        UNSUPPORTED_BLOCK_ELEMENT,
        EXPECTED_LAMBDA_BODY_ELEMENT,
        UNSUPPORTED_LAMBDA_BODY_FORM,
        EXPECTED_ARGUMENT_VALUE,
        EXPECTED_ARGUMENT_SEPARATOR,
        UNSUPPORTED_ARGUMENT_EMPTY_ELEMENT,
        UNSUPPORTED_ARGUMENT_TRAILING_COMMA,
        INVALID_ARGUMENT_MODE_ORDERING,
        DUPLICATE_ARGUMENT_MODE,
        DUPLICATE_PARAMETER_MODE,
        EXPECTED_DESTRUCTURING_BINDING,
        EXPECTED_DESTRUCTURING_SEPARATOR,
        UNSUPPORTED_DESTRUCTURING_FORM,
        UNSUPPORTED_DESTRUCTURING_CONTEXT,
        UNSUPPORTED_DESTRUCTURING_TRAILING_COMMA,
        EXPECTED_DESTRUCTURING_INITIALIZER_SEPARATOR,
        EXPECTED_DESTRUCTURING_INITIALIZER,
        EXPECTED_DECLARATION_SEPARATOR,
        EXPECTED_PACKAGE_NAME,
        EXPECTED_IMPORT_TARGET,
        EXPECTED_IMPORT_ALIAS,
        MISPLACED_PACKAGE_DIRECTIVE,
        MISPLACED_IMPORT_DIRECTIVE,
        EXPECTED_FILE_HEADER_SEPARATOR,
        WILDCARD_IMPORT_ALIAS,
        EXPECTED_CONDITION,
        EXPECTED_CONTROL_BODY,
        EXPECTED_ELSE_BRANCH,
        EXPECTED_WHEN_ENTRY,
        EXPECTED_WHEN_ARROW,
        EXPECTED_LOOP_BODY,
        EXPECTED_FOR_BINDING,
        EXPECTED_FOR_IN,
        EXPECTED_SUPER_INTERFACE,
        EXPECTED_SUPER_MEMBER_SEPARATOR,
        EXPECTED_WHEN_ENTRY_SEPARATOR,
        EXPECTED_CLASS_KEYWORD,
        EXPECTED_CLASSIFIER_NAME,
        EXPECTED_CONSTRUCTOR_FIELD,
        EXPECTED_CONSTRUCTOR_SEPARATOR,
        EXPECTED_SUPERTYPE,
        EXPECTED_MEMBER,
        EXPECTED_MEMBER_SEPARATOR,
        EXPECTED_ENUM_VARIANT,
        EXPECTED_ENUM_VARIANT_SEPARATOR,
        EXPECTED_ENUM_MEMBER_DELIMITER,
        INVALID_DECLARATION_MODIFIER,
        UNSUPPORTED_CLASS_FAMILY_FORM,
        EXPECTED_DELEGATION_TARGET,
        DUPLICATE_NAME,
        UNRESOLVED_NAME,
        NAME_USED_BEFORE_LOCAL,
        BUILTIN_TYPE_ARGUMENTS,
        CANNOT_INFER_TYPE,
        TYPE_MISMATCH,
        INVALID_OPERAND_TYPES,
        RETURN_OUTSIDE_CALLABLE,
        RETURN_SHAPE_MISMATCH,
        MISSING_RETURN,
        NO_COMMON_BRANCH_TYPE,
        NUMERIC_LITERAL_OUT_OF_RANGE,
        TYPE_ARGUMENT_ARITY,
        INVALID_TYPE_BOUND,
        TYPE_ARGUMENT_BOUND,
        INTERFACE_RUNTIME_VALUE,
        INVALID_SUPERTYPE,
        INTERFACE_CYCLE,
        DUPLICATE_CALLABLE_SHAPE,
        CONCRETE_MEMBER_BODY,
        INTERFACE_MEMBER_MISMATCH,
        INVALID_OVERRIDE,
        MISSING_INTERFACE_MEMBER,
        DEFAULT_MEMBER_CONFLICT,
        INVALID_DELEGATION_TARGET,
        DELEGATE_INTERFACE_MISMATCH,
        DELEGATION_MEMBER_CONFLICT,
        INVALID_TYPE_TEST,
        INVALID_WHEN_CONDITION,
        DUPLICATE_WHEN_ELSE,
        NON_FINAL_WHEN_ELSE,
        DUPLICATE_WHEN_COVERAGE,
        NON_EXHAUSTIVE_WHEN,
        WHEN_BRANCH_TYPE,
        INVALID_ENUM_PAYLOAD_ACCESS,
        ENUM_CASE_TYPE_POSITION,
        COPYABLE_TYPE_ARGUMENT_BOUND,
        INFINITE_INLINE_LAYOUT,
        INVALID_BOX_ARGUMENT,
        DESTRUCTURING_ARITY,
        NON_CALLABLE_TARGET,
        INVALID_NAMED_ARGUMENT,
        CALL_ARGUMENT_ARITY,
        CALL_ARGUMENT_MODE,
        NO_MATCHING_OVERLOAD,
        AMBIGUOUS_CALL,
        INVALID_CONTAINER_ELEMENT,
        CANNOT_INFER_CONTAINER_ELEMENT,
        INVALID_CONTAINER_CONSTRUCTION,
        INVALID_CONTAINER_INDEX,
        IMMUTABLE_CONTAINER_PLACE,
        INVALID_CONTAINER_MEMBER,
        USE_AFTER_MOVE,
        PARTIAL_MOVE,
        MOVE_FROM_BORROWED_BINDING,
        IMMUTABLE_INOUT_PLACE,
        LOAN_CONFLICT,
        MOVE_FROM_CONTAINER_ELEMENT,
        BORROWED_CLOSURE_ESCAPE,
        ILLEGAL_OWNED_CAPTURE,
        NON_TRANSFERABLE_DELIVERY,
        GENERIC_CALL_INFERENCE,
        TRANSFERABLE_TYPE_ARGUMENT_BOUND,
        JUMP_OUTSIDE_LOOP,
    ];

    /// 由集中定义创建生产错误码目录。
    ///
    /// # Errors
    ///
    /// 当源码中的目录条目格式无效或重复时返回具体错误。
    pub fn catalog() -> Result<DiagnosticCodeCatalog, DiagnosticCodeError> {
        DiagnosticCodeCatalog::try_new(ALL)
    }
}

/// 已由 [`DiagnosticCodeCatalog`] 验证的 `Ldddd` 错误码。
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticCode(u16);

impl fmt::Debug for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "L{:04}", self.0)
    }
}

/// 不可变、受检的错误码目录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticCodeCatalog {
    codes: Vec<DiagnosticCode>,
}

impl DiagnosticCodeCatalog {
    /// 校验并建立错误码目录。
    ///
    /// # Errors
    ///
    /// 条目不是精确的 ASCII `Ldddd` 格式，或同一错误码出现多次时返回具体错误。
    pub fn try_new(raw_codes: &[&str]) -> Result<Self, DiagnosticCodeError> {
        let mut codes = Vec::with_capacity(raw_codes.len());
        for raw in raw_codes {
            codes.push(parse_code(raw)?);
        }

        codes.sort_unstable_by_key(|code| code.0);
        if let Some(code) = codes
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(DiagnosticCodeError::DuplicateCode { code });
        }

        Ok(Self { codes })
    }

    /// 从目录解析一个已注册错误码。
    ///
    /// # Errors
    ///
    /// 输入格式无效，或格式正确但未在当前目录注册时返回具体错误。
    pub fn resolve(&self, raw: &str) -> Result<DiagnosticCode, DiagnosticCodeError> {
        let code = parse_code(raw)?;
        self.codes
            .binary_search_by_key(&code.0, |candidate| candidate.0)
            .map(|index| self.codes[index])
            .map_err(|_| DiagnosticCodeError::UnknownCode {
                code: raw.to_owned(),
            })
    }

    /// 返回目录中的错误码数量。
    #[must_use]
    pub fn len(&self) -> usize {
        self.codes.len()
    }

    /// 返回目录是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.codes.is_empty()
    }
}

/// 错误码目录校验或查询失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticCodeError {
    /// 输入不是精确的 ASCII `Ldddd` 格式。
    InvalidFormat {
        /// 被拒绝的原始文本。
        code: String,
    },
    /// 目录中出现重复编号。
    DuplicateCode {
        /// 重复的已验证错误码。
        code: DiagnosticCode,
    },
    /// 格式正确的编号未在目录注册。
    UnknownCode {
        /// 未注册的错误码文本。
        code: String,
    },
}

impl fmt::Display for DiagnosticCodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat { code } => {
                write!(formatter, "diagnostic code {code:?} is not ASCII Ldddd")
            }
            Self::DuplicateCode { code } => {
                write!(
                    formatter,
                    "diagnostic code {code} is registered more than once"
                )
            }
            Self::UnknownCode { code } => {
                write!(formatter, "diagnostic code {code:?} is not registered")
            }
        }
    }
}

impl Error for DiagnosticCodeError {}

/// 用户诊断的严重级别。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// 阻止当前编译继续产生有效产物的错误。
    Error,
    /// 不阻止编译但需要用户关注的警告。
    Warning,
}

impl Severity {
    const fn sort_rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
        }
    }
}

/// 已验证为非空、单行的诊断文本。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticText(String);

impl DiagnosticText {
    /// 返回未经改写的诊断文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 带源码范围的关联诊断标签。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticLabel {
    span: Span,
    message: DiagnosticText,
}

impl DiagnosticLabel {
    /// 返回关联范围。
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// 返回关联标签文本。
    #[must_use]
    pub fn message(&self) -> &str {
        self.message.as_str()
    }
}

/// 一条诊断的有序附加信息。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticDetail {
    /// 带关联源码范围的标签。
    Label(DiagnosticLabel),
    /// 补充说明。
    Note(DiagnosticText),
    /// 可操作建议。
    Help(DiagnosticText),
}

/// 一条结构化用户诊断。
///
/// 主范围不是 `Option`，字段也不能由调用方直接修改，因此公开 API 无法构造缺失主范围、
/// 未验证错误码或空必填文本的诊断。
///
/// ```compile_fail
/// use lang_frontend::{
///     diagnostic::{Diagnostic, DiagnosticCodeCatalog, Severity},
///     source::SourceMap,
/// };
///
/// fn missing_primary_span(sources: &SourceMap, catalog: &DiagnosticCodeCatalog) {
///     let code = catalog.resolve("L9000").expect("registered by the caller");
///     let _ = Diagnostic::new(sources, Severity::Error, code, "message");
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    severity: Severity,
    code: DiagnosticCode,
    message: DiagnosticText,
    primary_span: Span,
    details: Vec<DiagnosticDetail>,
}

impl Diagnostic {
    /// 创建带必填主范围的诊断。
    ///
    /// # Errors
    ///
    /// 主文本为空或包含换行，或主范围无法由给定 source map 解析时返回具体错误。
    pub fn new(
        sources: &SourceMap,
        severity: Severity,
        code: DiagnosticCode,
        message: impl Into<String>,
        primary_span: Span,
    ) -> Result<Self, DiagnosticError> {
        validate_span(sources, primary_span, SpanRole::Primary)?;

        Ok(Self {
            severity,
            code,
            message: checked_text(message, TextField::PrimaryMessage)?,
            primary_span,
            details: Vec::new(),
        })
    }

    /// 追加带源码范围的关联标签。
    ///
    /// # Errors
    ///
    /// 当前 source map 无法解析主范围或关联范围，或标签文本为空、包含换行时返回具体错误。
    pub fn add_label(
        &mut self,
        sources: &SourceMap,
        span: Span,
        message: impl Into<String>,
    ) -> Result<(), DiagnosticError> {
        validate_span(sources, self.primary_span, SpanRole::Primary)?;
        let detail_index = self.details.len();
        validate_span(sources, span, SpanRole::Label { detail_index })?;
        let message = checked_text(message, TextField::LabelMessage)?;
        self.details
            .push(DiagnosticDetail::Label(DiagnosticLabel { span, message }));
        Ok(())
    }

    /// 追加说明并保留生产者给出的顺序。
    ///
    /// # Errors
    ///
    /// 说明为空或包含换行时返回具体错误。
    pub fn add_note(&mut self, text: impl Into<String>) -> Result<(), DiagnosticError> {
        self.details
            .push(DiagnosticDetail::Note(checked_text(text, TextField::Note)?));
        Ok(())
    }

    /// 追加可操作建议并保留生产者给出的顺序。
    ///
    /// # Errors
    ///
    /// 建议为空或包含换行时返回具体错误。
    pub fn add_help(&mut self, text: impl Into<String>) -> Result<(), DiagnosticError> {
        self.details
            .push(DiagnosticDetail::Help(checked_text(text, TextField::Help)?));
        Ok(())
    }

    /// 返回严重级别。
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// 返回已验证错误码。
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// 返回主消息。
    #[must_use]
    pub fn message(&self) -> &str {
        self.message.as_str()
    }

    /// 返回主源码范围。
    #[must_use]
    pub const fn primary_span(&self) -> Span {
        self.primary_span
    }

    /// 返回生产者顺序下的附加信息。
    #[must_use]
    pub fn details(&self) -> &[DiagnosticDetail] {
        &self.details
    }
}

/// 诊断文本字段，用于定位构造错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextField {
    /// 主消息。
    PrimaryMessage,
    /// 关联标签消息。
    LabelMessage,
    /// 补充说明。
    Note,
    /// 可操作建议。
    Help,
}

/// 诊断范围在模型中的角色。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanRole {
    /// 诊断主范围。
    Primary,
    /// 附加信息序列中的关联标签。
    Label {
        /// 标签在完整附加信息序列中的下标。
        detail_index: usize,
    },
}

/// 诊断构造或稳定排序失败。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticError {
    /// 必填文本为空。
    EmptyText {
        /// 出错字段。
        field: TextField,
    },
    /// 文本包含会破坏逐行 renderer 形态的 CR 或 LF。
    MultilineText {
        /// 出错字段。
        field: TextField,
    },
    /// 范围无法由当前 source map 解析。
    InvalidSpan {
        /// 出错范围在诊断中的角色。
        role: SpanRole,
        /// source / span 层返回的具体原因。
        source: SourceError,
    },
}

impl fmt::Display for DiagnosticError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyText { field } => write!(formatter, "diagnostic {field:?} is empty"),
            Self::MultilineText { field } => {
                write!(formatter, "diagnostic {field:?} contains a line break")
            }
            Self::InvalidSpan { role, source } => {
                write!(formatter, "diagnostic {role:?} is invalid: {source}")
            }
        }
    }
}

impl Error for DiagnosticError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSpan { source, .. } => Some(source),
            Self::EmptyText { .. } | Self::MultilineText { .. } => None,
        }
    }
}

/// 按全部可渲染字段返回确定性全序下的诊断引用。
///
/// 原切片不会被重排。排序依次使用主 source 名称、主范围、严重级别、错误码、主消息和完整
/// 附加信息序列；不使用 `SourceId` 数值、source 加载顺序或输入下标。
///
/// # Errors
///
/// 任一主范围或关联标签范围无法由给定 source map 解析时返回具体错误，并且不返回部分结果。
pub fn ordered_diagnostics<'diagnostic>(
    sources: &SourceMap,
    diagnostics: &'diagnostic [Diagnostic],
) -> Result<Vec<&'diagnostic Diagnostic>, DiagnosticError> {
    let mut keyed = Vec::with_capacity(diagnostics.len());
    for diagnostic in diagnostics {
        keyed.push((order_key(sources, diagnostic)?, diagnostic));
    }

    keyed.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    Ok(keyed
        .into_iter()
        .map(|(_, diagnostic)| diagnostic)
        .collect())
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct DiagnosticOrderKey<'source, 'diagnostic> {
    source_name: &'source str,
    start: usize,
    end: usize,
    severity: u8,
    code: u16,
    message: &'diagnostic str,
    details: Vec<DetailOrderKey<'source, 'diagnostic>>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum DetailOrderKey<'source, 'diagnostic> {
    Label {
        source_name: &'source str,
        start: usize,
        end: usize,
        message: &'diagnostic str,
    },
    Note(&'diagnostic str),
    Help(&'diagnostic str),
}

fn order_key<'source, 'diagnostic>(
    sources: &'source SourceMap,
    diagnostic: &'diagnostic Diagnostic,
) -> Result<DiagnosticOrderKey<'source, 'diagnostic>, DiagnosticError> {
    validate_span(sources, diagnostic.primary_span, SpanRole::Primary)?;
    let source_name = sources
        .source_name(diagnostic.primary_span.source_id())
        .map_err(|source| DiagnosticError::InvalidSpan {
            role: SpanRole::Primary,
            source,
        })?;
    let mut details = Vec::with_capacity(diagnostic.details.len());

    for (detail_index, detail) in diagnostic.details.iter().enumerate() {
        match detail {
            DiagnosticDetail::Label(label) => {
                let role = SpanRole::Label { detail_index };
                validate_span(sources, label.span, role)?;
                let label_source = sources
                    .source_name(label.span.source_id())
                    .map_err(|source| DiagnosticError::InvalidSpan { role, source })?;
                details.push(DetailOrderKey::Label {
                    source_name: label_source,
                    start: label.span.start(),
                    end: label.span.end(),
                    message: label.message.as_str(),
                });
            }
            DiagnosticDetail::Note(text) => {
                details.push(DetailOrderKey::Note(text.as_str()));
            }
            DiagnosticDetail::Help(text) => {
                details.push(DetailOrderKey::Help(text.as_str()));
            }
        }
    }

    Ok(DiagnosticOrderKey {
        source_name,
        start: diagnostic.primary_span.start(),
        end: diagnostic.primary_span.end(),
        severity: diagnostic.severity.sort_rank(),
        code: diagnostic.code.0,
        message: diagnostic.message.as_str(),
        details,
    })
}

fn checked_text(
    text: impl Into<String>,
    field: TextField,
) -> Result<DiagnosticText, DiagnosticError> {
    let text = text.into();
    if text.is_empty() {
        return Err(DiagnosticError::EmptyText { field });
    }
    if text.contains('\r') || text.contains('\n') {
        return Err(DiagnosticError::MultilineText { field });
    }

    Ok(DiagnosticText(text))
}

fn validate_span(sources: &SourceMap, span: Span, role: SpanRole) -> Result<(), DiagnosticError> {
    sources
        .slice(span)
        .map(|_| ())
        .map_err(|source| DiagnosticError::InvalidSpan { role, source })
}

fn parse_code(raw: &str) -> Result<DiagnosticCode, DiagnosticCodeError> {
    let bytes = raw.as_bytes();
    if bytes.len() != 5 || bytes[0] != b'L' || !bytes[1..].iter().all(u8::is_ascii_digit) {
        return Err(DiagnosticCodeError::InvalidFormat {
            code: raw.to_owned(),
        });
    }

    let number = bytes[1..]
        .iter()
        .fold(0_u16, |value, digit| value * 10 + u16::from(*digit - b'0'));
    Ok(DiagnosticCode(number))
}

#[cfg(test)]
mod tests {
    use super::{
        DetailOrderKey, Diagnostic, DiagnosticCodeCatalog, DiagnosticDetail, DiagnosticError,
        DiagnosticLabel, DiagnosticText, Severity, SpanRole, TextField, ordered_diagnostics,
    };
    use crate::source::SourceMap;

    #[test]
    fn ordering_defensively_rejects_an_invalid_internal_label() {
        let catalog =
            DiagnosticCodeCatalog::try_new(&["L9000"]).expect("the test diagnostic code is valid");
        let code = catalog
            .resolve("L9000")
            .expect("the test diagnostic code is registered");
        let mut primary_sources = SourceMap::new();
        let primary_id = primary_sources
            .add_source("primary.ko", "primary")
            .expect("the source name is unique");
        let primary = primary_sources
            .span(primary_id, 0, 1)
            .expect("the primary span is valid");
        let mut foreign_sources = SourceMap::new();
        let foreign_id = foreign_sources
            .add_source("foreign.ko", "foreign")
            .expect("the source name is unique");
        let foreign = foreign_sources
            .span(foreign_id, 0, 1)
            .expect("the foreign span is valid");
        let mut diagnostic = Diagnostic::new(
            &primary_sources,
            Severity::Error,
            code,
            "primary message",
            primary,
        )
        .expect("the primary diagnostic is valid");

        // Public APIs cannot create this state. Constructing it here proves the aggregation boundary
        // still returns an internal error instead of panicking if an internal producer is defective.
        diagnostic
            .details
            .push(DiagnosticDetail::Label(DiagnosticLabel {
                span: foreign,
                message: DiagnosticText("foreign label".to_owned()),
            }));

        assert_eq!(
            ordered_diagnostics(&primary_sources, &[diagnostic]),
            Err(DiagnosticError::InvalidSpan {
                role: SpanRole::Label { detail_index: 0 },
                source: crate::source::SourceError::InvalidSourceId {
                    source_id: foreign_id,
                },
            })
        );
    }

    #[test]
    fn detail_key_variant_order_is_defined_in_one_place() {
        let label = DetailOrderKey::Label {
            source_name: "source.ko",
            start: 0,
            end: 0,
            message: "label",
        };

        assert!(label < DetailOrderKey::Note("note"));
        assert!(DetailOrderKey::Note("note") < DetailOrderKey::Help("help"));
    }

    #[test]
    fn checked_text_rejects_line_breaks_without_rewriting_content() {
        assert_eq!(
            super::checked_text("line\nbreak", TextField::Note),
            Err(DiagnosticError::MultilineText {
                field: TextField::Note,
            })
        );
    }
}
