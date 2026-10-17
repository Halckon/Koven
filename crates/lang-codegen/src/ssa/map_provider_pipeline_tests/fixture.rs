//! 普通源码、真实 AST 和阶段错误；不制造或改写 frontend facts。
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{Expression, ForBinding, NameMarker, ParsedFile, Statement, parse_file},
    source::{SourceMap, Span},
    type_checking::{FinalTypeAdmissionError, FinalTypeAdmissionInternalError},
};

use crate::ssa::{LoweringError, model::Origin, verify::VerifyErrors};

pub(super) const FOR_TEXT: &str = "for ((key, value) in source) {\n    read(key, value)\n  }";

#[derive(Debug)]
pub(super) struct Failure {
    pub stage: &'static str,
    pub original: String,
}

pub(super) fn failure(stage: &'static str, error: impl std::fmt::Debug) -> Failure {
    Failure {
        stage,
        original: format!("{error:?}"),
    }
}

pub(super) fn location(sources: &SourceMap, span: Span) -> String {
    format!(
        "span={span:?} path={:?} text={:?}",
        sources.source_name(span.source_id()),
        sources.slice(span)
    )
}

pub(super) fn at(
    sources: &SourceMap,
    stage: &'static str,
    error: impl std::fmt::Debug,
    span: Option<Span>,
) -> Failure {
    failure(stage, (error, span.map(|span| location(sources, span))))
}

fn diagnostics(sources: &SourceMap, diagnostics: &[Diagnostic]) -> String {
    format!(
        "{:#?}",
        diagnostics
            .iter()
            .map(|diagnostic| (diagnostic, location(sources, diagnostic.primary_span())))
            .collect::<Vec<_>>()
    )
}

pub(super) fn clean(
    sources: &SourceMap,
    stage: &'static str,
    items: &[Diagnostic],
) -> Result<(), Failure> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(failure(stage, diagnostics(sources, items)))
    }
}

pub(super) fn complete_failure(sources: &SourceMap, error: FinalTypeAdmissionError) -> Failure {
    let locations = match &error {
        FinalTypeAdmissionError::Diagnostics(items) => diagnostics(sources, items),
        FinalTypeAdmissionError::Internal(
            FinalTypeAdmissionInternalError::MissingRequiredFact { span, .. },
        ) => location(sources, *span),
        _ => "no source span supplied by the original error".to_owned(),
    };
    failure("complete admission", (error, locations))
}

pub(super) fn lower_failure(sources: &SourceMap, error: LoweringError) -> Failure {
    at(sources, "lower", error, error.span)
}

pub(super) fn verify_failure(sources: &SourceMap, errors: VerifyErrors) -> Failure {
    let locations = errors
        .errors
        .iter()
        .filter_map(|error| match &error.origin {
            Some(Origin::Source(span)) => Some(location(sources, *span)),
            _ => None,
        })
        .collect::<Vec<_>>();
    failure("SSA verifier", (errors, locations))
}

pub(super) struct Fixture {
    pub sources: SourceMap,
    pub parsed: ParsedFile,
    pub path: &'static str,
    pub map: bool,
}

impl Fixture {
    pub fn new(label: &str, text: &str) -> Result<Self, Failure> {
        let mut sources = SourceMap::new();
        let path = "provider.ko";
        let source = sources
            .add_source(path, text)
            .map_err(|error| failure("source infrastructure", error))?;
        let lexed = lex(&sources, source).map_err(|error| failure("lex infrastructure", error))?;
        clean(&sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed =
            parse_file(&sources, &lexed).map_err(|error| failure("parse infrastructure", error))?;
        clean(&sources, "parse diagnostics", parsed.diagnostics())?;
        let map = match label {
            "map" => true,
            "mutable-map" => false,
            _ => return Err(failure("fixture identity", label)),
        };
        Ok(Self {
            sources,
            parsed,
            path,
            map,
        })
    }
}

#[derive(Debug)]
pub(super) struct LoopSite {
    pub statement: StatementId,
    pub span: Span,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
}

pub(super) fn loop_site(fixture: &Fixture) -> Result<LoopSite, Failure> {
    let ast = fixture.parsed.ast();
    let sites = ast
        .statements()
        .iter()
        .filter(|(_, node)| matches!(node.payload(), Statement::For { .. }))
        .collect::<Vec<_>>();
    let [(statement, node)] = sites.as_slice() else {
        return Err(failure(
            "AST Map input query",
            ("expected one For", sites.len()),
        ));
    };
    let Statement::For {
        binding, source, ..
    } = node.payload()
    else {
        return Err(failure("AST Map input query", "not a real For"));
    };
    let ForBinding::Destructuring { names, .. } = binding else {
        return Err(failure("AST Map input query", "not destructuring"));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(failure(
            "AST Map input query",
            "expected two named bindings",
        ));
    };
    let expression = ast
        .expressions()
        .get(*source)
        .map_err(|error| failure("AST Map input query", error))?;
    if !matches!(expression.payload(), Expression::Name) {
        return Err(at(
            &fixture.sources,
            "AST Map input query",
            "source must be Name",
            Some(expression.span()),
        ));
    }
    Ok(LoopSite {
        statement: *statement,
        span: node.span(),
        source: *source,
        source_span: expression.span(),
        bindings: [*key, *value],
    })
}
