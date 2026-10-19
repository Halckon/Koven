//! The thunk is selected by the real lambda initializer and LambdaBody identity.
use super::super::{
    births::ast::Site,
    fixture::{Failure, failure, need},
    query_inputs::one,
    temporary_arguments::ast::{Function, function},
};
use super::Case;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};
#[derive(Debug)]
pub(super) struct Input {
    pub entry: Function,
    pub scan: Function,
    pub make: Function,
    pub produce: Function,
    pub reader: Span,
    pub observe: Span,
    pub scan_call: ExpressionId,
    pub callback: Span,
    pub lambda: ExpressionId,
    pub body: StatementId,
    pub invoke: ExpressionId,
    pub callee: ExpressionId,
    pub source: Span,
    pub birth: ExpressionId,
    pub birth_span: Span,
    pub site: Option<Site>,
    pub observe_call: Option<ExpressionId>,
    pub done: ExpressionId,
}
pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
pub(super) fn inside(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
fn local(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
) -> Result<(Span, ExpressionId), Failure> {
    one(
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if inside(scope, n.span()) && sources.slice(*name) == Ok(wanted) => {
                    Some((*name, *initializer))
                }
                _ => None,
            }),
        "lambda actual local declaration/initializer",
    )
}
fn call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    wanted: &str,
) -> Result<ExpressionId, Failure> {
    one(
        parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| match n.payload() {
                Expression::Call { callee, .. }
                    if inside(scope, n.span())
                        && sources.slice(span(parsed, *callee)) == Ok(wanted) =>
                {
                    Some(id)
                }
                _ => None,
            }),
        "lambda actual original Call",
    )
}
pub(super) fn input(c: &Case, sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
    let entry = function(sources, parsed, "entry", None)?;
    let scan = function(sources, parsed, "scan", None)?;
    let make = function(sources, parsed, "make", None)?;
    let produce = function(sources, parsed, "produce", Some("Int"))?;
    let named = |wanted| {
        one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..}if sources.slice(*name)==Ok(wanted))).map(|n|n.span()),"lambda actual top-level consumer")
    };
    let reader = named("read")?;
    let observe = named("observe")?;
    let scan_call = call(sources, parsed, entry.span, "scan")?;
    let (callback, lambda) = local(sources, parsed, scan.span, "callback")?;
    let Expression::Lambda { body, .. } = parsed.ast().expressions().get(lambda).unwrap().payload()
    else {
        return Err(failure("lambda callback actual Lambda initializer", lambda));
    };
    let body = *body;
    let node = parsed.ast().statements().get(body).unwrap();
    need(
        matches!(node.payload(), Statement::LambdaBody { .. }),
        "lambda actual LambdaBody, not outer function",
        node,
    )?;
    let (source, birth) = local(sources, parsed, node.span(), "source")?;
    need(
        birth == call(sources, parsed, node.span(), "make")?,
        "lambda named Map initializer exact actual make()",
        birth,
    )?;
    let invoke = call(sources, parsed, scan.span, "callback")?;
    let Expression::Call {
        callee, arguments, ..
    } = parsed.ast().expressions().get(invoke).unwrap().payload()
    else {
        unreachable!()
    };
    need(
        arguments.is_empty() && !inside(span(parsed, lambda), span(parsed, invoke)),
        "lambda callback invoked outside its own body with no arguments",
        invoke,
    )?;
    let callee = *callee;
    let site = if c.provider {
        let (statement, n, names, source_expr, loop_body) = one(
            parsed
                .ast()
                .statements()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Statement::For {
                        binding: ForBinding::Destructuring { names, .. },
                        source,
                        body,
                        ..
                    } if inside(span(parsed, lambda), n.span()) => {
                        Some((id, n, names, *source, *body))
                    }
                    _ => None,
                }),
            "lambda actual internal For",
        )?;
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("lambda actual K/V declarations", names));
        };
        let read = call(sources, parsed, n.span(), "read")?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(read).unwrap().payload()
        else {
            unreachable!()
        };
        let [key_arg, value_arg] = arguments.as_slice() else {
            return Err(failure("lambda actual ordered read arguments", arguments));
        };
        Some(Site {
            statement,
            span: n.span(),
            source: source_expr,
            body: loop_body,
            bindings: [*key, *value],
            read,
            arguments: [key_arg.value, value_arg.value],
        })
    } else {
        None
    };
    let observe_call = if c.provider {
        None
    } else {
        Some(call(sources, parsed, span(parsed, lambda), "observe")?)
    };
    let done = call(sources, parsed, span(parsed, lambda), "println")?;
    need(
        span(parsed, birth).end() < span(parsed, done).start()
            && span(parsed, done).end() < span(parsed, lambda).end(),
        "lambda Map birth and done belong to actual thunk body",
        (birth, done, lambda),
    )?;
    Ok(Input {
        entry,
        scan,
        make,
        produce,
        reader,
        observe,
        scan_call,
        callback,
        lambda,
        body,
        invoke,
        callee,
        source,
        birth,
        birth_span: span(parsed, birth),
        site,
        observe_call,
        done,
    })
}
