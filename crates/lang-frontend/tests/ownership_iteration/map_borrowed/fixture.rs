//! 原始 source/lexer/parser 保留与真实 AST 使用点提取。
use super::cases::Case;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    diagnostic::Diagnostic,
    lexer::{LexedFile, lex},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement, parse_file},
    source::{SourceMap, Span},
};

pub(super) const LOGICAL_PATH: &str = "iteration.ko";

#[derive(Debug)]
pub(super) struct Blocked {
    pub stage: &'static str,
    pub detail: String,
}

pub(super) fn blocked(stage: &'static str, detail: impl std::fmt::Debug) -> Blocked {
    Blocked {
        stage,
        detail: format!("{detail:?}"),
    }
}

pub(super) fn diagnostics(sources: &SourceMap, stage: &str, items: &[Diagnostic]) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            let span = item.primary_span();
            format!(
                "{stage}: {} span={span:?} {}..{} slice={:?} message={:?}",
                item.code(),
                span.start(),
                span.end(),
                sources.slice(span),
                item.message()
            )
        })
        .collect()
}

pub(super) fn require_clean(
    sources: &SourceMap,
    stage: &'static str,
    items: &[Diagnostic],
) -> Result<(), Blocked> {
    if items.is_empty() {
        Ok(())
    } else {
        Err(blocked(stage, diagnostics(sources, stage, items)))
    }
}

pub(super) struct Fixture {
    pub sources: SourceMap,
    pub lexed: LexedFile,
    pub file: ParsedFile,
}

impl Fixture {
    pub(super) fn new(text: &str) -> Result<Self, Blocked> {
        let mut sources = SourceMap::new();
        let source = sources
            .add_source(LOGICAL_PATH, text)
            .map_err(|error| blocked("source infrastructure", error))?;
        let lexed = lex(&sources, source).map_err(|error| blocked("lex infrastructure", error))?;
        require_clean(&sources, "lex", lexed.diagnostics())?;
        let file =
            parse_file(&sources, &lexed).map_err(|error| blocked("parse infrastructure", error))?;
        require_clean(&sources, "parse", file.diagnostics())?;
        Ok(Self {
            sources,
            lexed,
            file,
        })
    }
}

#[derive(Debug)]
pub(super) struct Sites {
    pub statement: StatementId,
    pub source: ExpressionId,
    pub source_span: Span,
    pub bindings: [Span; 2],
    pub reads: [Vec<(ExpressionId, Span)>; 2],
    pub local: Option<(Span, ExpressionId)>,
    pub transfer: Option<ExpressionId>,
    pub call: Option<(ExpressionId, Vec<ExpressionId>)>,
}

pub(super) fn sites(fixture: &Fixture, case: &Case) -> Result<Sites, Blocked> {
    let ast = fixture.file.ast();
    let loops = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| match node.payload() {
            Statement::For {
                binding,
                source,
                body,
                ..
            } => Some((id, binding, *source, *body)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(statement, binding, source, body)] = loops.as_slice() else {
        return Err(blocked("AST fixture", "expected exactly one for"));
    };
    let ForBinding::Destructuring { names, .. } = binding else {
        return Err(blocked("AST fixture", "expected destructuring"));
    };
    let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
        return Err(blocked("AST fixture", "expected two real names"));
    };
    let body = ast
        .statements()
        .get(*body)
        .map_err(|error| blocked("AST fixture", error))?;
    let Statement::Block { elements } = body.payload() else {
        return Err(blocked("AST fixture", "expected block body"));
    };
    let within_body =
        |span: Span| span.start() >= body.span().start() && span.end() <= body.span().end();
    let source_span = ast
        .expressions()
        .get(*source)
        .map_err(|error| blocked("AST fixture", error))?
        .span();
    for (span, expected) in [(*key, "key"), (*value, "value"), (source_span, case.source)] {
        if fixture.sources.slice(span) != Ok(expected) {
            return Err(blocked("AST fixture spelling", (span, expected)));
        }
    }
    let mut reads = [Vec::new(), Vec::new()];
    let mut transfers = Vec::new();
    let mut calls = Vec::new();
    for (id, node) in ast
        .expressions()
        .iter()
        .filter(|(_, node)| within_body(node.span()))
    {
        match node.payload() {
            Expression::Name => {
                for (index, name) in ["key", "value"].into_iter().enumerate() {
                    if fixture.sources.slice(node.span()) == Ok(name) {
                        reads[index].push((id, node.span()));
                    }
                }
            }
            Expression::Break { .. } | Expression::Continue { .. } | Expression::Return { .. } => {
                transfers.push(id);
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let callee = ast
                    .expressions()
                    .get(*callee)
                    .map_err(|error| blocked("AST fixture", error))?;
                if case
                    .call
                    .is_some_and(|name| fixture.sources.slice(callee.span()) == Ok(name))
                {
                    calls.push((id, arguments.iter().map(|arg| arg.value).collect()));
                }
            }
            _ => {}
        }
    }
    if [reads[0].len(), reads[1].len()] != case.reads || transfers.len() > 1 {
        return Err(blocked("AST fixture reads/transfers", (&reads, &transfers)));
    }
    if calls.len() != usize::from(case.call.is_some()) {
        return Err(blocked("AST fixture calls", calls));
    }
    let mut locals = Vec::new();
    for statement in elements {
        let node = ast
            .statements()
            .get(*statement)
            .map_err(|error| blocked("AST fixture", error))?;
        if let Statement::LocalVariable { declaration } = node.payload() {
            let node = ast
                .items()
                .get(*declaration)
                .map_err(|error| blocked("AST fixture", error))?;
            if let Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } = node.payload()
                && fixture.sources.slice(*name) == Ok("local")
            {
                locals.push((*name, *initializer));
            }
        }
    }
    if locals.len() != usize::from(case.expected.is_success()) {
        return Err(blocked("AST fixture local owner", locals));
    }
    Ok(Sites {
        statement: *statement,
        source: *source,
        source_span,
        bindings: [*key, *value],
        reads,
        local: locals.into_iter().next(),
        transfer: transfers.into_iter().next(),
        call: calls.into_iter().next(),
    })
}
