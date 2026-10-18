//! Normal local owners use real declaration/initializer identities, never parameter capabilities.
mod facts;
pub(super) use facts::factory;
mod family;
use super::*;
use lang_frontend::parser::{BinaryOperator, VariableKind};

/// A single continuation obligation between one provider and its adjacent size read.
/// This is function-local execution state, not a root-to-owner registry.
#[derive(Clone, Copy)]
pub(in super::super) struct LocalContinuation {
    pub(in super::super) source: SourceIdentity,
    pub(in super::super) drop: UnitDropFact,
    pub(in super::super) birth: EntityId,
    pub(in super::super) current: EntityId,
    pub(in super::super) block: BlockId,
    pub(in super::super) read: Option<ValueId>,
}

pub(super) struct Syntax {
    pub(super) body: StatementId,
    pub(super) root: UnitSymbolId,
    pub(super) origin: Span,
    pub(super) span: Span,
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    syntax: Syntax,
) -> Result<QualifiedMap, LoweringError> {
    let Syntax {
        body,
        root,
        origin,
        span,
    } = syntax;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let statement = descriptor.statement();
    let source = statement.source_unit();
    let plan = owned.map_iteration(statement).ok_or_else(bad)?;
    let IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) = plan.source_kind() else {
        return Err(bad());
    };
    if !place.is_root()
        || place.root() != root
        || plan.source() != Some(&UnitLoanTarget::Place(place.clone()))
        || plan.source_access() != UnitIterationSourceAccess::Owned
        || typed.expression_category(descriptor.source()) != Some(ExpressionCategory::Place)
        || typed.copyability(descriptor.source_type()) != Copyability::MoveOnly
    {
        return Err(bad());
    }
    if builtin_type(typed, descriptor.key_type()) != Some(BuiltinType::String)
        || builtin_type(typed, descriptor.value_type()) != Some(BuiltinType::String)
    {
        return Err(unsupported());
    }
    match typed.is_resource_type(descriptor.source_type()) {
        Some(false) => {}
        Some(true) => return Err(unsupported()),
        None => return Err(bad()),
    }
    // A formed entry is only an identity/history check. SSA current/birth is checked later.
    if !owned.formed_owned_locals().contains(&root) {
        return Err(bad());
    }
    let mut declarations = Vec::new();
    for (id, node) in parsed.ast().statements().iter() {
        let Statement::LocalVariable { declaration } = node.payload() else {
            continue;
        };
        let (_, item, _) = unwrap_modified(parsed, *declaration)?;
        if let Item::Variable {
            kind,
            name: NameMarker::Present(marker),
            initializer,
            ..
        } = item
            && marker == origin
        {
            if !matches!(kind, VariableKind::Val | VariableKind::Var) {
                return Err(unsupported());
            }
            declarations.push((id, UnitExpressionId::new(source, initializer)));
        }
    }
    let [(declaration, initializer)] = declarations.as_slice() else {
        return Err(bad());
    };
    if typed.expression_type(*initializer) != Some(descriptor.source_type()) {
        return Err(bad());
    }
    facts::factory(
        parsed,
        names,
        typed,
        owned,
        *initializer,
        descriptor.source_type(),
        span,
    )?;
    reject_nested(parsed, statement.statement(), span)?;
    // The local and For must be adjacent in the same ordinary sequential block.
    // Reassignment, moves, aliases and CFG producers have separate capability boundaries.
    let parents = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::Block { elements } = node.payload() else {
                return None;
            };
            let index = elements
                .iter()
                .position(|id| *id == statement.statement())?;
            Some((id, elements, index))
        })
        .collect::<Vec<_>>();
    let [(block, elements, index)] = parents.as_slice() else {
        return Err(unsupported());
    };
    // Completed SSA checks cover one callable. Keep the family at its actual
    // function-body block, never an unrelated nested block with its own whitelist.
    let functions = parsed
        .ast()
        .items()
        .iter()
        .filter(|(_, node)| {
            let Item::Function { form, .. } = node.payload() else {
                return false;
            };
            match form {
                FunctionForm::ImplicitUnitBlock(actual)
                | FunctionForm::Explicit {
                    body: FunctionBody::Block(actual),
                    ..
                } => actual == block,
                _ => false,
            }
        })
        .count();
    if functions != 1 {
        return Err(unsupported());
    }
    if *index == 0 || elements[*index - 1] != *declaration {
        return Err(unsupported());
    }
    family::qualify(
        parsed,
        names,
        typed,
        owned,
        family::Family {
            elements,
            source,
            span,
        },
    )?;
    facts::body(parsed, names, typed, owned, descriptor, body, span)?;
    let body_point = UnitDropPoint::AfterStatement(UnitStatementId::new(source, body));
    let [body_plan, exhaustion] = plan.exits() else {
        return Err(bad());
    };
    if plan.exits().iter().any(|exit| {
        !matches!(
            exit.kind(),
            UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Exhaustion
        )
    }) {
        return Err(unsupported());
    }
    if body_plan.kind() != UnitIterationExitKind::Fallthrough
        || body_plan.point() != body_point
        || exhaustion.kind() != UnitIterationExitKind::Exhaustion
        || exhaustion.point() != UnitDropPoint::LoopExit(statement)
        || owned.iteration_cleanup_at(body_point) != Some(body_plan.actions())
        || owned.iteration_cleanup_at(exhaustion.point()) != Some(exhaustion.actions())
        || body_plan.actions()
            != [
                Action::EndBinding {
                    statement,
                    symbol: descriptor.value_binding(),
                },
                Action::EndBinding {
                    statement,
                    symbol: descriptor.key_binding(),
                },
            ]
    {
        return Err(bad());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| reference.target() == &UnitReferenceTarget::Symbol(root))
        .collect::<Vec<_>>();
    let canonical = crate::ssa::map_source::transparent_expression(
        parsed,
        descriptor.source().expression(),
        |id| {
            let id = UnitExpressionId::new(source, id);
            typed.expression_type(id).zip(typed.expression_category(id))
        },
    )?;
    let source_span = parsed
        .ast()
        .expressions()
        .get(canonical)
        .map_err(|_| bad())?
        .span();
    let other_uses = references
        .iter()
        .filter(|reference| reference.span() != source_span)
        .collect::<Vec<_>>();
    let late_point = match other_uses.as_slice() {
        [] => None,
        [reference] => {
            if reference.source_unit() != source || reference.namespace() != Some(Namespace::Value)
            {
                return Err(bad());
            }
            let mut sizes = Vec::new();
            for size in typed.map_sizes() {
                if size.receiver().source_unit() != source {
                    continue;
                }
                let receiver = parsed
                    .ast()
                    .expressions()
                    .get(size.receiver().expression())
                    .map_err(|_| bad())?;
                if receiver.span() == reference.span() {
                    sizes.push(size);
                }
            }
            let [size] = sizes.as_slice() else {
                return Err(unsupported());
            };
            if size.map_type() != descriptor.source_type()
                || typed.expression_type(size.receiver()) != Some(descriptor.source_type())
                || builtin_type(
                    typed,
                    typed.expression_type(size.expression()).ok_or_else(bad)?,
                ) != Some(BuiltinType::Int)
            {
                return Err(bad());
            }
            let receiver = parsed
                .ast()
                .expressions()
                .get(size.receiver().expression())
                .map_err(|_| bad())?;
            if receiver.payload() != &Expression::Name {
                return Err(unsupported());
            }
            let next = elements.get(*index + 1).ok_or_else(unsupported)?;
            qualify_size_successor(parsed, typed, *next, size.expression(), span)?;
            Some(UnitDropPoint::CallReturn(size.expression()))
        }
        _ => return Err(unsupported()),
    };
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(bad());
    };
    if drop.value_origin() != origin || drop.point() != late_point.unwrap_or(exhaustion.point()) {
        return Err(bad());
    }
    // Unit has only its own conditional receiver table; no single phi/condition facts.
    if owned.conditional_receiver_drops().iter().any(|fact| {
        fact.point() == body_point
            || fact.point() == exhaustion.point()
            || Some(fact.point()) == late_point
    }) {
        return Err(bad());
    }
    let mut terminal = vec![
        Action::FinishProvider(statement),
        Action::EndSource(statement),
    ];
    if late_point.is_none() {
        terminal.push(Action::Drop(*drop));
    }
    if exhaustion.actions() != terminal {
        return Err(bad());
    }
    Ok(QualifiedMap {
        statement,
        future_transfer: None,
        source: SourceIdentity::Local {
            root,
            initializer: *initializer,
        },
        source_drop: late_point.is_none().then_some(*drop),
        late_drop: late_point.map(|_| *drop),
        transfer_drop: None,
        body_exit: UnitIterationExitKind::Fallthrough,
        body_point,
        conditional: None,
        nested: None,
    })
}

