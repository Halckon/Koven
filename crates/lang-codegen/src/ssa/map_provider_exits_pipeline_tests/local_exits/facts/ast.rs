//! Shared original AST anchors for local formation and control-transfer queries.
use super::{Failure, failure, need, one};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{DropPoint, IterationExitKind as E},
    parser::{
        Expression, ForBinding, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile,
        Statement, VariableKind,
    },
    source::{SourceMap, Span},
    type_checking::MapIterationKind,
};
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Local {
    pub(in crate::ssa::map_provider_exits_pipeline_tests) marker: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) initializer: ExpressionId,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) kind: VariableKind,
}
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Site {
    pub(in crate::ssa::map_provider_exits_pipeline_tests) scan: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) name: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) factory: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) result: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) map_kind: MapIterationKind,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) entry: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) root: Local,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) birth: Local,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) birth_span: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) statement: StatementId,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) source: ExpressionId,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) span: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) bindings: [Span; 2],
    pub(in crate::ssa::map_provider_exits_pipeline_tests) guard: Local,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) guard_span: Span,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) kind: E,
    pub(in crate::ssa::map_provider_exits_pipeline_tests) point: DropPoint,
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn within(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn expression_span(
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<Span, Failure> {
    parsed
        .ast()
        .expressions()
        .get(id)
        .map(|n| n.span())
        .map_err(|e| failure("G2 original expression", e))
}
fn function<'a>(
    sources: &SourceMap,
    parsed: &'a ParsedFile,
    wanted: &str,
) -> Result<(Span, Span, &'a FunctionForm), Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            match node.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    form,
                    parameters,
                    ..
                } if sources.slice(*name) == Ok(wanted) && parameters.is_empty() => {
                    Some((node.span(), *name, form))
                }
                _ => None,
            }
        }),
        "G2 actual zero-parameter function root",
    )
}
fn local(parsed: &ParsedFile, statement: StatementId) -> Result<Local, Failure> {
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|e| failure("G2 local statement", e))?;
    let Statement::LocalVariable { declaration } = node.payload() else {
        return Err(failure("G2 actual local statement", node));
    };
    let item = parsed
        .ast()
        .items()
        .get(*declaration)
        .map_err(|e| failure("G2 local declaration", e))?;
    let Item::Variable {
        kind,
        name: NameMarker::Present(marker),
        initializer,
        ..
    } = item.payload()
    else {
        return Err(failure("G2 original variable declaration", item));
    };
    Ok(Local {
        marker: *marker,
        initializer: *initializer,
        kind: *kind,
    })
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn sites(
    (mutable, formation, exit): (bool, usize, usize),
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<Vec<Site>, Failure> {
    let entry = function(sources, parsed, "entry")?.0;
    let result = one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            match node.payload() {
                Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } if sources.slice(*name) == Ok("result") && parameters.len() == 2 => Some(*name),
                _ => None,
            }
        }),
        "G2 original result function root",
    )?;
    let map_kind = if mutable {
        MapIterationKind::MutableMap
    } else {
        MapIterationKind::Map
    };
    [false, true]
        .into_iter()
        .map(|empty| {
            let (scan, name, form) =
                function(sources, parsed, if empty { "scanEmpty" } else { "scan" })?;
            let factory = function(sources, parsed, if empty { "makeEmpty" } else { "make" })?.1;
            let scope = match form {
                FunctionForm::ImplicitUnitBlock(b)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(b),
                    ..
                } => *b,
                _ => return Err(failure("G2 original scanner block", form)),
            };
            let Statement::Block { elements } = parsed
                .ast()
                .statements()
                .get(scope)
                .map_err(|e| failure("G2 original scanner scope", e))?
                .payload()
            else {
                return Err(failure("G2 actual scanner Block", scope));
            };
            let statement = one(
                elements.iter().copied().filter(|id| {
                    parsed
                        .ast()
                        .statements()
                        .get(*id)
                        .is_ok_and(|n| matches!(n.payload(), Statement::For { .. }))
                }),
                "G2 one actual scanner For",
            )?;
            let node = parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|e| failure("G2 actual For", e))?;
            let Statement::For {
                source,
                body,
                binding: ForBinding::Destructuring { names, .. },
                ..
            } = node.payload()
            else {
                return Err(failure("G2 original destructured For", node));
            };
            let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice() else {
                return Err(failure("G2 actual K/V markers", names));
            };
            let declarations = elements
                .iter()
                .copied()
                .take_while(|id| *id != statement)
                .filter(|id| {
                    parsed
                        .ast()
                        .statements()
                        .get(*id)
                        .is_ok_and(|n| matches!(n.payload(), Statement::LocalVariable { .. }))
                })
                .collect::<Vec<_>>();
            need(
                declarations.len() == if formation == 2 { 2 } else { 1 },
                "G2 actual local formation chain",
                &declarations,
            )?;
            let root = local(
                parsed,
                *declarations
                    .last()
                    .ok_or_else(|| failure("G2 source declaration", &declarations))?,
            )?;
            let birth = local(parsed, declarations[0])?;
            let wanted = if formation == 1 {
                VariableKind::Var
            } else {
                VariableKind::Val
            };
            need(
                root.kind == wanted
                    && birth.kind == wanted
                    && sources.slice(root.marker) == Ok("source")
                    && sources.slice(expression_span(parsed, *source)?) == Ok("source")
                    && parsed
                        .ast()
                        .expressions()
                        .get(*source)
                        .is_ok_and(|n| matches!(n.payload(), Expression::Name)),
                "G2 original val/var root and Name source",
                (&root, &birth, source),
            )?;
            if formation == 2 {
                need(
                    sources.slice(birth.marker) == Ok("original")
                        && sources.slice(expression_span(parsed, root.initializer)?)
                            == Ok("original")
                        && parsed
                            .ast()
                            .expressions()
                            .get(root.initializer)
                            .is_ok_and(|n| matches!(n.payload(), Expression::Name)),
                    "G2 actual alias Name Move chain",
                    (&root, &birth),
                )?;
            } else {
                need(
                    root.initializer == birth.initializer,
                    "G2 direct actual initializer",
                    (&root, &birth),
                )?;
            }
            let Expression::Call { arguments, .. } = parsed
                .ast()
                .expressions()
                .get(birth.initializer)
                .map_err(|e| failure("G2 original factory call", e))?
                .payload()
            else {
                return Err(failure("G2 selected original factory Call", &birth));
            };
            need(arguments.is_empty(), "G2 actual factory arity", arguments)?;
            let formed = one(
                elements.iter().copied().filter_map(|id| {
                    let n = parsed.ast().statements().get(id).ok()?;
                    let Statement::Expression { expression } = n.payload() else {
                        return None;
                    };
                    let span = expression_span(parsed, *expression).ok()?;
                    (birth.marker.end() < span.start()
                        && span.end() < node.span().start()
                        && sources.slice(span) == Ok("println(\"formed\")"))
                    .then_some(span)
                }),
                "G2 real formation-to-For marker",
            )?;
            need(
                formed.start() > expression_span(parsed, root.initializer)?.end(),
                "G2 source initialized before actual marker",
                formed,
            )?;
            let body_node = parsed
                .ast()
                .statements()
                .get(*body)
                .map_err(|e| failure("G2 original body", e))?;
            let Statement::Block {
                elements: body_elements,
            } = body_node.payload()
            else {
                return Err(failure("G2 actual body block", body_node));
            };
            let guard = local(
                parsed,
                *body_elements
                    .first()
                    .ok_or_else(|| failure("G2 original Guard local", body_elements))?,
            )?;
            need(
                guard.kind == VariableKind::Val
                    && sources.slice(guard.marker) == Ok("local")
                    && sources.slice(expression_span(parsed, guard.initializer)?) == Ok("Guard()"),
                "G2 original Guard declaration",
                &guard,
            )?;
            let transfers = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(id, n)| {
                    let kind = match n.payload() {
                        Expression::Continue { .. } => E::Continue(id),
                        Expression::Break { .. } => E::Break(id),
                        Expression::Return { .. } => E::Return(id),
                        _ => return None,
                    };
                    within(body_node.span(), n.span()).then_some(kind)
                })
                .collect::<Vec<_>>();
            let kind = match transfers.as_slice() {
                [] => E::Fallthrough,
                [kind] => *kind,
                _ => return Err(failure("G2 actual unique transfer", transfers)),
            };
            need(
                matches!(
                    (exit, kind),
                    (0, E::Fallthrough)
                        | (1, E::Continue(_))
                        | (2, E::Break(_))
                        | (3, E::Return(_))
                ),
                "G2 metadata matches actual exit",
                kind,
            )?;
            let point = match kind {
                E::Fallthrough => DropPoint::AfterStatement(*body),
                E::Continue(e) | E::Break(e) | E::Return(e) => DropPoint::ControlTransfer(e),
                _ => return Err(failure("G2 actual body exit", kind)),
            };
            let birth_span = expression_span(parsed, birth.initializer)?;
            let guard_span = expression_span(parsed, guard.initializer)?;
            Ok(Site {
                scan,
                name,
                factory,
                result,
                map_kind,
                entry,
                root,
                birth,
                birth_span,
                statement,
                source: *source,
                span: node.span(),
                bindings: [*key, *value],
                guard,
                guard_span,
                kind,
                point,
            })
        })
        .collect()
}

/// The original Return operand and its two Name arguments, without any lowering inference.
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn returned(
    site: &Site,
    parsed: &ParsedFile,
) -> Result<Option<(ExpressionId, [ExpressionId; 2])>, Failure> {
    let E::Return(id) = site.kind else {
        return Ok(None);
    };
    let node = parsed
        .ast()
        .expressions()
        .get(id)
        .map_err(|e| failure("G2 original Return", e))?;
    let Expression::Return {
        value: Some(value), ..
    } = node.payload()
    else {
        return Err(failure("G2 actual Return operand", node));
    };
    let call = parsed
        .ast()
        .expressions()
        .get(*value)
        .map_err(|e| failure("G2 original result Call", e))?;
    let Expression::Call { arguments, .. } = call.payload() else {
        return Err(failure("G2 actual result Call syntax", call));
    };
    let [key, val] = arguments.as_slice() else {
        return Err(failure("G2 actual result call arity", arguments));
    };
    Ok(Some((*value, [key.value, val.value])))
}
