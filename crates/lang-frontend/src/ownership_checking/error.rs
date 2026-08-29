use std::{error::Error, fmt};

use crate::{
    ast::AstError,
    diagnostic::{DiagnosticCodeError, DiagnosticError},
    name_resolution::UnitDiagnosticOrderError,
    source::SourceError,
};

/// 所有权阶段的编译器内部错误；用户程序错误进入结构化诊断产物。
#[derive(Debug)]
pub enum OwnershipCheckingError {
    /// ParsedFile 与 NameResolution 不属于同一源码。
    MismatchedNameSource,
    /// ParsedFile 与 TypedFile 不属于同一源码。
    MismatchedTypedSource,
    /// NameResolution 与 TypedFile 不属于同一分析身份链。
    MismatchedAnalysisIdentity,
    /// validated compilation-unit typed product 与 source/name/environment 输入不属于同一分析链。
    MismatchedCompilationUnitTypes,
    /// validated unit index 中的 source locator 无法回到唯一输入。
    InvalidUnitSource {
        /// 规范 source-unit 下标。
        source_unit: usize,
    },
    /// validated unit names 中的 source-qualified symbol locator 违反内部不变量。
    InvalidUnitSymbol {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 symbol 下标。
        symbol: usize,
    },
    /// validated typed unit 中重复发布同一个 source-qualified 参数 binding。
    DuplicateUnitBinding {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 symbol 下标。
        symbol: usize,
    },
    /// typed call locator 未指向对应 source unit 的 call expression。
    InvalidUnitCall {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 expression 下标。
        expression: usize,
    },
    /// typed call 的实参映射违反唯一且完备的下标不变量。
    InvalidUnitCallArgument {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 call expression 下标。
        expression: usize,
        /// 无效的源码实参下标。
        argument: usize,
    },
    /// source call target 或参数下标无法回到声明 signature。
    InvalidUnitCallParameter {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 call expression 下标。
        expression: usize,
        /// 无效的声明参数下标。
        parameter: usize,
    },
    /// typed place argument 无法回到同一 source unit 的稳定 place。
    InvalidUnitArgumentPlace {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 expression 下标。
        expression: usize,
    },
    /// validated Value argument 缺少可执行的 Copyability 判定。
    InvalidUnitArgumentType {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 expression 下标。
        expression: usize,
    },
    /// validated unit construction descriptor 违反 source-qualified Phase 2 不变量。
    InvalidUnitConstruction {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 construction expression 下标。
        expression: usize,
    },
    /// validated unit container construction descriptor 违反 Phase 2 不变量。
    InvalidUnitContainerConstruction {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 container call expression 下标。
        expression: usize,
    },
    /// validated unit 的 lambda/scope/capture locator 违反名称或类型阶段不变量。
    InvalidUnitClosureCapture {
        /// 规范 source-unit 下标。
        source_unit: usize,
        /// 文件局部 lambda expression 下标。
        expression: usize,
    },
    /// TypedFile 中的 construction descriptor 违反 Phase 2 产物不变量。
    InvalidConstructionDescriptor {
        /// 无效 descriptor 的 expression arena 下标。
        expression: usize,
    },
    /// AST typed ID 不满足 Parser 前置不变量。
    Ast(AstError),
    /// 输入源码或 Span 不满足前置不变量。
    Source(SourceError),
    /// 生产诊断码目录无效。
    DiagnosticCode(DiagnosticCodeError),
    /// 无法构造合法结构化诊断。
    Diagnostic(DiagnosticError),
    /// unit 诊断无法建立稳定全序。
    DiagnosticOrder(UnitDiagnosticOrderError),
}

