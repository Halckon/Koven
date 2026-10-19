//! Original creation/consume/invoke syntax, owner declaration and actual provider site.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{Case, Kind, Shape};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement,
    },
    source::{SourceMap, Span},
};
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Site {
    pub statement: StatementId,
    pub span: Span,
    pub source: ExpressionId,
    pub body: StatementId,
    pub bindings: [Span; 2],
    pub read: ExpressionId,
    pub arguments: [ExpressionId; 2],
}
#[derive(Debug)]
pub(super) struct Input {
    pub birth_span: Span,
    pub entry: Span,
    pub reader: Span,
    pub birth: ExpressionId,
    pub root: Option<Span>,
    pub construction: Option<ExpressionId>,
    pub initial: Option<Span>,
    pub put: Option<ExpressionId>,
    pub lambda: Option<ExpressionId>,
    pub callee: Option<ExpressionId>,
    pub factory: Option<Span>,
    pub produce: Option<ExpressionId>,
    pub argument: Option<ExpressionId>,
    pub site: Option<Site>,
    pub size: Option<ExpressionId>,
    pub after: Span,
}
pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
pub(super) fn inside(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() < inner.start()
        && inner.end() < outer.end()
}
fn named_function(
    sources: &SourceMap,
    parsed: &ParsedFile,
    name: &str,
) -> Result<(Span, StatementId), Failure> {
    let n=one(parsed.roots().iter().filter_map(|id|parsed.ast().items().get(*id).ok()).filter(|n|matches!(n.payload(),Item::Function{name:NameMarker::Present(marker),..}if sources.slice(*marker)==Ok(name))),"birth original top-level function")?;
    let Item::Function { form, .. } = n.payload() else {
        unreachable!()
    };
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return Err(failure("birth original callable block", n)),
    };
    Ok((n.span(), body))
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
        "birth actual source Call",
    )
}
fn local(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    name: &str,
) -> Result<(Span, ExpressionId), Failure> {
    one(
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(marker),
                    initializer,
                    ..
                } if inside(scope, n.span()) && sources.slice(*marker) == Ok(name) => {
                    Some((*marker, *initializer))
                }
                _ => None,
            }),
        "birth actual local declaration",
    )
}
pub(super) fn input(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<Input, Failure> {
    let (entry, _body) = named_function(sources, parsed, "entry")?;
    let (reader, _) = named_function(sources, parsed, "read")?;
    let (root, birth) = if case.shape == Shape::Direct {
        (
            None,
            call(
                sources,
                parsed,
                entry,
                match case.kind {
                    Kind::FunctionValue => "factory",
                    Kind::Consume => "initial.consume",
                    _ => {
                        if case.mutable {
                            "mutableMapOf"
                        } else {
                            "mapOf"
                        }
                    }
                },
            )?,
        )
    } else {
        let (marker, value) = local(sources, parsed, entry, "source")?;
        (Some(marker), value)
    };
    let (construction, initial, put) = match case.kind {
        Kind::New => (Some(birth), None, None),
        Kind::Consume => {
            let (marker, value) = local(sources, parsed, entry, "initial")?;
            (
                Some(value),
                Some(marker),
                Some(call(sources, parsed, entry, "initial.put")?),
            )
        }
        Kind::Updated => (
            Some(birth),
            root,
            Some(call(sources, parsed, entry, "source.put")?),
        ),
        Kind::FunctionValue => (None, None, None),
    };
    let (lambda, callee, factory, produce, argument) = if case.kind == Kind::FunctionValue {
        let (marker, lambda) = local(sources, parsed, entry, "factory")?;
        need(
            matches!(
                parsed.ast().expressions().get(lambda).unwrap().payload(),
                Expression::Lambda { .. }
            ),
            "birth actual callable is original lambda",
            lambda,
        )?;
        let Expression::Call {
            callee, arguments, ..
        } = parsed.ast().expressions().get(birth).unwrap().payload()
        else {
            return Err(failure("birth actual function-value Call", birth));
        };
        need(
            arguments.is_empty() && sources.slice(span(parsed, *callee)) == Ok("factory"),
            "birth actual zeroarg function-value use",
            birth,
        )?;
        (
            Some(lambda),
            Some(*callee),
            Some(marker),
            Some(call(sources, parsed, span(parsed, lambda), "produce")?),
            Some(call(sources, parsed, span(parsed, lambda), "argument")?),
        )
    } else {
        (None, None, None, None, None)
    };
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, n)| matches!(n.payload(), Statement::For { .. }))
        .collect::<Vec<_>>();
    need(
        loops.len() == usize::from(case.shape != Shape::Control),
        "birth exact original provider/control count",
        &loops,
    )?;
    let site = if let Some((statement, n)) = loops.first() {
        let Statement::For {
            source,
            body,
            binding: ForBinding::Destructuring { names, .. },
            ..
        } = n.payload()
        else {
            return Err(failure("birth destructuring For", n));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("birth actual K/V syntax", names));
        };
        need(
            if case.shape == Shape::Direct {
                *source == birth
            } else {
                sources.slice(span(parsed, *source)) == Ok("source")
                    && matches!(
                        parsed.ast().expressions().get(*source).unwrap().payload(),
                        Expression::Name
                    )
            },
            "birth For uses actual result or original local",
            source,
        )?;
        let read = call(sources, parsed, n.span(), "read")?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(read).unwrap().payload()
        else {
            unreachable!()
        };
        let [a, b] = arguments.as_slice() else {
            return Err(failure("birth original reader arguments", arguments));
        };
        Some(Site {
            statement: *statement,
            span: n.span(),
            source: *source,
            body: *body,
            bindings: [*key, *value],
            read,
            arguments: [a.value, b.value],
        })
    } else {
        None
    };
    let size = if case.shape == Shape::Control {
        Some(one(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| match n.payload() {
                    Expression::Member { name_span, .. }
                        if inside(entry, n.span()) && sources.slice(*name_span) == Ok("size") =>
                    {
                        Some(id)
                    }
                    _ => None,
                }),
            "birth original no-For size",
        )?)
    } else {
        None
    };
    let after = one(
        parsed.ast().expressions().iter().filter_map(|(_, n)| {
            (inside(entry, n.span()) && sources.slice(n.span()) == Ok("println(\"after\")"))
                .then_some(n.span())
        }),
        "birth original after marker",
    )?;
    Ok(Input {
        birth_span: span(parsed, birth),
        entry,
        reader,
        birth,
        root,
        construction,
        initial,
        put,
        lambda,
        callee,
        factory,
        produce,
        argument,
        site,
        size,
        after,
    })
}
