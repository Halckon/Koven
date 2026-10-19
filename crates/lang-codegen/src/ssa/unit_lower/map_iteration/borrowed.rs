//! Source-qualified stable Borrow binding; the canonical source remains a real parameter.
use super::*;
use lang_frontend::ownership_checking::BorrowBindingStorage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct Proof {
    pub(in super::super) binding: UnitSymbolId,
    pub(in super::super) source: UnitSymbolId,
    pub(in super::super) slot: usize,
    pub(in super::super) declaration: Span,
}

pub(super) fn prove(
    frontend: (
        &ParsedFile,
        &ValidatedCompilationUnitNames,
        &CompilationUnitTypes,
        &CompilationUnitOwnership,
    ),
    plan: &UnitMapIterationOwnershipPlan,
    binding_slot: (UnitSymbolId, usize),
    span: Span,
) -> Result<Proof, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let (binding, slot) = binding_slot;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let mut facts = owned
        .borrow_results()
        .bindings()
        .iter()
        .filter(|f| f.binding() == binding);
    let fact = facts.next().ok_or_else(bad)?;
    if facts.next().is_some() {
        return Err(bad());
    }
    let Some(UnitLoanTarget::Place(origin)) = plan.source() else {
        return Err(bad());
    };
    let symbol = names
        .names()
        .source_units()
        .get(binding.source_unit().index())
        .and_then(|u| u.resolution().symbols().get(binding.symbol().index()))
        .ok_or_else(bad)?;
    if fact.storage() != BorrowBindingStorage::BorrowedStorage
        || fact.parent().is_some()
        || fact.source_loan().is_some()
        || !origin.is_root()
        || plan.source_access() != UnitIterationSourceAccess::Shared
    {
        return Err(unsupported());
    }
    if binding.source_unit() != plan.descriptor().statement().source_unit()
        || fact.initializer().source_unit() != binding.source_unit()
        || origin.root().source_unit() != binding.source_unit()
        || symbol.kind() != SymbolKind::Variable
        || symbol.is_synthetic()
        || fact.origin() != plan.source().ok_or_else(bad)?
        || binding == origin.root()
        || plan.descriptor().bindings().contains(&binding)
        || typed.symbol_type(binding) != Some(plan.descriptor().source_type())
        || typed.expression_type(fact.initializer()) != Some(plan.descriptor().source_type())
        || typed.expression_category(fact.initializer()) != Some(ExpressionCategory::Place)
        || owned.formed_owned_locals().contains(&binding)
        || owned
            .drops()
            .iter()
            .any(|f| f.target() == UnitDropTarget::Named(binding))
    {
        return Err(bad());
    }
    let initializer = crate::ssa::map_source::transparent_expression(
        parsed,
        fact.initializer().expression(),
        |id| {
            let id = UnitExpressionId::new(binding.source_unit(), id);
            typed.expression_type(id).zip(typed.expression_category(id))
        },
    )?;
    let node = parsed
        .ast()
        .expressions()
        .get(initializer)
        .map_err(|_| bad())?;
    let references = names
        .names()
        .references()
        .iter()
        .filter(|r| {
            r.source_unit() == binding.source_unit()
                && r.namespace() == Some(Namespace::Value)
                && r.span() == node.span()
        })
        .collect::<Vec<_>>();
    if node.payload() != &Expression::Name
        || !matches!(references.as_slice(), [reference]
        if reference.target() == &UnitReferenceTarget::Symbol(origin.root()))
    {
        return Err(bad());
    }
    let ends = owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|f| f.binding() == binding)
        .collect::<Vec<_>>();
    let [end] = ends.as_slice() else {
        return Err(bad());
    };
    let UnitDropPoint::AfterStatement(scope) = end.point() else {
        return Err(unsupported());
    };
    if scope.source_unit() != binding.source_unit() {
        return Err(bad());
    }
    let declaration = crate::ssa::borrow_result_support::stable_map::declaration(
        parsed,
        symbol.span(),
        fact.marker_span(),
        fact.initializer().expression(),
        scope.statement(),
        plan.descriptor().statement().statement(),
    )?;
    Ok(Proof {
        binding,
        source: origin.root(),
        slot,
        declaration,
    })
}
