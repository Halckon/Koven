//! Sealed read-only facts consumed by the existing whole-unit backend preflight.
//! This interface does not itself certify source identity or successful preflight.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::borrow_result::UnitBorrowResults;

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

impl sealed::Sealed for CompilationUnitOwnership {}
impl UnitPreflightFacts for CompilationUnitOwnership {
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
