//! The original Int overload and nested argument Call, independent of arena positions.
use super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, ForBinding, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};

#[derive(Debug)]
pub(super) struct Function {
    pub span: Span,
    pub name: Span,
    pub parameter: Option<Span>,
}

#[derive(Debug)]
pub(super) struct Loop {
    pub statement: StatementId,
    pub span: Span,
    pub body: StatementId,
    pub bindings: [Span; 2],
    pub read: ExpressionId,
    pub arguments: [ExpressionId; 2],
}

pub(super) struct Input {
    pub entry: Function,
    pub argument: Function,
    pub factory: Function,
    pub rejected: Function,
    pub reader: Function,
    pub produce: ExpressionId,
    pub value: ExpressionId,
    pub loop_site: Option<Loop>,
    pub size: Option<Span>,
    pub after: Span,
}

pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}

pub(super) fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() > outer.start()
        && inner.end() < outer.end()
}

pub(super) fn function(
    sources: &SourceMap,
    parsed: &ParsedFile,
    wanted: &str,
    parameter: Option<&str>,
) -> Result<Function, Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            let Item::Function {
                name: NameMarker::Present(name),
                parameters,
                ..
            } = node.payload()
            else {
                return None;
            };
            if sources.slice(*name) != Ok(wanted) {
                return None;
            }
            let marker = match (parameter, parameters.as_slice()) {
                (None, []) => None,
                (Some(wanted), [p])
                    if p.mode_marker.is_none()
                        && sources.slice(parsed.ast().type_refs().get(p.type_ref).ok()?.span())
                            == Ok(wanted) =>
                {
                    let NameMarker::Present(name) = p.name else {
                        return None;
                    };
                    Some(name)
                }
                _ => return None,
            };
            Some(Function {
                span: node.span(),
                name: *name,
                parameter: marker,
            })
        }),
        "argument factory original function and parameter syntax",
    )
}

fn calls(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scope: Span,
    name: &str,
) -> Result<ExpressionId, Failure> {
    one(
        parsed.ast().expressions().iter().filter_map(|(id, node)| {
            let Expression::Call { callee, .. } = node.payload() else {
                return None;
            };
            (inside(node.span(), scope) && sources.slice(span(parsed, *callee)) == Ok(name))
                .then_some(id)
        }),
        "argument factory actual unique source Call",
    )
}

pub(super) fn input(
    sources: &SourceMap,
    parsed: &ParsedFile,
    control: bool,
) -> Result<Input, Failure> {
    let entry = function(sources, parsed, "entry", None)?;
    let argument = function(sources, parsed, "argument", None)?;
    let factory = function(sources, parsed, "produce", Some("Int"))?;
    let rejected = function(sources, parsed, "produce", Some("Boolean"))?;
    let reader = one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            matches!(node.payload(), Item::Function {name:NameMarker::Present(name), parameters, ..}
            if sources.slice(*name) == Ok("read") && parameters.len() == 2)
            .then(|| {
                let Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } = node.payload()
                else {
                    unreachable!()
                };
                Function {
                    span: node.span(),
                    name: *name,
                    parameter: None,
                }
            })
        }),
        "argument factory actual component reader",
    )?;
    let produce = calls(sources, parsed, entry.span, "produce")?;
    let value = calls(sources, parsed, entry.span, "argument")?;
    let Expression::Call { arguments, .. } =
        parsed.ast().expressions().get(produce).unwrap().payload()
    else {
        unreachable!()
    };
    need(
        matches!(arguments.as_slice(), [argument] if argument.value == value && argument.mode_marker.is_none() && argument.named_prefix.is_none())
            && matches!(parsed.ast().expressions().get(value).unwrap().payload(), Expression::Call {arguments, ..} if arguments.is_empty()),
        "argument factory source evaluates actual nested zeroarg Call",
        produce,
    )?;
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            matches!(node.payload(), Statement::For { .. }).then_some((id, node))
        })
        .collect::<Vec<_>>();
    need(
        loops.len() == usize::from(!control),
        "argument factory actual provider/control distinction",
        &loops,
    )?;
    let loop_site = if control {
        None
    } else {
        let (statement, node) = loops[0];
        let Statement::For {
            binding: ForBinding::Destructuring { names, .. },
            source,
            body,
            ..
        } = node.payload()
        else {
            return Err(failure("argument factory original destructuring For", node));
        };
        let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
            return Err(failure("argument factory K/V markers", names));
        };
        let read = calls(sources, parsed, node.span(), "read")?;
        let Expression::Call { arguments, .. } =
            parsed.ast().expressions().get(read).unwrap().payload()
        else {
            unreachable!()
        };
        need(
            *source == produce && arguments.len() == 2,
            "argument factory For source is the selected nested Call",
            node,
        )?;
        Some(Loop {
            statement,
            span: node.span(),
            body: *body,
            bindings: [*key, *value],
            read,
            arguments: [arguments[0].value, arguments[1].value],
        })
    };
    let size = if control {
        Some(one(
            parsed.ast().expressions().iter().filter_map(|(_, node)| {
                (inside(node.span(), entry.span)
                    && matches!(node.payload(), Expression::Member {name_span, ..}
            if sources.slice(*name_span) == Ok("size")))
                .then_some(node.span())
            }),
            "argument factory control original size",
        )?)
    } else {
        None
    };
    let after = one(parsed.ast().expressions().iter().filter_map(|(_, node)| {
        let Expression::Call {callee, arguments, ..} = node.payload() else { return None; };
        (inside(node.span(), entry.span) && sources.slice(span(parsed, *callee)) == Ok("println")
            && matches!(arguments.as_slice(), [argument] if sources.slice(span(parsed, argument.value)) == Ok("\"after\"")))
            .then_some(node.span())
    }), "argument factory original after marker")?;
    Ok(Input {
        entry,
        argument,
        factory,
        rejected,
        reader,
        produce,
        value,
        loop_site,
        size,
        after,
    })
}
