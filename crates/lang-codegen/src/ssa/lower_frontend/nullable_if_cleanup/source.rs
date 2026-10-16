//! Bounded source/fact qualification, without evaluating or rebuilding a condition.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    parser::{FunctionBody, FunctionForm, VariableKind},
};

#[allow(clippy::too_many_arguments)]
pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    members: [DropFact; 2],
    root: SymbolId,
    seed: SymbolId,
    replacement: ExpressionId,
    first_if: ExpressionId,
    second_if: ExpressionId,
    arm: usize,
) -> Option<GroupPlan> {
    let storage = typed.symbol_type(root)?;
    let TypeKind::Nullable(inner) = typed.types().get(storage)? else {
        return None;
    };
    let TypeKind::Nominal { nominal, arguments } = typed.types().get(*inner)? else {
        return None;
    };
    if !arguments.is_empty()
        || typed
            .nominals()
            .iter()
            .find(|item| item.id() == *nominal)?
            .kind()
            != NominalKind::Class
        || typed.copyability(storage) != Some(Copyability::MoveOnly)
        || typed.symbol_type(seed) != Some(storage)
    {
        return None;
    }
    let Expression::If {
        condition,
        then_branch,
        else_branch: None,
        ..
    } = expr(parsed, first_if)?
    else {
        return None;
    };
    name(parsed, names, *condition)?;
    if super::super::builtin_type(typed, typed.expression_type(*condition)?)
        != Some(BuiltinType::Boolean)
    {
        return None;
    }
    let assignment = only_expression(parsed, *then_branch)?;
    let Expression::Assignment {
        target,
        operator: AssignmentOperator::Assign,
        value,
        ..
    } = expr(parsed, assignment)?
    else {
        return None;
    };
    if name(parsed, names, *target)? != root
        || *value != replacement
        || !matches!(
            expr(parsed, replacement)?,
            Expression::Literal(LiteralKind::Null)
        )
        || members[0].value_origin() != names.symbols().get(root.index())?.span()
        || members[1].value_origin() != parsed.ast().expressions().get(assignment).ok()?.span()
    {
        return None;
    }
    let Expression::If {
        condition,
        then_branch,
        else_branch: None,
        ..
    } = expr(parsed, second_if)?
    else {
        return None;
    };
    nullable_guard(parsed, names, typed, *condition, root, storage)?;
    let use_assignment = only_expression(parsed, *then_branch)?;
    let Expression::Assignment {
        target,
        operator: AssignmentOperator::Assign,
        value: member,
        ..
    } = expr(parsed, use_assignment)?
    else {
        return None;
    };
    let target = name(parsed, names, *target)?;
    let Expression::Member {
        receiver,
        safe: false,
        ..
    } = expr(parsed, *member)?
    else {
        return None;
    };
    let use_fact = typed.non_null_use(*receiver)?;
    if name(parsed, names, *receiver)? != root
        || target == root
        || super::super::builtin_type(typed, typed.symbol_type(target)?) != Some(BuiltinType::Int)
        || super::super::builtin_type(typed, typed.expression_type(*member)?)
            != Some(BuiltinType::Int)
        || use_fact.symbol() != root
        || use_fact.declared_type() != storage
        || use_fact.narrowed_type() != *inner
        || members[0].point()
            != match arm {
                0 => DropPoint::AfterExpression(*member),
                1 => DropPoint::BranchExit {
                    control: second_if,
                    branch: 1,
                },
                _ => return None,
            }
    {
        return None;
    }
    let mut loops = parsed.ast().statements().iter().filter_map(|(id, node)| {
        let Statement::While {
            condition, body, ..
        } = node.payload()
        else {
            return None;
        };
        nullable_guard(parsed, names, typed, *condition, root, storage)?;
        let elements = elements(parsed, *body)?;
        let [first, second, last] = elements else {
            return None;
        };
        if statement_expression(parsed, *first)? != first_if
            || statement_expression(parsed, *second)? != second_if
            || !matches!(
                expr(parsed, statement_expression(parsed, *last)?)?,
                Expression::Break { .. }
            )
        {
            return None;
        }
        Some(id)
    });
    let while_statement = loops.next()?;
    if loops.next().is_some() {
        return None;
    }
    let mut old_drops = owned.drops().iter().copied().filter(|fact| {
        fact.point() == DropPoint::AfterExpression(replacement)
            && fact.target() == DropTarget::Named(root)
    });
    let old_drop = old_drops.next()?;
    if old_drops.next().is_some()
        || old_drop.condition().is_some()
        || old_drop.owner() != members[0].owner()
        || old_drop.capture_slot().is_some()
        || old_drop.instance_address().is_some()
        || old_drop.value_origin() != members[0].value_origin()
        || owned.drops().iter().position(|fact| *fact == old_drop)?
            >= owned.drops().iter().position(|fact| *fact == members[0])?
    {
        return None;
    }
    let (function_span, seed_parameter, parameter_count) =
        function_seed(parsed, names, typed, root, seed, storage, while_statement)?;
    Some(GroupPlan {
        members,
        root,
        storage,
        first_if,
        second_if,
        arm,
        replacement,
        old_drop,
        while_statement,
        function_span,
        seed_parameter,
        parameter_count,
    })
}

