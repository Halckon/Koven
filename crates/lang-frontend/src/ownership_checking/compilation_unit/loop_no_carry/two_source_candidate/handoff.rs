//! Checked experimental source handoff, NOT a completed backend preflight or ordinary view.
use super::{TwoSourceCounterFact, TwoSourceExperiment};
use crate::ownership_checking::borrow_result::UnitBorrowResults;
use crate::{
    name_resolution::{SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{OwnedCompilationUnitViewError, handoff::validate_source_analysis},
    source::SourceMap,
    type_checking::{TypeEnvironment, ValidatedCompilationUnitTypes},
};

/// Private construction, no raw ownership getter, Deref, conversion or callback escape.
/// All source borrows remain bound to the exact issuing experimental analysis.
/// Backend declaration/binding/global checks have NOT been run by this handoff.
///
/// ```compile_fail,E0308
/// use lang_frontend::ownership_checking::{OwnedCompilationUnitView, two_source_experiment::ExperimentalUnitHandoff};
/// fn ordinary(_: &OwnedCompilationUnitView<'_, '_>) {}
/// fn reject(experimental: &ExperimentalUnitHandoff<'_, '_>) {
///     ordinary(experimental);
/// }
/// ```
pub struct ExperimentalUnitHandoff<'view, 'parsed: 'view> {
    experiment: &'view TwoSourceExperiment,
    selected: Option<&'view TwoSourceCounterFact>,
    sources: &'view SourceMap,
    inputs: &'view [SourceUnitInput<'parsed>],
    names: &'view ValidatedCompilationUnitNames,
    _environment: &'view TypeEnvironment,
    typed: &'view ValidatedCompilationUnitTypes,
}
impl<'view, 'parsed: 'view> ExperimentalUnitHandoff<'view, 'parsed> {
    /// Select exactly one original complete fact; other Unit bindings remain unselected.
    pub fn for_fact(&self, fact: &'view TwoSourceCounterFact) -> Option<Self> {
        if !self.owns(fact) {
            return None;
        }
        self.experiment.function_shell(fact, self.typed)?;
        self.experiment.callee_leaves_for(fact, self.typed)?;
        Some(Self {
            experiment: self.experiment,
            selected: Some(fact),
            sources: self.sources,
            inputs: self.inputs,
            names: self.names,
            _environment: self._environment,
            typed: self.typed,
        })
    }
    /// Original checked source collection.
    pub fn sources(&self) -> &'view SourceMap {
        self.sources
    }
    /// Original parsed input slice; no AST copy.
    pub fn inputs(&self) -> &'view [SourceUnitInput<'parsed>] {
        self.inputs
    }
    /// Original validated names bound by the source gate.
    pub fn names(&self) -> &'view ValidatedCompilationUnitNames {
        self.names
    }
    /// Original typed analysis, not an ownership capability.
    pub fn types(&self) -> &'view ValidatedCompilationUnitTypes {
        self.typed
    }
    /// Controlled query against the actual retained ownership product, not reconstructed facts.
    pub fn owns(&self, fact: &TwoSourceCounterFact) -> bool {
        self.experiment.retained.owns_two_source_counter(fact)
    }
}
impl TwoSourceExperiment {
    /// Reuses the ordinary source-identity gate without creating ordinary owned authority.
    /// Error order remains source identity, then issuing analysis identity.
    pub fn handoff<'view, 'parsed: 'view>(
        &'view self,
        sources: &'view SourceMap,
        inputs: &'view [SourceUnitInput<'parsed>],
        names: &'view ValidatedCompilationUnitNames,
        environment: &'view TypeEnvironment,
        typed: &'view ValidatedCompilationUnitTypes,
    ) -> Result<ExperimentalUnitHandoff<'view, 'parsed>, OwnedCompilationUnitViewError> {
        validate_source_analysis(
            sources,
            inputs,
            names,
            environment,
            typed.types().signatures(),
        )?;
        if !self.matches_types(typed) || !self.retained.is_compatible_with(typed) {
            return Err(OwnedCompilationUnitViewError::MismatchedAnalysis);
        }
        Ok(ExperimentalUnitHandoff {
            experiment: self,
            selected: None,
            sources,
            inputs,
            names,
            _environment: environment,
            typed,
        })
    }
}

