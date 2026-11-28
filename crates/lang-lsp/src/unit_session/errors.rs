//! `UnitSession` 生命周期与查询错误类型。

use std::{error::Error, fmt};

use lang_frontend::{
    analysis::UnitNameAnalysisError,
    lexer::LexerInternalError,
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNameError, UnitDiagnosticOrderError,
    },
    ownership_checking::OwnershipCheckingError,
    parser::ParserInternalError,
    source::{SourceError, SourceId},
    type_checking::CompilationUnitTypeError,
};

use crate::{
    definition::DefinitionIndexError, diagnostic_adapter::DiagnosticMappingError,
    position_adapter::PositionMappingError, unit_analysis_snapshot::UnitAnalysisSnapshotError,
};

/// source-set lifecycle 的协议拒绝或内部 snapshot 失败。
#[derive(Debug)]
pub(crate) enum UnitSessionError {
    Protocol(String),
    Source(SourceError),
    Lexer(LexerInternalError),
    Parser(ParserInternalError),
    Input(CompilationUnitInputError),
    Name(CompilationUnitNameError),
    Type(CompilationUnitTypeError),
    Ownership(OwnershipCheckingError),
    Diagnostic(UnitDiagnosticOrderError),
    Mapping(DiagnosticMappingError),
    Definition(DefinitionIndexError),
}

impl fmt::Display for UnitSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Protocol(message) => formatter.write_str(message),
            Self::Source(error) => write!(formatter, "unit source registration failed: {error}"),
            Self::Lexer(error) => write!(formatter, "unit lexer failed internally: {error}"),
            Self::Parser(error) => write!(formatter, "unit parser failed internally: {error}"),
            Self::Input(error) => write!(formatter, "unit input validation failed: {error}"),
            Self::Name(error) => write!(formatter, "unit name analysis failed internally: {error}"),
            Self::Type(error) => write!(formatter, "unit type analysis failed internally: {error}"),
            Self::Ownership(error) => {
                write!(
                    formatter,
                    "unit ownership analysis failed internally: {error}"
                )
            }
            Self::Diagnostic(error) => {
                write!(formatter, "unit diagnostic ordering failed: {error}")
            }
            Self::Mapping(error) => write!(formatter, "unit diagnostic mapping failed: {error}"),
            Self::Definition(error) => {
                write!(formatter, "unit definition indexing failed: {error}")
            }
        }
    }
}

impl Error for UnitSessionError {}

impl From<UnitAnalysisSnapshotError> for UnitSessionError {
    fn from(error: UnitAnalysisSnapshotError) -> Self {
        match error {
            UnitAnalysisSnapshotError::Diagnostic(error) => Self::Diagnostic(error),
            UnitAnalysisSnapshotError::Definition(error) => Self::Definition(error),
        }
    }
}

impl From<UnitNameAnalysisError> for UnitSessionError {
    fn from(error: UnitNameAnalysisError) -> Self {
        match error {
            UnitNameAnalysisError::Lexer(error) => Self::Lexer(error),
            UnitNameAnalysisError::Parser(error) => Self::Parser(error),
            UnitNameAnalysisError::Input(error) => Self::Input(error),
            UnitNameAnalysisError::Name(error) => Self::Name(error),
        }
    }
}

macro_rules! unit_session_error_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for UnitSessionError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}

unit_session_error_from!(SourceError, Source);
unit_session_error_from!(LexerInternalError, Lexer);
unit_session_error_from!(ParserInternalError, Parser);
unit_session_error_from!(CompilationUnitInputError, Input);
unit_session_error_from!(CompilationUnitNameError, Name);
unit_session_error_from!(CompilationUnitTypeError, Type);
unit_session_error_from!(OwnershipCheckingError, Ownership);
unit_session_error_from!(UnitDiagnosticOrderError, Diagnostic);
unit_session_error_from!(DiagnosticMappingError, Mapping);
unit_session_error_from!(DefinitionIndexError, Definition);

/// definition request 的 position 映射或 snapshot source identity 错误。
#[derive(Debug)]
pub(crate) enum UnitDefinitionQueryError {
    Position(PositionMappingError),
    UnknownSource(SourceId),
    MismatchedTargetSource {
        expected: SourceId,
        actual: SourceId,
    },
}

impl fmt::Display for UnitDefinitionQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Position(error) => write!(formatter, "unit definition position failed: {error}"),
            Self::UnknownSource(source) => {
                write!(
                    formatter,
                    "unit definition references unknown source {source:?}"
                )
            }
            Self::MismatchedTargetSource { expected, actual } => write!(
                formatter,
                "unit definition target source mismatch: expected {expected:?}, got {actual:?}"
            ),
        }
    }
}

impl Error for UnitDefinitionQueryError {}

impl From<PositionMappingError> for UnitDefinitionQueryError {
    fn from(error: PositionMappingError) -> Self {
        Self::Position(error)
    }
}
