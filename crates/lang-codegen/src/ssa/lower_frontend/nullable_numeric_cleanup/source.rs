//! Bounded AST and typed-use qualification, independent of runtime SSA transport.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    parser::{FunctionBody, FunctionForm, VariableKind},
    type_checking::IntegerRangeOperationKind,
};

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    statement: StatementId,
    member: ExpressionId,
    root: SymbolId,
    seed: SymbolId,
) -> Option<StatementId> {
    let storage = typed.symbol_type(root)?;
    let Statement::For { source, body, .. } =
        parsed.ast().statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let leaf = ungroup(parsed, *source)?;
    let range = typed.integer_range_operation(leaf)?;
    let Expression::Binary {
        left,
        right,
        operator,
        ..
    } = expr(parsed, leaf)?
    else {
        return None;
    };
    if range.expression() != leaf
        || range.left() != *left
        || range.right() != *right
        || typed.expression_type(*source) != Some(range.result_type())
        || typed.expression_type(leaf) != Some(range.result_type())
        || typed.expression_type(*left) != Some(range.left_type())
        || typed.expression_type(*right) != Some(range.right_type())
        || !matches!(
            (operator, range.kind()),
            (
                AstBinaryOperator::InclusiveRange,
                IntegerRangeOperationKind::Inclusive
            ) | (
                AstBinaryOperator::ExclusiveRange,
                IntegerRangeOperationKind::Exclusive
            )
        )
        || !matches!(
            expr(parsed, range.left())?,
            Expression::Literal(LiteralKind::Integer(_))
        )
        || !matches!(
            expr(parsed, range.right())?,
            Expression::Literal(LiteralKind::Integer(_))
        )
    {
        return None;
    }
    let body_span = parsed.ast().statements().get(*body).ok()?.span();
    for (_, node) in parsed
        .ast()
        .statements()
        .iter()
        .filter(|(_, node)| contains(body_span, node.span()))
    {
        if !matches!(
            node.payload(),
            Statement::Block { .. } | Statement::ControlBody { .. } | Statement::Expression { .. }
        ) {
            return None;
        }
    }
    for (id, node) in parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, node)| contains(body_span, node.span()))
    {
        match node.payload() {
            Expression::If { .. } | Expression::Continue { .. } => {}
            Expression::Assignment {
                target,
                operator: AssignmentOperator::Assign,
                ..
            } => {
                let symbol = name(parsed, names, *target)?;
                if symbol == root || !scalar(typed, typed.symbol_type(symbol)?) {
                    return None;
                }
            }
            _ => {
                readonly_expression(parsed, names, typed, id, root, storage)?;
            }
        }
    }
    let Expression::Member {
        receiver,
        safe: false,
        ..
    } = expr(parsed, member)?
    else {
        return None;
    };
    let use_fact = typed.non_null_use(*receiver)?;
    if name(parsed, names, *receiver)? != root
        || use_fact.expression() != *receiver
        || use_fact.symbol() != root
        || use_fact.declared_type() != storage
        || typed.types().get(storage) != Some(&TypeKind::Nullable(use_fact.narrowed_type()))
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
        let [numeric, tail] = elements(parsed, *body)? else {
            return None;
        };
        let Statement::Expression { expression } =
            parsed.ast().statements().get(*tail).ok()?.payload()
        else {
            return None;
        };
        let Expression::Return {
            value: Some(value), ..
        } = expr(parsed, *expression)?
        else {
            return None;
        };
        let guard = typed.null_comparison(ungroup(parsed, *condition)?)?;
        if *numeric != statement
            || guard.symbol() != root
            || guard.nullable_type() != storage
            || !guard.non_null_when_true()
            || !contains(
                parsed.ast().expressions().get(*value).ok()?.span(),
                parsed.ast().expressions().get(member).ok()?.span(),
            )
        {
            return None;
        }
        let tail_span = parsed.ast().expressions().get(*value).ok()?.span();
        for (id, _) in parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| contains(tail_span, node.span()))
        {
            readonly_expression(parsed, names, typed, id, root, storage)?;
        }
        Some(id)
    });
    let outer = loops.next()?;
    if loops.next().is_some() {
        return None;
    }
    // Tie the frontend parameter seed to the real source binding before this outer loop.
    let mut functions = parsed.ast().items().iter().filter_map(|(_, node)| {
        let Item::Function {
            name: NameMarker::Present(function_name),
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
        let statements = elements(parsed, body)?;
        let index = statements.iter().position(|id| *id == outer)?;
        let mut initialized = false;
        for id in &statements[..index] {
            let Statement::LocalVariable { declaration } =
                parsed.ast().statements().get(*id).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(span),
                initializer,
                kind,
                ..
            } = parsed.ast().items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if *span == names.symbols().get(root.index())?.span() {
                if initialized
                    || *kind != VariableKind::Var
                    || name(parsed, names, *initializer)? != seed
                {
                    return None;
                }
                initialized = true;
            } else if !matches!(expr(parsed, *initializer)?, Expression::Literal(_)) {
                return None;
            }
        }
        let callable = typed.callables().iter().find(|callable| {
            names
                .symbols()
                .get(callable.symbol().index())
                .is_some_and(|symbol| symbol.span() == *function_name)
        })?;
        let index = callable
            .parameter_symbols()
            .iter()
            .position(|symbol| *symbol == Some(seed))?;
        let parameter = callable.parameters().get(index)?;
        (initialized
            && callable.owner().is_none()
            && callable.receiver().is_none()
            && callable.extension_receiver_symbol().is_none()
            && callable.type_parameters().is_empty()
            && parameter.mode == ParameterMode::Value
            && parameter.ty == storage)
            .then_some(())
    });
    functions.next()?;
    functions.next().is_none().then_some(outer)
}

