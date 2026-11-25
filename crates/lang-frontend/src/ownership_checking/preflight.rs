//! Sealed read-only facts consumed by the existing whole-unit backend preflight.
//! This interface does not itself certify source identity or successful preflight.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::borrow_result::UnitBorrowResults;
use crate::type_checking::UnitExpressionId;
use std::collections::{HashMap, HashSet};

pub(crate) mod sealed {
    pub trait Sealed {}
}
/// Only frontend-issued fact owners may implement this query surface.
/// No ownership product, conversion, mutation or arbitrary callback is exposed.
///
/// External callers cannot issue their own fact provider.
/// ```compile_fail,E0277
/// use lang_frontend::ownership_checking::UnitPreflightFacts;
/// struct Forged;
/// impl UnitPreflightFacts for Forged {}
/// ```
pub trait UnitPreflightFacts: sealed::Sealed {
    /// Test-only-domain selection; ordinary facts never grant this experimental admission.
    #[cfg(any(feature = "e2-two-source-test-support", feature = "two-source-unit"))]
    fn experimental_two_source_binding(
        &self,
        _binding: UnitSymbolId,
    ) -> Option<two_source_experiment::ExperimentalBinding<'_>> {
        None
    }

    /// Confirm the exact Range loop-body control against actual iteration,
    /// last-use and owner-delivery records.
    fn named_range_loop_reuse_controls(&self) -> HashMap<UnitSymbolId, UnitExpressionId> {
        HashMap::new()
    }

    /// Original binding kind query; no fact reconstruction.
    fn binding_kind(&self, symbol: UnitSymbolId) -> Option<OwnershipBindingKind>;
    /// Original borrow call prefix ends query; no fact reconstruction.
    fn borrow_call_prefix_ends(&self) -> &[UnitBorrowCallPrefixEndFact];
    /// Original borrow forward transports query; no fact reconstruction.
    fn borrow_forward_transports(&self) -> &[UnitBorrowForwardTransportFact];
    /// Original borrow results query; no fact reconstruction.
    fn borrow_results(&self) -> &UnitBorrowResults;
    /// Original borrow return origins query; no fact reconstruction.
    fn borrow_return_origins(
        &self,
    ) -> &[BorrowReturnOriginFact<crate::type_checking::UnitExpressionId, UnitLoanTarget>];
    /// Original call argument contracts query; no fact reconstruction.
    fn call_argument_contracts(&self) -> &[UnitCallArgumentOwnershipContract];
    /// Original field replacements query; no fact reconstruction.
    fn field_replacements(&self) -> &[UnitFieldReplaceOwnershipPlan];
    /// Original formed owned locals query; no fact reconstruction.
    fn formed_owned_locals(&self) -> &[UnitSymbolId];
    /// Original loans query; no fact reconstruction.
    fn loans(&self) -> &[UnitLoanFact];
    /// Original nullable when query.
    fn nullable_when(&self, control: UnitExpressionId) -> Option<&UnitNullableWhenOwnershipPlan>;
    /// Original map iterations query; no fact reconstruction.
    fn map_iterations(&self) -> &[UnitMapIterationOwnershipPlan];
    /// Original no carry branches query; no fact reconstruction.
    fn no_carry_branches(&self) -> &[UnitLoopBranchNoCarryFact];
    /// Original no carry break branches query; no fact reconstruction.
    fn no_carry_break_branches(&self) -> &[UnitLoopBreakBranchNoCarryFact];
    /// Original no carry break prefixes query; no fact reconstruction.
    fn no_carry_break_prefixes(&self) -> &[UnitLoopBreakPrefixFact];
    /// Original no carry counter branches query; no fact reconstruction.
    fn no_carry_counter_branches(&self) -> &[UnitLoopCounterBranchNoCarryFact];
    /// Original no carry counter break branches query; no fact reconstruction.
    fn no_carry_counter_break_branches(&self) -> &[UnitLoopCounterBreakBranchNoCarryFact];
    /// Original no carry loops query; no fact reconstruction.
    fn no_carry_loops(&self) -> &[UnitLoopNoCarryFact];
    /// Original no carry prefix counter breaks query; no fact reconstruction.
    fn no_carry_prefix_counter_breaks(&self) -> &[UnitLoopPrefixCounterBreakFact];
    /// Original owns no carry branch query; no fact reconstruction.
    fn owns_no_carry_branch(&self, fact: &UnitLoopBranchNoCarryFact) -> bool;
    /// Original owns no carry break branch query; no fact reconstruction.
    fn owns_no_carry_break_branch(&self, fact: &UnitLoopBreakBranchNoCarryFact) -> bool;
    /// Original owns no carry break prefix query; no fact reconstruction.
    fn owns_no_carry_break_prefix(&self, fact: &UnitLoopBreakPrefixFact) -> bool;
    /// Original owns no carry counter branch query; no fact reconstruction.
    fn owns_no_carry_counter_branch(&self, fact: &UnitLoopCounterBranchNoCarryFact) -> bool;
    /// Original owns no carry counter break branch query; no fact reconstruction.
    fn owns_no_carry_counter_break_branch(
        &self,
        fact: &UnitLoopCounterBreakBranchNoCarryFact,
    ) -> bool;
    /// Original owns no carry loop query; no fact reconstruction.
    fn owns_no_carry_loop(&self, fact: &UnitLoopNoCarryFact) -> bool;
    /// Original owns no carry prefix counter break query; no fact reconstruction.
    fn owns_no_carry_prefix_counter_break(&self, fact: &UnitLoopPrefixCounterBreakFact) -> bool;
}

