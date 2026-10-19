//! A stable shared binding is distinct from the canonical parameter it borrows.
use super::*;
use lang_frontend::name_resolution::{Namespace, ReferenceTarget};
use lang_frontend::ownership_checking::{BorrowBindingStorage, DropTarget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Proof {
    pub(super) binding: SymbolId,
    pub(super) source: SymbolId,
    pub(super) declaration: Span,
}

pub(super) fn prove(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    plan: &MapIterationOwnershipPlan,
    binding: SymbolId,
    span: Span,
) -> Result<Proof, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let fact = unique(
        owned
            .borrow_results()
            .bindings()
            .iter()
            .filter(|f| f.binding() == binding),
    )
    .ok_or_else(bad)?;
    let Some(LoanTarget::Place(origin)) = plan.source() else {
        return Err(bad());
    };
    let symbol = names.symbols().get(binding.index()).ok_or_else(bad)?;
    if fact.storage() != BorrowBindingStorage::BorrowedStorage
        || fact.parent().is_some()
        || fact.source_loan().is_some()
        || !origin.is_root()
        || plan.source_access() != MapIterationSourceAccess::Shared
    {
        return Err(unsupported());
    }
    if symbol.kind() != SymbolKind::Variable
        || symbol.is_synthetic()
        || fact.origin() != plan.source().ok_or_else(bad)?
        || binding == origin.root()
        || plan.descriptor().bindings().contains(&binding)
        || typed.symbol_type(binding) != Some(plan.descriptor().source_type())
        || typed.expression_type(fact.initializer()) != Some(plan.descriptor().source_type())
        || typed.expression_category(fact.initializer())
            != Some(lang_frontend::type_checking::ExpressionCategory::Place)
        || owned.formed_owned_locals().contains(&binding)
        || owned
            .drops()
            .iter()
            .any(|f| f.target() == DropTarget::Named(binding))
    {
        return Err(bad());
    }
    let initializer =
        crate::ssa::map_source::transparent_expression(parsed, fact.initializer(), |id| {
            typed.expression_type(id).zip(typed.expression_category(id))
        })?;
    let node = parsed
        .ast()
        .expressions()
        .get(initializer)
        .map_err(|_| bad())?;
    let reference = unique(
        names
            .references()
            .iter()
            .filter(|r| r.namespace() == Namespace::Value && r.span() == node.span()),
    )
    .ok_or_else(bad)?;
    if node.payload() != &Expression::Name
        || reference.target() != &ReferenceTarget::Symbol(origin.root())
    {
        return Err(bad());
    }
    let end = unique(
        owned
            .borrow_results()
            .ends()
            .iter()
            .filter(|f| f.binding() == binding),
    )
    .ok_or_else(bad)?;
    let DropPoint::AfterStatement(scope) = end.point() else {
        return Err(unsupported());
    };
    let declaration = crate::ssa::borrow_result_support::stable_map::declaration(
        parsed,
        symbol.span(),
        fact.marker_span(),
        fact.initializer(),
        scope,
        plan.descriptor().statement(),
    )?;
    Ok(Proof {
        binding,
        source: origin.root(),
        declaration,
    })
}

impl ExpressionLowerer<'_> {
    pub(super) fn borrowed_map_seed(
        &self,
        proof: Proof,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        let bad = || error(LoweringErrorKind::MissingFact, span);
        let current = EntityId::Loan(*self.borrow_bindings.get(&proof.binding).ok_or_else(bad)?);
        let parent = *self
            .map_iteration_state
            .entries
            .get(&proof.source)
            .ok_or_else(bad)?;
        if self.bindings.contains_key(&proof.binding)
            || self
                .result_source_loans
                .get(&proof.binding)
                .is_none_or(|loans| !loans.is_empty())
        {
            return Err(bad());
        }
        crate::ssa::borrow_result_support::stable_map::seed(
            self.function,
            current,
            parent,
            proof.declaration,
            span,
            nested::transport,
        )?;
        Ok(current)
    }
}
