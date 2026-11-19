//! Narrow ordinary branch admission for locally borrowed RangeView metadata.
use super::{super::short_circuit, *};
use lang_frontend::parser::{LiteralKind, PrefixOperator};

/// Follow only plain lexical blocks. An intervening control body is a separate proof.
fn enclosing_blocks(
    parsed: &ParsedFile,
    scope: StatementId,
) -> impl Iterator<Item = StatementId> + '_ {
    std::iter::successors(Some(scope), |child| {
        parsed.ast().statements().iter().find_map(|(id, node)| {
            matches!(node.payload(), Statement::Block { elements } if elements.contains(child))
                .then_some(id)
        })
    })
}

pub(in crate::ssa) fn scope(
    parsed: &ParsedFile,
    marker: Span,
    proven: impl Fn(Span, StatementId) -> bool,
) -> Option<StatementId> {
    statement::scope_for(parsed, marker, true, proven)
}

struct Wrapper {
    outer: StatementId,
    expression: ExpressionId,
    body: StatementId,
}

/// The direct statement if whose arm owns the outermost plain block on this path.
fn wrapper(parsed: &ParsedFile, scope: StatementId) -> Option<Wrapper> {
    let ast = parsed.ast();
    let protected = enclosing_blocks(parsed, scope).last()?;
    let node = ast.statements().get(protected).ok()?;
    let body = if matches!(node.payload(), Statement::ControlBody { .. }) {
        protected
    } else {
        ast.statements().iter().find_map(|(id, node)| {
            matches!(node.payload(), Statement::ControlBody { elements } if elements.contains(&protected))
                .then_some(id)
        })?
    };
    ast.statements().iter().find_map(|(block, node)| {
        let (Statement::Block { elements } | Statement::ControlBody { elements }) = node.payload()
        else {
            return None;
        };
        elements.iter().find_map(|statement| {
            let node = ast.statements().get(*statement).ok()?;
            let Statement::Expression { expression } = node.payload() else {
                return None;
            };
            let expression = ungroup(parsed, *expression, node.span()).ok()?;
            let node = ast.expressions().get(expression).ok()?;
            matches!(node.payload(), Expression::If { then_branch, else_branch, .. }
                if *then_branch == body || *else_branch == Some(body))
            .then_some(Wrapper {
                outer: block,
                expression,
                body,
            })
        })
    })
}

/// Candidate scopes only. The if path is validated separately before admission.
pub(in crate::ssa) fn enclosing_scopes(
    parsed: &ParsedFile,
    scope: StatementId,
) -> impl Iterator<Item = StatementId> + '_ {
    std::iter::successors(Some(scope), move |scope| {
        wrapper(parsed, *scope).map(|wrapper| wrapper.outer)
    })
    .flat_map(move |scope| {
        enclosing_blocks(parsed, scope).chain(
            wrapper(parsed, scope)
                .map(|wrapper| wrapper.body)
                .filter(|&body| body != scope),
        )
    })
}

fn unchecked_negation(parsed: &ParsedFile, root: Span) -> bool {
    !parsed.ast().expressions().iter().any(|(_, child)| {
        contains(root, child.span())
            && matches!(child.payload(), Expression::Prefix {
                operator: PrefixOperator::Minus, operand, ..
            } if !parsed.ast().expressions().get(*operand).is_ok_and(|operand| {
                matches!(operand.payload(), Expression::Literal(LiteralKind::Integer(_)))
            }))
    })
}

/// The descriptor call itself has a separate typed effect check.
pub(in crate::ssa) fn readonly_initializer(
    parsed: &ParsedFile,
    expression: ExpressionId,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
) -> bool {
    parsed
        .ast()
        .expressions()
        .get(expression)
        .is_ok_and(|node| {
            matches!(node.payload(), Expression::Call { .. })
                && statement::normal_tree(parsed, node.span(), builtin, owned_call, |id, _| {
                    id == expression
                })
        })
}

/// Collect normally continuing statement ifs in a proven lexical scope.
pub(in crate::ssa) fn normal_controls(
    parsed: &ParsedFile,
    scope: StatementId,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    binding_allowed: impl Fn(StatementId, Span) -> bool,
    allowed: &mut BTreeSet<usize>,
) {
    let Ok(node) = parsed.ast().statements().get(scope) else {
        return;
    };
    let (Statement::Block { elements } | Statement::ControlBody { elements }) = node.payload()
    else {
        return;
    };
    let mut controls: BTreeSet<_> = elements
        .iter()
        .filter_map(|statement| {
            let node = parsed.ast().statements().get(*statement).ok()?;
            let Statement::Expression { expression } = node.payload() else {
                return None;
            };
            let expression = ungroup(parsed, *expression, node.span()).ok()?;
            (allows(parsed, scope, expression, builtin, owned_call, None)
                && unchecked_negation(parsed, node.span()))
            .then_some(expression.index())
        })
        .collect();

    let mut wrapped_scope = scope;
    while let Some(Wrapper {
        outer,
        expression,
        body,
    }) = wrapper(parsed, wrapped_scope)
    {
        // Validate each block before its parent may treat that child as proved.
        let mut checked_child = None;
        let mut arm_scopes =
            enclosing_blocks(parsed, wrapped_scope).chain((body != wrapped_scope).then_some(body));
        let body_is_readonly = arm_scopes.all(|block| {
            let Ok(node) = parsed.ast().statements().get(block) else {
                return false;
            };
            let (Statement::Block { elements } | Statement::ControlBody { elements }) =
                node.payload()
            else {
                return false;
            };
            let readonly = elements.iter().all(|id| {
                if Some(*id) == checked_child {
                    return true;
                }
                let Ok(node) = parsed.ast().statements().get(*id) else {
                    return false;
                };
                if let Some(marker) = binding_marker(parsed, *id) {
                    return binding_allowed(block, marker);
                }
                let Statement::Expression { expression } = node.payload() else {
                    return false;
                };
                let Ok(expression) = ungroup(parsed, *expression, node.span()) else {
                    return false;
                };
                controls.contains(&expression.index())
                    || (builtin(expression) == Some(BuiltinType::Unit)
                        && parsed
                            .ast()
                            .expressions()
                            .get(expression)
                            .is_ok_and(|node| {
                                matches!(node.payload(), Expression::Call { .. })
                                    && statement::normal_tree(
                                        parsed,
                                        node.span(),
                                        builtin,
                                        owned_call,
                                        |_, _| false,
                                    )
                            }))
            });
            checked_child = Some(block);
            readonly
        });
        let Ok(node) = parsed.ast().expressions().get(expression) else {
            return;
        };
        if !body_is_readonly
            || !unchecked_negation(parsed, node.span())
            || !statement::allows_tree(
                parsed,
                outer,
                expression,
                builtin,
                owned_call,
                None,
                Some(body),
            )
        {
            return;
        }
        controls.insert(expression.index());
        // An enclosing if and its condition are checked separately by the binding gate.
        if let Expression::If { condition, .. } = node.payload()
            && let Ok(condition) = ungroup(parsed, *condition, node.span())
            && short_circuit::read_only_root(parsed, condition, builtin, owned_call)
        {
            controls.insert(condition.index());
        }
        wrapped_scope = outer;
    }
    allowed.extend(controls);
}
