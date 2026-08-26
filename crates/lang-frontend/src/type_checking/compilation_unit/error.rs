use std::{error::Error, fmt};

use crate::{
    diagnostic::{DiagnosticCodeError, DiagnosticError},
    name_resolution::UnitDiagnosticOrderError,
    type_checking::TypeCheckingError,
};

/// Compilation-unit 类型阶段的输入或内部契约错误。
#[derive(Debug)]
pub enum CompilationUnitTypeError {
    /// source inputs 无法重建名称产物的 canonical compilation unit。
    MismatchedInputs,
    /// 类型环境与名称产物不属于同一显式分析环境。
    MismatchedNameEnvironment,
    /// 名称产物缺少声明到 unit symbol 的不变量映射。
    MissingDeclarationSymbol,
    /// 单文件类型基础设施返回内部错误。
    Type(TypeCheckingError),
    /// unit 诊断无法建立稳定全序。
    DiagnosticOrder(UnitDiagnosticOrderError),
    /// 生产诊断目录无效。
    DiagnosticCode(DiagnosticCodeError),
    /// 诊断模型构造失败。
    Diagnostic(DiagnosticError),
}

impl fmt::Display for CompilationUnitTypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MismatchedInputs => formatter
                .write_str("compilation-unit type inputs do not match the validated name product"),
            Self::MismatchedNameEnvironment => formatter
                .write_str("compilation-unit names and type environment do not share an identity"),
            Self::MissingDeclarationSymbol => {
                formatter.write_str("validated names are missing a declaration-to-symbol mapping")
            }
            Self::Type(error) => write!(formatter, "unit type collection failed: {error}"),
            Self::DiagnosticOrder(error) => {
                write!(formatter, "unit diagnostic ordering failed: {error}")
            }
            Self::DiagnosticCode(error) => write!(formatter, "invalid diagnostic catalog: {error}"),
            Self::Diagnostic(error) => write!(formatter, "invalid unit type diagnostic: {error}"),
        }
    }
}

impl Error for CompilationUnitTypeError {}

impl From<TypeCheckingError> for CompilationUnitTypeError {
    fn from(error: TypeCheckingError) -> Self {
        Self::Type(error)
    }
}

impl From<UnitDiagnosticOrderError> for CompilationUnitTypeError {
    fn from(error: UnitDiagnosticOrderError) -> Self {
        Self::DiagnosticOrder(error)
    }
}

impl From<DiagnosticCodeError> for CompilationUnitTypeError {
    fn from(error: DiagnosticCodeError) -> Self {
        Self::DiagnosticCode(error)
    }
}

impl From<DiagnosticError> for CompilationUnitTypeError {
    fn from(error: DiagnosticError) -> Self {
        Self::Diagnostic(error)
    }
}
