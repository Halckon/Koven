//! Index immutable checker facts used to classify loan observations.
use super::{Checker, DropPlanner};
use crate::{
    name_resolution::{SymbolKind, UnitSymbolId},
    ownership_checking::{OwnershipBindingKind, UnitLoanTarget},
};
use std::collections::BTreeSet;

pub(super) struct LoanRootProvenance {
    pub(super) formed_owned: BTreeSet<UnitSymbolId>,
    pub(super) iteration_providers: BTreeSet<UnitSymbolId>,
    borrow_bindings: BTreeSet<UnitSymbolId>,
}

impl LoanRootProvenance {
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
        Self {
            formed_owned: checker.formed_owned_locals.iter().copied().collect(),
            iteration_providers,
            borrow_bindings: checker
                .borrow_results
                .bindings()
                .iter()
                .map(|binding| binding.binding())
                .collect(),
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn loan_root_provenance(&self) -> &LoanRootProvenance {
        // The planner only borrows the completed checker. Trial rollback and
        // ordered publication still use the checker's original collections.
        self.loan_root_provenance
            .get_or_init(|| LoanRootProvenance::collect(self.checker))
    }

    pub(super) fn is_observable_owned_loan_root(&self, root: UnitSymbolId) -> bool {
        let provenance = self.loan_root_provenance();
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
