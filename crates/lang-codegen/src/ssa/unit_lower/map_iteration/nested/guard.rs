//! Existing no-CFG temporary Guard capability, independent of provider nesting.
use super::super::*;
use lang_frontend::{
    parser::VariableKind,
    type_checking::{Copyability, NominalKind, UnitConstructionTarget},
};

pub(in super::super) fn guard(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    source: SourceUnitId,
    statement: StatementId,
    span: Span,
) -> Result<QualifiedGuard, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| bad())?;
    let Statement::LocalVariable { declaration } = node.payload() else {
        return Err(unsupported());
    };
    let (_, item, _) = unwrap_modified(parsed, *declaration)?;
    let Item::Variable {
        kind: VariableKind::Val,
        name,
        initializer,
        ..
    } = item
    else {
        return Err(unsupported());
    };
    let origin = present_name(name).ok_or_else(bad)?;
    let symbols = names
        .names()
        .source_units()
        .get(source.index())
        .ok_or_else(bad)?
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.span() == origin && symbol.kind() == SymbolKind::Variable)
        .collect::<Vec<_>>();
    let [symbol] = symbols.as_slice() else {
        return Err(bad());
    };
    let symbol = typed
        .body_symbol_types()
        .keys()
        .copied()
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol.id())
        .ok_or_else(bad)?;
    let initializer = UnitExpressionId::new(source, initializer);
    let ty = typed.symbol_type(symbol).ok_or_else(bad)?;
    // Ordinary local owners are not entries in the public parameter/iteration binding table.
    if typed.expression_type(initializer) != Some(ty) {
        return Err(bad());
    }
    if typed.copyability(ty) != Copyability::MoveOnly {
        return Err(unsupported());
    }
    match typed.is_resource_type(ty) {
        Some(true) => {}
        Some(false) => return Err(unsupported()),
        None => return Err(bad()),
    }
    let init = parsed
        .ast()
        .expressions()
        .get(initializer.expression())
        .map_err(|_| bad())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = init.payload()
    else {
        return Err(unsupported());
    };
    if !arguments.is_empty() || !type_arguments.is_empty() {
        return Err(unsupported());
    }
    let callee = parsed.ast().expressions().get(*callee).map_err(|_| bad())?;
    if callee.payload() != &Expression::Name {
        return Err(unsupported());
    }
    let construction = typed.construction(initializer).ok_or_else(bad)?;
    let UnitConstructionTarget::Nominal(declaration) = construction.target() else {
        return Err(unsupported());
    };
    let nominal = typed
        .signatures()
        .declaration(declaration)
        .and_then(|item| item.nominal())
        .ok_or_else(bad)?;
    if nominal.kind() != NominalKind::Class
        || !nominal.fields().is_empty()
        || !nominal.type_parameters().is_empty()
        || !construction.arguments().is_empty()
    {
        return Err(unsupported());
    }
    let Some(UnitTypeKind::Nominal {
        declaration: actual,
        arguments,
    }) = typed.types().get(ty)
    else {
        return Err(bad());
    };
    if *actual != declaration
        || !arguments.is_empty()
        || !construction.instance().type_arguments().is_empty()
        || !owned.formed_owned_locals().contains(&symbol)
    {
        return Err(bad());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == source
                && reference.namespace() == Some(Namespace::Type)
                && reference.span() == callee.span()
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(bad());
    };
    if !matches!(reference.target(), UnitReferenceTarget::Declaration(actual) if *actual == declaration)
        && !matches!(reference.target(), UnitReferenceTarget::Symbol(actual) if *actual == nominal.symbol())
    {
        return Err(bad());
    }
    let deinit = nominal.deinit().ok_or_else(bad)?;
    // This finite slice has the Guard declaration and body in the same source file.
    if deinit.item().source_unit() != source
        || deinit.body().source_unit() != source
        || nominal.symbol().source_unit() != source
    {
        return Err(unsupported());
    }
    let deinit_node = parsed
        .ast()
        .items()
        .get(deinit.item().item())
        .map_err(|_| bad())?;
    if !matches!(deinit_node.payload(), Item::Deinit { body, .. } if *body == deinit.body().statement())
        || !matches!(
            parsed
                .ast()
                .statements()
                .get(deinit.body().statement())
                .map_err(|_| bad())?
                .payload(),
            Statement::Block { .. }
        )
    {
        return Err(bad());
    }
    let nominal_symbol = names
        .names()
        .source_units()
        .get(source.index())
        .ok_or_else(bad)?
        .resolution()
        .symbols()
        .get(nominal.symbol().symbol().index())
        .ok_or_else(bad)?;
    let declarations = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, item)| match item.payload() {
            Item::Classifier(classifier)
                if classifier.name == NameMarker::Present(nominal_symbol.span()) =>
            {
                Some(classifier)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [classifier] = declarations.as_slice() else {
        return Err(bad());
    };
    if !classifier
        .body
        .as_ref()
        .is_some_and(|body| body.members.contains(&deinit.item().item()))
    {
        return Err(bad());
    }
    if nominal.ty() != ty
        || deinit.owner() != declaration
        || deinit.receiver_type() != ty
        || deinit.receiver_mode() != ParameterMode::Borrow
    {
        return Err(bad());
    }
    let plans = owned
        .construction_plans()
        .iter()
        .filter(|plan| plan.construction() == initializer)
        .collect::<Vec<_>>();
    let [plan] = plans.as_slice() else {
        return Err(bad());
    };
    let root = plan.root_obligation().ok_or_else(bad)?;
    if construction.expression() != initializer
        || construction.result_type() != ty
        || plan.target() != construction.target()
        || plan.terminating_operand().is_some()
        || !plan.deliveries().is_empty()
        || root.construction() != initializer
        || root.result_type() != ty
        || root.kind() != lang_frontend::ownership_checking::ConstructionRootKind::HeapOwner
    {
        return Err(bad());
    }
    Ok(QualifiedGuard {
        symbol,
        initializer,
        origin,
    })
}