impl fmt::Display for OwnershipCheckingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MismatchedNameSource => {
                formatter.write_str("name resolution belongs to a different source")
            }
            Self::MismatchedTypedSource => {
                formatter.write_str("typed file belongs to a different source")
            }
            Self::MismatchedAnalysisIdentity => {
                formatter.write_str("name and typed files belong to different analyses")
            }
            Self::MismatchedCompilationUnitTypes => formatter.write_str(
                "compilation-unit types belong to different source, name, or environment inputs",
            ),
            Self::InvalidUnitSource { source_unit } => {
                write!(
                    formatter,
                    "invalid compilation-unit source locator {source_unit}"
                )
            }
            Self::InvalidUnitSymbol {
                source_unit,
                symbol,
            } => write!(
                formatter,
                "invalid compilation-unit symbol locator {source_unit}:{symbol}"
            ),
            Self::DuplicateUnitBinding {
                source_unit,
                symbol,
            } => write!(
                formatter,
                "duplicate compilation-unit ownership binding {source_unit}:{symbol}"
            ),
            Self::InvalidUnitCall {
                source_unit,
                expression,
            } => write!(
                formatter,
                "invalid compilation-unit call locator {source_unit}:{expression}"
            ),
            Self::InvalidUnitCallArgument {
                source_unit,
                expression,
                argument,
            } => write!(
                formatter,
                "invalid compilation-unit call argument {source_unit}:{expression}:{argument}"
            ),
            Self::InvalidUnitCallParameter {
                source_unit,
                expression,
                parameter,
            } => write!(
                formatter,
                "invalid compilation-unit call parameter {source_unit}:{expression}:{parameter}"
            ),
            Self::InvalidUnitArgumentPlace {
                source_unit,
                expression,
            } => write!(
                formatter,
                "invalid compilation-unit argument place {source_unit}:{expression}"
            ),
            Self::InvalidUnitArgumentType {
                source_unit,
                expression,
            } => write!(
                formatter,
                "invalid compilation-unit argument type {source_unit}:{expression}"
            ),
            Self::InvalidUnitConstruction {
                source_unit,
                expression,
            } => write!(
                formatter,
                "invalid compilation-unit construction {source_unit}:{expression}"
            ),
            Self::InvalidUnitContainerConstruction {
                source_unit,
                expression,
            } => write!(
                formatter,
                "invalid compilation-unit container construction {source_unit}:{expression}"
            ),
            Self::InvalidUnitClosureCapture {
                source_unit,
                expression,
            } => write!(
                formatter,
                "invalid compilation-unit closure capture {source_unit}:{expression}"
            ),
            Self::InvalidConstructionDescriptor { expression } => {
                write!(
                    formatter,
                    "invalid construction descriptor for expression {expression}"
                )
            }
            Self::Ast(error) => write!(formatter, "ownership AST error: {error}"),
            Self::Source(error) => write!(formatter, "ownership source error: {error}"),
            Self::DiagnosticCode(error) => {
                write!(formatter, "ownership diagnostic code error: {error}")
            }
            Self::Diagnostic(error) => write!(formatter, "ownership diagnostic error: {error}"),
            Self::DiagnosticOrder(error) => {
                write!(
                    formatter,
                    "unit ownership diagnostic ordering failed: {error}"
                )
            }
        }
    }
}

impl Error for OwnershipCheckingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Ast(error) => Some(error),
            Self::Source(error) => Some(error),
            Self::DiagnosticCode(error) => Some(error),
            Self::Diagnostic(error) => Some(error),
            Self::DiagnosticOrder(error) => Some(error),
            Self::MismatchedNameSource
            | Self::MismatchedTypedSource
            | Self::MismatchedAnalysisIdentity
            | Self::MismatchedCompilationUnitTypes
            | Self::InvalidUnitSource { .. }
            | Self::InvalidUnitSymbol { .. }
            | Self::DuplicateUnitBinding { .. }
            | Self::InvalidUnitCall { .. }
            | Self::InvalidUnitCallArgument { .. }
            | Self::InvalidUnitCallParameter { .. }
            | Self::InvalidUnitArgumentPlace { .. }
            | Self::InvalidUnitArgumentType { .. }
            | Self::InvalidUnitConstruction { .. }
            | Self::InvalidUnitContainerConstruction { .. }
            | Self::InvalidUnitClosureCapture { .. }
            | Self::InvalidConstructionDescriptor { .. } => None,
        }
    }
}

impl From<AstError> for OwnershipCheckingError {
    fn from(error: AstError) -> Self {
        Self::Ast(error)
    }
}

impl From<SourceError> for OwnershipCheckingError {
    fn from(error: SourceError) -> Self {
        Self::Source(error)
    }
}

impl From<DiagnosticCodeError> for OwnershipCheckingError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}

impl From<DiagnosticError> for OwnershipCheckingError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}

impl From<UnitDiagnosticOrderError> for OwnershipCheckingError {
    fn from(error: UnitDiagnosticOrderError) -> Self {
        Self::DiagnosticOrder(error)
    }
}
