//! Ordinary formed locals consume actual source and terminal cleanup facts.
use super::*;
use crate::ssa::map_source::{BindingDeclaration, Birth, Formation, OwnerUpdate, named_formation};
pub(in super::super) mod epoch;
mod parameter;
use lang_frontend::parser::VariableKind;
pub(in super::super) use parameter::entry_parameter;

pub(in super::super) fn formation(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    root: UnitSymbolId,
    storage: UnitTypeId,
    span: Span,
) -> Result<Formation<UnitSymbolId>, LoweringError> {
    let mut formation = raw_formation(parsed, names, typed, owned, root, storage, span)?;
    for event in &formation.history {
        let resolved = epoch::resolve(
            (parsed, names, typed, owned),
            root,
            storage,
            event.rhs,
            span,
        )?;
        formation
            .births
            .push((event.rhs, resolved.root, resolved.birth));
    }
    Ok(formation)
}

fn raw_formation(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    root: UnitSymbolId,
    storage: UnitTypeId,
    span: Span,
) -> Result<Formation<UnitSymbolId>, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let source = root.source_unit();
    named_formation(
        parsed,
        span,
        root,
        |root| {
            let symbol = names
                .names()
                .source_units()
                .get(source.index())
                .and_then(|unit| unit.resolution().symbols().get(root.symbol().index()))
                .ok_or_else(bad)?;
            if symbol.kind() == SymbolKind::ValueParameter {
                let (_, marker) =
                    entry_parameter(parsed, names, typed, owned, root, storage, span)?;
                return Ok(BindingDeclaration::EntryParameter { marker });
            }
            if symbol.kind() != SymbolKind::Variable {
                return Err(unsupported());
            }
            if root.source_unit() != source
                || symbol.is_synthetic()
                || typed.symbol_type(root) != Some(storage)
                || !owned.formed_owned_locals().contains(&root)
            {
                return Err(bad());
            }
            let mut declarations = Vec::new();
            for (_, node) in parsed.ast().items().iter() {
                if let Item::Variable {
                    kind,
                    name: NameMarker::Present(marker),
                    initializer,
                    ..
                } = node.payload()
                    && *marker == symbol.span()
                {
                    declarations.push((*kind, *marker, *initializer));
                }
            }
            let [(kind, marker, initializer)] = declarations.as_slice() else {
                return Err(bad());
            };
            if !matches!(kind, VariableKind::Val | VariableKind::Var) {
                return Err(unsupported());
            }
            if typed.expression_type(UnitExpressionId::new(source, *initializer)) != Some(storage) {
                return Err(bad());
            }
            Ok(BindingDeclaration::Local {
                marker: *marker,
                initializer: *initializer,
            })
        },
        |span| {
            let references = names
                .names()
                .references()
                .iter()
                .filter(|r| r.source_unit() == source && r.span() == span)
                .collect::<Vec<_>>();
            match references.as_slice() {
                [r] => match r.target() {
                    UnitReferenceTarget::Symbol(root) => Ok(*root),
                    _ => Err(bad()),
                },
                _ => Err(bad()),
            }
        },
        |id| {
            let id = UnitExpressionId::new(source, id);
            typed.expression_type(id).zip(typed.expression_category(id))
        },
        |id| {
            let expression = UnitExpressionId::new(source, id);
            match parsed
                .ast()
                .expressions()
                .get(id)
                .map_err(|_| bad())?
                .payload()
            {
                Expression::Call { .. } => Ok(Birth::Call(id)),
                _ => {
                    expression_birth::qualify(
                        parsed, names, typed, owned, expression, storage, span,
                    )?;
                    Ok(Birth::Expression(id))
                }
            }
        },
        typed
            .map_puts()
            .iter()
            .filter(|d| d.expression().source_unit() == source)
            .map(|d| OwnerUpdate {
                expression: d.expression().expression(),
                receiver: d.receiver().expression(),
            })
            .chain(
                typed
                    .map_removes()
                    .iter()
                    .filter(|d| d.expression().source_unit() == source)
                    .map(|d| OwnerUpdate {
                        expression: d.expression().expression(),
                        receiver: d.receiver().expression(),
                    }),
            ),
    )
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    syntax: local::Syntax,
) -> Result<QualifiedMap, LoweringError> {
    let local::Syntax {
        body,
        root,
        origin: _,
        span,
    } = syntax;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let statement = descriptor.statement();
    let plan = owned.map_iteration(statement).ok_or_else(bad)?;
    let IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) = plan.source_kind() else {
        return Err(bad());
    };
    if plan.descriptor() != descriptor
        || !place.is_root()
        || place.root() != root
        || plan.source() != Some(&UnitLoanTarget::Place(place.clone()))
        || plan.source_access() != UnitIterationSourceAccess::Owned
        || typed.expression_category(descriptor.source()) != Some(ExpressionCategory::Place)
        || typed.expression_type(descriptor.source()) != Some(descriptor.source_type())
        || typed.copyability(descriptor.source_type()) != Copyability::MoveOnly
        || descriptor.key_binding() == descriptor.value_binding()
        || descriptor.bindings().contains(&root)
    {
        return Err(bad());
    }
    let resource = typed
        .is_resource_type(descriptor.source_type())
        .ok_or_else(bad)?;
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(descriptor.source_type()),
        Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor
                && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
    {
        return Err(bad());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        if binding.symbol() != descriptor.bindings()[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || loan.binding() != binding.symbol()
            || loan.component() != component
            || loan.statement() != statement
            || loan.source() != descriptor.source()
            || Some(loan.origin()) != plan.source()
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(binding.symbol())
                != Some([descriptor.key_type(), descriptor.value_type()][index])
        {
            return Err(bad());
        }
    }
    reject_nested(parsed, statement.statement(), span)?;
    if let Some(qualified) =
        binding_choice::qualify((parsed, names, typed, owned), plan, (body, root), span)?
    {
        return Ok(qualified);
    }
    let formation = formation(
        parsed,
        names,
        typed,
        owned,
        root,
        descriptor.source_type(),
        span,
    )?;
    match formation.birth {
        Birth::Call(producer) => local::factory(
            parsed,
            names,
            typed,
            owned,
            UnitExpressionId::new(statement.source_unit(), producer),
            descriptor.source_type(),
            span,
        )?,
        Birth::Expression(expression) => {
            expression_birth::qualify(
                parsed,
                names,
                typed,
                owned,
                UnitExpressionId::new(statement.source_unit(), expression),
                descriptor.source_type(),
                span,
            )?;
        }
        Birth::EntryParameter { .. } => {}
    }
    let source = SourceIdentity::Formed {
        root,
        birth: formation.birth,
    };
    // Other epochs retain ordinary call delivery, not provider capability.
    // Only the current epoch can be an already-proved alias Name.
    for epoch in &formation.history {
        let outer = parsed
            .ast()
            .expressions()
            .get(epoch.rhs)
            .map_err(|_| bad())?;
        if outer.span() == formation.binding {
            continue;
        }
        let resolved = epoch::resolve(
            (parsed, names, typed, owned),
            root,
            descriptor.source_type(),
            epoch.rhs,
            span,
        )?;
        let Birth::Call(call) = resolved.birth else {
            return Err(unsupported());
        };
        let call = UnitExpressionId::new(statement.source_unit(), call);
        if typed.map_construction(call).is_some() || typed.map_consume(call).is_some() {
            local::factory(
                parsed,
                names,
                typed,
                owned,
                call,
                descriptor.source_type(),
                span,
            )?;
            continue;
        }
        if typed.map_with_value(call).is_some() {
            return Err(unsupported());
        }
        let ordinary = typed.call(call).ok_or_else(bad)?;
        if !matches!(
            ordinary.target(),
            lang_frontend::type_checking::UnitCallTarget::Declaration(_)
                | lang_frontend::type_checking::UnitCallTarget::Symbol(_)
                | lang_frontend::type_checking::UnitCallTarget::FunctionValue
        ) {
            return Err(unsupported());
        }
    }
    if let Some(qualified) =
        conditional::qualify(parsed, names, typed, owned, plan, (body, source), span)?
    {
        return Ok(qualified);
    }
    let body_exit = qualify_body(parsed, names, plan, descriptor, body, span)?;
    let (source_drop, transfer_drop, late_drop) = if resource {
        let proof = resource::qualify(
            parsed,
            names,
            typed,
            owned,
            plan,
            (root, Some(&formation)),
            body_exit,
        )?;
        (None, proof.transfer, Some(proof.drop))
    } else {
        let late = future_use::qualify(
            parsed,
            names,
            typed,
            owned,
            plan,
            (body_exit, Some(&formation)),
            span,
        )?;
        let epoch_drops = drops(parsed, owned, root, &formation)?;
        let (source_drop, transfer_drop) = validate_cleanup(
            plan,
            owned,
            (body, body_exit),
            (root, Some(&epoch_drops)),
            formation.origin,
            late,
            span,
        )?;
        (source_drop, transfer_drop, late)
    };
    Ok(QualifiedMap {
        statement,
        future_transfer: None,
        source,
        source_drop,
        transfer_drop,
        late_drop,
        body_exit,
        body_point: body_point(statement, body, body_exit),
        conditional: None,
        nested: None,
    })
}

pub(in super::super) fn drops(
    parsed: &ParsedFile,
    owned: &CompilationUnitOwnership,
    root: UnitSymbolId,
    formation: &Formation<UnitSymbolId>,
) -> Result<Vec<UnitDropFact>, LoweringError> {
    let mut facts = Vec::new();
    for fact in owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.target() == UnitDropTarget::Named(root))
    {
        let point =
            crate::ssa::map_source::unit_drop_event(parsed, fact.point()).ok_or_else(|| {
                lowering_error(LoweringErrorKind::UnsupportedNode, fact.value_origin())
            })?;
        let epoch = crate::ssa::map_source::epoch_at(parsed, formation, point)?;
        if fact.value_origin() != epoch.origin {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                fact.value_origin(),
            ));
        }
        if crate::ssa::map_source::in_formation(parsed, formation, point)? {
            facts.push(fact);
        }
    }
    Ok(facts)
}