// The ordinary provider seals its original receipts in one pass for an entire
// preflight. A repeated row poisons the key even if both payloads are identical.
fn insert_unique<K: Copy + std::hash::Hash + Eq, V>(
    rows: &mut HashMap<K, Option<V>>,
    key: K,
    value: V,
) {
    match rows.entry(key) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(Some(value));
        }
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            entry.insert(None);
        }
    }
}

impl sealed::Sealed for CompilationUnitOwnership {}
impl UnitPreflightFacts for CompilationUnitOwnership {
    fn named_range_loop_reuse_controls(&self) -> HashMap<UnitSymbolId, UnitExpressionId> {
        CompilationUnitOwnership::named_range_loop_reuse_controls(self)
    }

    fn binding_kind(&self, symbol: UnitSymbolId) -> Option<OwnershipBindingKind> {
        CompilationUnitOwnership::binding_kind(self, symbol)
    }
    fn borrow_call_prefix_ends(&self) -> &[UnitBorrowCallPrefixEndFact] {
        CompilationUnitOwnership::borrow_call_prefix_ends(self)
    }
    fn borrow_forward_transports(&self) -> &[UnitBorrowForwardTransportFact] {
        CompilationUnitOwnership::borrow_forward_transports(self)
    }
    fn borrow_results(&self) -> &UnitBorrowResults {
        CompilationUnitOwnership::borrow_results(self)
    }
    fn borrow_return_origins(
        &self,
    ) -> &[BorrowReturnOriginFact<crate::type_checking::UnitExpressionId, UnitLoanTarget>] {
        CompilationUnitOwnership::borrow_return_origins(self)
    }
    fn call_argument_contracts(&self) -> &[UnitCallArgumentOwnershipContract] {
        CompilationUnitOwnership::call_argument_contracts(self)
    }
    fn field_replacements(&self) -> &[UnitFieldReplaceOwnershipPlan] {
        CompilationUnitOwnership::field_replacements(self)
    }
    fn formed_owned_locals(&self) -> &[UnitSymbolId] {
        CompilationUnitOwnership::formed_owned_locals(self)
    }
    fn loans(&self) -> &[UnitLoanFact] {
        CompilationUnitOwnership::loans(self)
    }
    fn nullable_when(&self, control: UnitExpressionId) -> Option<&UnitNullableWhenOwnershipPlan> {
        CompilationUnitOwnership::nullable_when(self, control)
    }
    fn map_iterations(&self) -> &[UnitMapIterationOwnershipPlan] {
        CompilationUnitOwnership::map_iterations(self)
    }
    fn no_carry_branches(&self) -> &[UnitLoopBranchNoCarryFact] {
        CompilationUnitOwnership::no_carry_branches(self)
    }
    fn no_carry_break_branches(&self) -> &[UnitLoopBreakBranchNoCarryFact] {
        CompilationUnitOwnership::no_carry_break_branches(self)
    }
    fn no_carry_break_prefixes(&self) -> &[UnitLoopBreakPrefixFact] {
        CompilationUnitOwnership::no_carry_break_prefixes(self)
    }
    fn no_carry_counter_branches(&self) -> &[UnitLoopCounterBranchNoCarryFact] {
        CompilationUnitOwnership::no_carry_counter_branches(self)
    }
    fn no_carry_counter_break_branches(&self) -> &[UnitLoopCounterBreakBranchNoCarryFact] {
        CompilationUnitOwnership::no_carry_counter_break_branches(self)
    }
    fn no_carry_loops(&self) -> &[UnitLoopNoCarryFact] {
        CompilationUnitOwnership::no_carry_loops(self)
    }
    fn no_carry_prefix_counter_breaks(&self) -> &[UnitLoopPrefixCounterBreakFact] {
        CompilationUnitOwnership::no_carry_prefix_counter_breaks(self)
    }
    fn owns_no_carry_branch(&self, fact: &UnitLoopBranchNoCarryFact) -> bool {
        CompilationUnitOwnership::owns_no_carry_branch(self, fact)
    }
    fn owns_no_carry_break_branch(&self, fact: &UnitLoopBreakBranchNoCarryFact) -> bool {
        CompilationUnitOwnership::owns_no_carry_break_branch(self, fact)
    }
    fn owns_no_carry_break_prefix(&self, fact: &UnitLoopBreakPrefixFact) -> bool {
        CompilationUnitOwnership::owns_no_carry_break_prefix(self, fact)
    }
    fn owns_no_carry_counter_branch(&self, fact: &UnitLoopCounterBranchNoCarryFact) -> bool {
        CompilationUnitOwnership::owns_no_carry_counter_branch(self, fact)
    }
    fn owns_no_carry_counter_break_branch(
        &self,
        fact: &UnitLoopCounterBreakBranchNoCarryFact,
    ) -> bool {
        CompilationUnitOwnership::owns_no_carry_counter_break_branch(self, fact)
    }
    fn owns_no_carry_loop(&self, fact: &UnitLoopNoCarryFact) -> bool {
        CompilationUnitOwnership::owns_no_carry_loop(self, fact)
    }
    fn owns_no_carry_prefix_counter_break(&self, fact: &UnitLoopPrefixCounterBreakFact) -> bool {
        CompilationUnitOwnership::owns_no_carry_prefix_counter_break(self, fact)
    }
}