fn qualify_size_successor(
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
    next: StatementId,
    size: UnitExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed.ast().statements().get(next).map_err(|_| bad())?;
    let Statement::Expression { expression } = node.payload() else {
        return Err(unsupported());
    };
    let node = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| bad())?;
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let condition = parsed
        .ast()
        .expressions()
        .get(*condition)
        .map_err(|_| bad())?;
    let Expression::Binary {
        left,
        operator: BinaryOperator::Equal,
        right,
        ..
    } = condition.payload()
    else {
        return Err(unsupported());
    };
    if *left != size.expression() {
        return Err(unsupported());
    }
    let right_node = parsed.ast().expressions().get(*right).map_err(|_| bad())?;
    if !matches!(
        right_node.payload(),
        Expression::Literal(LiteralKind::Integer(_))
    ) {
        return Err(unsupported());
    }
    if builtin_type(
        typed,
        typed
            .expression_type(UnitExpressionId::new(size.source_unit(), *right))
            .ok_or_else(bad)?,
    ) != Some(BuiltinType::Int)
    {
        return Err(bad());
    }
    facts::straight_control_body(parsed, *then_branch, span)?;
    facts::straight_control_body(parsed, *else_branch, span)?;
    let then_span = parsed
        .ast()
        .statements()
        .get(*then_branch)
        .map_err(|_| bad())?
        .span();
    let else_span = parsed
        .ast()
        .statements()
        .get(*else_branch)
        .map_err(|_| bad())?
        .span();
    let mut aborts = Vec::new();
    for (id, ty) in typed.expression_types() {
        if id.source_unit() != size.source_unit()
            || builtin_type(typed, *ty) != Some(BuiltinType::Nothing)
        {
            continue;
        }
        let node = parsed
            .ast()
            .expressions()
            .get(id.expression())
            .map_err(|_| bad())?;
        if contains(then_span, node.span()) || contains(else_span, node.span()) {
            aborts.push((*id, node.span()));
        }
    }
    let [(abort, location)] = aborts.as_slice() else {
        return Err(unsupported());
    };
    if !contains(else_span, *location) {
        return Err(unsupported());
    }
    if !typed.call(*abort).ok_or_else(bad)?.aborts() {
        return Err(unsupported());
    }
    Ok(())
}
