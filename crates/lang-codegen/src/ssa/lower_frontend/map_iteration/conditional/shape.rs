//! Capability only: identities and cleanup still come from the complete frontend products.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    parser::{BinaryOperator, NameMarker, StringPart},
    type_checking::MapIterationDescriptor,
};

pub(in super::super) fn shape(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    descriptor: MapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<Option<Shape>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed.ast().statements().get(body).map_err(|_| missing())?;
    let Statement::Block { elements } = node.payload() else {
        return Ok(None);
    };
    let mut controls = Vec::new();
    for (index, &statement) in elements.iter().enumerate() {
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| missing())?;
        if let Statement::Expression { expression } = node.payload() {
            let node = parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|_| missing())?;
            if matches!(node.payload(), Expression::If { .. }) {
                controls.push((index, *expression));
            }
        }
    }
    if controls.is_empty() {
        return Ok(None);
    }
    let [(first, _), (second, _), (third, _)] = controls.as_slice() else {
        return Err(unsupported());
    };
    if *second != *first + 1
        || *third != *second + 1
        || [descriptor.key_type(), descriptor.value_type()]
            .into_iter()
            .any(|ty| builtin_type(typed, ty) != Some(BuiltinType::String))
    {
        return Err(unsupported());
    }
    let mut locals = Vec::new();
    for (index, &statement) in elements.iter().enumerate() {
        if (*first..=*third).contains(&index) {
            continue;
        }
        if !super::super::qualify::straight_statement(parsed, typed, statement)? {
            return Err(unsupported());
        }
        let node = parsed
            .ast()
            .statements()
            .get(statement)
            .map_err(|_| missing())?;
        match node.payload() {
            Statement::LocalVariable { declaration } if index < *first => {
                let item = parsed
                    .ast()
                    .items()
                    .get(*declaration)
                    .map_err(|_| missing())?;
                let Item::Variable {
                    name: NameMarker::Present(origin),
                    initializer,
                    ..
                } = item.payload()
                else {
                    return Err(unsupported());
                };
                let symbol = unique(names.symbols().iter().filter(|symbol| {
                    symbol.span() == *origin && symbol.kind() == SymbolKind::Variable
                }))
                .ok_or_else(missing)?;
                let ty = typed.symbol_type(symbol.id()).ok_or_else(missing)?;
                if typed.is_resource_type(ty) != Some(true) {
                    return Err(unsupported());
                }
                let construction = typed.construction(*initializer).ok_or_else(unsupported)?;
                if construction.expression() != *initializer || construction.result_type() != ty {
                    return Err(missing());
                }
                locals.push(Local {
                    symbol: symbol.id(),
                    initializer: *initializer,
                    initializer_span: parsed
                        .ast()
                        .expressions()
                        .get(*initializer)
                        .map_err(|_| missing())?
                        .span(),
                    origin: *origin,
                });
            }
            Statement::Expression { .. } => {}
            _ => return Err(unsupported()),
        }
    }
    let [local] = locals.as_slice() else {
        return Err(unsupported());
    };
    let mut arms = Vec::new();
    for (position, (_, control)) in controls.iter().enumerate() {
        let node = parsed
            .ast()
            .expressions()
            .get(*control)
            .map_err(|_| missing())?;
        let Expression::If {
            condition,
            then_branch,
            else_branch: None,
            ..
        } = node.payload()
        else {
            return Err(unsupported());
        };
        let condition_node = parsed
            .ast()
            .expressions()
            .get(*condition)
            .map_err(|_| missing())?;
        let Expression::Binary {
            left,
            operator: BinaryOperator::Equal,
            right,
            ..
        } = condition_node.payload()
        else {
            return Err(unsupported());
        };
        let left_node = parsed
            .ast()
            .expressions()
            .get(*left)
            .map_err(|_| missing())?;
        let right_node = parsed
            .ast()
            .expressions()
            .get(*right)
            .map_err(|_| missing())?;
        if left_node.payload() != &Expression::Name
            || !matches!(right_node.payload(), Expression::String { parts } if parts.iter().all(|part| matches!(part, StringPart::Text(_))))
        {
            return Err(unsupported());
        }
        let reference = unique(
            names
                .references()
                .iter()
                .filter(|r| r.span() == left_node.span()),
        )
        .ok_or_else(missing)?;
        if reference.target() != &ReferenceTarget::Symbol(descriptor.key_binding()) {
            return Err(unsupported());
        }
        if typed.expression_type(*left) != Some(descriptor.key_type())
            || typed.expression_type(*right) != Some(descriptor.key_type())
            || typed
                .expression_type(*condition)
                .and_then(|ty| builtin_type(typed, ty))
                != Some(BuiltinType::Boolean)
            || typed
                .expression_type(*control)
                .and_then(|ty| builtin_type(typed, ty))
                != Some(BuiltinType::Unit)
        {
            return Err(missing());
        }
        let branch = parsed
            .ast()
            .statements()
            .get(*then_branch)
            .map_err(|_| missing())?;
        let Statement::ControlBody { elements } = branch.payload() else {
            return Err(unsupported());
        };
        let Some((&last, prefix)) = elements.split_last() else {
            return Err(unsupported());
        };
        for &statement in prefix {
            let node = parsed
                .ast()
                .statements()
                .get(statement)
                .map_err(|_| missing())?;
            if !matches!(node.payload(), Statement::Expression { .. })
                || !super::super::qualify::straight_statement(parsed, typed, statement)?
            {
                return Err(unsupported());
            }
        }
        let tail = parsed.ast().statements().get(last).map_err(|_| missing())?;
        let Statement::Expression { expression } = tail.payload() else {
            return Err(unsupported());
        };
        let tail = parsed
            .ast()
            .expressions()
            .get(*expression)
            .map_err(|_| missing())?;
        let kind = match (position, tail.payload()) {
            (0, Expression::Continue { .. }) => IterationExitKind::Continue(*expression),
            (1, Expression::Break { .. }) => IterationExitKind::Break(*expression),
            (
                2,
                Expression::Return {
                    value: Some(value), ..
                },
            ) => {
                exit_shape::return_operand(parsed, names, typed, descriptor, *value, span)?;
                IterationExitKind::Return(*expression)
            }
            _ => return Err(unsupported()),
        };
        let operand_span = match tail.payload() {
            Expression::Return {
                value: Some(value), ..
            } => Some(
                parsed
                    .ast()
                    .expressions()
                    .get(*value)
                    .map_err(|_| missing())?
                    .span(),
            ),
            _ => None,
        };
        arms.push(Arm {
            control: *control,
            kind,
            operand_span,
            span: node.span(),
        });
    }
    let arms: [Arm; 3] = arms.try_into().map_err(|_| missing())?;
    Ok(Some(Shape {
        arms,
        local: *local,
        body,
    }))
}
