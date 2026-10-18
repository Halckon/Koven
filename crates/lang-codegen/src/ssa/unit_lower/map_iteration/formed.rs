//! Ordinary formed locals consume actual source and terminal cleanup facts.
use super::*;
use crate::ssa::map_source::{Formation, named_formation};
use lang_frontend::parser::VariableKind;

pub(in super::super) fn formation(
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
            Ok((*marker, *initializer))
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
        typed
            .map_puts()
            .iter()
            .map(|d| d.receiver())
            .chain(typed.map_removes().iter().map(|d| d.receiver()))
            .filter(|id| id.source_unit() == source)
            .map(|id| id.expression()),
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
        origin,
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
    match typed.is_resource_type(descriptor.source_type()) {
        Some(false) => {}
        Some(true) => return Err(unsupported()),
        None => return Err(bad()),
    }
    if [descriptor.key_type(), descriptor.value_type()]
        .iter()
        .any(|t| builtin_type(typed, *t) != Some(BuiltinType::String))
    {
        return Err(unsupported());
    }
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(descriptor.source_type()),Some(UnitTypeKind::Intrinsic{constructor:actual,arguments}) if *actual==constructor && arguments.as_slice()==[descriptor.key_type(),descriptor.value_type()])
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
    let formation = formation(
        parsed,
        names,
        typed,
        owned,
        root,
        descriptor.source_type(),
        span,
    )?;
    let producer = UnitExpressionId::new(statement.source_unit(), formation.producer);
    local::factory(
        parsed,
        names,
        typed,
        owned,
        producer,
        descriptor.source_type(),
        span,
    )?;
    let body_exit = qualify_body(parsed, names, typed, descriptor, body, span)?;
    let (source_drop, transfer_drop) =
        validate_cleanup(plan, owned, (body, body_exit), root, origin, None, span)?;
    Ok(QualifiedMap {
        statement,
        source: SourceIdentity::Formed { root, producer },
        source_drop,
        transfer_drop,
        late_drop: None,
        body_exit,
        body_point: body_point(statement, body, body_exit),
        conditional: None,
        nested: None,
    })
}