fn function_seed(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    root: SymbolId,
    seed: SymbolId,
    storage: TypeId,
    while_statement: StatementId,
) -> Option<(Span, usize, usize)> {
    let mut candidates = parsed.ast().items().iter().filter_map(|(_, node)| {
        let Item::Function {
            name: NameMarker::Present(name_span),
            form,
            ..
        } = node.payload()
        else {
            return None;
        };
        let body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return None,
        };
        let elements = elements(parsed, body)?;
        let loop_at = elements.iter().position(|item| *item == while_statement)?;
        let locals = elements[..loop_at]
            .iter()
            .filter_map(|statement| {
                let Statement::LocalVariable { declaration } =
                    parsed.ast().statements().get(*statement).ok()?.payload()
                else {
                    return None;
                };
                let Item::Variable {
                    kind: VariableKind::Var,
                    name: NameMarker::Present(span),
                    initializer,
                    ..
                } = parsed.ast().items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                (names.symbols().get(root.index())?.span() == *span
                    && name(parsed, names, *initializer)? == seed)
                    .then_some(())
            })
            .count();
        if locals != 1 {
            return None;
        }
        let callable = typed.callables().iter().find(|callable| {
            names
                .symbols()
                .get(callable.symbol().index())
                .is_some_and(|symbol| symbol.span() == *name_span)
        })?;
        if callable.owner().is_some()
            || callable.receiver().is_some()
            || callable.extension_receiver_symbol().is_some()
            || !callable.type_parameters().is_empty()
            || callable.parameter_symbols().iter().any(Option::is_none)
            || callable.parameter_symbols().len() != callable.parameters().len()
            || callable
                .parameter_symbols()
                .iter()
                .filter(|symbol| **symbol == Some(seed))
                .count()
                != 1
        {
            return None;
        }
        let index = callable
            .parameter_symbols()
            .iter()
            .position(|symbol| *symbol == Some(seed))?;
        let parameter = callable.parameters().get(index)?;
        (parameter.mode == ParameterMode::Value && parameter.ty == storage).then_some((
            node.span(),
            index,
            callable.parameters().len(),
        ))
    });
    let result = candidates.next()?;
    candidates.next().is_none().then_some(result)
}

fn nullable_guard(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    condition: ExpressionId,
    root: SymbolId,
    storage: TypeId,
) -> Option<()> {
    let condition = ungroup(parsed, condition)?;
    let descriptor = typed.null_comparison(condition)?;
    let Expression::Binary {
        left,
        operator: AstBinaryOperator::NotEqual,
        right,
        ..
    } = expr(parsed, condition)?
    else {
        return None;
    };
    let subject = match (expr(parsed, *left)?, expr(parsed, *right)?) {
        (Expression::Literal(LiteralKind::Null), _) => *right,
        (_, Expression::Literal(LiteralKind::Null)) => *left,
        _ => return None,
    };
    let subject = ungroup(parsed, subject)?;
    (descriptor.expression() == condition
        && descriptor.symbol() == root
        && descriptor.nullable_type() == storage
        && descriptor.non_null_when_true()
        && name(parsed, names, subject)? == root
        && typed.expression_type(subject) == Some(storage)
        && typed.non_null_use(subject).is_none())
    .then_some(())
}

fn name(parsed: &ParsedFile, names: &NameResolution, expression: ExpressionId) -> Option<SymbolId> {
    let node = parsed.ast().expressions().get(expression).ok()?;
    if !matches!(node.payload(), Expression::Name) {
        return None;
    }
    names
        .references()
        .iter()
        .find_map(|reference| match reference.target() {
            ReferenceTarget::Symbol(symbol) if reference.span() == node.span() => Some(*symbol),
            _ => None,
        })
}

fn ungroup(parsed: &ParsedFile, mut expression: ExpressionId) -> Option<ExpressionId> {
    for _ in 0..=parsed.ast().expressions().len() {
        match expr(parsed, expression)? {
            Expression::Group { expression: inner } => expression = *inner,
            _ => return Some(expression),
        }
    }
    None
}

fn expr(parsed: &ParsedFile, expression: ExpressionId) -> Option<&Expression> {
    Some(parsed.ast().expressions().get(expression).ok()?.payload())
}

fn elements(parsed: &ParsedFile, statement: StatementId) -> Option<&[StatementId]> {
    match parsed.ast().statements().get(statement).ok()?.payload() {
        Statement::Block { elements, .. } | Statement::ControlBody { elements, .. } => {
            Some(elements)
        }
        _ => None,
    }
}

fn statement_expression(parsed: &ParsedFile, statement: StatementId) -> Option<ExpressionId> {
    match parsed.ast().statements().get(statement).ok()?.payload() {
        Statement::Expression { expression } => Some(*expression),
        _ => None,
    }
}

fn only_expression(parsed: &ParsedFile, statement: StatementId) -> Option<ExpressionId> {
    let [statement] = elements(parsed, statement)? else {
        return None;
    };
    statement_expression(parsed, *statement)
}
