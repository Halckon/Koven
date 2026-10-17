//! 普通 fixture、真实 AST 位置和阶段化结果；无生产 facts 构造器。

use super::super::{LoweringError, LoweringErrorKind};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    lexer::lex,
    parser::{ForBinding, NameMarker, ParsedFile, Statement, parse_file},
    source::{SourceMap, Span},
    type_checking::{IntrinsicTypeConstructor, MapIterationKind},
};

pub(super) const FOR_TEXT: &str =
    "for ((key, value) in source) { val copied: Int = key; println(value) }";

#[derive(Clone, Copy)]
pub(super) struct SourceCase {
    pub path: &'static str,
    pub text: &'static str,
    pub constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
}

#[derive(Debug)]
pub(super) struct Stopped {
    pub stage: &'static str,
    pub original: String,
}

pub(super) fn stopped(stage: &'static str, error: impl std::fmt::Debug) -> Stopped {
    Stopped {
        stage,
        original: format!("{error:?}"),
    }
}

pub(super) fn clean(
    sources: &SourceMap,
    stage: &'static str,
    diagnostics: &[Diagnostic],
) -> Result<(), Stopped> {
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(stopped(
            stage,
            diagnostics
                .iter()
                .map(|item| {
                    (
                        item,
                        sources.source_name(item.primary_span().source_id()),
                        sources.slice(item.primary_span()),
                    )
                })
                .collect::<Vec<_>>(),
        ))
    }
}

pub(super) struct File {
    pub path: String,
    pub parsed: ParsedFile,
    pub constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
}

pub(super) struct Fixture {
    pub sources: SourceMap,
    pub files: Vec<File>,
}

impl Fixture {
    pub fn new(cases: &[SourceCase]) -> Result<Self, Stopped> {
        let mut fixture = Self {
            sources: SourceMap::new(),
            files: Vec::new(),
        };
        for case in cases {
            fixture.add(case.path, case.text, case.constructor)?;
        }
        Ok(fixture)
    }

    pub fn single_whole(text: &str) -> Result<Self, Stopped> {
        let mut fixture = Self {
            sources: SourceMap::new(),
            files: Vec::new(),
        };
        fixture.add("whole.ko", text, None)?;
        Ok(fixture)
    }

    fn add(
        &mut self,
        path: &str,
        text: &str,
        constructor: Option<(IntrinsicTypeConstructor, MapIterationKind)>,
    ) -> Result<(), Stopped> {
        let source = self
            .sources
            .add_source(path, text)
            .map_err(|error| stopped("source infrastructure", error))?;
        let lexed =
            lex(&self.sources, source).map_err(|error| stopped("lex infrastructure", error))?;
        clean(&self.sources, "lex diagnostics", lexed.diagnostics())?;
        let parsed = parse_file(&self.sources, &lexed)
            .map_err(|error| stopped("parse infrastructure", error))?;
        clean(&self.sources, "parse diagnostics", parsed.diagnostics())?;
        self.files.push(File {
            path: path.to_owned(),
            parsed,
            constructor,
        });
        Ok(())
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

pub(super) fn loop_sites(file: &File) -> Result<Vec<LoopSite>, Stopped> {
    let ast = file.parsed.ast();
    let mut sites = Vec::new();
    for (statement, node) in ast.statements().iter() {
        let Statement::For {
            binding, source, ..
        } = node.payload()
        else {
            continue;
        };
        let ForBinding::Destructuring { names, .. } = binding else {
            return Err(stopped("AST query", "For requires destructuring"));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(stopped("AST query", "For requires two real bindings"));
        };
        let source_span = ast
            .expressions()
            .get(*source)
            .map_err(|error| stopped("AST query", error))?
            .span();
        sites.push(LoopSite {
            statement,
            span: node.span(),
            source: *source,
            source_span,
            bindings: [*key, *value],
        });
    }
    Ok(sites)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Location {
    pub span: Span,
    pub path: String,
    pub text: String,
}

pub(super) fn location(sources: &SourceMap, span: Span) -> Option<Location> {
    Some(Location {
        span,
        path: sources.source_name(span.source_id()).ok()?.to_owned(),
        text: sources.slice(span).ok()?.to_owned(),
    })
}

#[derive(Debug)]
pub(super) struct LowerOutcome {
    expected: Option<Location>,
    error: Option<LoweringError>,
    actual: Option<Location>,
}

impl LowerOutcome {
    pub fn new<T>(
        sources: &SourceMap,
        expected: Option<Location>,
        result: Result<T, LoweringError>,
    ) -> Self {
        let error = result.err();
        let actual = error
            .and_then(|error| error.span)
            .and_then(|span| location(sources, span));
        Self {
            expected,
            error,
            actual,
        }
    }

    pub fn matches(&self) -> bool {
        self.expected.as_ref().is_some_and(|expected| {
            self.error
                == Some(LoweringError {
                    kind: LoweringErrorKind::UnsupportedNode,
                    span: Some(expected.span),
                })
                && self.actual.as_ref() == Some(expected)
                && expected.text == FOR_TEXT
        })
    }
}
