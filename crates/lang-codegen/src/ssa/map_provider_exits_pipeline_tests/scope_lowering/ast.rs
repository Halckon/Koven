//! Actual scope, parameter, caller and body syntax from each frozen source.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::Case;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
    },
    source::{SourceMap, Span},
};
#[derive(Debug)]
pub(super) struct Function {
    pub span: Span,
    pub name: Span,
    pub parameters: Vec<Span>,
    pub body: StatementId,
}
#[derive(Debug)]
pub(super) struct Call {
    pub id: ExpressionId,
    pub arguments: Vec<ExpressionId>,
}
#[derive(Debug)]
pub(super) struct Site {
    pub statement: StatementId,
    pub span: Span,
    pub source: ExpressionId,
    pub body: StatementId,
    pub bindings: [Span; 2],
}
#[derive(Debug)]
pub(super) struct Input {
    pub scan: Function,
    pub entry: Function,
    pub reader: Option<Function>,
    pub observe: Option<Function>,
    pub make: Option<Function>,
    pub count: Option<Function>,
    pub callers: Vec<Call>,
    pub site: Option<Site>,
    pub read: Option<Call>,
    pub observe_call: Option<Call>,
    pub count_call: Option<Call>,
    pub assignment: Option<ExpressionId>,
    pub clone: Option<ExpressionId>,
    pub cloned: Option<Span>,
    pub clone_print: Option<Call>,
    pub early_return: Option<ExpressionId>,
}
pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
pub(super) fn inside(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
pub(super) fn function(
    sources: &SourceMap,
    parsed: &ParsedFile,
    wanted: &str,
) -> Result<Function, Failure> {
    let n=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..}if sources.slice(*name)==Ok(wanted))),"scope actual original top-level function")?;
    let Item::Function {
        name: NameMarker::Present(name),
        parameters,
        form,
        ..
    } = n.payload()
    else {
        unreachable!()
    };
    let parameters = parameters
        .iter()
        .map(|p| match p.name {
            NameMarker::Present(name) => Ok(name),
            _ => Err(failure("scope actual named parameter", p)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let body = match form {
        FunctionForm::ImplicitUnitBlock(b)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(b),
            ..
        } => *b,
        _ => return Err(failure("scope original callable block", form)),
    };
    Ok(Function {
        span: n.span(),
        name: *name,
        parameters,
        body,
    })
}
pub(super) fn calls(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
) -> Vec<Call> {
    let mut calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, n)| match n.payload() {
            Expression::Call {
                callee, arguments, ..
            } if inside(scope, n.span()) && sources.slice(span(parsed, *callee)) == Ok(wanted) => {
                Some(Call {
                    id,
                    arguments: arguments.iter().map(|a| a.value).collect(),
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|c| span(parsed, c.id).start());
    calls
}
fn one_call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
) -> Result<Call, Failure> {
    one(
        calls(sources, parsed, scope, wanted).into_iter(),
        "scope exact original Call",
    )
}
pub(super) fn input(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<Input, Failure> {
    let scan = function(sources, parsed, "scan")?;
    let entry = function(sources, parsed, "entry")?;
    let callers = calls(sources, parsed, entry.span, "scan");
    need(
        callers.len() == if case.gate == 2 { 1 } else { 2 }
            && scan.parameters.len() == if case.gate == 2 { 0 } else { 2 },
        "scope exact original entry invocations and scanner ABI",
        &callers,
    )?;
    if case.gate != 2 {
        for (c, wanted) in callers.iter().zip(["true", "false"]) {
            let [map, flag] = c.arguments.as_slice() else {
                return Err(failure("scope actual two scan arguments", c));
            };
            need(
                sources.slice(span(parsed, *flag)) == Ok(wanted)
                    && sources.slice(span(parsed, *map)) == Ok("make()"),
                "scope original entry encodes true/false with real make calls",
                c,
            )?;
        }
    }
    let sites = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(statement, n)| match n.payload() {
            Statement::For {
                binding: ForBinding::Destructuring { names, .. },
                source,
                body,
                ..
            } if inside(scan.span, n.span()) => Some((statement, n, names, *source, *body)),
            _ => None,
        })
        .collect::<Vec<_>>();
    need(
        sites.len() == usize::from(case.provider),
        "scope original map For/control distinction",
        sites.len(),
    )?;
    let site = if let Some((statement, n, names, source, body)) = sites.first() {
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("scope actual K/V markers", names));
        };
        Some(Site {
            statement: *statement,
            span: n.span(),
            source: *source,
            body: *body,
            bindings: [*key, *value],
        })
    } else {
        None
    };
    let early_return = if case.gate == 2 {
        Some(one(
            parsed.ast().expressions().iter().filter_map(|(id, n)| {
                (inside(scan.span, n.span())
                    && matches!(n.payload(), Expression::Return { value: None, .. }))
                .then_some(id)
            }),
            "scope dead-tail preceding Return",
        )?)
    } else {
        None
    };
    if let (Some(ret), Some(site)) = (early_return, &site) {
        need(
            span(parsed, ret).end() < site.span.start(),
            "scope legal Return precedes unacquired source",
            (ret, site),
        )?;
    }
    let (reader, observe, make) = if case.gate == 2 {
        (None, None, None)
    } else {
        (
            Some(function(sources, parsed, "read")?),
            Some(function(sources, parsed, "observe")?),
            Some(function(sources, parsed, "make")?),
        )
    };
    let read = if case.gate == 17 || case.gate == 18 || case.provider && case.gate != 2 {
        Some(one_call(sources, parsed, scan.span, "read")?)
    } else {
        None
    };
    let observe_call = if !case.provider && matches!(case.gate, 12 | 13 | 14 | 16) {
        Some(one_call(sources, parsed, scan.span, "observe")?)
    } else {
        None
    };
    let (count, count_call, assignment) = if case.gate == 17 {
        let assignment = one(
            parsed.ast().expressions().iter().filter_map(|(id, n)| {
                (inside(scan.span, n.span()) && sources.slice(n.span()) == Ok("n=n+1"))
                    .then_some(id)
            }),
            "scope actual scalar assignment",
        )?;
        (
            Some(function(sources, parsed, "checkCount")?),
            Some(one_call(sources, parsed, scan.span, "checkCount")?),
            Some(assignment),
        )
    } else {
        (None, None, None)
    };
    let (clone, cloned, clone_print) = if case.gate == 18 {
        let (marker, value) = one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if inside(scan.span, n.span()) && sources.slice(*name) == Ok("cloned") => {
                        Some((*name, *initializer))
                    }
                    _ => None,
                }),
            "scope original independent cloned local",
        )?;
        (
            Some(value),
            Some(marker),
            Some(one_call(sources, parsed, scan.span, "println")?),
        )
    } else {
        (None, None, None)
    };
    Ok(Input {
        scan,
        entry,
        reader,
        observe,
        make,
        count,
        callers,
        site,
        read,
        observe_call,
        count_call,
        assignment,
        clone,
        cloned,
        clone_print,
        early_return,
    })
}
