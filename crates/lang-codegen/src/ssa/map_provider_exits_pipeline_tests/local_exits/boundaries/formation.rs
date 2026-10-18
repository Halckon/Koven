//! Original formation events and the same-analysis bridge used by both observers.
mod facts;
mod graph;
mod representation;
use crate::ssa::map_provider_exits_pipeline_tests::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
pub(in super::super::super) use facts::{single, unit};
pub(in super::super::super) use graph::check;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{AssignmentOperator, Expression, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};

#[derive(Clone, Debug)]
pub(in super::super::super) struct Inputs {
    site: Site,
    calls: Vec<(Span, Span)>,
    drops: Vec<Drop>,
    representation: Option<representation::Representation>,
    guard: Option<representation::Guard>,
}
#[derive(Clone, Debug)]
struct Drop {
    root: Span,
    origin: Span,
    point: Point,
    birth: Birth,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    Expression(Span),
    Statement(Span),
    Call(Span),
    Loop(Span),
    Entry(Span),
}
#[derive(Clone, Debug)]
struct Site {
    scan: Span,
    statement: StatementId,
    for_span: Span,
    source: ExpressionId,
    body: StatementId,
    root: Span,
    birth: Birth,
    put: Option<ExpressionId>,
    sink: Option<ExpressionId>,
    aliases: Vec<(ExpressionId, Span)>,
}
#[derive(Clone, Copy, Debug)]
enum Birth {
    Call(ExpressionId),
    Parameter(Span),
}
fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() > outer.start()
        && inner.end() < outer.end()
}
fn function(sources: &SourceMap, parsed: &ParsedFile, name: &str) -> Result<(Span, Span), Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            match n.payload() {
                Item::Function {
                    name: NameMarker::Present(marker),
                    ..
                } if sources.slice(*marker) == Ok(name) => Some((n.span(), *marker)),
                _ => None,
            }
        }),
        "formation original function",
    )
}
fn declaration(parsed: &ParsedFile, marker: Span) -> Option<ExpressionId> {
    parsed
        .ast()
        .items()
        .iter()
        .find_map(|(_, n)| match n.payload() {
            Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } if *name == marker => Some(*initializer),
            _ => None,
        })
}
fn site(
    sources: &SourceMap,
    parsed: &ParsedFile,
    reference: impl Fn(ExpressionId) -> Result<Span, Failure>,
) -> Result<Site, Failure> {
    let scan = function(sources, parsed, "scan")?.0;
    let (statement, for_span, source, body) = one(
        parsed.ast().statements().iter().filter_map(|(id, n)| {
            if let Statement::For { source, body, .. } = n.payload() {
                inside(n.span(), scan).then_some((id, n.span(), *source, *body))
            } else {
                None
            }
        }),
        "formation original single For",
    )?;
    let root = reference(source)?;
    let (birth, aliases) = birth_at(parsed, scan, root, for_span.start(), &reference)?;
    let mut put = vec![];
    let mut sink = vec![];
    for (id, n) in parsed.ast().expressions().iter() {
        let Expression::Call { callee, .. } = n.payload() else {
            continue;
        };
        if !inside(n.span(), scan) {
            continue;
        }
        let text = sources
            .slice(span(parsed, *callee))
            .map_err(|e| failure("formation original callee text", e))?;
        if text.ends_with(".put") {
            put.push(id)
        }
        if text == "sinkMap" || text == "sinkMutable" || text == "sink" {
            sink.push(id)
        }
    }
    need(
        put.len() <= 1 && sink.len() <= 1,
        "formation complete put/transfer sites",
        (&put, &sink),
    )?;
    Ok(Site {
        scan,
        statement,
        for_span,
        source,
        body,
        root,
        birth,
        put: put.first().copied(),
        sink: sink.first().copied(),
        aliases,
    })
}

fn birth_at(
    parsed: &ParsedFile,
    scan: Span,
    root: Span,
    boundary: usize,
    reference: &impl Fn(ExpressionId) -> Result<Span, Failure>,
) -> Result<(Birth, Vec<(ExpressionId, Span)>), Failure> {
    let mut current = root;
    let mut before = boundary;
    let mut aliases = vec![];
    let mut seen = Vec::new();
    let birth = loop {
        need(
            !seen.contains(&(current, before)),
            "formation acyclic original Move chain",
            (current, before),
        )?;
        seen.push((current, before));
        let mut assignments = vec![];
        for (id, n) in parsed.ast().expressions().iter() {
            if let Expression::Assignment {
                target,
                value,
                operator: AssignmentOperator::Assign,
                ..
            } = n.payload()
                && inside(n.span(), scan)
                && n.span().end() < before
                && reference(*target)? == current
            {
                assignments.push((n.span().start(), id, *value));
            }
        }
        assignments.sort_by_key(|(start, _, _)| *start);
        let selected = assignments
            .last()
            .map(|(_, _, value)| *value)
            .or_else(|| declaration(parsed, current));
        let Some(id) = selected else {
            break Birth::Parameter(current);
        };
        match parsed.ast().expressions().get(id).unwrap().payload() {
            Expression::Name => {
                let moved = reference(id)?;
                need(
                    moved != current,
                    "formation real distinct Move roots",
                    (current, moved),
                )?;
                aliases.push((id, moved));
                current = moved;
                before = span(parsed, id).start();
            }
            Expression::Call { .. } => break Birth::Call(id),
            other => return Err(failure("formation original birth", other)),
        }
    };
    Ok((birth, aliases))
}

fn drop_birth(
    parsed: &ParsedFile,
    scan: Span,
    root: Span,
    point: Point,
    reference: &impl Fn(ExpressionId) -> Result<Span, Failure>,
) -> Result<Birth, Failure> {
    let boundary = match point {
        Point::Loop(at) | Point::Call(at) => at.start(),
        Point::Statement(at) => at.end() + 1,
        Point::Entry(at) => at.start(),
        Point::Expression(at) => {
            let assignments = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Expression::Assignment { value, .. } if span(parsed, *value) == at => {
                        Some(n.span().start())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            need(
                assignments.len() <= 1,
                "formation original RHS Drop assignment",
                &assignments,
            )?;
            assignments.first().copied().unwrap_or(at.end() + 1)
        }
    };
    birth_at(parsed, scan, root, boundary, reference).map(|(birth, _)| birth)
}
pub(super) fn alias_drop_origins(input: &Inputs) -> Result<(Span, Span), Failure> {
    let alias = one(
        input.drops.iter().filter(|d| d.root == input.site.root),
        "original alias Drop FE identity",
    )?;
    let rebound = one(
        input.drops.iter().filter(|d| d.root != input.site.root),
        "original rebound Drop FE identity",
    )?;
    Ok((alias.origin, rebound.origin))
}