impl CompilationUnitOwnership {
    /// Passive validation for the bounded named-Range loop use.
    pub(super) fn named_range_loop_reuse_controls(
        &self,
    ) -> HashMap<UnitSymbolId, UnitExpressionId> {
        use UnitIterationCleanupAction as Action;
        use UnitIterationExitKind as Exit;
        let results = self.borrow_results();
        if results.named_range_loop_reuses.is_empty()
            || !self.diagnostics().is_empty()
            || !self.deferred().is_empty()
        {
            return HashMap::new();
        }
        let mut witnesses = HashMap::new();
        for row in &results.named_range_loop_reuses {
            insert_unique(&mut witnesses, row.binding, row);
        }
        let mut ends = HashMap::new();
        let mut bindings = HashMap::new();
        let mut sources = HashSet::new();
        for end in results.ends() {
            if witnesses.contains_key(&end.binding()) {
                insert_unique(&mut ends, end.binding(), end.point());
            }
        }
        for binding in results.bindings() {
            if witnesses.contains_key(&binding.binding()) {
                insert_unique(&mut bindings, binding.binding(), binding);
                if let Some(source) = binding.source_loan() {
                    sources.insert((source.call(), source.argument()));
                }
            }
        }
        let roots: HashSet<_> = witnesses.values().flatten().map(|row| row.root).collect();
        let mut plans = HashMap::new();
        let mut loans = HashMap::new();
        let mut moves = HashMap::new();
        let mut drops = HashSet::new();
        for plan in self.iterations() {
            insert_unique(&mut plans, plan.descriptor().statement(), plan);
        }
        for loan in self.loans() {
            let key = (loan.call(), loan.argument());
            if sources.contains(&key) {
                insert_unique(&mut loans, key, loan);
            }
        }
        for delivery in self.value_deliveries() {
            if delivery.kind() == UnitValueDeliveryKind::Move
                && let Some(place) = delivery.place()
                && roots.contains(&place.root())
            {
                insert_unique(&mut moves, place.root(), delivery);
            }
        }
        for drop in self.drops() {
            if let UnitDropTarget::Named(root) = drop.target()
                && roots.contains(&root)
            {
                drops.insert(root);
            }
        }
        witnesses.into_iter().filter_map(|(symbol, row)| {
            let row = row?;
            let UnitDropPoint::LoopExit(statement) = row.loop_exit else { return None; };
            let UnitDropPoint::AfterStatement(end) = row.end else { return None; };
            let source_unit = symbol.source_unit();
            if statement.source_unit() != source_unit || end.source_unit() != source_unit
                || row.root.source_unit() != source_unit
                || [row.source, row.body_control, row.observer, row.consume, row.consumed_operand]
                    .iter().any(|id| id.source_unit() != source_unit)
                || row.body_control == row.source || row.observer == row.consume
                || drops.contains(&row.root)
                || ends.get(&symbol).copied().flatten() != Some(row.end)
            { return None; }
            let binding = (*bindings.get(&symbol)?)?;
            let original = binding.source_loan()?;
            let loan = (*loans.get(&(original.call(), original.argument()))?)?;
            if binding.storage() != BorrowBindingStorage::NewRangeDescriptor
                || !matches!(binding.origin(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == row.root)
                || loan.target() != binding.origin() || loan.kind() != LoanKind::Shared
            { return None; }
            let plan = (*plans.get(&statement)?)?;
            if plan.descriptor().source() != row.source
                || plan.descriptor().provider() != crate::type_checking::IterationProvider::RangeView
                || !matches!(plan.source(), Some(UnitLoanTarget::Place(place)) if place.is_root() && place.root() == symbol)
            { return None; }
            let [element] = plan.bindings() else { return None; };
            if element.symbol() == symbol || element.symbol().source_unit() != source_unit
                || element.kind() != OwnershipBindingKind::Shared
            { return None; }
            let mut fallthrough = false;
            let mut exhaustion = false;
            for exit in plan.exits() {
                match exit.kind() {
                    Exit::Fallthrough if !fallthrough => {
                        if !matches!(exit.point(), UnitDropPoint::AfterStatement(body)
                            if body.source_unit() == source_unit)
                            || !matches!(exit.actions(),
                                [Action::EndBinding { statement: owner, symbol: ended }, Action::EndElement(id)]
                                    if *owner == statement && *id == statement && *ended == element.symbol())
                        { return None; }
                        fallthrough = true;
                    }
                    Exit::Exhaustion if !exhaustion && exit.point() == row.loop_exit => {
                        if !matches!(exit.actions(), [Action::FinishProvider(provider), Action::EndSource(source)]
                            if *provider == statement && *source == statement)
                        { return None; }
                        exhaustion = true;
                    }
                    _ => return None,
                }
            }
            let moved = (*moves.get(&row.root)?)?;
            (fallthrough && exhaustion
                && moved.call() == row.consume
                && moved.argument() == row.consumed_operand
                && moved.place().is_some_and(|place| place.is_root()))
            .then_some((symbol, row.body_control))
        }).collect()
    }
}