fn readonly_expression(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    id: ExpressionId,
    root: SymbolId,
    storage: TypeId,
) -> Option<()> {
    match expr(parsed, id)? {
        Expression::Name => {
            let symbol = name(parsed, names, id)?;
            if symbol == root {
                let fact = typed.non_null_use(id)?;
                if fact.expression() != id
                    || fact.symbol() != root
                    || fact.declared_type() != storage
                    || typed.types().get(storage) != Some(&TypeKind::Nullable(fact.narrowed_type()))
                    || typed.expression_type(id) != Some(fact.narrowed_type())
                {
                    return None;
                }
            } else if !scalar(typed, typed.expression_type(id)?) {
                return None;
            }
        }
        Expression::Member {
            receiver,
            safe: false,
            ..
        } => {
            if name(parsed, names, *receiver)? != root
                || builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int)
            {
                return None;
            }
        }
        Expression::Literal(_) | Expression::Group { .. } | Expression::Binary { .. } => {
            if !scalar(typed, typed.expression_type(id)?) {
                return None;
            }
        }
        _ => return None,
    }
    Some(())
}
fn scalar(typed: &TypedFile, ty: TypeId) -> bool {
    matches!(
        builtin_type(typed, ty),
        Some(BuiltinType::Int | BuiltinType::Long | BuiltinType::Boolean)
    )
}
pub(super) fn expr(parsed: &ParsedFile, id: ExpressionId) -> Option<&Expression> {
    Some(parsed.ast().expressions().get(id).ok()?.payload())
}
fn ungroup(parsed: &ParsedFile, mut id: ExpressionId) -> Option<ExpressionId> {
    for _ in 0..=parsed.ast().expressions().len() {
        match expr(parsed, id)? {
            Expression::Group { expression } => id = *expression,
            _ => return Some(id),
        }
    }
    None
}
fn name(parsed: &ParsedFile, names: &NameResolution, id: ExpressionId) -> Option<SymbolId> {
    let node = parsed.ast().expressions().get(id).ok()?;
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
fn elements(parsed: &ParsedFile, id: StatementId) -> Option<&[StatementId]> {
    match parsed.ast().statements().get(id).ok()?.payload() {
        Statement::Block { elements, .. } | Statement::ControlBody { elements, .. } => {
            Some(elements)
        }
        _ => None,
    }
}
fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
