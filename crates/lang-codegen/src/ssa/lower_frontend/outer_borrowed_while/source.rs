//! Exact nesting and readonly uses; no provider, root write or early outer exit is inferred.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    parser::{FunctionBody, FunctionForm, VariableKind},
    type_checking::ContainerConstructionKind,
};

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    outer: StatementId,
    symbols: [SymbolId; 4],
    initializer: ExpressionId,
) -> Option<(StatementId, ExpressionId, usize)> {
    let [root, items, item, parameter] = symbols;
    let Statement::For { source, body, .. } = parsed.ast().statements().get(outer).ok()?.payload()
    else {
        return None;
    };
    if name(parsed, names, ungroup(parsed, *source)?)? != items
        || typed.expression_type(*source) != typed.symbol_type(items)
    {
        return None;
    }
    let [inner, tail] = elements(parsed, *body)? else {
        return None;
    };
    let Statement::While {
        condition, body, ..
    } = parsed.ast().statements().get(*inner).ok()?.payload()
    else {
        return None;
    };
    let guard = typed.null_comparison(ungroup(parsed, *condition)?)?;
    if guard.symbol() != root
        || Some(guard.nullable_type()) != typed.symbol_type(root)
        || !guard.non_null_when_true()
    {
        return None;
    }
    let [read, breaking] = elements(parsed, *body)? else {
        return None;
    };
    let Statement::Expression {
        expression: breaking,
    } = parsed.ast().statements().get(*breaking).ok()?.payload()
    else {
        return None;
    };
    if !matches!(expr(parsed, *breaking)?, Expression::Break { .. }) {
        return None;
    }
    for (statement, narrowed) in [(*read, true), (*tail, false)] {
        let Statement::Expression { expression } =
            parsed.ast().statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let Expression::Assignment {
            target,
            operator: AssignmentOperator::Assign,
            value,
        } = expr(parsed, *expression)?
        else {
            return None;
        };
        let target = name(parsed, names, *target)?;
        if [root, items, item].contains(&target)
            || builtin_type(typed, typed.symbol_type(target)?) != Some(BuiltinType::Int)
        {
            return None;
        }
        let span = parsed.ast().expressions().get(*value).ok()?.span();
        let mut item_read = false;
        let mut member_read = false;
        for (id, node) in parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, node)| contains(span, node.span()))
        {
            readonly(parsed, names, typed, id, root, narrowed)?;
            item_read |= name(parsed, names, id) == Some(item);
            member_read |= matches!(node.payload(), Expression::Member { .. });
        }
        if !item_read || member_read != narrowed {
            return None;
        }
    }
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
        let mut current = false;
        let mut source = false;
        for statement in &statements[..index] {
            let Statement::LocalVariable { declaration } =
                parsed.ast().statements().get(*statement).ok()?.payload()
            else {
                return None;
            };
            let Item::Variable {
                name: NameMarker::Present(span),
                kind,
                initializer: value,
                ..
            } = parsed.ast().items().get(*declaration).ok()?.payload()
            else {
                return None;
            };
            if *span == names.symbols().get(root.index())?.span() {
                if current
                    || *kind != VariableKind::Var
                    || name(parsed, names, *value)? != parameter
                {
                    return None;
                }
                current = true;
            } else if *span == names.symbols().get(items.index())?.span() {
                let leaf = ungroup(parsed, *value)?;
                let descriptor = typed.container_construction(leaf)?;
                let Expression::Call { arguments, .. } = expr(parsed, leaf)? else {
                    return None;
                };
                if source
                    || *kind != VariableKind::Val
                    || leaf != initializer
                    || descriptor.expression() != leaf
                    || descriptor.kind() != ContainerConstructionKind::ListForm
                    || descriptor.container() != SequentialContainerKind::List
                    || Some(descriptor.container_type()) != typed.symbol_type(items)
                    || builtin_type(typed, descriptor.element_type()) != Some(BuiltinType::Int)
                    || descriptor.parameter_modes().len() != arguments.len()
                    || descriptor
                        .parameter_modes()
                        .iter()
                        .any(|mode| *mode != ParameterMode::Value)
                    || arguments.iter().any(|argument| {
                        !matches!(
                            expr(parsed, argument.value),
                            Some(Expression::Literal(LiteralKind::Integer(_)))
                        )
                    })
                {
                    return None;
                }
                source = true;
            } else if !matches!(expr(parsed, *value)?, Expression::Literal(_)) {
                return None;
            }
        }
        let callable = typed.callables().iter().find(|callable| {
            names
                .symbols()
                .get(callable.symbol().index())
                .is_some_and(|symbol| symbol.span() == *function_name)
        })?;
        let slot = callable
            .parameter_symbols()
            .iter()
            .position(|symbol| *symbol == Some(parameter))?;
        if !current
            || !source
            || callable.owner().is_some()
            || callable.receiver().is_some()
            || callable.extension_receiver_symbol().is_some()
            || !callable.type_parameters().is_empty()
            || callable.parameters().get(slot)?.mode != ParameterMode::Value
            || Some(callable.parameters().get(slot)?.ty) != typed.symbol_type(root)
        {
            return None;
        }
        Some(slot)
    });
    let slot = functions.next()?;
    functions
        .next()
        .is_none()
        .then_some((*inner, *breaking, slot))
}
fn readonly(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    id: ExpressionId,
    root: SymbolId,
    narrowed: bool,
) -> Option<()> {
    let storage = typed.symbol_type(root)?;
    match expr(parsed, id)? {
        Expression::Name => {
            if name(parsed, names, id)? == root {
                let fact = typed.non_null_use(id)?;
                if !narrowed
                    || fact.expression() != id
                    || fact.symbol() != root
                    || fact.declared_type() != storage
                    || typed.types().get(storage) != Some(&TypeKind::Nullable(fact.narrowed_type()))
                    || typed.expression_type(id) != Some(fact.narrowed_type())
                {
                    return None;
                }
            } else if builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int) {
                return None;
            }
        }
        Expression::Member {
            receiver,
            safe: false,
            ..
        } => {
            if !narrowed
                || name(parsed, names, *receiver)? != root
                || builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int)
            {
                return None;
            }
        }
        Expression::Literal(_) | Expression::Group { .. } | Expression::Binary { .. } => {
            if builtin_type(typed, typed.expression_type(id)?) != Some(BuiltinType::Int) {
                return None;
            }
        }
        _ => return None,
    }
    Some(())
}
fn expr(parsed: &ParsedFile, id: ExpressionId) -> Option<&Expression> {
    Some(parsed.ast().expressions().get(id).ok()?.payload())
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
fn ungroup(parsed: &ParsedFile, mut id: ExpressionId) -> Option<ExpressionId> {
    for _ in 0..=parsed.ast().expressions().len() {
        match expr(parsed, id)? {
            Expression::Group { expression } => id = *expression,
            _ => return Some(id),
        }
    }
    None
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
