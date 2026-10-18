//! Original local declaration chains and source-order For sites; binding and form stay distinct.
use super::*;
use lang_frontend::parser::{
    ForBinding, FunctionBody, FunctionForm, Item, NameMarker, Statement, VariableKind,
};
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Local {
    pub marker: Span,
    pub initializer: ExpressionId,
    pub kind: VariableKind,
}
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Loop {
    pub statement: StatementId,
    pub source: ExpressionId,
    pub span: Span,
    pub bindings: [Span; 2],
    pub guard: Local,
    pub guard_span: Span,
    pub kind: E,
    pub point: DropPoint,
}
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests) struct Scanner {
    pub name: Span,
    pub span: Span,
    pub factory: Span,
    pub birth: Local,
    pub root: Local,
    pub birth_span: Span,
    pub loops: Vec<Loop>,
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn within(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn span(
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<Span, Failure> {
    parsed
        .ast()
        .expressions()
        .get(id)
        .map(|n| n.span())
        .map_err(|e| failure("local future original expression", e))
}
fn declaration(parsed: &ParsedFile, id: StatementId) -> Result<Local, Failure> {
    let n = parsed
        .ast()
        .statements()
        .get(id)
        .map_err(|e| failure("local future statement", e))?;
    let Statement::LocalVariable { declaration } = n.payload() else {
        return Err(failure("local future actual local statement", n));
    };
    let n = parsed
        .ast()
        .items()
        .get(*declaration)
        .map_err(|e| failure("local future declaration", e))?;
    let Item::Variable {
        kind,
        name: NameMarker::Present(marker),
        initializer,
        ..
    } = n.payload()
    else {
        return Err(failure("local future actual variable declaration", n));
    };
    Ok(Local {
        marker: *marker,
        initializer: *initializer,
        kind: *kind,
    })
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn scanners(
    (binding, exit, continuation): (usize, usize, usize),
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<Vec<Scanner>, Failure> {
    let function = |wanted: &str| {
        one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                match n.payload() {
                    Item::Function {
                        name: NameMarker::Present(name),
                        form,
                        parameters,
                        ..
                    } if sources.slice(*name) == Ok(wanted) && parameters.is_empty() => {
                        Some((n.span(), *name, form))
                    }
                    _ => None,
                }
            }),
            "local future original zero-argument function",
        )
    };
    [false, true]
        .into_iter()
        .map(|empty| {
            let (scanner, name, form) = function(if empty { "scanEmpty" } else { "scan" })?;
            let factory = function(if empty { "makeEmpty" } else { "make" })?.1;
            let scope = match form {
                FunctionForm::ImplicitUnitBlock(b)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(b),
                    ..
                } => *b,
                _ => return Err(failure("local future original scanner block", form)),
            };
            let Statement::Block { elements } = parsed
                .ast()
                .statements()
                .get(scope)
                .map_err(|e| failure("local future scope", e))?
                .payload()
            else {
                return Err(failure("local future actual Block", scope));
            };
            let first = one(
                elements
                    .iter()
                    .copied()
                    .filter(|id| {
                        parsed
                            .ast()
                            .statements()
                            .get(*id)
                            .is_ok_and(|n| matches!(n.payload(), Statement::For { .. }))
                    })
                    .take(1),
                "local future first actual For",
            )?;
            let locals = elements
                .iter()
                .copied()
                .take_while(|id| *id != first)
                .filter(|id| {
                    parsed
                        .ast()
                        .statements()
                        .get(*id)
                        .is_ok_and(|n| matches!(n.payload(), Statement::LocalVariable { .. }))
                })
                .collect::<Vec<_>>();
            need(
                locals.len() == if binding == 2 { 2 } else { 1 },
                "local future actual binding chain",
                &locals,
            )?;
            let birth = declaration(parsed, locals[0])?;
            let root = declaration(
                parsed,
                *locals
                    .last()
                    .ok_or_else(|| failure("local future actual root declaration", &locals))?,
            )?;
            let kind = if binding == 1 {
                VariableKind::Var
            } else {
                VariableKind::Val
            };
            need(
                root.kind == kind
                    && birth.kind == kind
                    && sources.slice(root.marker) == Ok("source"),
                "local future binding selects val/var/alias independently of form",
                (&root, &birth),
            )?;
            if binding == 2 {
                need(
                    sources.slice(birth.marker) == Ok("original")
                        && sources.slice(span(parsed, root.initializer)?) == Ok("original")
                        && parsed
                            .ast()
                            .expressions()
                            .get(root.initializer)
                            .is_ok_and(|n| matches!(n.payload(), Expression::Name)),
                    "local future original alias Name Move chain",
                    (&root, &birth),
                )?;
            } else {
                need(
                    root.marker == birth.marker && root.initializer == birth.initializer,
                    "local future direct initializer",
                    (&root, &birth),
                )?;
            }
            let node = parsed
                .ast()
                .expressions()
                .get(birth.initializer)
                .map_err(|e| failure("local future factory Call", e))?;
            need(
                matches!(node.payload(),Expression::Call{arguments,..} if arguments.is_empty()),
                "local future actual zero-argument Call birth",
                node,
            )?;
            let mut loops = Vec::new();
            for &statement in elements {
                let node = parsed
                    .ast()
                    .statements()
                    .get(statement)
                    .map_err(|e| failure("local future statement", e))?;
                let Statement::For {
                    source,
                    body,
                    binding: ForBinding::Destructuring { names, .. },
                    ..
                } = node.payload()
                else {
                    continue;
                };
                let [NameMarker::Present(key), NameMarker::Present(value)] = names.as_slice()
                else {
                    return Err(failure("local future actual K/V markers", names));
                };
                need(
                    parsed
                        .ast()
                        .expressions()
                        .get(*source)
                        .is_ok_and(|n| matches!(n.payload(), Expression::Name))
                        && sources.slice(span(parsed, *source)?) == Ok("source"),
                    "local future original source Name",
                    source,
                )?;
                let body_node = parsed
                    .ast()
                    .statements()
                    .get(*body)
                    .map_err(|e| failure("local future body", e))?;
                let Statement::Block { elements: children } = body_node.payload() else {
                    return Err(failure("local future actual body Block", body_node));
                };
                let guard = declaration(
                    parsed,
                    *children
                        .first()
                        .ok_or_else(|| failure("local future Guard", children))?,
                )?;
                need(
                    sources.slice(guard.marker) == Ok("local")
                        && sources.slice(span(parsed, guard.initializer)?) == Ok("Guard()"),
                    "local future original Guard declaration",
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
                            _ => return None,
                        };
                        within(body_node.span(), n.span()).then_some(kind)
                    })
                    .collect::<Vec<_>>();
                let kind = match transfers.as_slice() {
                    [] => E::Fallthrough,
                    [kind] => *kind,
                    _ => return Err(failure("local future finite actual transfer", transfers)),
                };
                need(
                    matches!(
                        (if loops.is_empty() { exit } else { 0 }, kind),
                        (0, E::Fallthrough) | (1, E::Continue(_)) | (2, E::Break(_))
                    ),
                    "local future original first/second exit",
                    kind,
                )?;
                let point = match kind {
                    E::Fallthrough => DropPoint::AfterStatement(*body),
                    E::Continue(e) | E::Break(e) => DropPoint::ControlTransfer(e),
                    other => return Err(failure("local future actual exit", other)),
                };
                let guard_span = span(parsed, guard.initializer)?;
                loops.push(Loop {
                    statement,
                    source: *source,
                    span: node.span(),
                    bindings: [*key, *value],
                    guard,
                    guard_span,
                    kind,
                    point,
                });
            }
            need(
                loops.len() == if continuation == 2 { 2 } else { 1 },
                "local future continuation form determines real For count",
                &loops,
            )?;
            let birth_span = span(parsed, birth.initializer)?;
            Ok(Scanner {
                name,
                span: scanner,
                factory,
                birth,
                root,
                birth_span,
                loops,
            })
        })
        .collect()
}
pub(in crate::ssa::map_provider_exits_pipeline_tests) fn argument(
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<ExpressionId, Failure> {
    let node = parsed
        .ast()
        .expressions()
        .get(id)
        .map_err(|e| failure("local future original Borrow call", e))?;
    let Expression::Call { arguments, .. } = node.payload() else {
        return Err(failure("local future actual Borrow Call", node));
    };
    let [argument] = arguments.as_slice() else {
        return Err(failure("local future actual Borrow arity", arguments));
    };
    Ok(argument.value)
}
