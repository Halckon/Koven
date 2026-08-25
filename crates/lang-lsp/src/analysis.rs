//! 打开文档到现有 frontend 诊断的单文件适配。

use std::{error::Error, fmt};

use lang_frontend::{
    diagnostic::{Diagnostic, DiagnosticError, ordered_diagnostics},
    lexer::{LexerInternalError, lex},
    name_resolution::{NameResolutionError, resolve_names},
    ownership_checking::{OwnershipCheckingError, check_ownership},
    parser::{ParserInternalError, parse_file},
    source::{SourceError, SourceMap},
    type_checking::{TypeCheckingError, check_types, standard_environments},
};

use crate::definition::{DefinitionIndex, DefinitionIndexError};

/// 对一份内存文档运行全部已实现的单文件 frontend 阶段。
pub(crate) fn analyze(source_name: &str, text: &str) -> Result<Analysis, AnalysisError> {
    let mut sources = SourceMap::new();
    let source = sources.add_source(source_name, text)?;
    let lexed = lex(&sources, source)?;
    let parsed = parse_file(&sources, &lexed)?;
    let (name_environment, type_environment) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_environment)?;
    let typed = check_types(&sources, &parsed, &names, &type_environment)?;
    let definitions = DefinitionIndex::build(&parsed, &names, &typed)?;
    let owned = check_ownership(&sources, &parsed, &names, &typed)?;

    let mut diagnostics = parsed.diagnostics().to_vec();
    diagnostics.extend_from_slice(names.diagnostics());
    diagnostics.extend_from_slice(typed.diagnostics());
    diagnostics.extend_from_slice(owned.diagnostics());
    let diagnostics = ordered_diagnostics(&sources, &diagnostics)?
        .into_iter()
        .cloned()
        .collect();

    Ok(Analysis {
        sources,
        diagnostics,
        definitions,
    })
}

/// 一次分析拥有的 source 与确定性诊断集合。
pub(crate) struct Analysis {
    pub(crate) sources: SourceMap,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) definitions: DefinitionIndex,
}

/// 单文档 frontend 编排的内部失败。
#[derive(Debug)]
pub(crate) enum AnalysisError {
    Source(SourceError),
    Lexer(LexerInternalError),
    Parser(ParserInternalError),
    Name(NameResolutionError),
    Type(TypeCheckingError),
    Ownership(OwnershipCheckingError),
    Diagnostic(DiagnosticError),
    Definition(DefinitionIndexError),
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "could not register LSP source: {error}"),
            Self::Lexer(error) => write!(formatter, "lexer failed internally: {error}"),
            Self::Parser(error) => write!(formatter, "parser failed internally: {error}"),
            Self::Name(error) => write!(formatter, "name resolution failed internally: {error}"),
            Self::Type(error) => write!(formatter, "type checking failed internally: {error}"),
            Self::Ownership(error) => {
                write!(formatter, "ownership checking failed internally: {error}")
            }
            Self::Diagnostic(error) => {
                write!(formatter, "diagnostic ordering failed internally: {error}")
            }
            Self::Definition(error) => write!(formatter, "definition indexing failed: {error}"),
        }
    }
}

impl Error for AnalysisError {}

macro_rules! analysis_error_from {
    ($source:ty, $variant:ident) => {
        impl From<$source> for AnalysisError {
            fn from(error: $source) -> Self {
                Self::$variant(error)
            }
        }
    };
}

analysis_error_from!(SourceError, Source);
analysis_error_from!(LexerInternalError, Lexer);
analysis_error_from!(ParserInternalError, Parser);
analysis_error_from!(NameResolutionError, Name);
analysis_error_from!(TypeCheckingError, Type);
analysis_error_from!(OwnershipCheckingError, Ownership);
analysis_error_from!(DiagnosticError, Diagnostic);
analysis_error_from!(DefinitionIndexError, Definition);

#[cfg(test)]
mod tests {
    use super::analyze;

    #[test]
    fn aggregates_diagnostics_from_every_frontend_stage_in_source_order() {
        let analysis = analyze(
            "file:///diagnostics.ko",
            "#\nfun names(): Unit { missing }\nfun types(): Unit { val item: String = 1 }\n\
             class Resource()\nfun take(own resource: Resource): Unit {}\n\
             fun moves(own resource: Resource): Unit {\nval first = take(resource)\nval second = take(resource)\n}",
        )
        .expect("analysis must succeed internally");
        let codes = analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code().to_string())
            .collect::<Vec<_>>();

        assert!(codes.iter().any(|code| code == "L0001"), "{codes:?}");
        assert!(codes.iter().any(|code| code == "L0080"), "{codes:?}");
        assert!(codes.iter().any(|code| code == "L0084"), "{codes:?}");
        assert!(codes.iter().any(|code| code == "L0131"), "{codes:?}");
        assert!(
            analysis
                .diagnostics
                .windows(2)
                .all(|pair| pair[0].primary_span().start() <= pair[1].primary_span().start())
        );
    }

    #[test]
    fn standard_analysis_environment_accepts_core_containers_and_capabilities() {
        let analysis = analyze(
            "file:///containers.ko",
            "fun values(): Unit { val items: List<Int> = listOf(1, 2) }\n\
             fun <T: Copyable> copy(own item: T): Unit {}",
        )
        .expect("analysis must succeed internally");

        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
    }
}
