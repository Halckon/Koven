//! A narrow cross-crate test bridge: no public activation callback or ordinary product escape.
use super::*;
use crate::{
    name_resolution::{DeclarationId, SourceUnitInput, ValidatedCompilationUnitNames},
    ownership_checking::{OwnershipCheckingError, check_compilation_unit_ownership},
    source::SourceMap,
    type_checking::{TypeEnvironment, ValidatedCompilationUnitTypes},
};

/// Successful experimental facts only; cannot be passed to normal lowering entry points.
#[derive(Clone, Debug)]
pub struct TwoSourceExperiment {
    facts: Vec<TwoSourceCounterFact>,
    typed_owner: std::sync::Arc<()>,
    callables: Vec<DeclarationId>,
    leaves: Option<Vec<super::CalleeLeaf>>,
}
impl TwoSourceExperiment {
    /// Returns only facts retained after the complete validation gate.
    pub fn facts(&self) -> &[TwoSourceCounterFact] {
        &self.facts
    }
    /// Returns the complete admitted callee leaf closure; unsupported bodies return None.
    pub fn callee_leaves(&self) -> Option<&[super::CalleeLeaf]> {
        self.leaves.as_deref()
    }
    /// Checks this experiment issued the leaf, not merely an equal signature/body.
    pub fn owns_leaf(&self, leaf: &super::CalleeLeaf) -> bool {
        self.leaves
            .as_ref()
            .is_some_and(|leaves| leaves.iter().any(|candidate| candidate.same_identity(leaf)))
    }
    /// Checks the issuing typed analysis, including across structurally equal reanalysis.
    pub fn matches_types(&self, typed: &ValidatedCompilationUnitTypes) -> bool {
        std::sync::Arc::ptr_eq(&self.typed_owner, typed.types().analysis_owner())
    }
    /// Returns the exact issuing source-unit/item declaration for an owned fact.
    pub fn callable(&self, fact: &TwoSourceCounterFact) -> Option<DeclarationId> {
        self.facts
            .iter()
            .position(|candidate| Rc::ptr_eq(&candidate.identity, &fact.identity))
            .and_then(|index| self.callables.get(index).copied())
    }
    /// Checks analysis-local identity, not structural equality.
    pub fn owns(&self, fact: &TwoSourceCounterFact) -> bool {
        self.facts
            .iter()
            .any(|candidate| Rc::ptr_eq(&candidate.identity, &fact.identity))
    }
}
impl TwoSourceCounterFact {
    /// Returns ordered closed arm metadata, not independent arm authority.
    pub fn arms(&self) -> &[UnitLoopNoCarryFact; 2] {
        &self.arms
    }
    /// Returns the exact ordered callsites and normalized instance keys.
    pub fn calls(&self) -> &[Vec<(UnitExpressionId, UnitCallableInstanceKey)>; 2] {
        &self.calls
    }
    /// Returns the source-qualified branch expression.
    pub fn branch(&self) -> UnitExpressionId {
        self.branch
    }
}

/// Analyze with the real checker, keeping all activation and ordinary products private.
/// `None` means the full validation gate failed or no two-source fact was produced.
/// The feature alone does not change normal analysis; this function never returns its
/// internal ordinary validated product, including on failure.
pub fn analyze_two_source_experiment(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
) -> Result<Option<TwoSourceExperiment>, OwnershipCheckingError> {
    let (analyzed, _) = with_domain(|| {
        check_compilation_unit_ownership(sources, inputs, names, environment, typed)
    });
    let Ok(validated) = analyzed?.validate() else {
        return Ok(None);
    };
    let facts = validated.into_ownership().no_carry.two_sources;
    let callables = facts
        .iter()
        .map(|fact| {
            let callable = fact.arms[0].callable();
            let mut declarations = names.names().index().declarations().iter().filter(|d| {
                d.source_unit() == callable.source_unit() && d.root() == callable.item()
            });
            let declaration = declarations.next()?;
            declarations.next().is_none().then_some(declaration.id())
        })
        .collect::<Option<Vec<_>>>();
    let Some(callables) = callables else {
        return Ok(None);
    };
    let leaves = super::leaves::collect(&facts, inputs, names, typed);
    Ok((!facts.is_empty()).then_some(TwoSourceExperiment {
        facts,
        typed_owner: typed.types().analysis_owner().clone(),
        callables,
        leaves,
    }))
}