use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::*;
impl crate::ownership_checking::preflight::sealed::Sealed for ExperimentalUnitHandoff<'_, '_> {}
impl UnitPreflightFacts for ExperimentalUnitHandoff<'_, '_> {
    fn experimental_two_source_binding(
        &self,
        binding: UnitSymbolId,
    ) -> Option<ExperimentalBinding<'_>> {
        let fact = self.selected?;
        if !self.owns(fact)
            || fact
                .arms()
                .iter()
                .filter(|arm| arm.binding() == binding)
                .count()
                != 1
        {
            return None;
        }
        self.experiment.callee_leaves_for(fact, self.typed)?;
        let shell = self.experiment.function_shell(fact, self.typed)?;
        Some(ExperimentalBinding { fact, shell })
    }

    fn binding_kind(&self, symbol: UnitSymbolId) -> Option<OwnershipBindingKind> {
        self.experiment.retained.binding_kind(symbol)
    }
    fn borrow_call_prefix_ends(&self) -> &[UnitBorrowCallPrefixEndFact] {
        self.experiment.retained.borrow_call_prefix_ends()
    }
    fn borrow_forward_transports(&self) -> &[UnitBorrowForwardTransportFact] {
        self.experiment.retained.borrow_forward_transports()
    }
    fn borrow_results(&self) -> &UnitBorrowResults {
        self.experiment.retained.borrow_results()
    }
    fn borrow_return_origins(
        &self,
    ) -> &[BorrowReturnOriginFact<crate::type_checking::UnitExpressionId, UnitLoanTarget>] {
        self.experiment.retained.borrow_return_origins()
    }
    fn call_argument_contracts(&self) -> &[UnitCallArgumentOwnershipContract] {
        self.experiment.retained.call_argument_contracts()
    }
    fn field_replacements(&self) -> &[UnitFieldReplaceOwnershipPlan] {
        self.experiment.retained.field_replacements()
    }
    fn formed_owned_locals(&self) -> &[UnitSymbolId] {
        self.experiment.retained.formed_owned_locals()
    }
    fn loans(&self) -> &[UnitLoanFact] {
        self.experiment.retained.loans()
    }
    fn map_iterations(&self) -> &[UnitMapIterationOwnershipPlan] {
        self.experiment.retained.map_iterations()
    }
    fn no_carry_branches(&self) -> &[UnitLoopBranchNoCarryFact] {
        self.experiment.retained.no_carry_branches()
    }
    fn no_carry_break_branches(&self) -> &[UnitLoopBreakBranchNoCarryFact] {
        self.experiment.retained.no_carry_break_branches()
    }
    fn no_carry_break_prefixes(&self) -> &[UnitLoopBreakPrefixFact] {
        self.experiment.retained.no_carry_break_prefixes()
    }
    fn no_carry_counter_branches(&self) -> &[UnitLoopCounterBranchNoCarryFact] {
        self.experiment.retained.no_carry_counter_branches()
    }
    fn no_carry_counter_break_branches(&self) -> &[UnitLoopCounterBreakBranchNoCarryFact] {
        self.experiment.retained.no_carry_counter_break_branches()
    }
    fn no_carry_loops(&self) -> &[UnitLoopNoCarryFact] {
        self.experiment.retained.no_carry_loops()
    }
    fn no_carry_prefix_counter_breaks(&self) -> &[UnitLoopPrefixCounterBreakFact] {
        self.experiment.retained.no_carry_prefix_counter_breaks()
    }
    fn owns_no_carry_branch(&self, fact: &UnitLoopBranchNoCarryFact) -> bool {
        self.experiment.retained.owns_no_carry_branch(fact)
    }
    fn owns_no_carry_break_branch(&self, fact: &UnitLoopBreakBranchNoCarryFact) -> bool {
        self.experiment.retained.owns_no_carry_break_branch(fact)
    }
    fn owns_no_carry_break_prefix(&self, fact: &UnitLoopBreakPrefixFact) -> bool {
        self.experiment.retained.owns_no_carry_break_prefix(fact)
    }
    fn owns_no_carry_counter_branch(&self, fact: &UnitLoopCounterBranchNoCarryFact) -> bool {
        self.experiment.retained.owns_no_carry_counter_branch(fact)
    }
    fn owns_no_carry_counter_break_branch(
        &self,
        fact: &UnitLoopCounterBreakBranchNoCarryFact,
    ) -> bool {
        self.experiment
            .retained
            .owns_no_carry_counter_break_branch(fact)
    }
    fn owns_no_carry_loop(&self, fact: &UnitLoopNoCarryFact) -> bool {
        self.experiment.retained.owns_no_carry_loop(fact)
    }
    fn owns_no_carry_prefix_counter_break(&self, fact: &UnitLoopPrefixCounterBreakFact) -> bool {
        self.experiment
            .retained
            .owns_no_carry_prefix_counter_break(fact)
    }
}

/// Same-analysis selected-fact projection, never an ordinary pair or completed preflight.
pub struct ExperimentalBinding<'a> {
    fact: &'a TwoSourceCounterFact,
    shell: &'a super::FunctionShell,
}
impl ExperimentalBinding<'_> {
    /// Original selected fact, with its separate experimental identity.
    pub fn fact(&self) -> &TwoSourceCounterFact {
        self.fact
    }
    /// Immutable complete source shell issued for that fact.
    pub fn shell(&self) -> &super::FunctionShell {
        self.shell
    }
}
