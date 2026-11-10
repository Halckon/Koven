//! Index immutable checker facts used to classify loan observations and select local borrow handoffs.
use super::{Checker, DropPlanner};
use crate::{
    name_resolution::{SymbolKind, UnitSymbolId},
    ownership_checking::{
        OwnershipBindingKind, UnitLoanTarget,
        borrow_result::{
            BorrowBindingFact, BorrowReturnOriginFact, BorrowSourceLoan, borrow_result_call,
        },
    },
    type_checking::UnitExpressionId,
};
use std::collections::{BTreeMap, BTreeSet};

type UnitBorrowBinding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;
type UnitBorrowReturnOrigin = BorrowReturnOriginFact<UnitExpressionId, UnitLoanTarget>;

pub(super) struct BorrowForwardIndex {
    by_call: BTreeMap<UnitExpressionId, ForwardedBorrowCall>,
}

struct ForwardedBorrowCall {
    source: BorrowSourceLoan<UnitExpressionId>,
    origin: Option<usize>,
}

impl BorrowForwardIndex {
    fn collect(checker: &Checker<'_>) -> Self {
        let mut by_call = BTreeMap::new();
        for &source in checker.borrow_results.forwarded_source_loans() {
            by_call.entry(source.call()).or_insert(ForwardedBorrowCall {
                source,
                origin: None,
            });
        }
        if !by_call.is_empty() {
            for (index, origin) in checker.borrow_return_origins.iter().enumerate() {
                // The forwarded facts already prove the typed source. Only
                // unwrap groups here, then require membership in that set.
                if let Some(call) =
                    borrow_result_call(checker.parsed, origin.expression().expression(), |_| true)
                    && let Some(entry) = by_call.get_mut(&checker.unit_expression(call))
                {
                    // Keep the original first matching return origin for this call.
                    entry.origin.get_or_insert(index);
                }
            }
        }
        Self { by_call }
    }
}

pub(super) struct LoanProvenance {
    pub(super) formed_owned: BTreeSet<UnitSymbolId>,
    pub(super) iteration_providers: BTreeSet<UnitSymbolId>,
    pub(super) borrow_bindings: BTreeSet<UnitSymbolId>,
    bindings_by_initializer: BTreeMap<UnitExpressionId, Vec<usize>>,
}

impl LoanProvenance {
    fn collect(checker: &Checker<'_>) -> Self {
        let mut iteration_providers = BTreeSet::new();
        let sequential = checker
            .iterations
            .values()
            .map(|plan| (plan.bindings(), plan.source()));
        let maps = checker
            .map_iterations
            .values()
            .map(|plan| (plan.bindings().as_slice(), plan.source()));
        for (bindings, source) in sequential.chain(maps) {
            iteration_providers.extend(bindings.iter().map(|binding| binding.symbol()));
            if let Some(UnitLoanTarget::Place(place)) = source {
                iteration_providers.insert(place.root());
            }
        }
        let mut borrow_bindings = BTreeSet::new();
        let mut bindings_by_initializer = BTreeMap::<_, Vec<_>>::new();
        for (index, binding) in checker.borrow_results.bindings().iter().enumerate() {
            borrow_bindings.insert(binding.binding());
            // Normalize only the lookup key through transparent Groups. The
            // published binding keeps its original initializer identity.
            let initializer = checker
                .borrow_result_call(binding.initializer().expression())
                .map(|call| checker.unit_expression(call))
                .unwrap_or(binding.initializer());
            bindings_by_initializer
                .entry(initializer)
                .or_default()
                .push(index);
        }
        Self {
            formed_owned: checker.formed_owned_locals.iter().copied().collect(),
            iteration_providers,
            borrow_bindings,
            bindings_by_initializer,
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn loan_provenance(&self) -> &LoanProvenance {
        // The planner only borrows the completed checker. Trial rollback and
        // ordered publication still use the checker's original collections.
        self.loan_provenance
            .get_or_init(|| LoanProvenance::collect(self.checker))
    }

    pub(super) fn borrow_bindings_initialized_by(
        &self,
        initializer: UnitExpressionId,
    ) -> impl Iterator<Item = &UnitBorrowBinding> {
        let bindings = self.checker.borrow_results.bindings();
        self.loan_provenance()
            .bindings_by_initializer
            .get(&initializer)
            .into_iter()
            .flatten()
            .map(|&index| &bindings[index])
    }

    pub(super) fn forwarded_borrow_source(
        &self,
        call: UnitExpressionId,
    ) -> Option<(BorrowSourceLoan<UnitExpressionId>, &UnitBorrowReturnOrigin)> {
        let entry = self
            .borrow_forward_index
            .get_or_init(|| BorrowForwardIndex::collect(self.checker))
            .by_call
            .get(&call)?;
        Some((
            entry.source,
            &self.checker.borrow_return_origins[entry.origin?],
        ))
    }

    pub(super) fn is_observable_owned_loan_root(&self, root: UnitSymbolId) -> bool {
        let provenance = self.loan_provenance();
        (provenance.formed_owned.contains(&root)
            || self
                .checker
                .bindings
                .get(&root)
                .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Owned))
            && !provenance.iteration_providers.contains(&root)
            && !provenance.borrow_bindings.contains(&root)
            && matches!(
                self.checker.symbol_kind(root),
                Some(SymbolKind::Variable | SymbolKind::ValueParameter)
            )
    }
}
