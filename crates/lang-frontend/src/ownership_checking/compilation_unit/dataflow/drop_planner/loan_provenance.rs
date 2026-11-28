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
    bindings_by_symbol: BTreeMap<UnitSymbolId, usize>,
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
            bindings_by_symbol: binding_symbol_index(
                checker
                    .borrow_results
                    .bindings()
                    .iter()
                    .map(|fact| fact.binding()),
            ),
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

impl<'a, 'checker> DropPlanner<'a, 'checker> {
    pub(super) fn borrow_binding_by_symbol(
        &self,
        symbol: UnitSymbolId,
    ) -> Option<&'a UnitBorrowBinding> {
        let index = *self.loan_provenance().bindings_by_symbol.get(&symbol)?;
        self.checker.borrow_results.bindings().get(index)
    }
}

// Preserve ordered first-match semantics; no facts are copied or republished.
fn binding_symbol_index(
    symbols: impl Iterator<Item = UnitSymbolId>,
) -> BTreeMap<UnitSymbolId, usize> {
    let mut result = BTreeMap::new();
    for (index, symbol) in symbols.enumerate() {
        result.entry(symbol).or_insert(index);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    use std::cell::Cell;

    #[test]
    fn binding_lookup_preserves_source_identity_and_ordered_first_match() {
        let source = |name: &str| {
            format!(
                "fun view{name}(source:String):borrow String from source=source\nfun use{name}(value:String):Unit {{}}\nfun run{name}():Unit {{ val owner=\"one\"; borrow val alias=view{name}(owner); use{name}(alias) }}"
            )
        };
        let a = source("A");
        let b = source("B");
        let fixture = Fixture::new(&[("a.ko", &a), ("b.ko", &b)]);
        let shared = fixture.shared();
        let raw = fixture.dataflow(&fixture.parts(&shared));
        assert!(raw.diagnostics.is_empty(), "{:?}", raw.diagnostics);
        let facts = raw.borrow_results.bindings();
        assert_eq!(facts.len(), 2);
        assert_ne!(facts[0].binding(), facts[1].binding());
        assert_eq!(facts[0].binding(), fixture.symbol(0, "alias"));
        assert_eq!(facts[1].binding(), fixture.symbol(1, "alias"));
        let visited = Cell::new(0);
        let symbols = facts.iter().map(|fact| {
            visited.set(visited.get() + 1);
            fact.binding()
        });
        let index = binding_symbol_index(symbols);
        assert_eq!(visited.get(), 2);
        for _ in 0..32 {
            for fact in facts {
                let expected = facts
                    .iter()
                    .find(|candidate| candidate.binding() == fact.binding())
                    .unwrap();
                let actual = &facts[index[&fact.binding()]];
                assert!(std::ptr::eq(actual, expected));
                assert_eq!(actual, expected);
            }
        }
        assert_eq!(
            visited.get(),
            2,
            "lookups do not revisit the original fact iterator"
        );
        assert!(!index.contains_key(&fixture.symbol(0, "owner")));
        let repeated = binding_symbol_index(
            facts
                .iter()
                .map(|fact| fact.binding())
                .chain([facts[0].binding(), facts[1].binding()]),
        );
        assert_eq!(
            repeated, index,
            "duplicate keys preserve original .find semantics, not last-wins"
        );
    }
}
