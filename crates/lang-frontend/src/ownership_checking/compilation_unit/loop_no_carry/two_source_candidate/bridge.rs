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
    scoped_leaves: Vec<Option<Vec<super::CalleeLeaf>>>,
    arm_spans: Vec<[super::ArmSourceSpans; 2]>,
    shells: Vec<Option<super::FunctionShell>>,
    entries: Vec<super::EntryDescriptor>,
}
impl TwoSourceExperiment {
    /// Checks the original descriptor identity, including when cloned.
    pub fn owns_entry(&self, entry: &super::EntryDescriptor) -> bool {
        self.entries
            .iter()
            .any(|candidate| candidate.same_identity(entry))
    }

    /// Same-analysis entry evidence only; not a normal ownership capability.
    pub fn entries(
        &self,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<&[super::EntryDescriptor]> {
        self.matches_types(typed).then_some(self.entries.as_slice())
    }

    /// Complete bounded function coverage for an issuing fact and typed analysis only.
    /// Unsupported prefixes, suffixes or ABI return None without revoking the local fact.
    pub fn function_shell(
        &self,
        fact: &TwoSourceCounterFact,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<&super::FunctionShell> {
        if !self.matches_types(typed) {
            return None;
        }
        let index = self
            .facts
            .iter()
            .position(|f| Rc::ptr_eq(&f.identity, &fact.identity))?;
        self.shells.get(index)?.as_ref()
    }
    /// Returns only facts retained after the complete validation gate.
    pub fn facts(&self) -> &[TwoSourceCounterFact] {
        &self.facts
    }
    /// Returns the complete admitted callee leaf closure; unsupported bodies return None.
    pub fn callee_leaves(&self) -> Option<&[super::CalleeLeaf]> {
        self.leaves.as_deref()
    }
    /// Complete leaf closure for an issuing fact and the original typed analysis.
    /// Unsupported closures return None; partial leaf sets never escape.
    pub fn callee_leaves_for(
        &self,
        fact: &TwoSourceCounterFact,
        typed: &ValidatedCompilationUnitTypes,
    ) -> Option<&[super::CalleeLeaf]> {
        if !self.matches_types(typed) {
            return None;
        }
        let index = self
            .facts
            .iter()
            .position(|f| Rc::ptr_eq(&f.identity, &fact.identity))?;
        self.scoped_leaves.get(index)?.as_deref()
    }
    /// Checks this experiment issued the leaf, not merely an equal signature/body.
    pub fn owns_leaf(&self, leaf: &super::CalleeLeaf) -> bool {
        self.leaves
            .iter()
            .chain(self.scoped_leaves.iter().flatten())
            .any(|leaves| leaves.iter().any(|candidate| candidate.same_identity(leaf)))
    }
    /// Returns original source projections only for an owned closed fact.
    pub fn arm_spans(&self, fact: &TwoSourceCounterFact) -> Option<&[super::ArmSourceSpans; 2]> {
        self.facts
            .iter()
            .position(|candidate| Rc::ptr_eq(&candidate.identity, &fact.identity))
            .and_then(|index| self.arm_spans.get(index))
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
    let owned = validated.into_ownership();
    let facts = owned.no_carry.two_sources.clone();
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
    let shells: Vec<_> = facts
        .iter()
        .zip(&callables)
        .map(|(fact, callable)| {
            super::function_shell::collect(fact, *callable, inputs, names, typed)
        })
        .collect();
    let entries = super::entry::collect(sources, &shells, inputs, names, typed, &owned);
    let leaves = super::leaves::collect(&facts, inputs, names, typed);
    // Keep the full ownership success gate above. Only backend-subset eligibility
    // is scoped; a shared key with inconsistent roles invalidates every dependent fact.
    let mut roles = Vec::new();
    let mut conflicts = Vec::new();
    for fact in &facts {
        for calls in &fact.calls {
            for (position, (_, key)) in calls.iter().enumerate() {
                let producer = position == 0;
                if roles
                    .iter()
                    .any(|(prior, role)| prior == key && *role != producer)
                {
                    conflicts.push(key.clone());
                }
                roles.push((key.clone(), producer));
            }
        }
    }
    let scoped_leaves = facts
        .iter()
        .map(|fact| {
            if fact
                .calls
                .iter()
                .flatten()
                .any(|(_, key)| conflicts.contains(key))
            {
                None
            } else {
                super::leaves::collect(std::slice::from_ref(fact), inputs, names, typed)
            }
        })
        .collect();
    let Some(arm_spans) = facts
        .iter()
        .map(|fact| super::arm_spans::collect(fact, inputs, names))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    Ok((!facts.is_empty()).then_some(TwoSourceExperiment {
        facts,
        typed_owner: typed.types().analysis_owner().clone(),
        callables,
        leaves,
        scoped_leaves,
        arm_spans,
        shells,
        entries,
    }))
}
